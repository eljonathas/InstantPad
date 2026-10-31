use std::ffi::c_void;
use std::path::Path;

use instantpad_application::ports::SystemAudio;
use windows::Win32::Media::Audio::{
    EDataFlow, ERole, IMMDeviceEnumerator, MMDeviceEnumerator, eCapture, eCommunications,
    eMultimedia, eRender,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    CoUninitialize,
};

use super::shell;

/// cpal alone cannot do this: it only asks Windows for the default of the `eConsole` role,
/// while calls and games use `eCommunications`.
pub struct Endpoints;

impl SystemAudio for Endpoints {
    fn default_microphone_id(&self) -> Option<String> {
        default_endpoint_id(eCapture, eCommunications)
    }

    fn default_output_id(&self) -> Option<String> {
        default_endpoint_id(eRender, eCommunications)
    }

    fn system_output_id(&self) -> Option<String> {
        default_endpoint_id(eRender, eMultimedia)
    }

    fn open_sound_settings(&self) {
        let _ = shell::open(Path::new("ms-settings:sound"));
    }
}

/// Keeps COM initialized on this thread, which may be any thread of the runtime.
struct ComScope {
    initialized: bool,
}

impl ComScope {
    fn enter() -> Self {
        // SAFETY: balanced by `CoUninitialize` in `drop`, only when this call succeeded. It
        // fails with a changed mode when the thread already runs COM, which is still usable.
        let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
        Self { initialized }
    }
}

impl Drop for ComScope {
    fn drop(&mut self) {
        if self.initialized {
            // SAFETY: pairs the successful `CoInitializeEx` of `enter`.
            unsafe { CoUninitialize() };
        }
    }
}

/// The id is the one cpal gives the same device: `wasapi:` and the endpoint id.
fn default_endpoint_id(flow: EDataFlow, role: ERole) -> Option<String> {
    let _com = ComScope::enter();
    // SAFETY: COM is initialized above, and the id is freed with the allocator that made it.
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
        let device = enumerator.GetDefaultAudioEndpoint(flow, role).ok()?;
        let id = device.GetId().ok()?;
        let text = id.to_string();
        CoTaskMemFree(Some(id.as_ptr() as *const c_void));
        text.ok().map(|id| format!("wasapi:{id}"))
    }
}
