//! The real-time audio engine: it decodes sounds, mixes them with the microphone and plays the
//! result on the chosen devices.

mod capture;
mod clip;
mod controls;
mod devices;
mod engine;
mod format;
mod mic;
mod mixer;
mod playback;
mod role;

pub use engine::AudioEngine;
