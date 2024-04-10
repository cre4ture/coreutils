// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//spell-checker: ignore xpixel ypixel Openpty winsize

use std::{
    fs::File,
    io::{self, Write},
    thread::JoinHandle,
};

use nix::pty::OpenptyResult;

use super::util::{ForwardedOutput, TerminalSimulation, TESTS_BINARY};

pub(crate) static END_OF_TRANSMISSION_SEQUENCE: &[u8] = &[b'\n', 0x04];

#[derive(Debug)]
pub(crate) struct ConsoleSpawnWrap {
    terminal_simulation: Option<TerminalSimulation>,
    dummy_out_reader: Option<JoinHandle<()>>,
}

impl ConsoleSpawnWrap {
    pub(crate) fn new(terminal_simulation: Option<TerminalSimulation>) -> Self {
        Self {
            terminal_simulation,
            dummy_out_reader: None,
        }
    }

    pub(crate) fn spawn<T: FnOnce(&mut ConsoleSpawnWrap)>(&mut self, spawn_function: T) {
        spawn_function(self);
    }

    pub(crate) fn setup_stdio_hook(
        &mut self,
        command: &mut std::process::Command,
        captured_stdout: &mut ForwardedOutput,
        captured_stderr: &mut ForwardedOutput,
        stdin_pty: &mut Option<Box<dyn Write + Send>>,
    ) {
        if let Some(simulated_terminal) = &self.terminal_simulation {
            let terminal_size = simulated_terminal.size.unwrap_or_default();
            let c_terminal_size = libc::winsize {
                ws_row: terminal_size.rows,
                ws_col: terminal_size.cols,
                ws_xpixel: terminal_size.pixels_x,
                ws_ypixel: terminal_size.pixels_y,
            };

            let OpenptyResult {
                slave: pi_slave,
                master: pi_master,
            } = nix::pty::openpty(&c_terminal_size, None).unwrap();

            if !simulated_terminal.echo {
                std::process::Command::new(TESTS_BINARY)
                    .args(["stty", "--", "-echo"])
                    .stdin(pi_slave.try_clone().unwrap())
                    .stdout(pi_slave.try_clone().unwrap())
                    .stderr(pi_slave.try_clone().unwrap())
                    .spawn()
                    .unwrap()
                    .wait()
                    .unwrap();
            }

            if simulated_terminal.stdin {
                *stdin_pty = Some(Box::new(File::from(pi_master.try_clone().unwrap())));
                command.stdin(pi_slave.try_clone().unwrap());
            }

            if simulated_terminal.stdout {
                command.stdout(pi_slave.try_clone().unwrap());
            }

            if simulated_terminal.stderr {
                command.stderr(pi_slave);
            }

            let forwarded = if simulated_terminal.stdout {
                Some(captured_stdout)
            } else if simulated_terminal.stderr {
                Some(captured_stderr)
            } else {
                None
            };

            if let Some(forwarded_io) = forwarded {
                forwarded_io
                    .spawn_reader_thread(
                        Box::new(File::from(pi_master)),
                        "console_out_reader".to_string(),
                    )
                    .unwrap();
            } else {
                self.dummy_out_reader = std::thread::Builder::new()
                    .name("dummy_console_out_reader".to_string())
                    .spawn(move || {
                        ForwardedOutput::read_from_pty(Box::new(File::from(pi_master)), io::sink());
                    })
                    .ok();
            }
        }
    }
}
