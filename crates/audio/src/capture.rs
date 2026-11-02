//! Turns what the microphone delivers into engine audio for the game mixer.

use std::sync::Arc;

use cpal::{FromSample, Sample};
use rtrb::Producer;
use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler, ResamplerConstructionError};

use crate::ai::AiStage;
use crate::controls::AtomicF32;
use crate::format::{CHANNELS, SAMPLE_RATE};
use crate::recording::RecordTap;
use crate::voice::{VoiceControls, VoiceProcessor};

/// Frames written to the ring at a time when no resampling is needed.
const BLOCK_FRAMES: usize = 480;

pub(crate) struct CaptureProcessor {
    channels: usize,
    resampler: Option<Fft<f32>>,
    /// Stereo frames at the device rate, waiting for a whole block.
    block: Vec<f32>,
    filled: usize,
    resampled: Vec<f32>,
    /// Keeps the microphone as it is while the user records.
    tap: RecordTap,
    voice: VoiceProcessor,
    ai: AiStage,
    /// Where the voice goes: the game, and the headphones for hearing yourself.
    rings: [Producer<f32>; 2],
    level: Arc<AtomicF32>,
}

impl CaptureProcessor {
    pub(crate) fn new(
        channels: usize,
        rate: u32,
        rings: [Producer<f32>; 2],
        level: Arc<AtomicF32>,
        voice: Arc<VoiceControls>,
        tap: RecordTap,
        ai: AiStage,
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
            tap,
            voice: VoiceProcessor::new(voice),
            ai,
            rings,
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

        self.tap.push(ready);
        let (ai, rings) = (&mut self.ai, &mut self.rings);
        self.voice.process(ready, |voice| {
            ai.process(voice, |voice| {
                // When nobody reads, because an output is closed, the newest audio is dropped.
                for ring in rings.iter_mut() {
                    let _ = ring.push_partial_slice(voice);
                }
            });
        });
        self.filled = 0;
    }
}

#[cfg(test)]
mod tests {
    use std::thread;
    use std::time::{Duration, Instant};

    use instantpad_domain::voice::{PitchShape, VoiceChain, VoiceColor};
    use instantpad_test_support::fakes::{FakeConverter, RecordingEvents};
    use rtrb::Consumer;
    use rtrb::RingBuffer;

    use super::*;
    use crate::ai::AiBridge;
    use crate::ai::tests::conversion;
    use crate::recording::Recorder;

    /// A processor with everything around it that a test may want to reach.
    struct Rig {
        processor: CaptureProcessor,
        game: Consumer<f32>,
        level: Arc<AtomicF32>,
        voice: Arc<VoiceControls>,
        recorder: Recorder,
        ai: AiBridge,
    }

    fn rig(channels: usize, rate: u32) -> Rig {
        let (writer, game) = RingBuffer::new(48_000 * CHANNELS);
        let (headphones, _) = RingBuffer::new(CHANNELS);
        let level = Arc::new(AtomicF32::new(0.0));
        let voice = Arc::new(VoiceControls::new());
        let recorder = Recorder::new();
        let (tap, recording) = recorder.tap();
        recorder.attach(Some(recording));
        let ai = AiBridge::new(
            Arc::new(FakeConverter::default()),
            Arc::new(RecordingEvents::default()),
        );
        let (stage, converting) = ai.rings();
        ai.attach(Some(converting));
        let processor = CaptureProcessor::new(
            channels,
            rate,
            [writer, headphones],
            level.clone(),
            voice.clone(),
            tap,
            stage,
        )
        .unwrap();
        Rig {
            processor,
            game,
            level,
            voice,
            recorder,
            ai,
        }
    }

    fn processor(channels: usize, rate: u32) -> (CaptureProcessor, Consumer<f32>, Arc<AtomicF32>) {
        let rig = rig(channels, rate);
        (rig.processor, rig.game, rig.level)
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

    /// A voice whose every sample is different and none is zero, as stereo.
    fn ramp(from: usize, frames: usize) -> Vec<f32> {
        (from..from + frames)
            .flat_map(|index| [(index + 1) as f32 * 1e-5; CHANNELS])
            .collect()
    }

    #[test]
    fn the_recording_keeps_the_raw_voice_even_with_an_effect_on() {
        let mut rig = rig(2, SAMPLE_RATE);
        rig.voice.set(Some(VoiceChain {
            pitch: PitchShape::Fixed { semitones: 7.0 },
            formant: 2.0,
            color: VoiceColor {
                growl: 1.0,
                echo: 0.5,
                ..VoiceColor::default()
            },
        }));
        let voice = ramp(0, BLOCK_FRAMES * 20);
        rig.recorder.start();

        rig.processor.push(&voice);

        let recorded = rig.recorder.stop();
        assert_eq!(
            recorded,
            voice.iter().step_by(CHANNELS).copied().collect::<Vec<_>>()
        );
        assert_ne!(drain(&mut rig.game), voice);
    }

    #[test]
    fn the_recording_is_mono_at_the_engine_rate_whatever_the_device_gives() {
        let mut rig = rig(1, 44_100);
        rig.recorder.start();

        for _ in 0..50 {
            rig.processor.push(&[0.5_f32; 441]);
        }

        let seconds = rig.recorder.stop().len() as f32 / SAMPLE_RATE as f32;
        assert!((seconds - 0.5).abs() < 0.03, "{seconds}");
    }

    #[test]
    fn the_converted_voice_reaches_the_game_after_priming() {
        let mut rig = rig(2, SAMPLE_RATE);
        rig.ai.route(Some(&conversion()));
        let mut heard = Vec::new();

        // The thread of the converter runs on its own time, so the voice goes in until it is out.
        let started = Instant::now();
        let mut sent = 0;
        while heard.iter().filter(|&&sample| sample != 0.0).count() < 4800 {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "no converted voice"
            );
            rig.processor.push(&ramp(sent * BLOCK_FRAMES, BLOCK_FRAMES));
            sent += 1;
            thread::sleep(Duration::from_millis(1));
            heard.extend(drain(&mut rig.game));
        }

        let left: Vec<f32> = heard.iter().step_by(CHANNELS).copied().collect();
        let first = left.iter().position(|&sample| sample != 0.0).unwrap();
        let converted: Vec<f32> = left.into_iter().filter(|&sample| sample != 0.0).collect();
        let voice: Vec<f32> = ramp(0, converted.len())
            .into_iter()
            .step_by(CHANNELS)
            .collect();
        assert!(first >= 48 * 160, "{first}");
        assert_eq!(converted, voice);
    }
}
