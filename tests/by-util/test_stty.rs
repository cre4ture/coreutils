// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.
// spell-checker:ignore parenb parmrk ixany iuclc onlcr ofdel icanon noflsh

use crate::common::util::{TerminalSimulation, TerminalSize, TestScenario};

#[test]
fn test_invalid_arg() {
    new_ucmd!().arg("--definitely-invalid").fails().code_is(1);
}

#[test]
fn runs() {
    new_ucmd!().terminal_simulation(true).succeeds();
}

#[test]
fn print_all() {
    let res = new_ucmd!()
        .arg("-a")
        .terminal_sim_stdio(TerminalSimulation {
            size: Some(TerminalSize { cols: 60, rows: 30 }),
            stdin: true,
            stdout: true,
            stderr: true,
        })
        .succeeds();

    res.stdout_contains("speed 38400 baud; rows 30; columns 60; line = 0;");

    // Random selection of flags to check for
    #[cfg(unix)]
    for flag in [
        "parenb", "parmrk", "ixany", "iuclc", "onlcr", "ofdel", "icanon", "noflsh",
    ] {
        res.stdout_contains(flag);
    }
}

#[test]
fn save_and_setting() {
    new_ucmd!()
        .args(&["--save", "nl0"])
        .fails()
        .stderr_contains("when specifying an output style, modes may not be set");
}

#[test]
fn all_and_setting() {
    new_ucmd!()
        .args(&["--all", "nl0"])
        .fails()
        .stderr_contains("when specifying an output style, modes may not be set");
}

#[test]
fn save_and_all() {
    new_ucmd!()
        .args(&["--save", "--all"])
        .fails()
        .stderr_contains(
            "the options for verbose and stty-readable output styles are mutually exclusive",
        );

    new_ucmd!()
        .args(&["--all", "--save"])
        .fails()
        .stderr_contains(
            "the options for verbose and stty-readable output styles are mutually exclusive",
        );
}
