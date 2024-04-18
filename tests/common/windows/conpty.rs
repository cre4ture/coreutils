// This file is part of the uutils coreutils package.
//
// For the full copyright and license information, please view the LICENSE
// file that was distributed with this source code.

//spell-checker: ignore HPCON STARTUPINFOEXW PSEUDOCONSOLE nextest PCWSTR STARTUPINFO osstr PWSTR LPPROC
//spell-checker: ignore HRESULT STARTF USESTDHANDLES STARTUPINFOW

use std::{
    ffi::{OsStr, OsString}, mem::{self, size_of}, os::{
        raw::c_void,
        windows::{
            ffi::OsStrExt,
            io::{AsRawHandle, FromRawHandle, OwnedHandle},
        },
    }, process::Command, ptr::{null, null_mut}
};

use uucore::windows_sys::{
    core::{PCWSTR, PWSTR},
    Win32::{
        Foundation::{BOOL, HANDLE, S_OK},
        System::{
            Console::{
                ClosePseudoConsole, CreatePseudoConsole, GetConsoleMode, SetConsoleMode,
                CONSOLE_MODE, COORD, ENABLE_VIRTUAL_TERMINAL_PROCESSING, HPCON,
            },
            Pipes::CreatePipe,
            Threading::{
                CreateProcessW, InitializeProcThreadAttributeList, UpdateProcThreadAttribute,
                CREATE_UNICODE_ENVIRONMENT, EXTENDED_STARTUPINFO_PRESENT, PROCESS_INFORMATION,
                STARTF_USESTDHANDLES, STARTUPINFOEXW,
            },
        },
    },
};

use super::process::ProcessHandle;
use super::{Result, Error};

// if given string is empty there will be produced a "\0" string in UTF-16
fn convert_osstr_to_utf16(s: &OsStr) -> Vec<u16> {
    let mut bytes: Vec<_> = s.encode_wide().collect();
    bytes.push(0);
    bytes
}

fn build_command_line(command: &Command) -> OsString {
    let mut buf = OsString::new();
    buf.push(command.get_program());

    for arg in command.get_args() {
        buf.push(" ");
        buf.push(arg);
    }

    buf
}

pub(crate) struct Process {
    pub(crate) input: OwnedHandle,
    pub(crate) output: OwnedHandle,
    _proc_info: StartupInfoEx,
    _console: OwnedPseudoConsoleHandle,
    pub(crate) process_handle: ProcessHandle,
    _thread_handle: OwnedHandle,
    process_id: u32,
    _thread_id: u32,
}
impl Process {
    pub(crate) fn pid(&self) -> u32 {
        self.process_id
    }
}

pub(crate) fn spawn_command(command: Command, size: (i16, i16)) -> Result<Process> {
    // A Windows Subsystem process (i.e. one with WinMain) will not have a STDOUT, STDERR or STDIN,
    // unless it was specifically given one on launch.
    // The assumption is that since it is a windows program you are interacting with it via Windows.
    //
    // https://stackoverflow.com/questions/5115569/c-win32-api-getstdhandlestd-output-handle-is-invalid-very-perplexing
    //
    // Because of this we are ignoring a error of VT sequence set and set a default size
    //
    // todo: It would be great to be able to identify whether a attribute #![windows_subsystem = "windows"] is set and ignore it only in such case
    // But there's no way to do so?

    let (console, output, input) = create_pseudo_console(COORD {
        X: size.0,
        Y: size.1,
    })?;
    let startup_info = initialize_startup_info_attached_to_con_pty(&console)?;
    let proc = exec_proc(command, startup_info.startup_info)?;
    Ok(Process {
        input,
        output,
        _proc_info: startup_info,
        _console: console,
        process_handle: ProcessHandle::new_from_handle(unsafe {
            OwnedHandle::from_raw_handle(proc.hProcess as *mut c_void)
        }),
        process_id: proc.dwProcessId,
        _thread_handle: unsafe { OwnedHandle::from_raw_handle(proc.hThread as *mut c_void) },
        _thread_id: proc.dwThreadId,
    })
}

fn ok_or_win_error(result: BOOL) -> Result<()> {
    if result != 0 {
        Ok(())
    } else {
        Err(Error::StdOsIo(std::io::Error::last_os_error()))
    }
}

fn enable_virtual_terminal_sequence_processing() -> Result<()> {
    let stdout_h = std::io::stdout().as_raw_handle() as isize;
    unsafe {
        let mut mode = CONSOLE_MODE::default();
        ok_or_win_error(GetConsoleMode(stdout_h, &mut mode))?;
        mode |= ENABLE_VIRTUAL_TERMINAL_PROCESSING; // DISABLE_NEWLINE_AUTO_RETURN
        ok_or_win_error(SetConsoleMode(stdout_h, mode))?;
    }

    Ok(())
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

struct OwnedPseudoConsoleHandle {
    handle: HPCON,
}

impl Drop for OwnedPseudoConsoleHandle {
    fn drop(&mut self) {
        unsafe { ClosePseudoConsole(self.handle) };
    }
}

fn create_pseudo_console(
    size: COORD,
) -> Result<(OwnedPseudoConsoleHandle, OwnedHandle, OwnedHandle)> {
    let (pty_in, con_writer) = pipe()?;
    let (con_reader, pty_out) = pipe()?;

    let mut console_handle = HPCON::default();
    let hresult = unsafe {
        CreatePseudoConsole(
            size,
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

struct ProcThreadAttributeList {
    attribute_list_dyn: Box<[u8]>,
}

impl ProcThreadAttributeList {
    fn new(handle_pty: &OwnedPseudoConsoleHandle) -> Result<Self> {
        let mut size: usize = 0;
        let res =
            unsafe { InitializeProcThreadAttributeList(null_mut(),
                1, 0, &mut size) };

        if res != 0 /* according to the documentation this initial call must fail! */ || size == 0 {
            // https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-initializeprocthreadattributelist#return-value
            return Err(Error::Message(
                "failed initialize proc attribute list".to_string(),
            ));
        }

        let mut lp_attribute_list = vec![0u8; size].into_boxed_slice();

        let lp_attribute_list_ptr = lp_attribute_list.as_mut_ptr() as _;

        let handle_pty_ptr = handle_pty.handle as *const HPCON as *const c_void;

        unsafe {
            ok_or_win_error(InitializeProcThreadAttributeList(
                lp_attribute_list_ptr,
                1,
                0,
                &mut size,
            ))?;
            ok_or_win_error(UpdateProcThreadAttribute(
                lp_attribute_list_ptr,
                0,
                PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
                handle_pty_ptr,
                size_of::<HPCON>(),
                null_mut(),
                null_mut(),
            ))?;
        }

        Ok(Self{
            attribute_list_dyn: lp_attribute_list,
        })
    }

    fn get_ptr(&mut self) -> *mut c_void {
        self.attribute_list_dyn.as_mut_ptr() as _
    }
}

struct StartupInfoEx {
    _attribute_list: ProcThreadAttributeList,
    startup_info: STARTUPINFOEXW,
}

impl StartupInfoEx {
    fn new(handle_pty: &OwnedPseudoConsoleHandle) -> Result<Self> {
        let mut attribute_list = ProcThreadAttributeList::new(handle_pty)?;

        let mut si_ext: STARTUPINFOEXW = unsafe { ::core::mem::zeroed() };
        si_ext.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        si_ext.lpAttributeList = attribute_list.get_ptr();

        // avoid issues when debugging or using cargo-nextest.
        // solution described here: https://github.com/microsoft/terminal/issues/4380#issuecomment-580865346
        si_ext.StartupInfo.hStdInput = 0;
        si_ext.StartupInfo.hStdOutput = 0;
        si_ext.StartupInfo.hStdError = 0;
        si_ext.StartupInfo.dwFlags |= STARTF_USESTDHANDLES;

        Ok(Self { _attribute_list: attribute_list, startup_info: si_ext })
    }
}

// const PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE: usize = 22 | 0x0002_0000;
const PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE: usize = 0x00020016;

fn initialize_startup_info_attached_to_con_pty(
    handle_pty: &OwnedPseudoConsoleHandle,
) -> Result<StartupInfoEx> {
    Ok(StartupInfoEx::new(handle_pty)?)
}

fn environment_block_unicode<'a>(
    env: impl IntoIterator<Item = (&'a OsStr, &'a OsStr)>,
) -> Vec<u16> {
    let mut b = Vec::new();
    for (key, value) in env {
        b.extend(key.encode_wide());
        b.extend("=".encode_utf16());
        b.extend(value.encode_wide());
        b.push(0);
    }

    if b.is_empty() {
        // two '\0' in UTF-16/UCS-2
        // four '\0' in UTF-8
        return vec![0, 0];
    }

    b.push(0);

    b
}

fn exec_proc(command: Command, startup_info: STARTUPINFOEXW) -> Result<PROCESS_INFORMATION> {
    let command_line = build_command_line(&command);
    let mut command_line_wide_str = convert_osstr_to_utf16(&command_line);
    let command_line_ptr = command_line_wide_str.as_mut_ptr() as PWSTR;

    let current_dir = command.get_current_dir();
    let current_dir = current_dir.map(|p| convert_osstr_to_utf16(p.as_os_str()));
    let current_dir = current_dir.as_ref().map_or(null(), |dir| dir.as_ptr());
    let current_dir = current_dir as PCWSTR;

    let envs_list = || {
        command
            .get_envs()
            .filter_map(|(key, value)| value.map(|value| (key, value)))
    };
    let envs = environment_block_unicode(envs_list());
    let envs = if envs_list().next().is_some() {
        envs.as_ptr() as *const c_void
    } else {
        null()
    };

    let dw_flags = EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT; // CREATE_UNICODE_ENVIRONMENT | CREATE_NEW_CONSOLE

    let mut proc_info = PROCESS_INFORMATION {
        hProcess: 0,
        hThread: 0,
        dwProcessId: 0,
        dwThreadId: 0,
    };
    unsafe {
        ok_or_win_error(CreateProcessW(
            null(),
            command_line_ptr,
            null(),
            null(),
            0,
            dw_flags,
            envs,
            current_dir,
            &startup_info.StartupInfo,
            &mut proc_info,
        ))?
    };

    Ok(proc_info)
}
