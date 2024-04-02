// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use std::fs::File;

use regex::Regex;

use crate::common::util::{TerminalSimulation, TestScenario};

#[cfg(unix)]
const DEV_NULL: &str = "/dev/null";
#[cfg(windows)]
const DEV_NULL: &str = "nul";

#[test]
fn test_terminal_simulation() {
    let output = new_ucmd!().terminal_simulation(true).succeeds();

    #[cfg(unix)]
    output.stdout_matches(&Regex::new(r"/dev/pts/\d+\r\n").unwrap());
    #[cfg(windows)]
    output.stdout_is("windows-terminal\r\n");
}

#[test]
fn test_terminal_simulation_all_stdio() {
    let output = new_ucmd!()
        .args(&["-d", "in,out,err"])
        .terminal_simulation(true)
        .succeeds();

    #[cfg(unix)]
    output.stdout_matches(
        &Regex::new(r"in: /dev/pts/\d+\r\nout: /dev/pts/\d+\r\nerr: /dev/pts/\d+\r\n").unwrap(),
    );
    #[cfg(windows)]
    output.stdout_is("in: windows-terminal\r\nout: windows-terminal\r\nerr: windows-terminal\r\n");
}

#[test]
fn test_terminal_simulation_only_outputs() {
    let output = new_ucmd!()
        .args(&["-d", "in,out,err"])
        .terminal_sim_stdio(TerminalSimulation {
            stdin: false,
            stdout: true,
            stderr: true,
            ..Default::default()
        })
        .fails();

    output.print_outputs();

    output.code_is(1);
    #[cfg(unix)]
    output.stdout_matches(
        &Regex::new(r"in: not a tty\r\nout: /dev/pts/\d+\r\nerr: /dev/pts/\d+\r\n").unwrap(),
    );
    #[cfg(windows)]
    output.stdout_is("in: not a tty\r\nout: windows-terminal\r\nerr: windows-terminal\r\n");
}

#[test]
fn test_terminal_simulation_only_outputs_required() {
    let output = new_ucmd!()
        .args(&["-d", "out,err"])
        .terminal_sim_stdio(TerminalSimulation {
            stdin: false,
            stdout: true,
            stderr: true,
            ..Default::default()
        })
        .succeeds();

    output.print_outputs();

    #[cfg(unix)]
    output.stdout_matches(&Regex::new(r"/dev/pts/\d+\r\nerr: /dev/pts/\d+\r\n").unwrap());
    #[cfg(windows)]
    output.stdout_is("out: windows-terminal\r\nerr: windows-terminal\r\n");
}

#[test]
fn test_terminal_simulation_only_input() {
    let output = new_ucmd!()
        .args(&["-d", "in,out,err"])
        .terminal_sim_stdio(TerminalSimulation {
            stdin: true,
            stdout: false,
            stderr: false,
            ..Default::default()
        })
        .fails();

    output.code_is(1);
    #[cfg(unix)]
    output.stdout_matches(
        &Regex::new(r"in: /dev/pts/\d+\nout: not a tty\nerr: not a tty\n").unwrap(),
    );
    #[cfg(windows)]
    output.stdout_is("in: windows-terminal\nout: not a tty\nerr: not a tty\n");
}

#[test]
fn test_terminal_simulation_only_input_required() {
    let output = new_ucmd!()
        .terminal_sim_stdio(TerminalSimulation {
            stdin: true,
            stdout: false,
            stderr: false,
            ..Default::default()
        })
        .succeeds();

    output.print_outputs();

    #[cfg(unix)]
    output.stdout_matches(&Regex::new(r"/dev/pts/\d+\n").unwrap());
    #[cfg(windows)]
    output.stdout_is("windows-terminal\n");
}

#[test]
fn test_dev_null() {
    new_ucmd!()
        .set_stdin(File::open(DEV_NULL).unwrap())
        .fails()
        .code_is(1)
        .stdout_is("not a tty\n");
}

#[test]
fn test_dev_null_silent() {
    new_ucmd!()
        .args(&["-s"])
        .set_stdin(File::open(DEV_NULL).unwrap())
        .fails()
        .code_is(1)
        .stdout_is("");
}

#[test]
fn test_close_stdin() {
    let mut child = new_ucmd!().run_no_wait();
    child.close_stdin();
    child.wait().unwrap().code_is(1).stdout_is("not a tty\n");
}

#[test]
fn test_close_stdin_silent() {
    let mut child = new_ucmd!().arg("-s").run_no_wait();
    child.close_stdin();
    child.wait().unwrap().code_is(1).no_stdout();
}

#[test]
fn test_close_stdin_silent_long() {
    let mut child = new_ucmd!().arg("--silent").run_no_wait();
    child.close_stdin();
    child.wait().unwrap().code_is(1).no_stdout();
}

#[test]
fn test_close_stdin_silent_alias() {
    let mut child = new_ucmd!().arg("--quiet").run_no_wait();
    child.close_stdin();
    child.wait().unwrap().code_is(1).no_stdout();
}

#[test]
fn test_wrong_argument() {
    new_ucmd!().args(&["a"]).fails().code_is(2);
}

#[test]
fn test_help() {
    new_ucmd!().args(&["--help"]).succeeds();
}

#[test]
// FixME: freebsd panic
#[cfg(not(target_os = "freebsd"))]
fn test_stdout_fail() {
    use std::process::{Command, Stdio};
    let ts = TestScenario::new(util_name!());
    // Sleep inside a shell to ensure the process doesn't finish before we've
    // closed its stdout
    let mut proc = Command::new(&ts.bin_path)
        .arg("env") // use env as cross compatible very basic shell
        .arg(&ts.bin_path)
        .args(["sleep", "0.2", "&&"])
        .arg(&ts.bin_path)
        .arg(ts.util_name)
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    drop(proc.stdout.take());
    let status = proc.wait().unwrap();
    assert_eq!(status.code(), Some(3));
}
