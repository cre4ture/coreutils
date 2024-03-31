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

use crate::Options;

pub(crate) fn open_file_of_options(f: &str) -> io::Result<OwnedFileDescriptorOrHandle> {
    OwnedFileDescriptorOrHandle::from(std::fs::OpenOptions::new().read(true).open(f)?)
}

pub(crate) fn stty(opts: &Options) -> UResult<()> {
    if opts.settings.is_some() {
        return Err(USimpleError::new(
            2,
            "changing settings on windows not (yet) supported!",
        ));
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
    }

    Ok(())
}
