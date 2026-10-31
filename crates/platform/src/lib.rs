//! Per-OS adapters for `SystemAudio` and `VirtualCable`. Device ids are the ones cpal gives the
//! same device (`<host>:<device>`), so they match the ids the audio crate lists.

use std::sync::Arc;

use instantpad_application::ports::{SystemAudio, VirtualCable};

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod cpal_defaults;
// The cable's name matching is plain code, so tests run it on every OS.
#[cfg(any(test, target_os = "linux"))]
mod linux;
#[cfg(any(test, target_os = "macos"))]
mod macos;
// Calls into the Windows API need `unsafe`, which is denied everywhere else.
#[cfg(any(test, target_os = "windows"))]
#[allow(unsafe_code)]
mod windows;

pub fn system_audio() -> Arc<dyn SystemAudio> {
    #[cfg(target_os = "windows")]
    return Arc::new(windows::Endpoints);
    #[cfg(target_os = "macos")]
    return Arc::new(macos::MacSystemAudio);
    #[cfg(target_os = "linux")]
    return Arc::new(linux::LinuxSystemAudio);
}

/// On Linux the cable is created here. It stays after the app closes, and the next start reuses
/// it, because Tauri never drops what it manages.
pub fn virtual_cable() -> Arc<dyn VirtualCable> {
    #[cfg(target_os = "windows")]
    return Arc::new(windows::VbCable);
    #[cfg(target_os = "macos")]
    return Arc::new(macos::BlackHole);
    #[cfg(target_os = "linux")]
    return Arc::new(linux::PulseCable::load());
}
