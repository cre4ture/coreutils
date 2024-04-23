// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//spell-checker: ignore HPCON STARTUPINFOEXW PSEUDOCONSOLE nextest PCWSTR STARTUPINFO osstr PWSTR LPPROC
//spell-checker: ignore HRESULT STARTF USESTDHANDLES STARTUPINFOW

use std::{
    mem,
    os::{
        raw::c_void,
        windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    ptr::null,
};

use uucore::windows_sys::Win32::{
    Foundation::{BOOL, HANDLE, S_OK},
    System::{
        Console::{ClosePseudoConsole, CreatePseudoConsole, COORD, HPCON},
        Pipes::CreatePipe,
    },
};

use super::{Error, Result};

fn ok_or_win_error(result: BOOL) -> Result<()> {
    if result != 0 {
        Ok(())
    } else {
        Err(Error::StdOsIo(std::io::Error::last_os_error()))
    }
}

fn pipe() -> Result<(OwnedHandle, OwnedHandle)> {
    let mut p_in = HANDLE::default();
    let mut p_out = HANDLE::default();
    ok_or_win_error(unsafe { CreatePipe(&mut p_in, &mut p_out, null(), 0) })?;

    unsafe {
        Ok((
            OwnedHandle::from_raw_handle(p_in as *mut c_void),
            OwnedHandle::from_raw_handle(p_out as *mut c_void),
        ))
    }
}

pub(crate) struct OwnedPseudoConsoleHandle {
    handle: HPCON,
}

impl Drop for OwnedPseudoConsoleHandle {
    fn drop(&mut self) {
        unsafe { ClosePseudoConsole(self.handle) };
    }
}

impl OwnedPseudoConsoleHandle {
    pub(crate) fn get_raw_handle(&self) -> HPCON {
        self.handle
    }
}

pub(crate) fn create_pseudo_console(
    size: (i16, i16),
) -> Result<(OwnedPseudoConsoleHandle, OwnedHandle, OwnedHandle)> {
    let (pty_in, con_writer) = pipe()?;
    let (con_reader, pty_out) = pipe()?;

    let native_size = COORD {
        X: size.0,
        Y: size.1,
    };

    let mut console_handle = HPCON::default();
    let hresult = unsafe {
        CreatePseudoConsole(
            native_size,
            pty_in.as_raw_handle() as HANDLE,
            pty_out.as_raw_handle() as HANDLE,
            0,
            &mut console_handle,
        )
    };
    if hresult != S_OK {
        return Err(Error::StdOsIo(std::io::Error::from_raw_os_error(hresult)));
    }

    let console = OwnedPseudoConsoleHandle {
        handle: console_handle,
    };

    // Note: We can close the handles to the PTY-end of the pipes here
    // because the handles are dup'ed into the ConHost and will be released
    // when the ConPTY is destroyed.
    mem::drop(pty_in);
    mem::drop(pty_out);

    Ok((console, con_reader, con_writer))
}
