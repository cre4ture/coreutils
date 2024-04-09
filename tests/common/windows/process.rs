use std::os::{raw::c_void, windows::io::{AsRawHandle, FromRawHandle, OwnedHandle}};

use windows::Win32::{Foundation::{E_ABORT, E_FAIL, FALSE, HANDLE, WAIT_ABANDONED_0, WAIT_OBJECT_0, WAIT_TIMEOUT}, System::Threading::{OpenProcess, TerminateProcess, WaitForSingleObject, PROCESS_TERMINATE}};

use windows::core::Result;

#[derive(Debug)]
pub(crate) struct ProcessHandle {
    handle: OwnedHandle,
}

impl ProcessHandle {
    pub(crate) fn new_from_id(process_id: u32) -> Result<Self> {
        let handle = unsafe { OwnedHandle::from_raw_handle(OpenProcess(PROCESS_TERMINATE, FALSE, process_id)?.0 as *mut c_void) };
        Ok(Self {
            handle,
        })
    }

    pub(crate) fn terminate(&self, exit_code: u32) -> Result<()>
    {
        unsafe{ TerminateProcess(self.win_handle(), exit_code)? };
        Ok(())
    }

    pub(crate) fn wait_for_end(&self, timeout_ms: u32) -> Result<()>
    {
        match unsafe{ WaitForSingleObject(self.win_handle(), timeout_ms) } {
            WAIT_OBJECT_0 | WAIT_ABANDONED_0 => Ok(()),
            WAIT_TIMEOUT => Err(windows::core::Error::new(E_ABORT, "Timeout on wait for process")),
            event => Err(windows::core::Error::new(E_FAIL, format!("unexpected response when waiting: {:?}", event))),
        }
    }

    fn win_handle(&self) -> HANDLE {
        HANDLE(self.handle.as_raw_handle() as isize)
    }

}