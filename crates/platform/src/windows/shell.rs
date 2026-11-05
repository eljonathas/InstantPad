use std::path::Path;

use windows::Win32::Foundation::{CloseHandle, ERROR_CANCELLED};
use windows::Win32::System::Threading::{INFINITE, WaitForSingleObject};
use windows::Win32::UI::Shell::{
    SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW, ShellExecuteW,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{HSTRING, PCWSTR, w};

/// What `ShellExecuteW` returns at or below this value is an error code.
const LAST_ERROR_CODE: isize = 32;

pub enum Refusal {
    /// The user declined the elevation prompt.
    Declined,
    /// The system's error, an HRESULT.
    Failed(u32),
}

/// Opens `target` with whatever the system uses for it. Returns whether it was opened.
pub fn open(target: &Path) -> bool {
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
    result.0 as isize > LAST_ERROR_CODE
}

/// Runs `program` as administrator and waits until it closes, so whoever asked knows when the
/// setup it opened is done. It starts in `directory`. Blocks the thread.
pub fn run_elevated(program: &Path, directory: &Path) -> Result<(), Refusal> {
    let program = HSTRING::from(program);
    let directory = HSTRING::from(directory);
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: w!("runas"),
        lpFile: PCWSTR(program.as_ptr()),
        lpDirectory: PCWSTR(directory.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    // SAFETY: `info` is sized for this call, and its strings are null-terminated and outlive it.
    if let Err(error) = unsafe { ShellExecuteExW(&mut info) } {
        return Err(if error.code() == ERROR_CANCELLED.to_hresult() {
            Refusal::Declined
        } else {
            Refusal::Failed(error.code().0 as u32)
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
