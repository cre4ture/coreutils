// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//spell-checker: ignore conpty conin conout

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;
use windows::Win32::Foundation::HANDLE as WinHANDLE;

use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Console::{
    AttachConsole, FreeConsole, GetStdHandle, SetStdHandle, ATTACH_PARENT_PROCESS,
    STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
};

use super::util::{ForwardedOutput, TerminalSimulation, TESTS_BINARY};

pub(crate) static END_OF_TRANSMISSION_SEQUENCE: &[u8] = &[b'\r', b'\n', 0x1A]; // send ^Z

#[derive(Debug)]
pub(crate) struct ConsoleSpawnWrap {
    terminal_simulation: Option<TerminalSimulation>,
    child_console: Option<conpty::Process>,
    original_stdin: Option<WinHANDLE>,
    original_stdout: Option<WinHANDLE>,
    original_stderr: Option<WinHANDLE>,
}

static CONSOLE_SPAWNING_MUTEX: std::sync::Mutex<u32> = std::sync::Mutex::new(0);

impl ConsoleSpawnWrap {
    pub(crate) fn new(terminal_simulation: Option<TerminalSimulation>) -> Self {
        Self {
            terminal_simulation,
            child_console: None,
            original_stdin: None,
            original_stdout: None,
            original_stderr: None,
        }
    }

    pub(crate) fn spawn<T: FnOnce(&mut ConsoleSpawnWrap)>(&mut self, spawn_function: T) {
        // To be able to properly spawn the child process inside of the new console,
        // We need to attach our own process temporarily to the new console.
        // This is due to the lack of the std::process interface accepting windows startup information parameters.
        // In this critical phase where our own process is attached to the new console,
        // we can't allow other threads to spawn own consoles or read/write something from/to stdio.
        // This can happen e.g. during execution of multiple test cases in parallel.
        // Therefor this list of guards here:
        let _guards = if self.terminal_simulation.is_some() {
            Some((
                CONSOLE_SPAWNING_MUTEX.lock().unwrap(),
                std::io::stdin().lock(),
                std::io::stdout().lock(),
                std::io::stderr().lock(),
            ))
        } else {
            None
        };

        spawn_function(self);

        self.reattach_to_original_console_and_stdio();
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
                // Disable the echo mode that is on by default on windows.
                // Otherwise, one would get every input line automatically back as an output.
                TESTS_BINARY, "stty", "--", "-echo", "&&",
                // this newline is needed to trigger the windows console header generation now
                TESTS_BINARY, "echo", "", "&&",
                // this sleep will be killed shortly, but we need it to prevent the console to close
                // before we attached our own process
                TESTS_BINARY, "sleep", "100",
            ]);
            let terminal_size = simulated_terminal.size.unwrap_or_default();
            let mut cmd_child = conpty::ProcessOptions {
                console_size: Some((terminal_size.cols as i16, terminal_size.rows as i16)),
            }
            .spawn(dummy_cmd)
            .unwrap();

            // read and ignore full windows console header (ANSI escape sequences).
            read_till_show_cursor_ansi_escape(&mut cmd_child);

            captured_stdout
                .spawn_reader_thread(
                    Box::new(cmd_child.output().unwrap()),
                    "win_conpty_reader".into(),
                )
                .unwrap();

            self.store_and_reset_original_stdio_handles();

            self.switch_to_pseudo_console(&cmd_child);

            self.configure_stdio_for_spawn_of_child(&simulated_terminal, command);

            cmd_child.exit(0).unwrap(); // kill the "sleep 100"
            cmd_child.wait(Some(500)).unwrap();

            *stdin_pty = Some(Box::new(cmd_child.input().unwrap()));
            self.child_console = Some(cmd_child);
        }
    }

    fn reattach_to_original_console_and_stdio(&mut self) {
        if let Some(_console) = &self.child_console {
            unsafe { FreeConsole() }.unwrap();
            // this fails during debugging sessions. apparently there is no console
            // attached to the parent process. ignore it.
            let _ = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };

            if let Some(h) = self.original_stdin {
                unsafe { SetStdHandle(STD_INPUT_HANDLE, h) }.unwrap();
            }
            if let Some(h) = self.original_stdout {
                unsafe { SetStdHandle(STD_OUTPUT_HANDLE, h) }.unwrap();
            }
            if let Some(h) = self.original_stderr {
                unsafe { SetStdHandle(STD_ERROR_HANDLE, h) }.unwrap();
            }
        }
    }

    fn store_and_reset_original_stdio_handles(&mut self) {
        self.original_stdin = unsafe { GetStdHandle(STD_INPUT_HANDLE) }.ok();
        self.original_stdout = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) }.ok();
        self.original_stderr = unsafe { GetStdHandle(STD_ERROR_HANDLE) }.ok();
        // setting the handles to null prevents that the spawned child inherits from something
        // other than the pseudo console.
        unsafe { SetStdHandle(STD_INPUT_HANDLE, HANDLE(0)) }.unwrap();
        unsafe { SetStdHandle(STD_OUTPUT_HANDLE, HANDLE(0)) }.unwrap();
        unsafe { SetStdHandle(STD_ERROR_HANDLE, HANDLE(0)) }.unwrap();
    }

    fn switch_to_pseudo_console(&mut self, cmd_child: &conpty::Process) {
        unsafe { FreeConsole() }.unwrap();

        let mut result = Err(windows::core::Error::empty());
        for _i in 0..1 {
            result = unsafe { AttachConsole(cmd_child.pid()) };
            if result.is_ok() {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        if let Err(e) = result {
            panic!("attaching to new console failed! {:?}", e);
        }
    }

    fn configure_stdio_for_spawn_of_child(
        &mut self,
        simulated_terminal: &TerminalSimulation,
        command: &mut Command,
    ) {
        if simulated_terminal.stdin {
            let pty_conin = std::fs::OpenOptions::new()
                .read(true)
                .open("CONIN$")
                .unwrap();
            command.stdin(pty_conin);
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
        if let Some(console) = &mut self.child_console {
            let _ = console.exit(0);
            console.wait(Some(500)).unwrap();
        }
        self.child_console = None;
    }
}

fn read_till_show_cursor_ansi_escape(cmd_child: &mut conpty::Process) {
    let mut reader = cmd_child.output().unwrap();
    // this keyword/sequence is the ANSI escape sequence that is printed
    // as last part of the header.
    // It make the cursor visible again, after it was hidden in the beginning.
    let keyword = "\x1b[?25h".as_bytes();
    let key_len = keyword.len();
    let mut last = VecDeque::with_capacity(key_len);
    loop {
        let mut buf = [0u8];
        reader.read_exact(&mut buf).unwrap();
        while last.len() >= key_len {
            last.pop_front();
        }
        last.push_back(buf[0]);
        if (last.len() == key_len) && last.iter().zip(keyword.iter()).all(|(a, b)| a == b) {
            break;
        }
    }
}
