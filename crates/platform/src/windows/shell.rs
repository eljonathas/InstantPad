use std::path::Path;

use windows::Win32::Foundation::{ERROR_CANCELLED, GetLastError};
use windows::Win32::UI::Shell::ShellExecuteW;
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

/// Opens `target` with the `verb` of the shell.
fn execute(verb: PCWSTR, target: &Path) -> Result<(), Refusal> {
    let target = HSTRING::from(target);
    // SAFETY: both strings are null-terminated and outlive the call.
    let result = unsafe {
        ShellExecuteW(
            None,
            verb,
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

pub fn open(target: &Path) -> Result<(), Refusal> {
    execute(w!("open"), target)
}

pub fn open_elevated(target: &Path) -> Result<(), Refusal> {
    execute(w!("runas"), target)
}
