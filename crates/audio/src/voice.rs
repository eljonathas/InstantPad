//! Changes the voice on its way from the microphone to the rings, in the capture callback.

use std::f32::consts::TAU;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use instantpad_domain::voice::{PitchShape, VoiceShift};
use signalsmith_stretch::Stretch;

use crate::controls::AtomicF32;
use crate::format::{CHANNELS, SAMPLE_RATE};

/// The delay the effect adds. A shorter window hurts low voices.
const WINDOW_MS: usize = 40;
const INTERVAL_MS: usize = 10;
/// How often the pitch of a cycle moves, which is also how often the library runs.
const BLOCK_FRAMES: usize = SAMPLE_RATE as usize * INTERVAL_MS / 1000;
const BLOCK_SECONDS: f32 = INTERVAL_MS as f32 / 1000.0;

/// What the control side wants, read by the callback once per block.
pub(crate) struct VoiceControls {
    is_on: AtomicBool,
    low: AtomicF32,
    high: AtomicF32,
    seconds: AtomicF32,
    formant: AtomicF32,
}

impl VoiceControls {
    pub(crate) fn new() -> Self {
        Self {
            is_on: AtomicBool::new(false),
            low: AtomicF32::new(0.0),
            high: AtomicF32::new(0.0),
            seconds: AtomicF32::new(1.0),
            formant: AtomicF32::new(0.0),
        }
    }

    pub(crate) fn set(&self, shift: Option<VoiceShift>) {
        let Some(VoiceShift { pitch, formant }) = shift else {
            self.is_on.store(false, Ordering::Relaxed);
            return;
        };

        // A fixed pitch is a cycle that has nowhere to go.
        let (low, high, seconds) = match pitch {
            PitchShape::Fixed { semitones } => (semitones, semitones, 1.0),
            PitchShape::Cycle { low, high, seconds } => (low, high, seconds),
        };
        self.low.store(low);
        self.high.store(high);
        self.seconds.store(seconds);
        self.formant.store(formant);
        self.is_on.store(true, Ordering::Relaxed);
    }
}

pub(crate) struct VoiceProcessor {
    controls: Arc<VoiceControls>,
    stretch: Stretch,
    was_on: bool,
    /// Position inside the pitch cycle, from 0 to 1.
    phase: f32,
    mono: [f32; BLOCK_FRAMES],
    filled: usize,
    changed: [f32; BLOCK_FRAMES],
    stereo: [f32; BLOCK_FRAMES * CHANNELS],
}

impl VoiceProcessor {
    pub(crate) fn new(controls: Arc<VoiceControls>) -> Self {
        let frames_per_ms = SAMPLE_RATE as usize / 1000;
        Self {
            controls,
            stretch: Stretch::new(1, WINDOW_MS * frames_per_ms, INTERVAL_MS * frames_per_ms),
            was_on: false,
            phase: 0.0,
            mono: [0.0; BLOCK_FRAMES],
            filled: 0,
            changed: [0.0; BLOCK_FRAMES],
            stereo: [0.0; BLOCK_FRAMES * CHANNELS],
        }
    }

    /// Hands `emit` the voice in engine format. Runs in the capture callback.
    pub(crate) fn process(&mut self, stereo: &[f32], mut emit: impl FnMut(&[f32])) {
        let is_on = self.controls.is_on.load(Ordering::Relaxed);
        if !is_on {
            self.was_on = false;
            emit(stereo);
            return;
        }
        if !self.was_on {
            self.was_on = true;
            self.stretch.reset();
            self.filled = 0;
            self.phase = 0.0;
        }

        for [left, right] in stereo.as_chunks::<CHANNELS>().0 {
            self.mono[self.filled] = (left + right) / 2.0;
            self.filled += 1;
            if self.filled == BLOCK_FRAMES {
                self.filled = 0;
                self.change_block();
                emit(&self.stereo);
            }
        }
    }

    fn change_block(&mut self) {
        let (low, high) = (self.controls.low.load(), self.controls.high.load());
        let rise = ((self.phase * TAU).sin() + 1.0) / 2.0;
        self.phase = (self.phase + BLOCK_SECONDS / self.controls.seconds.load()).fract();

        self.stretch
            .set_transpose_factor_semitones(low + (high - low) * rise, None);
        self.stretch
            .set_formant_factor_semitones(self.controls.formant.load(), true);
        self.stretch.process(self.mono, &mut self.changed);

        for (frame, sample) in self
            .stereo
            .as_chunks_mut::<CHANNELS>()
            .0
            .iter_mut()
            .zip(self.changed)
        {
            *frame = [sample; CHANNELS];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(frequency: f32, frames: usize) -> Vec<f32> {
        (0..frames)
            .flat_map(|frame| {
                let time = frame as f32 / SAMPLE_RATE as f32;
                let sample = (time * frequency * TAU).sin() * 0.5;
                [sample, sample]
            })
            .collect()
    }

    fn run(shift: Option<VoiceShift>, input: &[f32]) -> Vec<f32> {
        let controls = Arc::new(VoiceControls::new());
        controls.set(shift);
        let mut voice = VoiceProcessor::new(controls);
        let mut output = Vec::new();
        for block in input.chunks(BLOCK_FRAMES) {
            voice.process(block, |samples| output.extend_from_slice(samples));
        }
        output
    }

    /// Counts rising zero crossings of the left channel over its second half.
    fn frequency_of(stereo: &[f32]) -> f32 {
        let left: Vec<f32> = stereo
            .as_chunks::<CHANNELS>()
            .0
            .iter()
            .map(|frame| frame[0])
            .collect();
        let settled = &left[left.len() / 2..];
        let rises = settled
            .windows(2)
            .filter(|pair| pair[0] <= 0.0 && pair[1] > 0.0)
            .count();
        rises as f32 / (settled.len() as f32 / SAMPLE_RATE as f32)
    }

    fn fixed(semitones: f32) -> Option<VoiceShift> {
        Some(VoiceShift {
            pitch: PitchShape::Fixed { semitones },
            formant: 0.0,
        })
    }

    #[test]
    fn with_no_effect_the_voice_goes_through_untouched_and_at_once() {
        let input = tone(220.0, 480);

        assert_eq!(run(None, &input), input);
    }

    #[test]
    fn twelve_semitones_up_doubles_the_pitch() {
        let output = run(fixed(12.0), &tone(220.0, SAMPLE_RATE as usize));

        let frequency = frequency_of(&output);
        assert!((frequency - 440.0).abs() < 15.0, "{frequency}");
    }

    #[test]
    fn twelve_semitones_down_halves_the_pitch() {
        let output = run(fixed(-12.0), &tone(440.0, SAMPLE_RATE as usize));

        let frequency = frequency_of(&output);
        assert!((frequency - 220.0).abs() < 15.0, "{frequency}");
    }

    #[test]
    fn the_output_is_the_same_length_as_the_input() {
        let input = tone(220.0, BLOCK_FRAMES * 20);

        assert_eq!(run(fixed(5.0), &input).len(), input.len());
    }

    #[test]
    fn a_pitch_cycle_goes_both_above_and_below_the_voice() {
        let frames = SAMPLE_RATE as usize * 2;
        let shift = Some(VoiceShift {
            pitch: PitchShape::Cycle {
                low: -7.0,
                high: 7.0,
                seconds: 2.0,
            },
            formant: 0.0,
        });

        let output = run(shift, &tone(330.0, frames));

        let quarter = frames / 4 * CHANNELS;
        let rising = frequency_of(&output[quarter / 2..quarter * 3 / 2]);
        let falling = frequency_of(&output[quarter * 5 / 2..quarter * 7 / 2]);
        assert!(rising > 330.0 * 1.2, "{rising}");
        assert!(falling < 330.0 / 1.2, "{falling}");
    }

    #[test]
    fn turning_the_effect_off_goes_straight_back_to_the_natural_voice() {
        let controls = Arc::new(VoiceControls::new());
        controls.set(fixed(5.0));
        let mut voice = VoiceProcessor::new(controls.clone());
        let input = tone(220.0, BLOCK_FRAMES);
        voice.process(&input, |_| {});

        controls.set(None);

        let mut output = Vec::new();
        voice.process(&input, |samples| output.extend_from_slice(samples));
        assert_eq!(output, input);
    }
}
