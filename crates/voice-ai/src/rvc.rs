//! A voice trained with RVC, speaking as the user talks. It follows the realtime converter of
//! RVC: every block of speech, HuBERT hears what is said in a window that reaches back before
//! the block, YIN finds its pitch, moved to the pitch of the trained voice, and the synthesizer
//! speaks the end of the window. Each block overlaps the one before, and the join is found where
//! the two sound most alike (SOLA), so nothing clicks.

use std::f32::consts::{FRAC_PI_2, TAU};

use instantpad_application::message::Failure;

use crate::resample::{SPEECH_RATE, StreamResampler};
use crate::runtime::load_failed;
use crate::yin;

/// What every trained voice hears with, which the app keeps once for all of them.
pub(crate) const CONTENT: &str = "content.onnx";
/// The voice itself, in the folder of each trained model.
pub(crate) const SYNTHESIZER: &str = "synthesizer.onnx";
/// Where the trained models are, among the models of the app.
pub(crate) const TRAINED: &str = "trained";

/// The rate the trained voices speak at.
const VOICE_RATE: usize = 40_000;
/// The voice speaks a frame of this many samples for each frame of what it hears.
const VOICE_HOP: usize = 400;
/// The frames of 10 ms the speech is read in, at the rate of HuBERT and of the pitch.
const FRAME: usize = yin::HOP;
const HEARING_RATE: usize = yin::RATE;
/// The frames that come out of each window: 300 ms, which a 4-core 2.1 GHz processor speaks in
/// about 290 ms. Shorter blocks cost more each than they save.
const BLOCK: usize = 30;
/// What the window hears before the block, so the voice knows how the words began.
const CONTEXT: usize = 30;
/// Where one block fades into the next.
const CROSSFADE: usize = 4;
/// How far the join may move to where the two blocks sound most alike.
const SEARCH: usize = 1;
/// The frames the synthesizer speaks: the block, the crossfade and the search, and one more at
/// the end that is dropped, because it hears nothing after it.
const SPOKEN: usize = BLOCK + CROSSFADE + SEARCH + 1;
const WINDOW: usize = CONTEXT + SPOKEN;
/// The flow of the synthesizer reads this many frames before the ones it speaks.
const FLOW_CONTEXT: usize = 24;
/// The pitch is read on the end of the window, from a little before the spoken frames.
const PITCH_FRAMES: usize = BLOCK + 5;
/// The first and last frames of the pitch hear the edge of their stretch and are left out.
const PITCH_EDGE: usize = 3;
/// The noise the synthesizer adds, as RVC scales it.
const NOISE_SCALE: f32 = 0.666_66;
/// The size of what HuBERT says about each frame.
pub(crate) const FEATURES: usize = 768;

// Every block hands over the same number of samples at each rate.
const _: () = assert!(
    (BLOCK * FRAME).is_multiple_of(SPEECH_RATE / HEARING_RATE)
        && PITCH_FRAMES * FRAME <= WINDOW * FRAME
);

/// The two models a trained voice runs.
pub(crate) trait Models {
    /// What is said in `speech` at 16 kHz: [`FEATURES`] values for each 20 ms.
    fn content(&mut self, speech: &[f32]) -> Result<Vec<f32>, Failure>;

    /// The voice for the last `pitchf.len()` frames of the window, at [`VOICE_RATE`].
    fn speak(&mut self, words: &Words) -> Result<Vec<f32>, Failure>;
}

/// What the synthesizer speaks from, named as its inputs are.
pub(crate) struct Words<'a> {
    /// [`FEATURES`] values for each frame of the window.
    pub(crate) phone: &'a [f32],
    /// The pitch of each frame of the window, in the steps the model reads.
    pub(crate) pitch: &'a [i64],
    /// The pitch in hertz of the frames to speak.
    pub(crate) pitchf: &'a [f32],
    /// Random values for the flow, for its context and the frames to speak.
    pub(crate) noise: &'a [f32],
}

impl Words<'_> {
    pub(crate) fn frames(&self) -> usize {
        self.pitch.len()
    }
}

/// Runs both models once on a silent window, which loads them and makes their first run,
/// always slower, happen before any speech waits on them.
pub(crate) fn warm_up(models: &mut impl Models) -> Result<(), Failure> {
    let phone = frames_of(&models.content(&vec![0.0; WINDOW * FRAME])?)?;
    models.speak(&Words {
        phone: &phone,
        pitch: &[1; WINDOW],
        pitchf: &[0.0; SPOKEN],
        noise: &[0.0; FEATURES_OF_FLOW * (SPOKEN + FLOW_CONTEXT)],
    })?;
    Ok(())
}

/// A trained voice for speech that never ends.
pub(crate) struct TrainedStream {
    down: StreamResampler,
    up: StreamResampler,
    /// Heard and not yet in a window.
    heard: Vec<f32>,
    /// The last window, at 16 kHz.
    window: Vec<f32>,
    /// The pitch of each frame of the window, moved and in steps, and in hertz.
    pitch: Vec<i64>,
    pitchf: Vec<f32>,
    /// The end of the last block, which the next one fades in from.
    tail: Vec<f32>,
    voice_hz: f32,
    semitones: f32,
    user: PitchTracker,
    noise: Noise,
}

impl TrainedStream {
    /// `voice_hz` is the pitch the trained voice usually speaks at, and `semitones` how far
    /// from it the user's voice goes.
    pub(crate) fn new(voice_hz: f32, semitones: f32) -> Self {
        Self {
            down: StreamResampler::new(SPEECH_RATE, HEARING_RATE, 480),
            up: StreamResampler::new(VOICE_RATE, SPEECH_RATE, VOICE_HOP),
            heard: Vec::new(),
            window: vec![0.0; WINDOW * FRAME],
            pitch: vec![1; WINDOW],
            pitchf: vec![0.0; WINDOW],
            tail: vec![0.0; CROSSFADE * VOICE_HOP],
            voice_hz,
            semitones,
            user: PitchTracker::default(),
            noise: Noise::new(0x5eed),
        }
    }

    /// Moves the voice by `semitones` from its own pitch from the next block on.
    pub(crate) fn retune(&mut self, semitones: f32) {
        self.semitones = semitones;
    }

    /// Takes the next speech at 48 kHz and returns the voice that is ready, at 48 kHz.
    pub(crate) fn convert(
        &mut self,
        speech: &[f32],
        models: &mut impl Models,
    ) -> Result<Vec<f32>, Failure> {
        self.down.push(speech, &mut self.heard);
        let mut spoken = Vec::new();
        while self.heard.len() >= BLOCK * FRAME {
            self.window.drain(..BLOCK * FRAME);
            self.window.extend(self.heard.drain(..BLOCK * FRAME));
            spoken.extend(self.speak_window(models)?);
        }
        let mut converted = Vec::new();
        self.up.push(&spoken, &mut converted);
        Ok(converted)
    }

    /// The voice for the newest block of the window, at [`VOICE_RATE`].
    fn speak_window(&mut self, models: &mut impl Models) -> Result<Vec<f32>, Failure> {
        let phone = frames_of(&models.content(&self.window)?)?;
        self.follow_pitch();

        let noise: Vec<f32> = (0..FEATURES_OF_FLOW * (SPOKEN + FLOW_CONTEXT))
            .map(|_| self.noise.normal() * NOISE_SCALE)
            .collect();
        let voice = models.speak(&Words {
            phone: &phone,
            pitch: &self.pitch,
            pitchf: &self.pitchf[CONTEXT..],
            noise: &noise,
        })?;
        let wanted = (SPOKEN - 1) * VOICE_HOP;
        let voice = voice
            .get(..wanted)
            .ok_or_else(|| load_failed("the trained voice returned too little speech"))?;
        Ok(self.join(voice))
    }

    /// Reads the pitch of the new block and moves it to the voice.
    fn follow_pitch(&mut self) {
        let heard = yin::pitch(&self.window[(WINDOW - PITCH_FRAMES) * FRAME..]);
        let fresh = &heard[PITCH_EDGE..heard.len() - 1];
        for &hertz in &fresh[fresh.len() - BLOCK..] {
            self.user.hear(hertz);
        }
        self.user.fade();

        let ratio = 2_f32.powf(self.shift() / 12.0);
        self.pitch.drain(..BLOCK);
        self.pitchf.drain(..BLOCK);
        self.pitch.resize(WINDOW, 1);
        self.pitchf.resize(WINDOW, 0.0);
        let start = WINDOW - fresh.len();
        for (index, &hertz) in fresh.iter().enumerate() {
            let moved = hertz * ratio;
            self.pitchf[start + index] = moved;
            self.pitch[start + index] = pitch_step(moved);
        }
    }

    /// Semitones from the user's pitch to the voice's, and the user's own on top.
    fn shift(&self) -> f32 {
        let user = self.user.median().unwrap_or(ASSUMED_USER_HZ);
        (12.0 * (self.voice_hz / user).log2() + self.semitones).clamp(-MAX_SHIFT, MAX_SHIFT)
    }

    /// The block, faded in from the end of the one before where the two sound most alike.
    fn join(&mut self, voice: &[f32]) -> Vec<f32> {
        let fade = self.tail.len();
        let offset = best_offset(&voice[..fade + SEARCH * VOICE_HOP], &self.tail);
        let mut block = voice[offset..offset + BLOCK * VOICE_HOP + fade].to_vec();
        for (index, (sample, &before)) in block.iter_mut().zip(&self.tail).enumerate() {
            let weight = (FRAC_PI_2 * index as f32 / (fade - 1) as f32).sin().powi(2);
            *sample = *sample * weight + before * (1.0 - weight);
        }
        self.tail = block.split_off(BLOCK * VOICE_HOP);
        block
    }
}

/// The channels of the noise the flow reads.
pub(crate) const FEATURES_OF_FLOW: usize = 192;
/// Until the user has talked for a moment, their pitch is taken to be between a man's and a
/// woman's.
const ASSUMED_USER_HZ: f32 = 150.0;
/// Two octaves either way is already far past any voice.
const MAX_SHIFT: f32 = 24.0;

/// Each frame of the window from HuBERT's frames, which are twice as long: each is used twice,
/// and the last once more, for the window has one frame more than HuBERT hears.
fn frames_of(features: &[f32]) -> Result<Vec<f32>, Failure> {
    let heard = features.len() / FEATURES;
    if heard == 0 || !features.len().is_multiple_of(FEATURES) {
        return Err(load_failed("the trained voice heard nothing"));
    }
    let mut frames = Vec::with_capacity(WINDOW * FEATURES);
    for frame in 0..WINDOW {
        let source = (frame / 2).min(heard - 1);
        frames.extend_from_slice(&features[source * FEATURES..(source + 1) * FEATURES]);
    }
    Ok(frames)
}

/// The 255 steps of pitch the model reads, from 50 to 1100 Hz on the mel scale; 1 is none.
pub(crate) fn pitch_step(hertz: f32) -> i64 {
    let mel = |hertz: f32| 1127.0 * (1.0 + hertz / 700.0).ln();
    let (low, high) = (mel(50.0), mel(1100.0));
    let value = mel(hertz);
    let step = if value > 0.0 {
        (value - low) * 254.0 / (high - low) + 1.0
    } else {
        value
    };
    step.clamp(1.0, 255.0).round() as i64
}

/// Where in `voice` the next block sounds most like the end of the last one, as RVC finds it:
/// the correlation of the two over the length of the fade, divided by the loudness of the voice
/// there, so a loud stretch does not win for being loud.
fn best_offset(voice: &[f32], tail: &[f32]) -> usize {
    let length = tail.len();
    let mut loudness: f32 = voice[..length].iter().map(|sample| sample * sample).sum();
    let mut best = (0, f32::MIN);
    for offset in 0..=voice.len() - length {
        if offset > 0 {
            let (gone, came) = (voice[offset - 1], voice[offset + length - 1]);
            loudness += came * came - gone * gone;
        }
        let alike: f32 = voice[offset..offset + length]
            .iter()
            .zip(tail)
            .map(|(a, b)| a * b)
            .sum();
        let score = alike / (loudness.max(0.0) + 1e-8).sqrt();
        if score > best.1 {
            best = (offset, score);
        }
    }
    best.0
}

/// The pitch the user usually talks at, from what they said lately.
struct PitchTracker {
    /// How often each half semitone from [`LOWEST_HZ`] was heard, fading with time.
    counts: [f32; BINS],
    total: f32,
}

const LOWEST_HZ: f32 = 50.0;
const BINS: usize = 108;
const BINS_PER_OCTAVE: f32 = 24.0;
/// What is left of what was heard after each block: what was said a minute ago counts a third.
const FADE: f32 = 0.995;
/// The median waits for this many voiced frames, half a second of talk.
const ENOUGH: f32 = 50.0;

impl Default for PitchTracker {
    fn default() -> Self {
        Self {
            counts: [0.0; BINS],
            total: 0.0,
        }
    }
}

impl PitchTracker {
    fn hear(&mut self, hertz: f32) {
        if hertz <= 0.0 {
            return;
        }
        let bin = (BINS_PER_OCTAVE * (hertz / LOWEST_HZ).log2()).max(0.0) as usize;
        self.counts[bin.min(BINS - 1)] += 1.0;
        self.total += 1.0;
    }

    fn fade(&mut self) {
        for count in &mut self.counts {
            *count *= FADE;
        }
        self.total *= FADE;
    }

    fn median(&self) -> Option<f32> {
        if self.total < ENOUGH {
            return None;
        }
        let mut below = 0.0;
        for (bin, &count) in self.counts.iter().enumerate() {
            below += count;
            if below >= self.total / 2.0 {
                return Some(LOWEST_HZ * 2_f32.powf((bin as f32 + 0.5) / BINS_PER_OCTAVE));
            }
        }
        None
    }
}

/// Normal random values, from a generator small enough to live here.
struct Noise {
    state: u64,
    spare: Option<f32>,
}

impl Noise {
    fn new(seed: u64) -> Self {
        Self {
            state: seed.max(1),
            spare: None,
        }
    }

    /// From 0 (left out) to 1.
    fn uniform(&mut self) -> f32 {
        // xorshift64*
        self.state ^= self.state >> 12;
        self.state ^= self.state << 25;
        self.state ^= self.state >> 27;
        let bits = self.state.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 40;
        (bits as f32 + 1.0) / (1u64 << 24) as f32
    }

    fn normal(&mut self) -> f32 {
        if let Some(spare) = self.spare.take() {
            return spare;
        }
        // Box-Muller: two uniform values make two normal ones.
        let radius = (-2.0 * self.uniform().ln()).sqrt();
        let angle = TAU * self.uniform();
        self.spare = Some(radius * angle.sin());
        radius * angle.cos()
    }
}

#[cfg(test)]
mod tests {
    use instantpad_application::message::TextKey;

    use super::*;

    /// HuBERT and the synthesizer, made up: what they were given is kept, and the voice is a
    /// tone at the pitch they were asked for, which goes on where the last block left it.
    #[derive(Default)]
    struct Fake {
        heard: Vec<usize>,
        spoken: Vec<(usize, Vec<i64>, Vec<f32>, usize)>,
        phase: f32,
    }

    impl Models for Fake {
        fn content(&mut self, speech: &[f32]) -> Result<Vec<f32>, Failure> {
            self.heard.push(speech.len());
            // HuBERT hears a frame every 20 ms after the first 25 ms.
            Ok(vec![0.5; ((speech.len() - 400) / 320 + 1) * FEATURES])
        }

        fn speak(&mut self, words: &Words) -> Result<Vec<f32>, Failure> {
            self.spoken.push((
                words.frames(),
                words.pitch.to_vec(),
                words.pitchf.to_vec(),
                words.noise.len(),
            ));
            let mut voice = Vec::new();
            for &hertz in words.pitchf {
                for _ in 0..VOICE_HOP {
                    voice.push(0.5 * self.phase.sin());
                    self.phase = (self.phase + TAU * hertz.max(1.0) / VOICE_RATE as f32) % TAU;
                }
            }
            // The block after this one starts where this one's blocks end, less the overlap.
            let overlap = (CROSSFADE + SEARCH + 1) * VOICE_HOP;
            let back: f32 = words.pitchf[words.pitchf.len() - 1].max(1.0);
            self.phase =
                (self.phase - TAU * back * overlap as f32 / VOICE_RATE as f32).rem_euclid(TAU);
            Ok(voice)
        }
    }

    /// A voice at 48 kHz with harmonics, which YIN hears at its pitch.
    fn voice(hertz: f32, seconds: f32) -> Vec<f32> {
        (0..(SPEECH_RATE as f32 * seconds) as usize)
            .map(|n| {
                let phase = TAU * hertz * n as f32 / SPEECH_RATE as f32;
                0.3 * phase.sin() + 0.15 * (2.0 * phase).sin() + 0.08 * (3.0 * phase).sin()
            })
            .collect()
    }

    fn convert_in_pieces(stream: &mut TrainedStream, speech: &[f32], fake: &mut Fake) -> Vec<f32> {
        let mut converted = Vec::new();
        for piece in speech.chunks(480) {
            converted.extend(stream.convert(piece, fake).unwrap());
        }
        converted
    }

    #[test]
    fn each_block_of_speech_comes_back_as_long_once_the_first_is_heard() {
        let mut stream = TrainedStream::new(200.0, 0.0);
        let mut fake = Fake::default();

        let converted = convert_in_pieces(&mut stream, &voice(120.0, 3.0), &mut fake);

        // Ten blocks of 300 ms, less what the resamplers hold back.
        assert_eq!(fake.heard.len(), 9);
        assert!(fake.heard.iter().all(|&length| length == WINDOW * FRAME));
        let blocks = 9 * BLOCK * VOICE_HOP * SPEECH_RATE / VOICE_RATE;
        assert!(
            converted.len().abs_diff(blocks) < 1200,
            "{}",
            converted.len()
        );
    }

    #[test]
    fn the_synthesizer_reads_the_window_and_speaks_its_end() {
        let mut stream = TrainedStream::new(200.0, 0.0);
        let mut fake = Fake::default();

        convert_in_pieces(&mut stream, &voice(120.0, 1.0), &mut fake);

        let (frames, pitch, pitchf, noise) = &fake.spoken[0];
        assert_eq!(*frames, WINDOW);
        assert_eq!(pitch.len(), WINDOW);
        assert_eq!(pitchf.len(), SPOKEN);
        assert_eq!(*noise, FEATURES_OF_FLOW * (SPOKEN + FLOW_CONTEXT));
    }

    #[test]
    fn the_user_is_moved_to_the_pitch_of_the_voice_once_they_have_talked() {
        let mut stream = TrainedStream::new(220.0, 0.0);
        let mut fake = Fake::default();

        convert_in_pieces(&mut stream, &voice(110.0, 4.0), &mut fake);

        let (_, pitch, pitchf, _) = fake.spoken.last().unwrap();
        let spoken = &pitchf[..BLOCK];
        for &hertz in spoken {
            assert!((hertz / 220.0 - 1.0).abs() < 0.03, "spoke at {hertz} Hz");
        }
        assert!(pitch[CONTEXT..WINDOW - 1].iter().all(|&step| step > 1));
    }

    #[test]
    fn the_pitch_of_the_user_moves_further_by_the_semitones_asked_without_starting_over() {
        let mut stream = TrainedStream::new(220.0, 0.0);
        let mut fake = Fake::default();
        convert_in_pieces(&mut stream, &voice(110.0, 3.0), &mut fake);

        stream.retune(12.0);
        convert_in_pieces(&mut stream, &voice(110.0, 1.0), &mut fake);

        let (_, _, pitchf, _) = fake.spoken.last().unwrap();
        assert!(
            (pitchf[BLOCK / 2] / 440.0 - 1.0).abs() < 0.03,
            "{}",
            pitchf[BLOCK / 2]
        );
        // Still the same stream: the window kept growing from where it was.
        assert_eq!(fake.heard.len(), 13);
    }

    #[test]
    fn silence_has_no_pitch_for_the_voice() {
        let mut stream = TrainedStream::new(220.0, 0.0);
        let mut fake = Fake::default();

        convert_in_pieces(&mut stream, &vec![0.0; SPEECH_RATE], &mut fake);

        let (_, pitch, pitchf, _) = fake.spoken.last().unwrap();
        assert!(pitchf.iter().all(|&hertz| hertz == 0.0));
        assert!(pitch.iter().all(|&step| step == 1));
    }

    #[test]
    fn a_tone_that_goes_on_across_blocks_comes_out_without_clicks() {
        let mut stream = TrainedStream::new(200.0, 0.0);
        let mut fake = Fake::default();

        let converted = convert_in_pieces(&mut stream, &voice(200.0, 4.0), &mut fake);

        // Once the voice is moved to its pitch, no sample jumps further than a tone allows.
        let settled = &converted[2 * SPEECH_RATE..converted.len() - 2400];
        let steepest = settled
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).abs())
            .fold(0.0, f32::max);
        let tone_step = 0.5 * TAU * 210.0 / SPEECH_RATE as f32;
        assert!(steepest < tone_step * 1.5, "a jump of {steepest}");
    }

    #[test]
    fn a_synthesizer_that_says_too_little_is_a_failure() {
        struct Short;
        impl Models for Short {
            fn content(&mut self, _: &[f32]) -> Result<Vec<f32>, Failure> {
                Ok(vec![0.0; 32 * FEATURES])
            }
            fn speak(&mut self, _: &Words) -> Result<Vec<f32>, Failure> {
                Ok(vec![0.0; 10])
            }
        }
        let mut stream = TrainedStream::new(200.0, 0.0);

        let failure = stream
            .convert(&vec![0.0; SPEECH_RATE], &mut Short)
            .unwrap_err();

        assert_eq!(
            failure,
            load_failed("the trained voice returned too little speech")
        );
        assert_ne!(failure, Failure::text(TextKey::AiTooSlow));
    }

    #[test]
    fn warming_up_runs_both_models_once_on_a_whole_window() {
        let mut fake = Fake::default();

        warm_up(&mut fake).unwrap();

        assert_eq!(fake.heard, [WINDOW * FRAME]);
        assert_eq!(fake.spoken.len(), 1);
        assert_eq!(fake.spoken[0].0, WINDOW);
        assert_eq!(fake.spoken[0].2.len(), SPOKEN);
    }

    #[test]
    fn the_steps_of_pitch_are_the_ones_rvc_reads() {
        assert_eq!(pitch_step(0.0), 1);
        assert_eq!(pitch_step(50.0), 1);
        assert_eq!(pitch_step(1100.0), 255);
        assert_eq!(pitch_step(5000.0), 255);
        // As the kit computes it: 220 Hz.
        assert_eq!(pitch_step(220.0), 60);
    }

    #[test]
    fn the_join_is_where_the_next_block_matches_the_last() {
        let tail: Vec<f32> = (0..1600).map(|n| (n as f32 * 0.05).sin()).collect();
        let mut voice = vec![0.0; 137];
        voice.extend(&tail);
        voice.resize(2000, 0.0);

        assert_eq!(best_offset(&voice, &tail), 137);
    }

    #[test]
    fn the_noise_is_normal() {
        let mut noise = Noise::new(7);
        let values: Vec<f32> = (0..20_000).map(|_| noise.normal()).collect();

        let mean = values.iter().sum::<f32>() / values.len() as f32;
        let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / values.len() as f32;
        assert!(mean.abs() < 0.03, "{mean}");
        assert!((variance - 1.0).abs() < 0.05, "{variance}");
        assert!(values.iter().all(|value| value.is_finite()));
    }

    #[test]
    fn the_usual_pitch_of_the_user_is_the_middle_of_what_they_said() {
        let mut tracker = PitchTracker::default();
        assert_eq!(tracker.median(), None);

        for _ in 0..60 {
            tracker.hear(100.0);
        }
        for _ in 0..40 {
            tracker.hear(300.0);
            tracker.hear(0.0);
        }

        let median = tracker.median().unwrap();
        assert!((median / 100.0 - 1.0).abs() < 0.03, "{median}");
    }
}
