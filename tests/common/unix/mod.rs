// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//spell-checker: ignore xpixel ypixel Openpty

use std::{fs::File, io::Write};

use nix::pty::OpenptyResult;

use super::util::{ForwardedOutput, TerminalSimulation};

pub(crate) static END_OF_TRANSMISSION_SEQUENCE: &[u8] = &[b'\n', 0x04];

#[derive(Debug)]
pub(crate) struct ConsoleSpawnWrap {
    terminal_simulation: Option<TerminalSimulation>,
}

impl ConsoleSpawnWrap {
    pub(crate) fn new(terminal_simulation: Option<TerminalSimulation>) -> Self {
        Self {
            terminal_simulation,
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

            if simulated_terminal.stdin {
                let OpenptyResult {
                    slave: pi_slave,
                    master: pi_master,
                } = nix::pty::openpty(&c_terminal_size, None).unwrap();
                *stdin_pty = Some(Box::new(File::from(pi_master)));
                command.stdin(pi_slave);
            }

            if simulated_terminal.stdout {
                let OpenptyResult {
                    slave: po_slave,
                    master: po_master,
                } = nix::pty::openpty(&c_terminal_size, None).unwrap();
                captured_stdout
                    .spawn_reader_thread(
                        Box::new(File::from(po_master)),
                        "stdout_reader".to_string(),
                    )
                    .unwrap();
                command.stdout(po_slave);
            }

            if simulated_terminal.stderr {
                let OpenptyResult {
                    slave: pe_slave,
                    master: pe_master,
                } = nix::pty::openpty(&c_terminal_size, None).unwrap();
                captured_stderr
                    .spawn_reader_thread(
                        Box::new(File::from(pe_master)),
                        "stderr_reader".to_string(),
                    )
                    .unwrap();
                command.stderr(pe_slave);
            }
        }
    }
}
