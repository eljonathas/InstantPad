//! Resamples stereo in blocks, for the callbacks that adapt to a device's rate.

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler, ResamplerConstructionError};

use crate::format::CHANNELS;

/// Frames of input that make a block when the rate is the same on both sides of a callback.
pub(crate) const BLOCK_FRAMES: usize = 480;

pub(crate) struct BlockResampler(Fft<f32>);

impl BlockResampler {
    pub(crate) fn new(from: u32, to: u32) -> Result<Self, ResamplerConstructionError> {
        Fft::new(
            from as usize,
            to as usize,
            BLOCK_FRAMES,
            CHANNELS,
            FixedSync::Input,
        )
        .map(Self)
    }

    /// The samples (not frames) of the stereo block that `process` takes.
    pub(crate) fn input_len(&self) -> usize {
        self.0.input_frames_next() * CHANNELS
    }

    /// The samples (not frames) that the output of `process` needs room for.
    pub(crate) fn output_len(&self) -> usize {
        self.0.output_frames_max() * CHANNELS
    }

    /// Resamples a block of `input_len` samples into `output`, which holds `output_len`, and
    /// returns how many samples came out. The buffers are sized from this resampler, so the
    /// adapters cannot refuse them; if they ever did, the output is silence, because this runs in
    /// an audio callback, where a panic would take the stream down without a word.
    pub(crate) fn process(&mut self, input: &[f32], output: &mut [f32]) -> usize {
        let resampled = self.try_process(input, output);
        debug_assert!(
            resampled.is_some(),
            "the buffers are sized for the resampler"
        );
        resampled.unwrap_or_else(|| {
            output.fill(0.0);
            output.len()
        })
    }

    fn try_process(&mut self, input: &[f32], output: &mut [f32]) -> Option<usize> {
        let input = InterleavedSlice::new(input, CHANNELS, input.len() / CHANNELS).ok()?;
        let capacity = output.len() / CHANNELS;
        let mut output = InterleavedSlice::new_mut(output, CHANNELS, capacity).ok()?;
        let (_, produced) = self.0.process_into_buffer(&input, &mut output, None).ok()?;
        Some(produced * CHANNELS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_comes_out_at_the_new_rate() {
        let mut resampler = BlockResampler::new(24_000, 48_000).unwrap();
        let input = vec![0.5; resampler.input_len()];
        let mut output = vec![0.0; resampler.output_len()];

        let produced = resampler.process(&input, &mut output);

        assert_eq!(produced, input.len() * 2);
    }

    #[test]
    fn buffers_of_the_wrong_size_give_silence_instead_of_a_panic() {
        let mut resampler = BlockResampler::new(24_000, 48_000).unwrap();
        let mut output = vec![1.0; 8];

        let resampled = resampler.try_process(&[0.5; 6], &mut output);

        assert_eq!(resampled, None);
    }
}
