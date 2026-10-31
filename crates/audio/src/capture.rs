//! Turns what the microphone delivers into engine audio for the game mixer.

use std::sync::Arc;

use cpal::{FromSample, Sample};
use rtrb::Producer;
use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler, ResamplerConstructionError};

use crate::controls::AtomicF32;
use crate::format::{CHANNELS, SAMPLE_RATE};

/// Frames written to the ring at a time when no resampling is needed.
const BLOCK_FRAMES: usize = 480;

pub(crate) struct CaptureProcessor {
    channels: usize,
    resampler: Option<Fft<f32>>,
    /// Stereo frames at the device rate, waiting for a whole block.
    block: Vec<f32>,
    filled: usize,
    resampled: Vec<f32>,
    ring: Producer<f32>,
    level: Arc<AtomicF32>,
}

impl CaptureProcessor {
    pub(crate) fn new(
        channels: usize,
        rate: u32,
        ring: Producer<f32>,
        level: Arc<AtomicF32>,
    ) -> Result<Self, ResamplerConstructionError> {
        let resampler = (rate != SAMPLE_RATE)
            .then(|| {
                Fft::new(
                    rate as usize,
                    SAMPLE_RATE as usize,
                    BLOCK_FRAMES,
                    CHANNELS,
                    FixedSync::Input,
                )
            })
            .transpose()?;
        let (block_frames, resampled_frames) =
            resampler.as_ref().map_or((BLOCK_FRAMES, 0), |resampler| {
                (resampler.input_frames_next(), resampler.output_frames_max())
            });

        Ok(Self {
            channels,
            resampler,
            block: vec![0.0; block_frames * CHANNELS],
            filled: 0,
            resampled: vec![0.0; resampled_frames * CHANNELS],
            ring,
            level,
        })
    }

    /// Runs in the capture callback.
    pub(crate) fn push<T: Copy>(&mut self, data: &[T])
    where
        f32: FromSample<T>,
    {
        let mut peak = 0.0_f32;
        for frame in data.chunks_exact(self.channels) {
            // Mono is played on both sides, and more than two channels keep the first two.
            let left = f32::from_sample(frame[0]);
            let right = frame
                .get(1)
                .map_or(left, |&sample| f32::from_sample(sample));
            peak = peak.max(left.abs()).max(right.abs());

            self.block[self.filled] = left;
            self.block[self.filled + 1] = right;
            self.filled += CHANNELS;
            if self.filled == self.block.len() {
                self.flush();
            }
        }
        self.level.store(peak);
    }

    fn flush(&mut self) {
        let ready = match &mut self.resampler {
            Some(resampler) => {
                let frames = self.block.len() / CHANNELS;
                let input = InterleavedSlice::new(&self.block[..], CHANNELS, frames)
                    .expect("the block holds whole frames");
                let capacity = self.resampled.len() / CHANNELS;
                let mut output =
                    InterleavedSlice::new_mut(&mut self.resampled[..], CHANNELS, capacity)
                        .expect("the output holds whole frames");
                let (_, produced) = resampler
                    .process_into_buffer(&input, &mut output, None)
                    .expect("the buffers are sized for the resampler");
                &self.resampled[..produced * CHANNELS]
            }
            None => &self.block[..],
        };

        // When nobody reads, because the game output is closed, the newest audio is dropped.
        let _ = self.ring.push_partial_slice(ready);
        self.filled = 0;
    }
}

#[cfg(test)]
mod tests {
    use rtrb::Consumer;
    use rtrb::RingBuffer;

    use super::*;

    fn processor(channels: usize, rate: u32) -> (CaptureProcessor, Consumer<f32>, Arc<AtomicF32>) {
        let (writer, reader) = RingBuffer::new(48_000 * CHANNELS);
        let level = Arc::new(AtomicF32::new(0.0));
        let processor = CaptureProcessor::new(channels, rate, writer, level.clone()).unwrap();
        (processor, reader, level)
    }

    fn drain(reader: &mut Consumer<f32>) -> Vec<f32> {
        let mut samples = vec![0.0; reader.slots()];
        let (popped, _) = reader.pop_partial_slice(&mut samples);
        popped.to_vec()
    }

    #[test]
    fn stereo_at_the_engine_rate_passes_through() {
        let (mut processor, mut reader, level) = processor(2, SAMPLE_RATE);

        processor.push(&[0.25_f32, -0.5].repeat(BLOCK_FRAMES));

        assert_eq!(drain(&mut reader), [0.25, -0.5].repeat(BLOCK_FRAMES));
        assert_eq!(level.load(), 0.5);
    }

    #[test]
    fn mono_is_played_on_both_sides() {
        let (mut processor, mut reader, _) = processor(1, SAMPLE_RATE);

        processor.push(&[0.25_f32; BLOCK_FRAMES]);

        assert_eq!(drain(&mut reader), [0.25; BLOCK_FRAMES * 2]);
    }

    #[test]
    fn more_than_two_channels_keep_the_first_two() {
        let (mut processor, mut reader, _) = processor(3, SAMPLE_RATE);

        processor.push(&[0.1_f32, 0.2, 0.9].repeat(BLOCK_FRAMES));

        assert_eq!(drain(&mut reader), [0.1, 0.2].repeat(BLOCK_FRAMES));
    }

    #[test]
    fn integer_samples_become_floats() {
        let (mut processor, mut reader, _) = processor(2, SAMPLE_RATE);

        processor.push(&[16384_i16, -16384].repeat(BLOCK_FRAMES));

        assert_eq!(drain(&mut reader)[..2], [0.5, -0.5]);
    }

    #[test]
    fn audio_arriving_in_small_pieces_is_written_once_a_block_is_full() {
        let (mut processor, mut reader, _) = processor(2, SAMPLE_RATE);

        for _ in 0..BLOCK_FRAMES - 1 {
            processor.push(&[0.5_f32, 0.5]);
        }
        assert!(drain(&mut reader).is_empty());

        processor.push(&[0.5_f32, 0.5]);
        assert_eq!(drain(&mut reader).len(), BLOCK_FRAMES * 2);
    }

    #[test]
    fn another_sample_rate_is_resampled_to_the_engine_rate() {
        let (mut processor, mut reader, _) = processor(2, 44_100);

        // Half a second in callbacks of 10 ms, like a device would deliver it.
        for _ in 0..50 {
            processor.push(&[0.5_f32; 441 * CHANNELS]);
        }

        let frames = drain(&mut reader).len() / CHANNELS;
        assert!(
            frames.abs_diff(24_000) <= BLOCK_FRAMES * 2,
            "{frames} frames"
        );
    }
}
