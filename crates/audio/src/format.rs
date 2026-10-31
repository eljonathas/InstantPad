//! The format everything is converted to before mixing.

pub(crate) const SAMPLE_RATE: u32 = 48_000;
pub(crate) const CHANNELS: usize = 2;

/// How many samples (not frames) `milliseconds` of engine audio take.
pub(crate) const fn samples_for(milliseconds: usize) -> usize {
    SAMPLE_RATE as usize * CHANNELS * milliseconds / 1000
}
