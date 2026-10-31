//! Values the control side and the audio callbacks share without locks.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub(crate) struct AtomicF32(AtomicU32);

impl AtomicF32 {
    pub(crate) fn new(value: f32) -> Self {
        Self(AtomicU32::new(value.to_bits()))
    }

    pub(crate) fn load(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }

    pub(crate) fn store(&self, value: f32) {
        self.0.store(value.to_bits(), Ordering::Relaxed);
    }
}

/// The knobs and the meter of one mixer.
pub(crate) struct Controls {
    /// Gain applied to sounds, 0 to 1.
    pub(crate) sound_volume: AtomicF32,
    /// Gain applied to the live input, 0 to 1.
    pub(crate) mic_volume: AtomicF32,
    pub(crate) mic_enabled: AtomicBool,
    /// Peak of the most recent block, for the meters.
    pub(crate) peak: AtomicF32,
}

impl Controls {
    pub(crate) fn new() -> Self {
        Self {
            sound_volume: AtomicF32::new(1.0),
            mic_volume: AtomicF32::new(1.0),
            mic_enabled: AtomicBool::new(true),
            peak: AtomicF32::new(0.0),
        }
    }
}
