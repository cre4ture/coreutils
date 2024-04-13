// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//spell-checker: ignore conpty conin conout ENDHEA openpty

mod process;

use portable_pty::{CommandBuilder, PtySize, PtySystem};
use std::cmp::max;
use std::collections::VecDeque;
use std::io::{Read, StderrLock, StdinLock, StdoutLock, Write};
use std::mem;
use std::os::windows::io::AsRawHandle;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::MutexGuard;
use std::time::Duration;
use uucore::windows_sys::Win32::Foundation::HANDLE;
use uucore::windows_sys::Win32::System::Console::{
    AttachConsole, FreeConsole, GetConsoleMode, GetConsoleProcessList, GetStdHandle,
    SetConsoleMode, SetStdHandle, ATTACH_PARENT_PROCESS, CONSOLE_MODE, ENABLE_ECHO_INPUT,
    ENABLE_LINE_INPUT, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
};

use super::util::{ForwardedOutput, TerminalSimulation, TESTS_BINARY};

pub(crate) static END_OF_TRANSMISSION_SEQUENCE: &[u8] = &[b'\r', b'\n', 0x1A, b'\r', b'\n']; // send ^Z

static CONSOLE_SPAWNING_MUTEX: std::sync::Mutex<u32> = std::sync::Mutex::new(0);
static END_OF_HEADER_KEYWORD: &str = "ENDHEA";

#[derive(Debug)]
struct BlockOtherThreadsGuard {
    _list: (
        MutexGuard<'static, u32>,
        StdinLock<'static>,
        StdoutLock<'static>,
        StderrLock<'static>,
    ),
}

impl BlockOtherThreadsGuard {
    fn new() -> Self {
        // To be able to properly spawn the child process inside of the new console,
        // We need to attach our own process temporarily to the new console.
        // This is due to the lack of the std::process interface accepting windows startup information parameters.
        // In this critical phase where our own process is attached to the new console,
        // we can't allow other threads to spawn own consoles or read/write something from/to stdio.
        // This can happen e.g. during execution of multiple test cases in parallel.
        // Therefor this list of guards here:
        Self {
            _list: (
                CONSOLE_SPAWNING_MUTEX.lock().unwrap(),
                std::io::stdin().lock(),
                std::io::stdout().lock(),
                std::io::stderr().lock(),
            ),
        }
    }
}

#[derive(Debug)]
struct AttachStdioGuard {
    original_stdin: HANDLE,
    original_stdout: HANDLE,
    original_stderr: HANDLE,
}

impl AttachStdioGuard {
    fn new() -> Self {
        let original_stdin = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
        let original_stdout = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
        let original_stderr = unsafe { GetStdHandle(STD_ERROR_HANDLE) };
        // setting the handles to null prevents that the spawned child inherits from something
        // other than the pseudo console.
        let _ = unsafe { SetStdHandle(STD_INPUT_HANDLE, 0 as HANDLE) };
        let _ = unsafe { SetStdHandle(STD_OUTPUT_HANDLE, 0 as HANDLE) };
        let _ = unsafe { SetStdHandle(STD_ERROR_HANDLE, 0 as HANDLE) };
        Self {
            original_stdin,
            original_stdout,
            original_stderr,
        }
    }
}

impl Drop for AttachStdioGuard {
    fn drop(&mut self) {
        let _ = unsafe { SetStdHandle(STD_INPUT_HANDLE, self.original_stdin) };
        let _ = unsafe { SetStdHandle(STD_OUTPUT_HANDLE, self.original_stdout) };
        let _ = unsafe { SetStdHandle(STD_ERROR_HANDLE, self.original_stderr) };
    }
}

#[derive(Debug)]
struct SwitchToConsoleGuard {}

impl SwitchToConsoleGuard {
    fn new(process_id: u32) -> Self {
        let _ = unsafe { FreeConsole() };

        let mut success = false;
        for _i in 0..1 {
            success = unsafe { AttachConsole(process_id) } != 0;
            if success {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        if !success {
            panic!("attaching to new console failed!");
        }
        Self {}
    }
}

impl Drop for SwitchToConsoleGuard {
    fn drop(&mut self) {
        let _ = unsafe { FreeConsole() };
        // this fails during debugging sessions. apparently there is no console
        // attached to the parent process. ignore it.
        let _ = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
    }
}

#[derive(Debug)]
struct AllReAttachConsoleGuard {
    _ot: BlockOtherThreadsGuard,
    _io: AttachStdioGuard,
    _cn: SwitchToConsoleGuard,
}

impl AllReAttachConsoleGuard {
    fn new(process_id: u32) -> Self {
        Self {
            _ot: BlockOtherThreadsGuard::new(),
            _io: AttachStdioGuard::new(),
            _cn: SwitchToConsoleGuard::new(process_id),
        }
    }
}

pub(crate) struct ConsoleSpawnWrap {
    terminal_simulation: Option<TerminalSimulation>,
    pty_system: Option<Box<dyn portable_pty::PtySystem>>,
    pty_pair: Option<portable_pty::PtyPair>,
    child_console: Option<Box<dyn portable_pty::Child + Sync + Send>>,
    guard: Option<AllReAttachConsoleGuard>,
}

impl ConsoleSpawnWrap {
    pub(crate) fn new(terminal_simulation: Option<TerminalSimulation>) -> Self {
        Self {
            terminal_simulation,
            pty_system: None,
            pty_pair: None,
            child_console: None,
            guard: None,
        }
    }

    pub(crate) fn spawn<T: FnOnce(&mut ConsoleSpawnWrap)>(&mut self, spawn_function: T) {
        spawn_function(self);

        self.guard = None;
    }

    pub(crate) fn setup_stdio_hook(
        &mut self,
        command: &mut std::process::Command,
        captured_stdout: &mut ForwardedOutput,
        _captured_stderr: &mut ForwardedOutput,
        stdin_pty: &mut Option<Box<dyn Write + Send>>,
    ) {
        if let Some(simulated_terminal) = self.terminal_simulation.clone() {
            // 0. we spawn a dummy (sleep) process inside a new console
            // 1. we attach our process to the new console.
            // 2. we kill the dummy process
            // 3. we spawn the child inheriting the stdio of the console
            // 4. we re-attach our process to the console of the parent

            let mut dummy_cmd = std::process::Command::new(PathBuf::from(TESTS_BINARY));
            // using "env" with extended functionality as a tool for very basic scripting ("&&")
            #[rustfmt::skip]
            dummy_cmd.arg("env");
            // There was a instability in the CI that was caused by still active echo.
            // Try to make this more stable by delaying the setting change a bit.
            // dummy_cmd.args([TESTS_BINARY, "sleep", "0.05", "&&"]);
            if !simulated_terminal.echo {
                // Disable the echo mode that is on by default.
                // Otherwise, one would get every input line automatically back as an output.
                dummy_cmd.args([TESTS_BINARY, "stty", "--", "-echo", "&&"]);
            }
            dummy_cmd.args([TESTS_BINARY, "stty", "-a", "&&"]);
            #[rustfmt::skip]
            dummy_cmd.args([
                // this newline is needed to trigger the windows console header generation now
                TESTS_BINARY, "echo", "-n", END_OF_HEADER_KEYWORD, "&&",
                // this sleep will be killed shortly, but we need it to prevent the console to close
                // before we attached our own process
                TESTS_BINARY, "sleep", "3600",
            ]);
            let terminal_size = simulated_terminal.size.unwrap_or_default();
            let pty = Box::new(portable_pty::NativePtySystem::default());
            let pair = pty
                .openpty(PtySize {
                    rows: terminal_size.rows,
                    cols: terminal_size.cols,
                    pixel_height: 0,
                    pixel_width: 0,
                })
                .unwrap();

            let mut cmd2 = CommandBuilder::new(dummy_cmd.get_program());
            cmd2.args(dummy_cmd.get_args());

            println!("spawning ... {:?}", cmd2);
            let child = pair.slave.spawn_command(cmd2).unwrap();
            println!("spawning ... Done! pid: {}", child.process_id().unwrap());

            *stdin_pty = Some(Box::new(pair.master.take_writer().unwrap()));
            let mut reader = pair.master.try_clone_reader().unwrap();

            // read and ignore full windows console header (ANSI escape sequences).
            println!("start read header ... ");
            let header = read_till_show_cursor_ansi_escape(&mut reader);
            println!("read header: {}", header.escape_ascii());

            captured_stdout
                .spawn_reader_thread(Box::new(reader), "win_conpty_reader".into())
                .unwrap();

            self.guard = Some(AllReAttachConsoleGuard::new(child.process_id().unwrap()));

            self.configure_stdio_for_spawn_of_child(&simulated_terminal, command);

            self.pty_system = Some(pty);
            self.pty_pair = Some(pair);
            self.child_console = Some(child);
        }
    }

    fn get_console_process_id_list(exclude_self: bool) -> Vec<u32> {
        let mut dummy_buf = [0, 0, 0];
        let process_count =
            unsafe { GetConsoleProcessList(dummy_buf.as_mut_ptr(), dummy_buf.len() as u32) };
        if process_count > 0 {
            let mut buffer = vec![0; process_count as usize + 20];
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

    fn kill_and_wait_all_console_processes(&mut self) {
        if let Some(console) = &self.child_console {
            let _guards = AllReAttachConsoleGuard::new(console.process_id().unwrap());
            let process_ids = Self::get_console_process_id_list(true);
            mem::drop(_guards);

            let handles = process_ids
                .into_iter()
                .filter_map(|id| process::ProcessHandle::new_from_id(id).ok());
            handles.clone().for_each(|ph| {
                let _ = ph.terminate(88);
            });
            handles.for_each(|ph| {
                let _ = ph.wait_for_end(5000);
            });
        }
    }

    fn configure_stdio_for_spawn_of_child(
        &mut self,
        simulated_terminal: &TerminalSimulation,
        command: &mut Command,
    ) {
        if simulated_terminal.stdin {
            let _pty_conin = std::fs::OpenOptions::new()
                .read(true)
                .open("CONIN$")
                .unwrap();

            if !simulated_terminal.echo {
                //set_echo_mode(false, HANDLE(_pty_conin.as_raw_handle() as isize));
                set_echo_mode(false, std::io::stdin().as_raw_handle() as HANDLE);
            }
            //disable_virtual_terminal_sequence_processing();
            // using this handle here directly pipes the data correctly, also the .is_terminal() returns true.
            // But on CI, the echo is still activated. Unclear why.
            command.stdin(Stdio::inherit());
        }
        if simulated_terminal.stdout {
            let mut _pty_conout = std::fs::OpenOptions::new()
                .write(true)
                .open("CONOUT$")
                .unwrap();
            // using this handle here directly pipes the data correctly, but the .is_terminal() returns false
            // unclear why, but workaround of inherit() works somehow.
            command.stdout(Stdio::inherit());
        }
        if simulated_terminal.stderr {
            let mut _pty_conout = std::fs::OpenOptions::new()
                .write(true)
                .open("CONOUT$")
                .unwrap();
            // using this handle here directly pipes the data correctly, but the .is_terminal() returns false
            // unclear why, but workaround of inherit() works somehow.
            command.stderr(Stdio::inherit());
        }
    }
}

impl Drop for ConsoleSpawnWrap {
    fn drop(&mut self) {
        self.kill_and_wait_all_console_processes();
        if let Some(console) = &mut self.child_console {
            let _ = console.kill();
            console.wait().unwrap();
        }
        self.child_console = None;
    }
}

fn read_till_show_cursor_ansi_escape<T: Read>(reader: &mut T) -> Vec<u8> {
    // this keyword/sequence is the ANSI escape sequence that is printed
    // as last part of the header.
    // It make the cursor visible again, after it was hidden in the beginning.
    let keyword1 = "\x1b[?25h".as_bytes();
    let keyword2 = END_OF_HEADER_KEYWORD.as_bytes();
    if keyword1.len() != keyword2.len() {
        panic!("keywords need to have same length");
    }
    let key_len = max(keyword1.len(), keyword2.len());
    let mut last = VecDeque::with_capacity(key_len);
    let mut full_buf = Vec::new();
    let (mut found1, mut found2) = (false, false);
    let mut _s = String::new();
    loop {
        let mut buf = [0u8];
        reader.read_exact(&mut buf).unwrap();
        while last.len() >= key_len {
            last.pop_front();
        }
        last.push_back(buf[0]);
        full_buf.push(buf[0]);
        _s = format!("{}", full_buf.escape_ascii());
        println!("read: {}", _s);
        if last.len() == key_len {
            let compare_fn = |keyword: &[u8]| last.iter().zip(keyword.iter()).all(|(a, b)| a == b);
            found1 = found1 || compare_fn(keyword1);
            found2 = found2 || compare_fn(keyword2);
            if found1 && found2 {
                break;
            }
        }
    }

    full_buf
}

fn set_echo_mode(on: bool, stdin_h: HANDLE) {
    let mut mode = CONSOLE_MODE::default();
    let success = unsafe { GetConsoleMode(stdin_h, &mut mode) } != 0;
    if !success {
        eprintln!("failed to GetConsoleMode.");
        return;
    }

    if on {
        mode |= ENABLE_ECHO_INPUT | ENABLE_LINE_INPUT;
    } else {
        mode &= !ENABLE_ECHO_INPUT;
    }

    let success = unsafe { SetConsoleMode(stdin_h, mode) } != 0;
    if !success {
        eprintln!("failed to SetConsoleMode.");
    }
}

#[cfg(test)]
mod tests {
    use super::read_till_show_cursor_ansi_escape;

    #[test]
    #[should_panic(expected = "failed to fill whole buffer")]
    fn test_detection_of_keywords_fails_only_first_keyword() {
        let mut string = "jaslkdfjklasfjaklsdfjsalkfdjalskfjklsajdf\x1b[?25h".as_bytes();
        read_till_show_cursor_ansi_escape(&mut string);
    }

    #[test]
    #[should_panic(expected = "failed to fill whole buffer")]
    fn test_detection_of_keywords_fails_only_second_keyword() {
        let mut string = "jaslkdfjklasfjaklsdfjsalkfdjalskfjklsajdfENDHEA".as_bytes();
        read_till_show_cursor_ansi_escape(&mut string);
    }

    #[test]
    fn test_detection_of_keywords_succeeds_with_first_and_second_keyword() {
        let mut string = "jaslkdfjklasfjaklsdfjsalkfdjalskfjklsajdf\x1b[?25hENDHEA".as_bytes();
        read_till_show_cursor_ansi_escape(&mut string);
    }

    #[test]
    fn test_detection_of_keywords_succeeds_with_second_and_first_keyword() {
        let mut string = "jaslkdfjklasfjaklsdfjsalkfdjalskfjklsajdfENDHEA\x1b[?25h".as_bytes();
        read_till_show_cursor_ansi_escape(&mut string);
    }

    #[test]
    fn test_detection_of_keywords_succeeds_with_second_and_first_keyword_and_stuff_in_between() {
        let mut string =
            "jaslkdfjklasfjaklsdfjsalkfdjalskfjklsajdfENDHEAadsadafsdgsadgsa\x1b[?25h".as_bytes();
        read_till_show_cursor_ansi_escape(&mut string);
    }
}
