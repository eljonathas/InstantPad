//! The character added to the voice after its pitch: breath, echo, wobble, growl and shine.
//! Everything is made in `new`, so processing a block never allocates.

use std::f32::consts::TAU;
use std::sync::Arc;

use instantpad_domain::voice::VoiceColor;
use realfft::num_complex::Complex32;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};

use crate::controls::AtomicF32;
use crate::format::SAMPLE_RATE;

/// What the control side wants of each effect, read by the callback once per block.
pub(crate) struct ColorControls {
    whisper: AtomicF32,
    echo: AtomicF32,
    tremolo: AtomicF32,
    growl: AtomicF32,
    brightness: AtomicF32,
}

impl ColorControls {
    pub(crate) fn new() -> Self {
        Self {
            whisper: AtomicF32::new(0.0),
            echo: AtomicF32::new(0.0),
            tremolo: AtomicF32::new(0.0),
            growl: AtomicF32::new(0.0),
            brightness: AtomicF32::new(0.0),
        }
    }

    pub(crate) fn set(&self, color: VoiceColor) {
        self.whisper.store(color.whisper);
        self.echo.store(color.echo);
        self.tremolo.store(color.tremolo);
        self.growl.store(color.growl);
        self.brightness.store(color.brightness);
    }

    pub(crate) fn load(&self) -> VoiceColor {
        VoiceColor {
            whisper: self.whisper.load(),
            echo: self.echo.load(),
            tremolo: self.tremolo.load(),
            growl: self.growl.load(),
            brightness: self.brightness.load(),
        }
    }
}

/// The five effects, run in the order that sounds best: the growl works on the voice itself,
/// and the echo is last so that its tail is not shaken by the wobble.
pub(crate) struct Color {
    growl: Growl,
    whisper: Whisper,
    brightness: Brightness,
    tremolo: Tremolo,
    echo: Echo,
}

impl Color {
    pub(crate) fn new() -> Self {
        Self {
            growl: Growl::new(),
            whisper: Whisper::new(),
            brightness: Brightness::new(),
            tremolo: Tremolo::new(),
            echo: Echo::new(),
        }
    }

    /// Forgets the voice heard so far, so that old audio does not come out of a tail.
    pub(crate) fn reset(&mut self) {
        self.growl = Growl::new();
        self.brightness.state = [0.0; 2];
        self.whisper.is_idle = true;
        self.echo.is_idle = true;
    }

    /// Colors a block of the voice. An effect with an amount of zero does nothing at all.
    pub(crate) fn process(&mut self, block: &mut [f32], amounts: VoiceColor) {
        self.growl.process(block, amounts.growl);
        self.whisper.process(block, amounts.whisper);
        self.brightness.process(block, amounts.brightness);
        self.tremolo.process(block, amounts.tremolo);
        self.echo.process(block, amounts.echo);
    }
}

/// Zero for a number so small that the processor would slow down to work with it, which a
/// filter that decays into silence otherwise reaches.
fn flush(value: f32) -> f32 {
    if value.abs() < 1e-20 { 0.0 } else { value }
}

/// A filter with a single pole, which smooths whatever goes through it.
struct OnePole {
    coefficient: f32,
    state: f32,
}

impl OnePole {
    fn new(cutoff: f32) -> Self {
        Self {
            coefficient: 1.0 - (-TAU * cutoff / SAMPLE_RATE as f32).exp(),
            state: 0.0,
        }
    }

    fn process(&mut self, sample: f32) -> f32 {
        self.state = flush(self.state + self.coefficient * (sample - self.state));
        self.state
    }
}

/// Where the sub voice and the tail of the growl stop being low.
const GROWL_LOW_HZ: f32 = 350.0;
/// How loud the octave below is next to the voice, at full growl.
const GROWL_SUB_LEVEL: f32 = 0.8;
const GROWL_MAX_DRIVE: f32 = 6.0;
/// The follower of the loudness reacts in a few milliseconds and lets go in a few dozen.
const ENVELOPE_ATTACK: f32 = 0.01;
const ENVELOPE_RELEASE: f32 = 0.0005;

/// Distortion and a second voice an octave below. A flip-flop that changes with every cycle
/// of the low part of the voice makes a square wave at half its frequency, which the loudness
/// of the voice then shapes.
struct Growl {
    low: [OnePole; 2],
    smooth: OnePole,
    envelope: f32,
    is_above: bool,
    is_flipped: bool,
}

impl Growl {
    fn new() -> Self {
        Self {
            low: [OnePole::new(GROWL_LOW_HZ), OnePole::new(GROWL_LOW_HZ)],
            smooth: OnePole::new(GROWL_LOW_HZ),
            envelope: 0.0,
            is_above: false,
            is_flipped: false,
        }
    }

    fn process(&mut self, block: &mut [f32], amount: f32) {
        if amount <= 0.0 {
            return;
        }
        let drive = 1.0 + (GROWL_MAX_DRIVE - 1.0) * amount;
        // The same for the whole block, and a power is slow.
        let loudness = drive.powf(0.75);
        for sample in block {
            let low = self
                .low
                .iter_mut()
                .fold(*sample, |heard, pole| pole.process(heard));
            let rate = if low.abs() > self.envelope {
                ENVELOPE_ATTACK
            } else {
                ENVELOPE_RELEASE
            };
            self.envelope = flush(self.envelope + rate * (low.abs() - self.envelope));

            // Noise around zero must not flip it, so crossing needs a margin.
            let margin = self.envelope * 0.2 + 1e-4;
            if self.is_above && low < -margin {
                self.is_above = false;
            } else if !self.is_above && low > margin {
                self.is_above = true;
                self.is_flipped = !self.is_flipped;
            }
            let sign = if self.is_flipped { 1.0 } else { -1.0 };
            let sub = self.smooth.process(sign * self.envelope);

            let driven = (*sample * drive).tanh() / loudness;
            *sample += (driven - *sample) * amount + sub * GROWL_SUB_LEVEL * amount;
        }
    }
}

const WHISPER_FRAME: usize = 512;
const WHISPER_HOP: usize = 128;

/// Keeps how loud each frequency is and throws away when each one happens, which turns the
/// tone of a voice into breath. Frames overlap, and each is windowed again on the way out.
struct Whisper {
    forward: Arc<dyn RealToComplex<f32>>,
    inverse: Arc<dyn ComplexToReal<f32>>,
    window: Vec<f32>,
    /// The latest voice, as a ring where `position` is the oldest sample.
    heard: Vec<f32>,
    /// Frames added up in a ring of the same shape, read one sample at a time as they complete.
    breath: Vec<f32>,
    position: usize,
    frame: Vec<f32>,
    spectrum: Vec<Complex32>,
    scratch: Vec<Complex32>,
    random: u32,
    /// Whether it has been off, so that it holds old audio.
    is_idle: bool,
    /// Makes the overlapped frames as loud as the voice, and undoes the unscaled inverse.
    gain: f32,
}

impl Whisper {
    fn new() -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        let forward = planner.plan_fft_forward(WHISPER_FRAME);
        let inverse = planner.plan_fft_inverse(WHISPER_FRAME);
        let window: Vec<f32> = (0..WHISPER_FRAME)
            .map(|index| 0.5 - 0.5 * (TAU * index as f32 / WHISPER_FRAME as f32).cos())
            .collect();

        // Frames with random phases add up in power, not in amplitude. Each is shaped by the
        // window twice, and the voice has the power of its windowed copy.
        let mean_square =
            window.iter().map(|value| value * value).sum::<f32>() / WHISPER_FRAME as f32;
        let overlaps = (WHISPER_FRAME / WHISPER_HOP) as f32;
        let gain = 1.0 / (mean_square * overlaps.sqrt()) / WHISPER_FRAME as f32;

        let scratch = forward.get_scratch_len().max(inverse.get_scratch_len());
        Self {
            spectrum: forward.make_output_vec(),
            scratch: vec![Complex32::default(); scratch],
            forward,
            inverse,
            window,
            heard: vec![0.0; WHISPER_FRAME],
            breath: vec![0.0; WHISPER_FRAME],
            position: 0,
            frame: vec![0.0; WHISPER_FRAME],
            random: 0x9E37_79B9,
            is_idle: true,
            gain,
        }
    }

    fn clear(&mut self) {
        self.heard.fill(0.0);
        self.breath.fill(0.0);
    }

    fn process(&mut self, block: &mut [f32], amount: f32) {
        if amount <= 0.0 {
            self.is_idle = true;
            return;
        }
        if self.is_idle {
            self.is_idle = false;
            self.clear();
        }
        for sample in block {
            let (voice, breath) = self.push(*sample);
            *sample = voice + (breath - voice) * amount;
        }
    }

    /// Takes a sample of the voice and returns the voice and the breath one frame later.
    fn push(&mut self, sample: f32) -> (f32, f32) {
        let voice = std::mem::replace(&mut self.heard[self.position], sample);
        let breath = std::mem::take(&mut self.breath[self.position]);
        self.position = (self.position + 1) % WHISPER_FRAME;
        if self.position.is_multiple_of(WHISPER_HOP) {
            self.add_frame();
        }
        (voice, breath)
    }

    fn add_frame(&mut self) {
        for (index, value) in self.frame.iter_mut().enumerate() {
            *value = self.heard[(self.position + index) % WHISPER_FRAME] * self.window[index];
        }
        // This runs in an audio callback, where a panic would end the stream without a word. The
        // buffers are made for the plans, so a refusal would be a bug, and the frame goes on.
        let analysed = self.forward.process_with_scratch(
            &mut self.frame,
            &mut self.spectrum,
            &mut self.scratch,
        );
        debug_assert!(analysed.is_ok(), "the buffers are sized for the plan");

        for bin in &mut self.spectrum {
            let (sin, cos) = (next_random(&mut self.random) * TAU).sin_cos();
            let magnitude = bin.norm();
            *bin = Complex32::new(magnitude * cos, magnitude * sin);
        }
        // The first and the last bin have no phase, and the inverse refuses one.
        self.spectrum[0].im = 0.0;
        self.spectrum[WHISPER_FRAME / 2].im = 0.0;
        let synthesised = self.inverse.process_with_scratch(
            &mut self.spectrum,
            &mut self.frame,
            &mut self.scratch,
        );
        debug_assert!(synthesised.is_ok(), "the buffers are sized for the plan");

        for (index, value) in self.frame.iter().enumerate() {
            self.breath[(self.position + index) % WHISPER_FRAME] +=
                value * self.window[index] * self.gain;
        }
    }
}

/// A number from 0 up to 1, from a generator that is fast and is not meant to be secret.
fn next_random(state: &mut u32) -> f32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    (*state >> 8) as f32 / (1 << 24) as f32
}

const SHELF_HZ: f32 = 3000.0;
const SHELF_MAX_DB: f32 = 6.0;

/// A high shelf, which lifts everything above 3 kHz.
struct Brightness {
    amount: f32,
    /// Numerator and denominator of the filter, divided by the first of the denominator.
    b: [f32; 3],
    a: [f32; 2],
    state: [f32; 2],
}

impl Brightness {
    fn new() -> Self {
        Self {
            amount: 0.0,
            b: [1.0, 0.0, 0.0],
            a: [0.0, 0.0],
            state: [0.0; 2],
        }
    }

    fn process(&mut self, block: &mut [f32], amount: f32) {
        if amount <= 0.0 {
            return;
        }
        if amount != self.amount {
            self.amount = amount;
            self.shape(amount);
        }
        for sample in block {
            let output = self.b[0] * *sample + self.state[0];
            self.state[0] = flush(self.b[1] * *sample - self.a[0] * output + self.state[1]);
            self.state[1] = flush(self.b[2] * *sample - self.a[1] * output);
            *sample = output;
        }
    }

    /// The high shelf of the audio EQ cookbook, with the gentlest slope.
    fn shape(&mut self, amount: f32) {
        let gain = 10.0_f32.powf(SHELF_MAX_DB * amount / 40.0);
        let (sin, cos) = (TAU * SHELF_HZ / SAMPLE_RATE as f32).sin_cos();
        let alpha = sin / 2.0 * 2.0_f32.sqrt();
        let edge = 2.0 * gain.sqrt() * alpha;

        let a0 = (gain + 1.0) - (gain - 1.0) * cos + edge;
        self.b = [
            gain * ((gain + 1.0) + (gain - 1.0) * cos + edge) / a0,
            -2.0 * gain * ((gain - 1.0) + (gain + 1.0) * cos) / a0,
            gain * ((gain + 1.0) + (gain - 1.0) * cos - edge) / a0,
        ];
        self.a = [
            2.0 * ((gain - 1.0) - (gain + 1.0) * cos) / a0,
            ((gain + 1.0) - (gain - 1.0) * cos - edge) / a0,
        ];
    }
}

const TREMOLO_HZ: f32 = 5.0;

/// A slow wobble of the volume.
struct Tremolo {
    phase: f32,
}

impl Tremolo {
    fn new() -> Self {
        Self { phase: 0.0 }
    }

    fn process(&mut self, block: &mut [f32], depth: f32) {
        if depth <= 0.0 {
            return;
        }
        for sample in block {
            // The volume swings between full and `1 - depth`.
            *sample *= 1.0 - depth * (1.0 - (self.phase * TAU).sin()) / 2.0;
            self.phase = (self.phase + TREMOLO_HZ / SAMPLE_RATE as f32).fract();
        }
    }
}

/// The lengths of the delays of Freeverb at 44.1 kHz, which are chosen not to ring together.
const COMB_DELAYS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS_DELAYS: [usize; 4] = [556, 441, 341, 225];
/// A long decay, a few seconds, and a dark one, because the highs die quicker.
const COMB_FEEDBACK: f32 = 0.9;
const COMB_DAMPING: f32 = 0.4;
const ALLPASS_FEEDBACK: f32 = 0.5;
const ECHO_INPUT: f32 = 0.015;
const ECHO_LEVEL: f32 = 3.0;

fn delay_for(samples_at_44_100: usize) -> usize {
    samples_at_44_100 * SAMPLE_RATE as usize / 44_100
}

struct Comb {
    buffer: Vec<f32>,
    index: usize,
    damped: f32,
}

impl Comb {
    fn new(delay: usize) -> Self {
        Self {
            buffer: vec![0.0; delay_for(delay)],
            index: 0,
            damped: 0.0,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let output = self.buffer[self.index];
        self.damped = flush(output * (1.0 - COMB_DAMPING) + self.damped * COMB_DAMPING);
        self.buffer[self.index] = input + self.damped * COMB_FEEDBACK;
        self.index = (self.index + 1) % self.buffer.len();
        output
    }
}

struct Allpass {
    buffer: Vec<f32>,
    index: usize,
}

impl Allpass {
    fn new(delay: usize) -> Self {
        Self {
            buffer: vec![0.0; delay_for(delay)],
            index: 0,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let delayed = self.buffer[self.index];
        self.buffer[self.index] = flush(input + delayed * ALLPASS_FEEDBACK);
        self.index = (self.index + 1) % self.buffer.len();
        delayed - input
    }
}

/// A reverb of the Freeverb kind: combs side by side make the tail, and allpasses in a row
/// blur it into a wash.
struct Echo {
    combs: [Comb; 8],
    allpasses: [Allpass; 4],
    /// Whether it has been off, so that it holds old audio.
    is_idle: bool,
}

impl Echo {
    fn new() -> Self {
        Self {
            combs: COMB_DELAYS.map(Comb::new),
            allpasses: ALLPASS_DELAYS.map(Allpass::new),
            is_idle: true,
        }
    }

    fn clear(&mut self) {
        for comb in &mut self.combs {
            comb.buffer.fill(0.0);
            comb.damped = 0.0;
        }
        for allpass in &mut self.allpasses {
            allpass.buffer.fill(0.0);
        }
    }

    fn process(&mut self, block: &mut [f32], amount: f32) {
        if amount <= 0.0 {
            self.is_idle = true;
            return;
        }
        if self.is_idle {
            self.is_idle = false;
            self.clear();
        }
        for sample in block {
            let input = *sample * ECHO_INPUT;
            let mut wet: f32 = self.combs.iter_mut().map(|comb| comb.process(input)).sum();
            for allpass in &mut self.allpasses {
                wet = allpass.process(wet);
            }
            *sample = *sample * (1.0 - amount / 2.0) + wet * ECHO_LEVEL * amount;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLOCK: usize = 480;

    fn tone(frequency: f32, frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|frame| (frame as f32 / SAMPLE_RATE as f32 * frequency * TAU).sin() * 0.5)
            .collect()
    }

    /// A voice-like sound, which is a pitch with its harmonics.
    fn voice(frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|frame| {
                let time = frame as f32 / SAMPLE_RATE as f32;
                (1..=6)
                    .map(|harmonic| (time * 150.0 * harmonic as f32 * TAU).sin() / harmonic as f32)
                    .sum::<f32>()
                    * 0.2
            })
            .collect()
    }

    fn run(color: VoiceColor, input: &[f32]) -> Vec<f32> {
        let mut processor = Color::new();
        let mut output = input.to_vec();
        for block in output.chunks_mut(BLOCK) {
            processor.process(block, color);
        }
        output
    }

    fn only(color: impl FnOnce(&mut VoiceColor)) -> VoiceColor {
        let mut amounts = VoiceColor::default();
        color(&mut amounts);
        amounts
    }

    fn power(signal: &[f32]) -> f32 {
        signal.iter().map(|sample| sample * sample).sum::<f32>() / signal.len() as f32
    }

    /// How much of `signal` is a sine of `frequency`, as power.
    fn power_at(signal: &[f32], frequency: f32) -> f32 {
        let (mut real, mut imaginary) = (0.0, 0.0);
        for (index, sample) in signal.iter().enumerate() {
            let angle = TAU * frequency * index as f32 / SAMPLE_RATE as f32;
            real += sample * angle.cos();
            imaginary += sample * angle.sin();
        }
        let length = signal.len() as f32;
        (real * real + imaginary * imaginary) / (length * length) * 4.0
    }

    /// How much of the power sits in the strongest few frequencies: close to 1 for one pure
    /// tone, and less the more the sound is spread over the spectrum.
    fn concentration(signal: &[f32]) -> f32 {
        let size = 4096;
        let mut frame: Vec<f32> = signal[..size]
            .iter()
            .enumerate()
            .map(|(index, sample)| sample * (0.5 - 0.5 * (TAU * index as f32 / size as f32).cos()))
            .collect();
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(size);
        let mut spectrum = fft.make_output_vec();
        fft.process(&mut frame, &mut spectrum).unwrap();

        let powers: Vec<f32> = spectrum.iter().map(|bin| bin.norm_sqr()).collect();
        let peak = powers
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap()
            .0;
        let near: f32 = powers[peak.saturating_sub(3)..=peak + 3].iter().sum();
        near / powers.iter().sum::<f32>()
    }

    #[test]
    fn with_every_amount_at_zero_the_voice_is_left_alone() {
        let input = voice(BLOCK * 10);

        assert_eq!(run(VoiceColor::default(), &input), input);
    }

    #[test]
    fn whisper_spreads_a_pure_tone_over_the_spectrum() {
        let input = tone(1000.0, SAMPLE_RATE as usize);

        let whispered = run(only(|color| color.whisper = 1.0), &input);

        let settled = |signal: &[f32]| concentration(&signal[SAMPLE_RATE as usize / 2..]);
        assert!(settled(&input) > 0.99, "{}", settled(&input));
        assert!(settled(&whispered) < 0.6, "{}", settled(&whispered));
    }

    #[test]
    fn whisper_is_as_loud_as_the_voice() {
        let input = voice(SAMPLE_RATE as usize);

        let whispered = run(only(|color| color.whisper = 1.0), &input);

        let half = SAMPLE_RATE as usize / 2;
        let ratio = power(&whispered[half..]) / power(&input[half..]);
        assert!((0.6..1.6).contains(&ratio), "{ratio}");
    }

    #[test]
    fn a_little_whisper_keeps_most_of_the_voice() {
        let input = tone(220.0, SAMPLE_RATE as usize);

        let whispered = run(only(|color| color.whisper = 0.1), &input);

        let half = SAMPLE_RATE as usize / 2;
        let kept = power_at(&whispered[half..], 220.0) / power_at(&input[half..], 220.0);
        assert!(kept > 0.7, "{kept}");
    }

    #[test]
    fn echo_leaves_a_tail_after_the_voice_stops() {
        let mut input = tone(300.0, SAMPLE_RATE as usize / 2);
        input.extend(vec![0.0; SAMPLE_RATE as usize]);

        let echoed = run(only(|color| color.echo = 1.0), &input);

        let tail = &echoed[SAMPLE_RATE as usize * 3 / 4..SAMPLE_RATE as usize];
        let later = &echoed[SAMPLE_RATE as usize * 5 / 4..];
        assert!(power(tail) > 1e-4, "{}", power(tail));
        assert!(power(later) > 1e-5, "{}", power(later));
        assert!(power(later) < power(tail));
    }

    #[test]
    fn without_echo_nothing_is_left_after_the_voice_stops() {
        let mut input = tone(300.0, SAMPLE_RATE as usize / 2);
        input.extend(vec![0.0; SAMPLE_RATE as usize]);

        let colored = run(only(|color| color.tremolo = 1.0), &input);

        assert_eq!(power(&colored[SAMPLE_RATE as usize * 3 / 4..]), 0.0);
    }

    #[test]
    fn an_echo_that_was_off_does_not_play_what_it_heard_before() {
        let echo = only(|color| color.echo = 1.0);
        let mut processor = Color::new();
        for block in voice(BLOCK * 20).chunks_mut(BLOCK) {
            processor.process(block, echo);
        }
        processor.process(&mut [0.0; BLOCK], VoiceColor::default());

        let mut silence = vec![0.0; BLOCK];
        processor.process(&mut silence, echo);

        assert_eq!(power(&silence), 0.0);
    }

    #[test]
    fn a_whisper_that_was_off_does_not_play_what_it_heard_before() {
        let whisper = only(|color| color.whisper = 1.0);
        let mut processor = Color::new();
        for block in voice(BLOCK * 20).chunks_mut(BLOCK) {
            processor.process(block, whisper);
        }
        processor.process(&mut [0.0; BLOCK], VoiceColor::default());

        let mut silence = vec![0.0; BLOCK * 4];
        processor.process(&mut silence, whisper);

        assert_eq!(power(&silence), 0.0);
    }

    #[test]
    fn without_whisper_the_voice_is_not_delayed() {
        let mut impulse = vec![0.0; BLOCK];
        impulse[0] = 1.0;

        let wobbled = run(only(|color| color.tremolo = 1.0), &impulse);

        assert_eq!(wobbled[0], 0.5);
    }

    #[test]
    fn a_reset_leaves_no_trace_of_the_voice_in_the_growl_and_the_shelf() {
        let both = VoiceColor {
            growl: 1.0,
            brightness: 1.0,
            ..VoiceColor::default()
        };
        let mut processor = Color::new();
        processor.process(&mut voice(BLOCK), both);

        processor.reset();

        assert_eq!(processor.growl.envelope, 0.0);
        assert_eq!(processor.brightness.state, [0.0; 2]);
        let mut quiet = [0.0; BLOCK];
        processor.process(&mut quiet, both);
        assert!(quiet.iter().all(|&sample| sample == 0.0));
    }

    #[test]
    fn silence_brings_every_filter_to_exactly_zero() {
        let all = VoiceColor {
            whisper: 0.5,
            echo: 1.0,
            tremolo: 0.5,
            growl: 1.0,
            brightness: 1.0,
        };
        let mut processor = Color::new();
        for block in voice(BLOCK * 20).chunks_mut(BLOCK) {
            processor.process(block, all);
        }

        // Longer than the tail of the echo takes to fall below what a float can hold.
        for _ in 0..SAMPLE_RATE as usize * 20 / BLOCK {
            processor.process(&mut [0.0; BLOCK], all);
        }

        let growl = &processor.growl;
        assert_eq!(growl.envelope, 0.0);
        let poles = growl.low.iter().chain([&growl.smooth]);
        assert!(poles.into_iter().all(|pole| pole.state == 0.0));
        assert_eq!(processor.brightness.state, [0.0; 2]);
        let echo = &processor.echo;
        assert!(echo.combs.iter().all(|comb| comb.damped == 0.0));
        assert!(
            echo.combs
                .iter()
                .all(|comb| comb.buffer.iter().all(|&v| v == 0.0))
        );
        assert!(
            echo.allpasses
                .iter()
                .all(|a| a.buffer.iter().all(|&v| v == 0.0))
        );
    }

    #[test]
    fn tremolo_swings_the_volume_between_full_and_quiet() {
        let input = tone(440.0, SAMPLE_RATE as usize);

        let wobbled = run(only(|color| color.tremolo = 1.0), &input);

        let levels: Vec<f32> = wobbled.chunks(BLOCK).map(power).collect();
        let loudest = levels.iter().copied().fold(0.0, f32::max);
        let quietest = levels.iter().copied().fold(f32::MAX, f32::min);
        assert!(quietest < loudest / 10.0, "{quietest} {loudest}");
        assert!(loudest > power(&input) * 0.9);
    }

    #[test]
    fn growl_adds_energy_an_octave_below() {
        let input = tone(220.0, SAMPLE_RATE as usize);
        let half = SAMPLE_RATE as usize / 2;

        let growled = run(only(|color| color.growl = 1.0), &input);

        let below = power_at(&growled[half..], 110.0);
        assert!(power_at(&input[half..], 110.0) < 1e-6);
        assert!(below > 0.005, "{below}");
    }

    #[test]
    fn growl_adds_harmonics_and_keeps_the_voice_in_range() {
        let input = tone(220.0, SAMPLE_RATE as usize);

        let growled = run(only(|color| color.growl = 1.0), &input);

        let loudest = growled
            .iter()
            .fold(0.0, |peak: f32, sample| peak.max(sample.abs()));
        assert!(loudest < 2.0, "{loudest}");
        assert!(power_at(&growled, 660.0) > 1e-4);
    }

    #[test]
    fn brightness_raises_the_energy_above_three_kilohertz() {
        let high = tone(8000.0, SAMPLE_RATE as usize);
        let low = tone(200.0, SAMPLE_RATE as usize);
        let half = SAMPLE_RATE as usize / 2;
        let bright = only(|color| color.brightness = 1.0);

        let lifted =
            power_at(&run(bright, &high)[half..], 8000.0) / power_at(&high[half..], 8000.0);
        let kept = power_at(&run(bright, &low)[half..], 200.0) / power_at(&low[half..], 200.0);

        // 6 dB is four times the power.
        assert!((3.5..4.5).contains(&lifted), "{lifted}");
        assert!((0.95..1.05).contains(&kept), "{kept}");
    }

    #[test]
    fn a_shelf_is_stable_when_the_amount_changes_all_the_time() {
        let mut processor = Color::new();
        let mut peak = 0.0_f32;

        for round in 0..400 {
            let bright = only(|color| color.brightness = (round % 10) as f32 / 10.0 + 0.1);
            let mut block = tone(5000.0, BLOCK);
            processor.process(&mut block, bright);
            peak = block
                .iter()
                .fold(peak, |peak, sample| peak.max(sample.abs()));
        }

        assert!(peak < 2.0, "{peak}");
    }

    #[test]
    fn the_same_voice_whispers_the_same_way_every_time() {
        let input = voice(BLOCK * 20);
        let whisper = only(|color| color.whisper = 1.0);

        assert_eq!(run(whisper, &input), run(whisper, &input));
    }

    #[test]
    fn the_random_numbers_stay_inside_zero_and_one() {
        let mut state = 1;

        let all_inside = (0..10_000).all(|_| (0.0..1.0).contains(&next_random(&mut state)));

        assert!(all_inside);
    }
}
