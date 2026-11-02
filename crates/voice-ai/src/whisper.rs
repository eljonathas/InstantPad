//! Whisper: writes down what was said, for the app to check the phrase the user read aloud.

use std::path::Path;

use instantpad_application::message::Failure;
use ort::session::Session;
use ort::value::{Tensor, TensorRef};
use serde_json::Value;

use crate::pack::{TOKENIZER, WHISPER_DECODER, WHISPER_ENCODER};
use crate::runtime::{load_failed, session};
use crate::stft::Stft;

/// The rate the model works at.
pub(crate) const RATE: usize = 16_000;
/// The model always hears 30 seconds, padded with silence or cut short.
const SAMPLES: usize = 30 * RATE;
const FRAME: usize = 400;
const HOP: usize = 160;
const PAD: usize = FRAME / 2;
const BINS: usize = FRAME / 2 + 1;
const MELS: usize = 80;
const FRAMES: usize = SAMPLES / HOP;
/// What the encoder hands to the decoder: 1500 positions of 512 values.
const HIDDEN: [usize; 3] = [1, 1500, 512];
/// More than a phrase of digits takes.
const MAX_TOKENS: usize = 40;

const END: i64 = 50257;
const START: i64 = 50258;
const ENGLISH: i64 = 50259;
const TRANSCRIBE: i64 = 50359;
const NO_TIMESTAMPS: i64 = 50363;
const PORTUGUESE: i64 = 50267;

pub(crate) struct Whisper {
    encoder: Session,
    decoder: Session,
    tokens: Tokens,
    mel: LogMel,
}

impl Whisper {
    pub(crate) fn load(folder: &Path) -> Result<Self, Failure> {
        let file = folder.join(TOKENIZER);
        let tokenizer = std::fs::read(&file).map_err(load_failed)?;
        // A tokenizer that cannot be read is not there as far as the app knows, so the
        // interface offers the download again.
        let tokens = Tokens::parse(&tokenizer).inspect_err(|_| {
            let _ = std::fs::remove_file(&file);
        })?;
        Ok(Self {
            encoder: session(folder, WHISPER_ENCODER)?,
            decoder: session(folder, WHISPER_DECODER)?,
            tokens,
            mel: LogMel::new(),
        })
    }

    /// What was said in `speech`, which is mono at 16 kHz.
    pub(crate) fn transcribe(&mut self, speech: &[f32]) -> ort::Result<String> {
        let features = self.mel.features(speech);
        let outputs = self.encoder.run(ort::inputs![
            "input_features" => Tensor::from_array(([1, MELS, FRAMES], features))?,
        ])?;
        let (_, hidden) = outputs["last_hidden_state"].try_extract_tensor::<f32>()?;
        let hidden = hidden.to_vec();
        drop(outputs);

        // The language is whichever of the two the decoder expects after the start.
        let first = self.logits(&[START], &hidden)?;
        let language = if first[PORTUGUESE as usize] >= first[ENGLISH as usize] {
            PORTUGUESE
        } else {
            ENGLISH
        };

        let mut ids = vec![START, language, TRANSCRIBE, NO_TIMESTAMPS];
        let prompt = ids.len();
        for _ in 0..MAX_TOKENS {
            let logits = self.logits(&ids, &hidden)?;
            // Only words and the end are allowed: the ids past them are languages and times.
            let next = most_likely(&logits[..=END as usize]);
            if next == END {
                break;
            }
            ids.push(next);
        }
        Ok(self.tokens.text(&ids[prompt..]))
    }

    /// What the decoder expects after `ids`, which it reads whole every time.
    fn logits(&mut self, ids: &[i64], hidden: &[f32]) -> ort::Result<Vec<f32>> {
        let outputs = self.decoder.run(ort::inputs![
            "input_ids" => Tensor::from_array(([1, ids.len()], ids.to_vec()))?,
            "encoder_hidden_states" => TensorRef::from_array_view((HIDDEN, hidden))?,
        ])?;
        let (_, logits) = outputs["logits"].try_extract_tensor::<f32>()?;
        let words = logits.len() / ids.len();
        Ok(logits[logits.len() - words..].to_vec())
    }
}

fn most_likely(logits: &[f32]) -> i64 {
    let mut best = 0;
    for (id, logit) in logits.iter().enumerate() {
        if *logit > logits[best] {
            best = id;
        }
    }
    best as i64
}

/// The speech as the encoder hears it: a mel spectrogram on a logarithmic scale.
struct LogMel {
    stft: Stft,
    filters: Vec<MelFilter>,
}

/// A triangle on the mel scale, which is zero outside the bins it spans.
struct MelFilter {
    first: usize,
    weights: Vec<f32>,
}

impl LogMel {
    fn new() -> Self {
        Self {
            stft: Stft::new(FRAME, HOP, PAD),
            filters: mel_filters(),
        }
    }

    /// One row per mel, as long as the speech or 30 seconds.
    fn features(&mut self, speech: &[f32]) -> Vec<f32> {
        let mut padded = speech[..speech.len().min(SAMPLES)].to_vec();
        padded.resize(SAMPLES, 0.0);

        let mut mels = vec![0.0; MELS * FRAMES];
        let mut power = [0.0; BINS];
        self.stft.run(&padded, |index, spectrum| {
            // The last frame has nothing the encoder looks at.
            if index == FRAMES {
                return;
            }
            for (power, bin) in power.iter_mut().zip(spectrum) {
                *power = bin.norm_sqr();
            }
            for (mel, filter) in self.filters.iter().enumerate() {
                let energy: f32 = filter
                    .weights
                    .iter()
                    .zip(&power[filter.first..])
                    .map(|(weight, power)| weight * power)
                    .sum();
                mels[mel * FRAMES + index] = energy.max(1e-10).log10();
            }
        });

        // Anything more than 80 dB under the loudest sound is silence.
        let floor = mels.iter().copied().fold(f32::MIN, f32::max) - 8.0;
        for mel in &mut mels {
            *mel = (mel.max(floor) + 4.0) / 4.0;
        }
        mels
    }
}

/// The Slaney scale: linear below 1 kHz and logarithmic above, with each filter scaled to
/// the same area.
fn mel_filters() -> Vec<MelFilter> {
    let log_step = 6.4_f64.ln() / 27.0;
    let to_mel = |hertz: f64| {
        if hertz < 1000.0 {
            3.0 * hertz / 200.0
        } else {
            15.0 + (hertz / 1000.0).ln() / log_step
        }
    };
    let to_hertz = |mel: f64| {
        if mel < 15.0 {
            200.0 * mel / 3.0
        } else {
            1000.0 * (log_step * (mel - 15.0)).exp()
        }
    };

    let nyquist = RATE as f64 / 2.0;
    let edges: Vec<f64> = (0..MELS + 2)
        .map(|n| to_hertz(to_mel(nyquist) * n as f64 / (MELS + 1) as f64))
        .collect();
    let bin_hertz: Vec<f64> = (0..BINS)
        .map(|bin| nyquist * bin as f64 / (BINS - 1) as f64)
        .collect();

    (0..MELS)
        .map(|mel| {
            let (low, middle, high) = (edges[mel], edges[mel + 1], edges[mel + 2]);
            let weights: Vec<f64> = bin_hertz
                .iter()
                .map(|&hertz| {
                    let rising = (hertz - low) / (middle - low);
                    let falling = (high - hertz) / (high - middle);
                    rising.min(falling).max(0.0) * 2.0 / (high - low)
                })
                .collect();
            let first = weights.iter().position(|&weight| weight > 0.0).unwrap_or(0);
            let end = weights
                .iter()
                .rposition(|&weight| weight > 0.0)
                .map_or(0, |last| last + 1);
            MelFilter {
                first,
                weights: weights[first..end]
                    .iter()
                    .map(|&weight| weight as f32)
                    .collect(),
            }
        })
        .collect()
}

/// The text of each token, by id.
struct Tokens(Vec<String>);

impl Tokens {
    fn parse(tokenizer: &[u8]) -> Result<Self, Failure> {
        let tokenizer: Value = serde_json::from_slice(tokenizer).map_err(load_failed)?;
        let vocabulary = tokenizer["model"]["vocab"]
            .as_object()
            .ok_or_else(|| load_failed("the tokenizer has no vocabulary"))?;

        let mut tokens = vec![String::new(); vocabulary.len()];
        for (token, id) in vocabulary {
            let slot = id.as_u64().and_then(|id| tokens.get_mut(id as usize));
            *slot.ok_or_else(|| load_failed("the tokenizer has a gap in its vocabulary"))? =
                token.clone();
        }
        Ok(Self(tokens))
    }

    fn text(&self, ids: &[i64]) -> String {
        let bytes: Vec<u8> = ids
            .iter()
            .filter_map(|&id| self.0.get(id as usize))
            .flat_map(|token| token.chars().filter_map(byte_of))
            .collect();
        String::from_utf8_lossy(&bytes).trim().to_owned()
    }
}

/// The tokens spell bytes with printable characters: the bytes that print as themselves
/// stay, and the others (controls, space and a few more) take the characters from U+0100.
fn byte_of(character: char) -> Option<u8> {
    let prints = |byte: u8| matches!(byte, b'!'..=b'~' | 0xA1..=0xAC | 0xAE..=0xFF);
    let code = u32::from(character);
    match u8::try_from(code) {
        Ok(byte) if prints(byte) => Some(byte),
        _ => (0..=u8::MAX)
            .filter(|&byte| !prints(byte))
            .nth(code.checked_sub(256)? as usize),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn speech() -> Vec<f32> {
        (0..16_000_u32)
            .map(|n| {
                let tone = |hertz: f32, level: f32| {
                    level * (std::f32::consts::TAU * hertz * n as f32 / RATE as f32).sin()
                };
                let noise = 0.1 * ((n * 7919 % 1000) as f32 / 500.0 - 1.0);
                tone(440.0, 0.3) + tone(3000.0, 0.2) + noise
            })
            .collect()
    }

    #[test]
    fn the_log_mel_of_a_second_of_sound_matches_the_reference() {
        let features = LogMel::new().features(&speech());

        assert_eq!(features.len(), MELS * FRAMES);
        for (mel, frame, expected) in [
            (0, 0, 0.8929979),
            (5, 0, 0.9624219),
            (14, 5, -0.2189959),
            (20, 50, 0.3109434),
            (40, 50, 0.3915877),
            (60, 50, 0.2522765),
            (79, 50, 0.4192431),
            (30, 99, 0.1878634),
            (30, 100, 0.5375741),
            (30, 101, -0.0775290),
            (30, 120, -0.6688046),
            (10, 2999, -0.6688046),
        ] {
            let value = features[mel * FRAMES + frame];
            assert!((value - expected).abs() < 2e-3, "{mel} {frame}: {value}");
        }
    }

    #[test]
    fn speech_longer_than_thirty_seconds_is_cut_short() {
        let features = LogMel::new().features(&vec![0.1; SAMPLES + 5000]);

        assert_eq!(features.len(), MELS * FRAMES);
    }

    #[test]
    fn every_mel_filter_rises_to_one_peak_and_falls() {
        for (mel, filter) in mel_filters().iter().enumerate() {
            let peak = filter
                .weights
                .iter()
                .enumerate()
                .fold(0, |best, (index, weight)| {
                    if *weight > filter.weights[best] {
                        index
                    } else {
                        best
                    }
                });
            assert!(filter.weights.iter().all(|&weight| weight > 0.0), "{mel}");
            assert!(
                filter.weights[..=peak]
                    .windows(2)
                    .all(|pair| pair[0] <= pair[1]),
                "{mel}"
            );
            assert!(
                filter.weights[peak..]
                    .windows(2)
                    .all(|pair| pair[0] >= pair[1]),
                "{mel}"
            );
        }
    }

    #[test]
    fn the_mel_filters_climb_the_spectrum_in_order() {
        let filters = mel_filters();

        assert_eq!(filters.len(), MELS);
        for pair in filters.windows(2) {
            assert!(pair[0].first <= pair[1].first);
        }
        let top = &filters[MELS - 1];
        assert_eq!(top.first + top.weights.len(), BINS - 1);
    }

    #[test]
    fn each_mel_filter_covers_the_same_area() {
        let bin_hertz = RATE as f32 / FRAME as f32;

        for (mel, filter) in mel_filters().iter().enumerate() {
            let area: f32 = filter.weights.iter().sum::<f32>() * bin_hertz;
            assert!((area - 1.0).abs() < 0.15, "{mel}: {area}");
        }
    }

    #[test]
    fn the_first_filter_matches_the_reference() {
        let filters = mel_filters();

        assert_eq!(filters[0].first, 1);
        assert!((filters[0].weights[0] - 0.02486259).abs() < 1e-6);
    }

    #[test]
    fn tokens_spell_bytes_with_printable_characters() {
        assert_eq!(byte_of('a'), Some(b'a'));
        assert_eq!(byte_of('Ġ'), Some(b' '));
        assert_eq!(byte_of('Ċ'), Some(b'\n'));
        assert_eq!(byte_of('Ã'), Some(0xC3));
        assert_eq!(byte_of('ġ'), Some(0x7F));
        assert_eq!(byte_of('ł'), Some(0xA0));
        assert_eq!(byte_of('世'), None);
    }

    #[test]
    fn tokens_decode_to_text_and_ignore_what_is_not_a_word() {
        let tokenizer = r#"{"model": {"vocab": {"Ġol": 0, "Ã¡": 1, "!": 2}}}"#;
        let tokens = Tokens::parse(tokenizer.as_bytes()).unwrap();

        assert_eq!(tokens.text(&[0, 1, 2]), "olá!");
        assert_eq!(tokens.text(&[1, 99]), "á");
    }

    #[test]
    fn a_tokenizer_without_a_vocabulary_is_refused() {
        let failure = Tokens::parse(b"{}").err().unwrap();

        assert_eq!(failure, load_failed("the tokenizer has no vocabulary"));
    }

    #[test]
    fn the_most_likely_token_is_the_first_of_the_highest() {
        assert_eq!(most_likely(&[0.1, 0.9, 0.9, -1.0]), 1);
    }
}
