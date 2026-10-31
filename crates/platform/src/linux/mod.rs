//! Linux adapters. The cable is a pair of PulseAudio/PipeWire modules, created through `pactl`.

mod pulse_cable;

#[cfg(target_os = "linux")]
mod system_audio;

#[cfg(target_os = "linux")]
pub use pulse_cable::PulseCable;
#[cfg(target_os = "linux")]
pub use system_audio::LinuxSystemAudio;
