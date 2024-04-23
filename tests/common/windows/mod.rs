// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//spell-checker: ignore conpty conin conout ENDHEA PSEUDOCONSOLE STARTF USESTDHANDLES

pub(crate) mod conpty;

use std::mem::size_of_val;
use std::os::raw::c_void;
use std::os::windows::io::FromRawHandle;
use std::ptr::null_mut;
use std::thread::JoinHandle;
use std::{
    fs::File,
    io::{self, Read, Write},
};
use std::{
    os::windows::process::CommandExt,
    process::{Command, Stdio},
};
use uucore::windows_sys::Win32::System::{
    Console::GetConsoleProcessList, Threading::PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
};
use self::conpty::OwnedPseudoConsoleHandle;

use super::util::{ForwardedOutput, TerminalSimulation, TESTS_BINARY};

pub(crate) static END_OF_TRANSMISSION_SEQUENCE: &[u8] = &[b'\r', b'\n', 0x1A, b'\r', b'\n']; // send ^Z

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum Error {
    StdOsIo(std::io::Error),
    Message(String),
}

pub(crate) type Result<T> = std::result::Result<T, Error>;

pub(crate) struct ConsoleSpawnWrap {
    terminal_simulation: Option<TerminalSimulation>,
    child_console: Option<conpty::OwnedPseudoConsoleHandle>,
    dummy_out_reader: Option<JoinHandle<()>>,
    background_cmd: Option<std::process::Child>,
}

impl ConsoleSpawnWrap {
    pub(crate) fn new(terminal_simulation: Option<TerminalSimulation>) -> Self {
        Self {
            terminal_simulation,
            child_console: None,
            dummy_out_reader: None,
            background_cmd: None,
        }
    }

    pub(crate) fn spawn<T: FnOnce(&mut ConsoleSpawnWrap)>(&mut self, spawn_function: T) {
        spawn_function(self);
    }

    fn prepare_command_to_use_console(
        command: &mut std::process::Command,
        pty: &OwnedPseudoConsoleHandle,
        simulated_terminal: &TerminalSimulation,
    ) {
        let raw_hpc = pty.get_raw_handle();
        unsafe {
            command.raw_attribute_ptr(
                PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
                raw_hpc as *const c_void,
                size_of_val(&raw_hpc),
            )
        };
        //command.creation_flags(STARTF_USESTDHANDLES);
        Self::configure_stdio_for_spawn_of_child(&simulated_terminal, command);
    }

    fn run_command_in_console(mut cmd: Command, pty: &OwnedPseudoConsoleHandle) {
        Self::prepare_command_to_use_console(&mut cmd, &pty, &TerminalSimulation::full());
        cmd.spawn().unwrap().wait().unwrap();
    }

    fn set_title_of_console(title: &str, pty: &OwnedPseudoConsoleHandle) {
        let mut cmd = std::process::Command::new("cmd");
        cmd.args(["/C", "title", title]);
        Self::run_command_in_console(cmd, pty);
    }

    fn disable_echo_of_console(&mut self, pty: &OwnedPseudoConsoleHandle) {
        let mut cmd = std::process::Command::new(TESTS_BINARY);
        cmd.args(["env", TESTS_BINARY, "stty", "--", "-echo"]);
        cmd.args(["&&", TESTS_BINARY, "echo", "disabled echo"]);
        cmd.args(["&&", TESTS_BINARY, "sleep", "3600"]);
        Self::prepare_command_to_use_console(&mut cmd, &pty, &TerminalSimulation::full());
        // This process is spawned, but we don't wait for it.
        // The long sleep is intended as the console will reset
        // the echo setting as soon as the process terminates.
        self.background_cmd = Some(cmd.spawn().unwrap());
    }

    pub(crate) fn setup_stdio_hook(
        &mut self,
        command: &mut std::process::Command,
        captured_stdout: &mut ForwardedOutput,
        captured_stderr: &mut ForwardedOutput,
        stdin_pty: &mut Option<Box<dyn Write + Send>>,
    ) {
        if let Some(simulated_terminal) = self.terminal_simulation.clone() {
            let terminal_size = simulated_terminal.size.unwrap_or_default();

            let (pty, output, input) = conpty::create_pseudo_console((
                terminal_size.cols as i16,
                terminal_size.rows as i16,
            ))
            .unwrap();

            let title = "uutils_unittest_console";
            Self::set_title_of_console(title, &pty);
            if !simulated_terminal.echo {
                self.disable_echo_of_console(&pty);
            }

            Self::prepare_command_to_use_console(command, &pty, &simulated_terminal);

            self.child_console = Some(pty);
            *stdin_pty = Some(Box::new(File::from(input)));
            let mut reader = File::from(output);

            let ansi_show_cursor = b"\x1b[?25h";
            let disabled_echo = b"disabled echo\r\n";
            let console_title = title.as_bytes();
            let mut keywords = Vec::new();
            keywords.push(ansi_show_cursor.as_ref());
            keywords.push(console_title);
            if !simulated_terminal.echo {
                keywords.push(disabled_echo);
            }
            let _header = read_till_keywords(&mut reader, &keywords);
            println!("read header: {}", _header.escape_ascii());

            let forwarded = if simulated_terminal.stdout {
                Some(captured_stdout)
            } else if simulated_terminal.stderr {
                Some(captured_stderr)
            } else {
                None
            };

            if let Some(forwarded_io) = forwarded {
                forwarded_io
                    .spawn_reader_thread(Box::new(reader), "win_conpty_reader".into())
                    .unwrap();
            } else {
                self.dummy_out_reader = std::thread::Builder::new()
                    .name("dummy_console_out_reader".to_string())
                    .spawn(move || {
                        ForwardedOutput::read_from_pty(Box::new(File::from(reader)), io::sink());
                    })
                    .ok();
            }
        }
    }

    fn get_console_process_id_list(exclude_self: bool) -> Vec<u32> {
        let mut dummy_buf = [0u32, 0u32, 0u32];
        let process_count =
            unsafe { GetConsoleProcessList(dummy_buf.as_mut_ptr(), dummy_buf.len() as u32) };
        if process_count > 0 {
            let mut buffer = vec![0u32; process_count as usize + 20];
            let process_count =
                unsafe { GetConsoleProcessList(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
            if process_count <= buffer.len() {
                buffer.resize(process_count, 0);
                if exclude_self {
                    let own_id = std::process::id();
                    buffer.retain(|id| (*id) != own_id);
                }
                return buffer;
            }
        }

        panic!("failed to get console process id list!");
    }

    //fn kill_and_wait_all_console_processes(&mut self) {
    //    if let Some(console) = &self.child_console {
    //        let _guards = AllReAttachConsoleGuard::new(console.pid());
    //        let process_ids = Self::get_console_process_id_list(true);
    //        mem::drop(_guards);
    //        let handles = process_ids
    //            .into_iter()
    //            .filter_map(|id| process::ProcessHandle::new_from_id(id).ok());
    //        handles.clone().for_each(|ph| {
    //            let _ = ph.terminate(88);
    //        });
    //        handles.for_each(|ph| {
    //            let _ = ph.wait_for_end(5000);
    //        });
    //    }
    //}

    fn configure_stdio_for_spawn_of_child(
        simulated_terminal: &TerminalSimulation,
        command: &mut Command,
    ) {
        let handle_fn = || unsafe { Stdio::from_raw_handle(0 as isize as *mut c_void) };

        if simulated_terminal.stdin {
            command.stdin(handle_fn());
        }
        if simulated_terminal.stdout {
            command.stdout(handle_fn());
        }
        if simulated_terminal.stderr {
            command.stderr(handle_fn());
        }
    }
}

impl Drop for ConsoleSpawnWrap {
    fn drop(&mut self) {
        //self.kill_and_wait_all_console_processes();
        self.child_console = None;
        if let Some(mut cmd) = std::mem::take(&mut self.background_cmd) {
            let _ = cmd.kill();
            let _ = cmd.wait();
        }
    }
}

fn compare_keyword_with_end(keyword: &[u8], buffer: &[u8]) -> bool {
    keyword
        .iter()
        .rev()
        .zip(buffer.iter().rev())
        .all(|(a, b)| a == b)
}

fn read_till_keywords<T: Read>(reader: &mut T, keywords: &[&[u8]]) -> Vec<u8> {
    let mut full_buf = Vec::new();
    let mut found_flags = Vec::new();
    found_flags.resize(keywords.len(), false);
    loop {
        let mut buf = [0u8];
        reader.read_exact(&mut buf).unwrap();
        full_buf.push(buf[0]);
        for (i, keyword) in keywords.iter().enumerate() {
            if !found_flags[i] && (full_buf.len() >= keyword.len()) {
                found_flags[i] = compare_keyword_with_end(keyword, &full_buf);
            }
        }
        if found_flags.iter().all(|x| *x) {
            return full_buf;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::read_till_keywords;
    const KEYWORDS: [&[u8]; 2] = ["\x1b[?25h".as_bytes(), "ENDHEA".as_bytes()];

    #[test]
    #[should_panic(expected = "failed to fill whole buffer")]
    fn test_detection_of_keywords_fails_only_first_keyword() {
        let mut string = "====================================\x1b[?25h".as_bytes();
        read_till_keywords(&mut string, &KEYWORDS);
    }

    #[test]
    #[should_panic(expected = "failed to fill whole buffer")]
    fn test_detection_of_keywords_fails_only_second_keyword() {
        let mut string = "--------------------------------ENDHEA".as_bytes();
        read_till_keywords(&mut string, &KEYWORDS);
    }

    #[test]
    fn test_detection_of_keywords_succeeds_with_first_and_second_keyword() {
        let mut string = "^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^\x1b[?25hENDHEA".as_bytes();
        read_till_keywords(&mut string, &KEYWORDS);
    }

    #[test]
    fn test_detection_of_keywords_succeeds_with_second_and_first_keyword() {
        let mut string = ".....................................ENDHEA\x1b[?25h".as_bytes();
        read_till_keywords(&mut string, &KEYWORDS);
    }

    #[test]
    fn test_detection_of_keywords_succeeds_with_second_and_first_keyword_and_stuff_in_between() {
        let mut string = "+++++++++++++++++++++ENDHEA+++++++++++++++++++++++\x1b[?25h".as_bytes();
        read_till_keywords(&mut string, &KEYWORDS);
    }
}
