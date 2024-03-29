use std::{io, os::windows::io::AsRawHandle};

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

    let baud = 38400; // just a fake default value
    let (terminal_width, terminal_height) = terminal_size::terminal_size_using_handle(opts.file.as_raw().as_raw_handle()).ok_or(
        USimpleError::new(2, "failed to determine terminal size")
    )?;
    let line_discipline = 0; // don't mix up with cursor position!

    if opts.all {
        print!("speed {baud} baud");
        print!("; rows {}; cols {}", terminal_height.0, terminal_width.0);
        print!("; line = {line_discipline}");
    }

    Ok(())
}