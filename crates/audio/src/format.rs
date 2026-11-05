//! The format everything is converted to before mixing.

use instantpad_domain::cloning::MAX_SAMPLE_SECONDS;
use instantpad_domain::training::MAX_TAKE_SECONDS;

pub(crate) const SAMPLE_RATE: u32 = 48_000;
pub(crate) const CHANNELS: usize = 2;

/// The longest speech the engine takes from a file or a recording: the longest the app takes, a
/// sample to clone or a take to train, and a second more, so that the application is the one
/// that reports a sample just over the limit.
pub(crate) const MAX_SPEECH_SECONDS: u32 = max(MAX_SAMPLE_SECONDS, MAX_TAKE_SECONDS) + 1;

const fn max(a: u32, b: u32) -> u32 {
    if a > b { a } else { b }
}

// A take to train is twice as long as a sample to clone; neither may be cut by the engine.
const _: () = assert!(MAX_SPEECH_SECONDS > MAX_TAKE_SECONDS);
const _: () = assert!(MAX_SPEECH_SECONDS > MAX_SAMPLE_SECONDS);

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

/// Frames downmixed at a time by [`push_downmixed`].
const DOWNMIX_FRAMES: usize = 256;

/// Writes the stereo voice to `ring` as mono, in a few writes instead of one per sample. What
/// does not fit is dropped, newest first.
pub(crate) fn push_downmixed(ring: &mut rtrb::Producer<f32>, stereo: &[f32]) {
    let mut mono = [0.0; DOWNMIX_FRAMES];
    for part in stereo.chunks(DOWNMIX_FRAMES * CHANNELS) {
        let count = part.len() / CHANNELS;
        for (out, sample) in mono.iter_mut().zip(downmix(part)) {
            *out = sample;
        }
        if ring.push_partial_slice(&mono[..count]).0.len() < count {
            return;
        }
    }
}
