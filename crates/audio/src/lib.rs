//! The real-time audio engine: it decodes sounds, mixes them with the microphone and plays the
//! result on the chosen devices.

mod ai;
mod capture;
mod clip;
mod color;
mod controls;
mod devices;
mod engine;
mod format;
mod mic;
mod mixer;
mod playback;
mod recording;
mod role;
mod voice;
mod worker;

pub use engine::AudioEngine;

#[cfg(test)]
mod allocation_tests;
