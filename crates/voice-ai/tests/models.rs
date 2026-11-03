//! Runs the real models. They are far too big to commit and slow to load, so this test is
//! ignored and runs by hand:
//!
//! ```text
//! INSTANTPAD_VOICE_MODELS=<folder> INSTANTPAD_VOICE_CLIPS=<folder> \
//!     cargo test -p instantpad-voice-ai --test models -- --ignored
//! ```
//!
//! `INSTANTPAD_VOICE_MODELS` is a model folder the app filled: the ONNX Runtime library and the
//! OpenVoice files. `INSTANTPAD_VOICE_CLIPS` holds mono 48 kHz 16-bit WAV files of
//! English speech: `speaker_a1.wav` and `speaker_a2.wav` are the same person talking, and
//! `speaker_b1.wav` is someone else.
//!
//! The trained voice is a model the kit in `tools/voice-training` exported, which the test
//! finds in `trained/<INSTANTPAD_TRAINED_ID>` of the model folder, with `content.onnx` in the
//! folder itself. `INSTANTPAD_TRAINED_HZ` is the pitch in its `voice.json`, and
//! `INSTANTPAD_VOICE_OUT`, when set, is where the test writes what it converted, to hear it.

use std::path::{Path, PathBuf};

use instantpad_application::ports::{SpeechConverter, VoiceAnalyzer, VoiceModels};
use instantpad_domain::cloning::{TrainedModel, Voiceprint};
use instantpad_domain::voice::{Conversion, VoicePreset};
use instantpad_voice_ai::OnnxVoices;

fn folder(variable: &str) -> PathBuf {
    PathBuf::from(std::env::var(variable).unwrap_or_else(|_| panic!("{variable} is not set")))
}

/// The samples of a 16-bit PCM WAV file, found by walking its chunks.
fn clip(name: &str) -> Vec<f32> {
    let bytes = std::fs::read(folder("INSTANTPAD_VOICE_CLIPS").join(name)).unwrap();
    let mut chunks = &bytes[12..];
    while chunks.len() >= 8 {
        let size = u32::from_le_bytes(chunks[4..8].try_into().unwrap()) as usize;
        if &chunks[..4] == b"data" {
            return chunks[8..8 + size]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|sample| f32::from(i16::from_le_bytes(*sample)) / 32768.0)
                .collect();
        }
        chunks = &chunks[8 + size..];
    }
    panic!("{name} has no audio");
}

fn voices() -> OnnxVoices {
    let voices = OnnxVoices::new(Path::new(&folder("INSTANTPAD_VOICE_MODELS")));
    assert!(voices.is_installed(), "the model folder is not complete");
    voices
}

async fn voiceprint(voices: &OnnxVoices, name: &str) -> Voiceprint {
    voices.voiceprint(clip(name)).await.unwrap()
}

#[tokio::test]
#[ignore = "needs the downloaded models"]
async fn two_clips_of_one_speaker_have_alike_voiceprints_and_another_speaker_does_not() {
    let voices = voices();

    let first = voiceprint(&voices, "speaker_a1.wav").await;
    let second = voiceprint(&voices, "speaker_a2.wav").await;
    let other = voiceprint(&voices, "speaker_b1.wav").await;

    assert_eq!(first.values().len(), 256);
    assert!(
        first.similarity(&second) > 0.8,
        "{}",
        first.similarity(&second)
    );
    assert!(first.similarity(&other) < first.similarity(&second));
}

#[tokio::test]
#[ignore = "needs the downloaded models"]
async fn the_converted_speech_is_as_long_as_the_speech_less_the_delay() {
    let voices = voices();
    let speech = clip("speaker_a1.wav");
    voices.start(&Conversion::Timbre {
        source: voiceprint(&voices, "speaker_a1.wav").await,
        target: voices.preset(VoicePreset::Feminine),
    });

    let mut converted = 0;
    for piece in speech.chunks(480) {
        converted += voices.convert(piece).unwrap().len();
    }

    // The conversion holds back about a quarter of a second, in blocks of 140 ms.
    let delay = speech.len() - converted;
    assert!(delay < 48_000 / 2, "{delay}");
}

/// Writes mono 48 kHz 16-bit speech as a WAV file.
fn write_wav(path: &Path, speech: &[f32]) {
    let data: Vec<u8> = speech
        .iter()
        .flat_map(|sample| ((sample.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())
        .collect();
    let mut bytes = Vec::new();
    bytes.extend(b"RIFF");
    bytes.extend((36 + data.len() as u32).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16_u32.to_le_bytes());
    bytes.extend(1_u16.to_le_bytes());
    bytes.extend(1_u16.to_le_bytes());
    bytes.extend(48_000_u32.to_le_bytes());
    bytes.extend((48_000_u32 * 2).to_le_bytes());
    bytes.extend(2_u16.to_le_bytes());
    bytes.extend(16_u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend((data.len() as u32).to_le_bytes());
    bytes.extend(data);
    std::fs::write(path, bytes).unwrap();
}

#[test]
#[ignore = "needs the downloaded models and a trained voice"]
fn a_trained_voice_speaks_as_fast_as_the_user_talks() {
    let voices = voices();
    let speech = clip("speaker_a1.wav");
    let pitch_hz = std::env::var("INSTANTPAD_TRAINED_HZ").map_or(200.0, |hz| hz.parse().unwrap());
    voices.start(&Conversion::Trained {
        model: TrainedModel {
            id: std::env::var("INSTANTPAD_TRAINED_ID").expect("INSTANTPAD_TRAINED_ID is not set"),
            name: "trained".to_owned(),
            credit: String::new(),
            pitch_hz,
        },
        semitones: 0.0,
    });

    let started = std::time::Instant::now();
    let mut converted = Vec::new();
    let mut slowest_after_the_first = std::time::Duration::ZERO;
    for (index, piece) in speech.chunks(480).enumerate() {
        let call = std::time::Instant::now();
        converted.extend(voices.convert(piece).unwrap());
        if index > 0 {
            slowest_after_the_first = slowest_after_the_first.max(call.elapsed());
        }
    }
    let seconds = started.elapsed().as_secs_f32();
    // The models load in the first call, which the engine does not count as falling behind;
    // no later call may take much longer than a block of 300 ms.
    eprintln!("slowest call after the first: {slowest_after_the_first:?}");
    assert!(
        slowest_after_the_first < std::time::Duration::from_millis(700),
        "{slowest_after_the_first:?}"
    );

    let talked = speech.len() as f32 / 48_000.0;
    eprintln!(
        "converted {talked:.1} s of speech in {seconds:.1} s, {:.2} of real time",
        seconds / talked
    );
    // Blocks of 300 ms, and what the resamplers hold back.
    let delay = speech.len() - converted.len();
    assert!(delay < 48_000 * 2 / 5, "{delay}");
    assert!(converted.iter().all(|sample| sample.is_finite()));
    let loudness = (converted.iter().map(|s| s * s).sum::<f32>() / converted.len() as f32).sqrt();
    assert!(loudness > 0.01, "the voice is silent: {loudness}");
    if let Ok(out) = std::env::var("INSTANTPAD_VOICE_OUT") {
        write_wav(Path::new(&out), &converted);
    }
}
