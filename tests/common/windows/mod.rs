// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//spell-checker: ignore conpty conin conout

mod process;

use std::borrow::Borrow;
use std::cmp::max;
use std::collections::VecDeque;
use std::io::{Read, StderrLock, StdinLock, StdoutLock, Write};
use std::mem;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::MutexGuard;
use std::time::Duration;
use uucore::io::OwnedFileDescriptorOrHandle;
use uucore::windows_sys::Win32::Storage::FileSystem::HandleLogFull;
use windows::Win32::Foundation::{CloseHandle, HANDLE as WinHANDLE};

use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Console::{
    AttachConsole, FreeConsole, GetConsoleMode, GetConsoleProcessList, GetStdHandle, SetConsoleMode, SetStdHandle, ATTACH_PARENT_PROCESS, CONSOLE_MODE, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_VIRTUAL_TERMINAL_INPUT, ENABLE_VIRTUAL_TERMINAL_PROCESSING, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE
};
use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, WaitForSingleObject};

use super::util::{ForwardedOutput, TerminalSimulation, TESTS_BINARY};

pub(crate) static END_OF_TRANSMISSION_SEQUENCE: &[u8] = &[b'\r', b'\n', 0x1A, b'\r', b'\n']; // send ^Z
//pub(crate) static END_OF_TRANSMISSION_SEQUENCE: &[u8] = &[b'\r', b'\n', 0x5A, b'\r', b'\n']; // send ^Z
//pub(crate) static END_OF_TRANSMISSION_SEQUENCE: &[u8] = &[b'\r', b'\n', 0x04, b'\r', b'\n'];
//pub(crate) static END_OF_TRANSMISSION_SEQUENCE: &[u8] = &[b'\r', b'\n', 0x17, b'\r', b'\n'];

static CONSOLE_SPAWNING_MUTEX: std::sync::Mutex<u32> = std::sync::Mutex::new(0);
static END_OF_HEADER_KEYWORD: &str = "ENDHEA";

trait None {}

#[derive(Debug)]
struct BlockOtherThreadsGuard {
    list: (MutexGuard<'static, u32>, StdinLock<'static>, StdoutLock<'static>, StderrLock<'static>),
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
            list: (
                CONSOLE_SPAWNING_MUTEX.lock().unwrap(),
                std::io::stdin().lock(),
                std::io::stdout().lock(),
                std::io::stderr().lock(),
            )
        }
    }
}

#[derive(Debug)]
struct AttachStdioGuard {
    original_stdin: Option<WinHANDLE>,
    original_stdout: Option<WinHANDLE>,
    original_stderr: Option<WinHANDLE>,
}

impl AttachStdioGuard {
    fn new() -> Self {
        let original_stdin = unsafe { GetStdHandle(STD_INPUT_HANDLE) }.ok();
        let original_stdout = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) }.ok();
        let original_stderr = unsafe { GetStdHandle(STD_ERROR_HANDLE) }.ok();
        // setting the handles to null prevents that the spawned child inherits from something
        // other than the pseudo console.
        unsafe { SetStdHandle(STD_INPUT_HANDLE, HANDLE(0)) }.unwrap();
        unsafe { SetStdHandle(STD_OUTPUT_HANDLE, HANDLE(0)) }.unwrap();
        unsafe { SetStdHandle(STD_ERROR_HANDLE, HANDLE(0)) }.unwrap();
        Self{
            original_stdin,
            original_stdout,
            original_stderr
        }
    }
}

impl Drop for AttachStdioGuard {
    fn drop(&mut self) {
        self.original_stdin.inspect(|h| unsafe { SetStdHandle(STD_INPUT_HANDLE, *h) }.unwrap());
        self.original_stdout.inspect(|h| unsafe { SetStdHandle(STD_OUTPUT_HANDLE, *h) }.unwrap());
        self.original_stderr.inspect(|h| unsafe { SetStdHandle(STD_ERROR_HANDLE, *h) }.unwrap());
    }
}

#[derive(Debug)]
struct SwitchToConsoleGuard {

}

impl SwitchToConsoleGuard {
    fn new(process_id: u32) -> Self {
        unsafe { FreeConsole() }.unwrap();

        let mut result = Err(windows::core::Error::empty());
        for _i in 0..1 {
            result = unsafe { AttachConsole(process_id) };
            if result.is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        if let Err(e) = result {
            panic!("attaching to new console failed! {:?}", e);
        }
        Self{}
    }
}

impl Drop for SwitchToConsoleGuard {
    fn drop(&mut self) {
        unsafe { FreeConsole() }.unwrap();
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

#[derive(Debug)]
pub(crate) struct ConsoleSpawnWrap {
    terminal_simulation: Option<TerminalSimulation>,
    child_console: Option<conpty::Process>,
    guard: Option<AllReAttachConsoleGuard>,
}

impl ConsoleSpawnWrap {
    pub(crate) fn new(terminal_simulation: Option<TerminalSimulation>) -> Self {
        Self {
            terminal_simulation,
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
            #[rustfmt::skip]
            dummy_cmd.args([
                // using "env" with extended functionality as a tool for very basic scripting ("&&")
                "env",
                // There was a instability in the CI that was caused by still active echo.
                // Try to make this more stable by delaying the setting change a bit.
                //TESTS_BINARY, "sleep", "0.05", "&&",
                // Disable the echo mode that is on by default.
                // Otherwise, one would get every input line automatically back as an output.
                TESTS_BINARY, "stty", "--", "-echo", "&&",
                TESTS_BINARY, "stty", "-a", "&&",
                //TESTS_BINARY, "echo", "-n", "DUMMY1", "&&",
                //TESTS_BINARY, "cat", "-", "&&",
                //TESTS_BINARY, "sleep", "1", "&&",
                //TESTS_BINARY, "echo", "DUMMY2", "&&",
                //TESTS_BINARY, "sleep", "1", "&&",
                //TESTS_BINARY, "echo", "DUMMY3", "&&",
                //TESTS_BINARY, "sleep", "1", "&&",
                //TESTS_BINARY, "echo", "DUMMY4", "&&",
                // this newline is needed to trigger the windows console header generation now
                TESTS_BINARY, "echo", "-n", END_OF_HEADER_KEYWORD, "&&",
                // this sleep will be killed shortly, but we need it to prevent the console to close
                // before we attached our own process
                TESTS_BINARY, "sleep", "100",
            ]);
            let terminal_size = simulated_terminal.size.unwrap_or_default();
            let options = conpty::ProcessOptions {
                console_size: Some((terminal_size.cols as i16, terminal_size.rows as i16)),
            };

            let mut cmd_child = options.spawn(dummy_cmd).unwrap();

            *stdin_pty = Some(Box::new(cmd_child.input().unwrap()));
            let mut reader = cmd_child.output().unwrap();

            //cmd_child.input().unwrap().write_all(END_OF_TRANSMISSION_SEQUENCE);

            // read and ignore full windows console header (ANSI escape sequences).
            let header = read_till_show_cursor_ansi_escape(&mut reader);
            println!("read header: {}", header.escape_ascii());

            captured_stdout
                .spawn_reader_thread(
                    Box::new(reader),
                    "win_conpty_reader".into(),
                )
                .unwrap();


            self.guard = Some(AllReAttachConsoleGuard::new(cmd_child.pid()));

            //std::thread::sleep(Duration::from_millis(5000));

            //std::thread::sleep(Duration::from_millis(5000));

            self.configure_stdio_for_spawn_of_child(&simulated_terminal, command);

            //cmd_child.exit(0).unwrap(); // kill the "sleep 100"
            //cmd_child.wait(Some(500)).unwrap();

            self.child_console = Some(cmd_child);
        }
    }

    fn get_console_process_id_list(exclude_self: bool) -> Vec<u32> {
        let process_count = unsafe { GetConsoleProcessList(&mut [0,0,0]) };
        if process_count > 0 {
            let mut buffer = Vec::new();
            buffer.resize(process_count as usize + 20, 0);
            let process_count = unsafe { GetConsoleProcessList(&mut buffer) } as usize;
            if process_count <= buffer.len() {
                buffer.resize(process_count, 0);
                if exclude_self {
                    let own_id = std::process::id();
                    buffer = buffer.into_iter().filter(|id| (*id) != own_id).collect();
                }
                return buffer;
            }
        }

        panic!("failed to get console process id list!");
    }

    fn kill_and_wait_all_console_processes(&mut self)
    {
        if let Some(console) = &self.child_console {
            let _guards = AllReAttachConsoleGuard::new(console.pid());
            let process_ids = Self::get_console_process_id_list(true);
            mem::drop(_guards);

            let handles = process_ids.into_iter().filter_map(|id|{
                process::ProcessHandle::new_from_id(id).ok()
            });
            handles.clone().for_each(|ph| { let _ = ph.terminate(88); } );
            handles.for_each(|ph| { let _ = ph.wait_for_end(5000); } );
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
            //set_echo_mode(false, HANDLE(_pty_conin.as_raw_handle() as isize));
            set_echo_mode(false, HANDLE(std::io::stdin().as_raw_handle() as isize));
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
            let _ = console.exit(0);
            console.wait(Some(500)).unwrap();
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
    let mut s = String::new();
    loop {
        let mut buf = [0u8];
        reader.read_exact(&mut buf).unwrap();
        while last.len() >= key_len {
            last.pop_front();
        }
        last.push_back(buf[0]);
        full_buf.push(buf[0]);
        s = format!("{}", full_buf.escape_ascii());
        if last.len() == key_len {
            let compare_fn = |keyword: &[u8]| { last.iter().zip(keyword.iter()).all(|(a, b)| a == b) };
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
    unsafe { GetConsoleMode(stdin_h, &mut mode) }.unwrap();

    if on {
        mode |= ENABLE_ECHO_INPUT | ENABLE_LINE_INPUT;
    } else {
        mode &= !ENABLE_ECHO_INPUT;
    }

    // mode |= ENABLE_VIRTUAL_TERMINAL_INPUT;

    unsafe { SetConsoleMode(stdin_h, mode) }.unwrap();
}

fn disable_virtual_terminal_sequence_processing() -> windows::core::Result<()> {
    let stdout_h = HANDLE(std::io::stdout().as_raw_handle() as isize);
    unsafe {
        let mut mode = CONSOLE_MODE::default();
        GetConsoleMode(stdout_h, &mut mode)?;
        mode &= !ENABLE_VIRTUAL_TERMINAL_PROCESSING; // DISABLE_NEWLINE_AUTO_RETURN
        SetConsoleMode(stdout_h, mode)?;
    }

    Ok(())
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
        let mut string = "jaslkdfjklasfjaklsdfjsalkfdjalskfjklsajdfENDHEAadsadafsdgsadgsa\x1b[?25h".as_bytes();
        read_till_show_cursor_ansi_escape(&mut string);
    }

}