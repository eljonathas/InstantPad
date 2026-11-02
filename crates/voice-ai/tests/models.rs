//! Runs the real models. They are far too big to commit and slow to load, so this test is
//! ignored and runs by hand:
//!
//! ```text
//! INSTANTPAD_VOICE_MODELS=<folder> INSTANTPAD_VOICE_CLIPS=<folder> \
//!     cargo test -p instantpad-voice-ai --test models -- --ignored
//! ```
//!
//! `INSTANTPAD_VOICE_MODELS` is a model folder the app filled: the ONNX Runtime library and the
//! OpenVoice and Whisper files. `INSTANTPAD_VOICE_CLIPS` holds mono 48 kHz 16-bit WAV files of
//! English speech: `speaker_a1.wav` and `speaker_a2.wav` are the same person talking, and
//! `speaker_b1.wav` is someone else.

use std::path::{Path, PathBuf};

use instantpad_application::ports::{SpeechConverter, VoiceAnalyzer, VoiceModels};
use instantpad_domain::cloning::Voiceprint;
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
async fn speech_is_written_down() {
    let voices = voices();

    let text = voices.transcribe(clip("speaker_a1.wav")).await.unwrap();

    assert!(!text.trim().is_empty());
}

#[tokio::test]
#[ignore = "needs the downloaded models"]
async fn the_converted_speech_is_as_long_as_the_speech_less_the_delay() {
    let voices = voices();
    let speech = clip("speaker_a1.wav");
    voices
        .start(&Conversion {
            source: voiceprint(&voices, "speaker_a1.wav").await,
            target: voices.preset(VoicePreset::Feminine),
        })
        .unwrap();

    let mut converted = 0;
    for piece in speech.chunks(480) {
        converted += voices.convert(piece).unwrap().len();
    }

    // The conversion holds back about a quarter of a second, in blocks of 140 ms.
    let delay = speech.len() - converted;
    assert!(delay < 48_000 / 2, "{delay}");
}
