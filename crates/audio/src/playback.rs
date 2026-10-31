//! Feeds a mixer to an output stream, adapting to the device when it cannot take the engine
//! format itself.

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler, ResamplerConstructionError};

use crate::format::{CHANNELS, SAMPLE_RATE};
use crate::mixer::Mixer;

/// Frames mixed at a time when no resampling is needed.
const BLOCK_FRAMES: usize = 480;

/// What a device that is not stereo at the engine rate needs.
pub(crate) struct Conversion {
    channels: usize,
    resampler: Option<Fft<f32>>,
    /// One block of engine audio, the input of the resampler.
    mixed: Vec<f32>,
    /// Stereo at the device rate.
    converted: Vec<f32>,
    ready: usize,
    next: usize,
}

impl Conversion {
    pub(crate) fn new(channels: usize, rate: u32) -> Result<Self, ResamplerConstructionError> {
        let resampler = (rate != SAMPLE_RATE)
            .then(|| {
                Fft::new(
                    SAMPLE_RATE as usize,
                    rate as usize,
                    BLOCK_FRAMES,
                    CHANNELS,
                    FixedSync::Input,
                )
            })
            .transpose()?;
        let (mixed_frames, converted_frames) =
            resampler.as_ref().map_or((0, BLOCK_FRAMES), |resampler| {
                (resampler.input_frames_next(), resampler.output_frames_max())
            });

        Ok(Self {
            channels,
            resampler,
            mixed: vec![0.0; mixed_frames * CHANNELS],
            converted: vec![0.0; converted_frames * CHANNELS],
            ready: 0,
            next: 0,
        })
    }

    /// Left and right go to the first two channels and any others stay silent.
    fn fill(&mut self, mixer: &mut Mixer, block: &mut [f32]) {
        for frame in block.chunks_exact_mut(self.channels) {
            // A resampler can hold on to a whole block and produce nothing for it.
            while self.next == self.ready {
                self.refill(mixer);
            }

            frame.fill(0.0);
            let stereo = &self.converted[self.next..self.next + CHANNELS];
            let shared = frame.len().min(CHANNELS);
            frame[..shared].copy_from_slice(&stereo[..shared]);
            self.next += CHANNELS;
        }
    }

    fn refill(&mut self, mixer: &mut Mixer) {
        self.ready = match &mut self.resampler {
            Some(resampler) => {
                mixer.render(&mut self.mixed);
                let frames = self.mixed.len() / CHANNELS;
                let input = InterleavedSlice::new(&self.mixed[..], CHANNELS, frames)
                    .expect("the block holds whole frames");
                let capacity = self.converted.len() / CHANNELS;
                let mut output =
                    InterleavedSlice::new_mut(&mut self.converted[..], CHANNELS, capacity)
                        .expect("the output holds whole frames");
                let (_, produced) = resampler
                    .process_into_buffer(&input, &mut output, None)
                    .expect("the buffers are sized for the resampler");
                produced * CHANNELS
            }
            None => {
                mixer.render(&mut self.converted);
                self.converted.len()
            }
        };
        self.next = 0;
    }
}

/// What the output callback owns.
pub(crate) struct Playback {
    mixer: Mixer,
    conversion: Option<Conversion>,
}

impl Playback {
    pub(crate) fn new(mixer: Mixer, conversion: Option<Conversion>) -> Self {
        Self { mixer, conversion }
    }

    /// Runs in the output callback.
    pub(crate) fn fill(&mut self, block: &mut [f32]) {
        match &mut self.conversion {
            Some(conversion) => conversion.fill(&mut self.mixer, block),
            None => self.mixer.render(block),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use instantpad_application::ports::PlaybackTag;

    use super::*;
    use crate::clip::SoundClip;
    use crate::controls::Controls;
    use crate::mixer;

    fn playback(channels: usize, rate: u32, level: f32, frames: usize) -> Playback {
        let (mixer, mut handle) = mixer::pair(Arc::new(Controls::new()), None);
        let clip = Arc::new(SoundClip::new(vec![level; frames * CHANNELS]));
        handle.add(clip, 1.0, PlaybackTag::of("a"));
        Playback::new(mixer, Some(Conversion::new(channels, rate).unwrap()))
    }

    #[test]
    fn a_device_at_the_engine_rate_with_more_channels_gets_silence_on_the_extra_ones() {
        let mut playback = playback(6, SAMPLE_RATE, 0.5, 4800);

        let mut block = [1.0; 6 * 100];
        playback.fill(&mut block);

        assert!(
            block
                .chunks(6)
                .all(|frame| frame == [0.5, 0.5, 0.0, 0.0, 0.0, 0.0])
        );
    }

    #[test]
    fn a_mono_device_gets_the_left_side() {
        let mut playback = playback(1, SAMPLE_RATE, 0.5, 4800);

        let mut block = [1.0; 100];
        playback.fill(&mut block);

        assert!(block.iter().all(|&sample| sample == 0.5));
    }

    #[test]
    fn a_rate_where_the_resampler_skips_blocks_still_fills_every_callback() {
        for rate in [11_025, 22_050] {
            let mut playback = playback(2, rate, 0.5, 48_000);

            for _ in 0..20 {
                playback.fill(&mut vec![0.0; 600 * CHANNELS]);
            }
        }
    }

    #[test]
    fn a_device_at_another_rate_is_fed_across_callbacks_of_any_size() {
        let mut playback = playback(2, 44_100, 0.5, 48_000);

        // Callback sizes that never line up with the resampler blocks.
        let mut heard = Vec::new();
        for frames in [333, 701, 64, 1500, 997] {
            let mut block = vec![0.0; frames * CHANNELS];
            playback.fill(&mut block);
            heard.extend(block);
        }

        // After the resampler's start-up the constant level comes through.
        let settled = &heard[heard.len() / 2..];
        assert!(settled.iter().all(|&sample| (sample - 0.5).abs() < 0.01));
    }
}
