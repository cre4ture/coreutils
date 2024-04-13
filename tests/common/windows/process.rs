use std::os::{
    raw::c_void,
    windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
};

use uucore::windows_sys::Win32::{
    Foundation::{
        FALSE, HANDLE, INVALID_HANDLE_VALUE, WAIT_ABANDONED_0, WAIT_OBJECT_0, WAIT_TIMEOUT,
    },
    System::Threading::{OpenProcess, TerminateProcess, WaitForSingleObject, PROCESS_TERMINATE},
};

pub(crate) enum Error {
    Timeout,
    ProcessOpenFailed,
    TerminateProcessFailed,
    WaitFailedUnknown(String),
}

#[derive(Debug)]
pub(crate) struct ProcessHandle {
    handle: OwnedHandle,
}

impl ProcessHandle {
    pub(crate) fn new_from_id(process_id: u32) -> Result<Self, Error> {
        let handle = unsafe {
            let result = OpenProcess(PROCESS_TERMINATE, FALSE, process_id);
            if result == INVALID_HANDLE_VALUE {
                return Err(Error::ProcessOpenFailed);
            }
            OwnedHandle::from_raw_handle(result as *mut c_void)
        };
        Ok(Self { handle })
    }

    pub(crate) fn terminate(&self, exit_code: u32) -> Result<(), Error> {
        let success = unsafe { TerminateProcess(self.win_handle(), exit_code) } != 0;
        if success {
            Ok(())
        } else {
            Err(Error::TerminateProcessFailed)
        }
    }

    pub(crate) fn wait_for_end(&self, timeout_ms: u32) -> Result<(), Error> {
        match unsafe { WaitForSingleObject(self.win_handle(), timeout_ms) } {
            WAIT_OBJECT_0 | WAIT_ABANDONED_0 => Ok(()),
            WAIT_TIMEOUT => Err(Error::Timeout),
            event => Err(Error::WaitFailedUnknown(format!(
                "unexpected response when waiting: {:?}",
                event
            ))),
        }
    }

    fn win_handle(&self) -> HANDLE {
        self.handle.as_raw_handle() as HANDLE
    }
}
