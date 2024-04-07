// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

use std::{
    io::{self, IsTerminal},
    os::windows::io::AsRawHandle,
};

use uucore::{
    error::{UResult, USimpleError},
    io::OwnedFileDescriptorOrHandle,
};
use windows::Win32::{
    Foundation::HANDLE,
    System::Console::{
        GetConsoleMode, SetConsoleMode, CONSOLE_MODE, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT,
    },
};

use crate::Options;

pub(crate) fn open_file_of_options(f: &str) -> io::Result<OwnedFileDescriptorOrHandle> {
    OwnedFileDescriptorOrHandle::from(std::fs::OpenOptions::new().read(true).open(f)?)
}

fn set_echo_mode(on: bool) {
    // setting the echo mode works only on stdin.
    let stdin_h = HANDLE(std::io::stdin().as_raw_handle() as isize);

    let mut mode = CONSOLE_MODE::default();
    unsafe { GetConsoleMode(stdin_h, &mut mode) }.unwrap();

    if on {
        mode |= ENABLE_ECHO_INPUT | ENABLE_LINE_INPUT;
    } else {
        mode &= !ENABLE_ECHO_INPUT;
    }

    unsafe { SetConsoleMode(stdin_h, mode) }.unwrap();
}

fn get_echo_mode() -> bool {
    // getting the echo mode works only on stdin.
    let stdin_h = HANDLE(std::io::stdin().as_raw_handle() as isize);

    let mut mode = CONSOLE_MODE::default();
    unsafe { GetConsoleMode(stdin_h, &mut mode) }.unwrap();

    (mode & ENABLE_ECHO_INPUT).0 != 0
}

fn apply_setting(setting: &str) -> UResult<()> {
    match setting {
        "-echo" => set_echo_mode(false),
        "echo" => set_echo_mode(true),
        other => {
            return Err(USimpleError::new(
                2,
                format!("changing the setting '{other}' on windows is not (yet) supported!"),
            ))
        }
    };

    Ok(())
}

pub(crate) fn stty(opts: &Options) -> UResult<()> {
    if let Some(settings) = &opts.settings {
        for setting in settings {
            apply_setting(setting)?;
        }
    }

    if !opts.file.as_raw().is_terminal() {
        return Err(USimpleError::new(1, "is not a tty"));
    }

    let baud = 38400; // just a fake default value
    let (terminal_width, terminal_height) =
        terminal_size::terminal_size_using_handle(opts.file.as_raw().as_raw_handle())
            .ok_or(USimpleError::new(2, "failed to determine terminal size"))?;
    let line_discipline = 0; // don't mix up with cursor position!

    if opts.all {
        print!("speed {baud} baud");
        print!("; rows {}; columns {}", terminal_height.0, terminal_width.0);
        println!("; line = {line_discipline};");
        println!("{}echo", if get_echo_mode() { "" } else { "-" });
    }

    Ok(())
}
