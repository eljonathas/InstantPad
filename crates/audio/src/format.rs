//! The format everything is converted to before mixing.

use instantpad_domain::cloning::MAX_SAMPLE_SECONDS;

pub(crate) const SAMPLE_RATE: u32 = 48_000;
pub(crate) const CHANNELS: usize = 2;

/// The longest speech the engine takes from a file or a recording: what the app takes and a
/// second more, so that the application is the one that reports a sample just over the limit.
pub(crate) const MAX_SPEECH_SECONDS: u32 = MAX_SAMPLE_SECONDS + 1;

/// How many frames `milliseconds` of engine audio take, which is also how many mono samples.
pub(crate) const fn frames_for(milliseconds: usize) -> usize {
    SAMPLE_RATE as usize * milliseconds / 1000
}

/// How many samples (not frames) `milliseconds` of engine audio take.
pub(crate) const fn samples_for(milliseconds: usize) -> usize {
    frames_for(milliseconds) * CHANNELS
}

/// The two channels of each frame averaged into one.
pub(crate) fn downmix(stereo: &[f32]) -> impl Iterator<Item = f32> + '_ {
    stereo
        .as_chunks::<CHANNELS>()
        .0
        .iter()
        .map(|[left, right]| (left + right) / 2.0)
}
