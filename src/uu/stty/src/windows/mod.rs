use std::{io::{self, IsTerminal}, os::windows::io::{AsHandle, AsRawHandle}};

use uucore::{error::{UResult, USimpleError}, io::OwnedFileDescriptorOrHandle, windows_sys::Win32::System::Console::{GetConsoleScreenBufferInfo, CONSOLE_SCREEN_BUFFER_INFO, COORD, SMALL_RECT}};

use crate::Options;

pub(crate) fn open_file_of_options(f: &str) -> io::Result<OwnedFileDescriptorOrHandle> {
    Ok(OwnedFileDescriptorOrHandle::from(
        std::fs::OpenOptions::new()
            .read(true)
            // .custom_flags(O_NONBLOCK)
            .open(f)?)?,
    )
}

pub(crate) fn stty(opts: &Options) -> UResult<()> {

    if opts.settings.is_some() {
        return Err(USimpleError::new(2, "changing settings on windows not (yet) supported!"));
    }

    if !opts.file.as_raw().is_terminal() {
        return Err(USimpleError::new(1, "is not a tty"));
    }

    //let zc = COORD { X: 0, Y: 0 };
    //let mut csbi = CONSOLE_SCREEN_BUFFER_INFO {
    //    dwSize: zc,
    //    dwCursorPosition: zc,
    //    wAttributes: 0,
    //    srWindow: SMALL_RECT {
    //        Left: 0,
    //        Top: 0,
    //        Right: 0,
    //        Bottom: 0,
    //    },
    //    dwMaximumWindowSize: zc,
    //};
    //if unsafe { GetConsoleScreenBufferInfo(opts.file.as_raw().as_raw_handle() as isize, &mut csbi) } == 0 {
    //    return Err(USimpleError::new(1, "GetConsoleScreenBufferInfo failed!"));
    //}

    let baud = 38400; // just a fake default value
    let (terminal_width, terminal_height) =
        terminal_size::terminal_size_using_handle(opts.file.as_raw().as_raw_handle())
            .ok_or(
                USimpleError::new(2, "failed to determine terminal size")
            )?;
    let line_discipline = 0; // don't mix up with cursor position!

    if opts.all {
        print!("speed {baud} baud");
        print!("; rows {}; columns {}", terminal_height.0, terminal_width.0);
        println!("; line = {line_discipline};");
    }

    Ok(())
}