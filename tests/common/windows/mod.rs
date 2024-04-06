//spell-checker: ignore conpty

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::os::windows::io::AsRawHandle;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use uucore::windows_sys::Win32::System::Console::{GetConsoleMode, SetConsoleMode, CONSOLE_MODE, ENABLE_ECHO_INPUT};
use uucore::windows_sys::Win32::System::Console::{
    AttachConsole, FreeConsole, ATTACH_PARENT_PROCESS,
};

use super::util::{TerminalSimulation, TESTS_BINARY, ForwardedOutput};

pub(crate) static END_OF_TRANSMISSION_SEQUENCE: &[u8] = &[b'\r', b'\n', 0x1A]; // send ^Z

#[derive(Debug)]
pub(crate) struct ConsoleSpawnWrap {
    terminal_simulation: Option<TerminalSimulation>,
    child_console: Option<conpty::Process>,
}

static CONSOLE_SPAWNING_MUTEX: std::sync::Mutex<u32> = std::sync::Mutex::new(0);

impl ConsoleSpawnWrap {
    pub fn new(terminal_simulation: Option<TerminalSimulation>) -> Self {
        Self { terminal_simulation,
              child_console: None,
        }
    }

    pub(crate) fn spawn<T: FnOnce(&mut ConsoleSpawnWrap) -> ()>(&mut self, spawn_function: T) {
        let _guards = if self.terminal_simulation.is_some() {
            Some((
                CONSOLE_SPAWNING_MUTEX.lock().unwrap(),
                std::io::stdout().lock(),
                std::io::stderr().lock(),
            ))
        } else {
            None
        };

        spawn_function(self);

        self.post_spawn();
    }

    pub(crate) fn setup_stdio_hook(&mut self,
        command: &mut std::process::Command,
        captured_stdout: &mut ForwardedOutput,
        _captured_stderr: &mut ForwardedOutput,
        stdin_pty: &mut Option<Box<dyn Write + Send>>) {
        if let Some(simulated_terminal) = &self.terminal_simulation {
            // 1. we attach our process to the new console.
            // 2. we spawn the child inheriting the stdio of the console
            // 3. we re-attach our process to the console of the parent
            if simulated_terminal.stdin {
                command.stdin(Stdio::inherit());
            }
            if simulated_terminal.stdout {
                command.stdout(Stdio::inherit());
            }
            if simulated_terminal.stderr {
                command.stderr(Stdio::inherit());
            }

            let mut dummy_cmd = std::process::Command::new(PathBuf::from(TESTS_BINARY));
            #[rustfmt::skip]
            dummy_cmd.args([
                "env",
                // this newline is needed to trigger the windows console header generation now
                TESTS_BINARY, "echo", "", "&&",
                // this sleep will be killed shortly, but we need it to prevent the console to close
                TESTS_BINARY, "sleep", "100",
            ]);
            let terminal_size = simulated_terminal.size.unwrap_or_default();
            let mut cmd_child = conpty::ProcessOptions {
                console_size: Some((terminal_size.cols as i16, terminal_size.rows as i16)),
            }
            .spawn(dummy_cmd)
            .unwrap();

            read_till_show_cursor(&mut cmd_child); // read and ignore full windows console header
            captured_stdout
                .spawn_reader_thread(
                    Box::new(cmd_child.output().unwrap()),
                    "win_conpty_reader".into(),
                )
                .unwrap();

            let result = unsafe { FreeConsole() };
            if result == 0 {
                panic!("detaching from current console failed!");
            }
            let mut result = 0;
            for _i in 0..500 {
                result = unsafe { AttachConsole(cmd_child.pid()) };
                if result != 0 {
                    break;
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            if result == 0 {
                panic!("attaching to new console failed!");
            }

            Self::disable_echo_mode();

            cmd_child.exit(0).unwrap(); // kill the sleep 100
            cmd_child.wait(Some(500)).unwrap();

            *stdin_pty = Some(Box::new(cmd_child.input().unwrap()));
            self.child_console = Some(cmd_child);
        }
    }

    fn disable_echo_mode() {
        let stdin_h = std::io::stdin().as_raw_handle() as isize;

        let mut mode = CONSOLE_MODE::default();
        let failed = unsafe { GetConsoleMode(stdin_h, &mut mode) == 0 };
        if failed {
            panic!("GetConsoleMode failed!");
        }

        mode &= !ENABLE_ECHO_INPUT;

        let failed = unsafe {
            SetConsoleMode(stdin_h, mode) == 0
        };
        if failed {
            panic!("SetConsoleMode failed!");
        }
    }

    fn post_spawn(&mut self) {
        if let Some(_console) = &self.child_console {
            // after spawning of the child, we reset the console to the original one
            unsafe { FreeConsole() };
            unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
        }
    }
}

impl Drop for ConsoleSpawnWrap {
    fn drop(&mut self) {
        println!("dropping ConsoleSpawnWrap ...");
        if let Some(console) = &mut self.child_console {
            println!("dropping ConsoleSpawnWrap.console ...");
            let _ = console.exit(0);
            console.wait(Some(500)).unwrap();
        }
        self.child_console = None;
        println!("dropping ConsoleSpawnWrap ... done!");
    }
}

fn read_till_show_cursor(cmd_child: &mut conpty::Process) {
    let mut reader = cmd_child.output().unwrap();
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
