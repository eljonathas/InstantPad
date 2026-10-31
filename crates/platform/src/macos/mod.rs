//! macOS adapters. The cable is BlackHole, which the user installs themselves.

mod blackhole;

#[cfg(target_os = "macos")]
mod system_audio;

#[cfg(target_os = "macos")]
pub use blackhole::BlackHole;
#[cfg(target_os = "macos")]
pub use system_audio::MacSystemAudio;
