use std::path::Path;

use windows::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, GetLastError};
use windows::Win32::System::Threading::{INFINITE, WaitForSingleObject};
use windows::Win32::UI::Shell::{
    SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW, ShellExecuteW,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HSTRING, PCWSTR, w};

/// What `ShellExecuteW` returns at or below this value is an error code.
const LAST_ERROR_CODE: isize = 32;
/// What it returns when the user declines the elevation prompt.
const ACCESS_DENIED: isize = 5;

pub enum Refusal {
    /// The user declined the elevation prompt.
    Declined,
    Failed(isize),
}

pub fn open(target: &Path) -> Result<(), Refusal> {
    let target = HSTRING::from(target);
    // SAFETY: both strings are null-terminated and outlive the call.
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            &target,
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    // SAFETY: reads the thread's own last error, which nothing has touched since the call.
    let last_error = unsafe { GetLastError() };
    match result.0 as isize {
        code if code > LAST_ERROR_CODE => Ok(()),
        ACCESS_DENIED => Err(Refusal::Declined),
        _ if last_error == ERROR_CANCELLED => Err(Refusal::Declined),
        code => Err(Refusal::Failed(code)),
    }
}

/// Runs `program` as administrator and waits until it closes, so whoever asked knows when the
/// setup it opened is done. Blocks the thread.
pub fn run_elevated(program: &Path) -> Result<(), Refusal> {
    let program = HSTRING::from(program);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(program.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    // SAFETY: `info` is sized for this call, and its strings are null-terminated and outlive it.
    if let Err(error) = unsafe { ShellExecuteExW(&mut info) } {
        return Err(if error.code() == ERROR_CANCELLED.to_hresult() {
            Refusal::Declined
        } else {
            Refusal::Failed(error.code().0 as isize)
        });
    }
    if !info.hProcess.is_invalid() {
        // SAFETY: the handle was opened for this process by the call above and is closed once.
        unsafe {
            WaitForSingleObject(info.hProcess, INFINITE);
            let _ = CloseHandle(info.hProcess);
        }
    }
    Ok(())
}
