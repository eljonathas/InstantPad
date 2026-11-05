//! Feeds a mixer to an output stream, adapting to the device when it cannot take the engine
//! format itself.

use std::error::Error;

use crate::format::{CHANNELS, SAMPLE_RATE};
use crate::mixer::Mixer;
use crate::resample::{BLOCK_FRAMES, BlockResampler};

/// What a device that is not stereo at the engine rate needs.
pub(crate) struct Conversion {
    channels: usize,
    resampler: Option<BlockResampler>,
    /// One block of engine audio, the input of the resampler.
    mixed: Vec<f32>,
    /// Stereo at the device rate.
    converted: Vec<f32>,
    ready: usize,
    next: usize,
}

impl Conversion {
    pub(crate) fn new(channels: usize, rate: u32) -> Result<Self, Box<dyn Error>> {
        // A device without channels has nothing to put a frame in.
        if channels == 0 {
            return Err("the device has no channels".into());
        }
        let resampler = (rate != SAMPLE_RATE)
            .then(|| BlockResampler::new(SAMPLE_RATE, rate))
            .transpose()?;
        let (mixed, converted) = resampler
            .as_ref()
            .map_or((0, BLOCK_FRAMES * CHANNELS), |r| {
                (r.input_len(), r.output_len())
            });

        Ok(Self {
            channels,
            resampler,
            mixed: vec![0.0; mixed],
            converted: vec![0.0; converted],
            ready: 0,
            next: 0,
        })
    }

    /// Left and right go to the first two channels and any others stay silent. One channel gets
    /// both sides mixed.
    fn fill(&mut self, mixer: &mut Mixer, block: &mut [f32]) {
        for frame in block.chunks_exact_mut(self.channels) {
            // A resampler can hold on to a whole block and produce nothing for it.
            while self.next == self.ready {
                self.refill(mixer);
            }

            frame.fill(0.0);
            let stereo = &self.converted[self.next..self.next + CHANNELS];
            if let [only] = frame {
                // Right would be lost on a device with one speaker.
                *only = (stereo[0] + stereo[1]) / 2.0;
            } else {
                frame[..CHANNELS].copy_from_slice(stereo);
            }
            self.next += CHANNELS;
        }
    }

    fn refill(&mut self, mixer: &mut Mixer) {
        self.ready = match &mut self.resampler {
            Some(resampler) => {
                mixer.render(&mut self.mixed);
                resampler.process(&self.mixed, &mut self.converted)
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
        handle.add_whole(clip, 1.0, PlaybackTag::of("a"));
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
    fn a_mono_device_gets_both_sides_mixed() {
        let mut playback = playback(1, SAMPLE_RATE, 0.5, 4800);

        let mut block = [1.0; 100];
        playback.fill(&mut block);

        assert!(block.iter().all(|&sample| sample == 0.5));
    }

    #[test]
    fn a_sound_on_the_right_side_alone_is_heard_on_a_mono_device() {
        let (mixer, mut handle) = mixer::pair(Arc::new(Controls::new()), None);
        let right_only = [0.0, 0.8].repeat(4800);
        handle.add_whole(
            Arc::new(SoundClip::new(right_only)),
            1.0,
            PlaybackTag::of("a"),
        );
        let mut playback = Playback::new(mixer, Some(Conversion::new(1, SAMPLE_RATE).unwrap()));

        let mut block = [0.0; 100];
        playback.fill(&mut block);

        assert!(block.iter().all(|&sample| sample > 0.3));
    }

    #[test]
    fn a_device_without_channels_is_refused() {
        assert!(Conversion::new(0, SAMPLE_RATE).is_err());
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
