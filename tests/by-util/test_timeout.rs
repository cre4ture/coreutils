use std::fs;

use uucore::display::Quotable;

// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.
// spell-checker:ignore dont
use crate::common::util::TestScenario;

#[test]
fn test_invalid_arg() {
    new_ucmd!().arg("--definitely-invalid").fails().code_is(125);
}

// FIXME: this depends on the system having true and false in PATH
//        the best solution is probably to generate some test binaries that we can call for any
//        utility that requires executing another program (kill, for instance)
#[test]
fn test_subcommand_return_code() {
    new_ucmd!().arg("1").arg("true").succeeds();

    new_ucmd!().arg("1").arg("false").run().code_is(1);
}

#[test]
fn test_invalid_time_interval() {
    new_ucmd!()
        .args(&["xyz", "sleep", "0"])
        .fails()
        .code_is(125)
        .usage_error("invalid time interval 'xyz'");
}

#[test]
fn test_invalid_kill_after() {
    new_ucmd!()
        .args(&["-k", "xyz", "1", "sleep", "0"])
        .fails()
        .code_is(125)
        .usage_error("invalid time interval 'xyz'");
}

#[test]
fn test_command_with_args() {
    new_ucmd!()
        .args(&["1700", "echo", "-n", "abcd"])
        .succeeds()
        .stdout_only("abcd");
}

#[test]
fn test_verbose() {
    for verbose_flag in ["-v", "--verbose"] {
        new_ucmd!()
            .args(&[verbose_flag, ".1", "sleep", "10"])
            .fails()
            .stderr_only("timeout: sending signal TERM to command 'sleep'\n");
        new_ucmd!()
            .args(&[verbose_flag, "-s0", "-k.1", ".1", "sleep", "10"])
            .fails()
            .stderr_only("timeout: sending signal EXIT to command 'sleep'\ntimeout: sending signal KILL to command 'sleep'\n");
    }
}

#[test]
fn test_zero_timeout() {
    new_ucmd!()
        .args(&["-v", "0", "sleep", ".1"])
        .succeeds()
        .no_stderr()
        .no_stdout();
    new_ucmd!()
        .args(&["-v", "0", "-s0", "-k0", "sleep", ".1"])
        .succeeds()
        .no_stderr()
        .no_stdout();
}

#[test]
fn test_command_empty_args() {
    new_ucmd!()
        .args(&["", ""])
        .fails()
        .stderr_contains("timeout: empty string");
}

#[test]
fn test_preserve_status() {
    new_ucmd!()
        .args(&["--preserve-status", ".1", "sleep", "10"])
        .fails()
        // 128 + SIGTERM = 128 + 15
        .code_is(128 + 15)
        .no_stderr()
        .no_stdout();
}

#[test]
fn test_preserve_status_even_when_send_signal() {
    // When sending CONT signal, process doesn't get killed or stopped.
    // So, expected result is success and code 0.
    new_ucmd!()
        .args(&["-s", "CONT", "--preserve-status", ".1", "sleep", "5"])
        .succeeds()
        .code_is(0)
        .no_stderr()
        .no_stdout();
}

#[test]
fn test_dont_overflow() {
    new_ucmd!()
        .args(&["9223372036854775808d", "sleep", "0"])
        .succeeds()
        .code_is(0)
        .no_stderr()
        .no_stdout();
    new_ucmd!()
        .args(&["-k", "9223372036854775808d", "10", "sleep", "0"])
        .succeeds()
        .code_is(0)
        .no_stderr()
        .no_stdout();
}

#[test]
fn test_negative_interval() {
    new_ucmd!()
        .args(&["--", "-1", "sleep", "0"])
        .fails()
        .usage_error("invalid time interval '-1'");
}

#[test]
fn test_invalid_signal() {
    new_ucmd!()
        .args(&["-s", "invalid", "1", "sleep", "0"])
        .fails()
        .usage_error("'invalid': invalid signal");
}

#[test]
fn test_invalid_multi_byte_characters() {
    new_ucmd!()
        .args(&["10€", "sleep", "0"])
        .fails()
        .usage_error("invalid time interval '10€'");
}

/// Test that the long form of the `--kill-after` argument is recognized.
#[test]
fn test_kill_after_long() {
    new_ucmd!()
        .args(&["--kill-after=1", "1", "sleep", "0"])
        .succeeds()
        .no_stdout()
        .no_stderr();
}

#[test]
fn test_kill_subprocess() {
    let ts = TestScenario::new(util_name!());
    let command = ts.bin_path.as_path();

    let subscript = "trap 'echo start_trap; echo end_trap' TERM; echo 'trap installed, start sleep'; sleep 30; echo 'sleep done'";
    let script = format!(
        "echo -n \"start time:\"; date +\"%T.%3N\"
        {} timeout 10 sh -xc {}
        exit_code=$?
        echo -n \"after timeout time:\" ; date +\"%T.%3N\"
        sleep 5
        echo -n \"after outer sleep 5 time:\" ; date +\"%T.%3N\"
        exit $exit_code
        ",
        command.maybe_quote(),
        subscript.maybe_quote(),
    );

    let result = ts.cmd("sh").args(&["-x"]).pipe_in(script).run();

    eprintln!("stdout:\n{}", result.stdout_str());

    eprintln!("stderr:\n{}", result.stderr_str());

    result
        .failure()
        .code_is(124)
        .stdout_contains("start_trap")
        .stderr_contains("Terminated");

    let reference_path = ts.fixtures.plus_as_string("reference_kill_subprocess.txt");
    let reference_template = fs::read_to_string(reference_path).unwrap();
    let reference = reference_template.replace(
        "######BIN_PATH######",
        command.maybe_quote().to_string().as_str(),
    );
    result.stderr_is(reference);
}
