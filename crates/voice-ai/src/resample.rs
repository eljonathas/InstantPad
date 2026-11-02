//! Moves speech between the rate of the app and the rates of the models.

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};

/// The rate of the speech that goes in and out of the app.
pub(crate) const SPEECH_RATE: usize = 48_000;
/// Frames the resampler works on at a time.
const CHUNK: usize = 1024;

/// A whole recording at once.
pub(crate) fn resample(speech: &[f32], from: usize, to: usize) -> Vec<f32> {
    if speech.is_empty() {
        return Vec::new();
    }
    let mut resampler =
        Fft::<f32>::new(from, to, CHUNK, 1, FixedSync::Input).expect("the sample rates are valid");
    let input = InterleavedSlice::new(speech, 1, speech.len()).expect("the speech is one channel");
    resampler
        .process_all(&input, speech.len(), None)
        .expect("the buffers are sized for the resampler")
        .take_data()
}

/// Speech that never ends, resampled as it arrives.
pub(crate) struct StreamResampler {
    resampler: Fft<f32>,
    /// What arrived, waiting for a whole block.
    pending: Vec<f32>,
    block: Vec<f32>,
    /// The silence the resampler starts with, which would only delay the speech.
    skip: usize,
}

impl StreamResampler {
    /// `block` is the smallest number of input frames the two rates divide into, so nothing
    /// waits for more speech than it has to.
    pub(crate) fn new(from: usize, to: usize, block: usize) -> Self {
        let resampler = Fft::<f32>::new(from, to, block, 1, FixedSync::Input)
            .expect("the sample rates are valid");
        Self {
            block: vec![0.0; resampler.output_frames_max()],
            skip: resampler.output_delay(),
            pending: Vec::new(),
            resampler,
        }
    }

    /// Adds the resampled speech that is ready to `output`.
    pub(crate) fn push(&mut self, speech: &[f32], output: &mut Vec<f32>) {
        self.pending.extend_from_slice(speech);

        let mut used = 0;
        loop {
            let needed = self.resampler.input_frames_next();
            if self.pending.len() - used < needed {
                break;
            }
            let input = InterleavedSlice::new(&self.pending[used..used + needed], 1, needed)
                .expect("the block is one channel");
            let capacity = self.block.len();
            let mut produced = InterleavedSlice::new_mut(&mut self.block[..], 1, capacity)
                .expect("the output is one channel");
            let (_, frames) = self
                .resampler
                .process_into_buffer(&input, &mut produced, None)
                .expect("the buffers are sized for the resampler");
            used += needed;

            let skipped = self.skip.min(frames);
            self.skip -= skipped;
            output.extend_from_slice(&self.block[skipped..frames]);
        }
        self.pending.drain(..used);
    }
}

#[cfg(test)]
mod tests {
    use std::f32::consts::TAU;

    use super::*;

    fn tone(rate: usize, hertz: f32, seconds: f32) -> Vec<f32> {
        (0..(rate as f32 * seconds) as usize)
            .map(|n| 0.5 * (TAU * hertz * n as f32 / rate as f32).sin())
            .collect()
    }

    fn rms(speech: &[f32]) -> f32 {
        (speech.iter().map(|sample| sample * sample).sum::<f32>() / speech.len() as f32).sqrt()
    }

    #[test]
    fn a_recording_keeps_its_length_and_loudness_at_the_new_rate() {
        let speech = tone(SPEECH_RATE, 440.0, 1.0);

        let slower = resample(&speech, SPEECH_RATE, 22_050);

        assert!(slower.len().abs_diff(22_050) <= 2, "{}", slower.len());
        assert!((rms(&slower[2000..20_000]) - 0.5 / 2.0_f32.sqrt()).abs() < 0.01);
    }

    #[test]
    fn nothing_resampled_is_nothing() {
        assert!(resample(&[], SPEECH_RATE, 16_000).is_empty());
    }

    #[test]
    fn a_stream_in_pieces_matches_the_recording_all_at_once() {
        let speech = tone(SPEECH_RATE, 440.0, 1.0);
        let mut stream = StreamResampler::new(SPEECH_RATE, 22_050, 320);

        let mut streamed = Vec::new();
        for piece in speech.chunks(480) {
            stream.push(piece, &mut streamed);
        }

        // The last, incomplete block waits for speech that never comes.
        assert!(streamed.len() <= 22_050 && streamed.len() > 22_050 - 147 - 2);
        assert!((rms(&streamed[2000..20_000]) - 0.5 / 2.0_f32.sqrt()).abs() < 0.01);
    }
}
