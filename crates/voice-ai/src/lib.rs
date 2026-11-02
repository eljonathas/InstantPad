//! The AI voices: downloads the models on demand and runs them with ONNX Runtime.

mod feminine;
mod fingerprint;
mod openvoice;
mod pack;
mod resample;
mod runtime;
mod stft;
mod stream;
mod whisper;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use instantpad_application::message::{Failure, TextKey};
use instantpad_application::ports::{SpeechConverter, VoiceAnalyzer, VoiceModels};
use instantpad_domain::cloning::Voiceprint;
use instantpad_domain::voice::{Conversion, VoicePreset};
use ort::session::Session;

use crate::openvoice::Spectrogram;
use crate::resample::{SPEECH_RATE, StreamResampler, resample};
use crate::runtime::Lazy;
use crate::stream::Stream;
use crate::whisper::Whisper;

/// The smallest blocks of speech that 48 kHz and 22.05 kHz divide into, one way and the other.
const DOWN_BLOCK: usize = 320;
const UP_BLOCK: usize = 147;

pub struct OnnxVoices {
    folder: PathBuf,
    models: Arc<Models>,
    /// The conversion in progress, which `start` begins.
    conversion: Mutex<Option<Running>>,
}

/// The models, each loaded when it is first needed.
struct Models {
    tone_extract: Lazy<Session>,
    tone_color: Lazy<Session>,
    whisper: Lazy<Whisper>,
}

impl OnnxVoices {
    pub fn new(folder: &Path) -> Self {
        Self {
            folder: folder.to_owned(),
            models: Arc::new(Models {
                tone_extract: Lazy::new(folder, |folder| {
                    runtime::session(folder, pack::TONE_EXTRACT)
                }),
                tone_color: Lazy::new(folder, |folder| runtime::session(folder, pack::TONE_COLOR)),
                whisper: Lazy::new(folder, Whisper::load),
            }),
            conversion: Mutex::new(None),
        }
    }
}

/// Runs heavy work off the threads that serve the interface.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, Failure> + Send + 'static,
) -> Result<T, Failure> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| Failure::because(TextKey::AiLoadFailed, error))?
}

#[async_trait]
impl VoiceModels for OnnxVoices {
    fn is_installed(&self) -> bool {
        pack::is_installed(&self.folder)
    }

    fn download_megabytes(&self) -> u32 {
        pack::download_megabytes()
    }

    async fn install(&self, progress: &(dyn Fn(u8) + Send + Sync)) -> Result<(), Failure> {
        pack::install(&self.folder, progress).await
    }
}

#[async_trait]
impl VoiceAnalyzer for OnnxVoices {
    async fn voiceprint(&self, speech: Vec<f32>) -> Result<Voiceprint, Failure> {
        let models = self.models.clone();
        blocking(move || {
            let speech = resample(&speech, SPEECH_RATE, openvoice::RATE);
            let tone = models
                .tone_extract
                .with(|session| openvoice::voiceprint(session, &speech))?;
            Ok(Voiceprint::new(tone))
        })
        .await
    }

    async fn transcribe(&self, speech: Vec<f32>) -> Result<String, Failure> {
        let models = self.models.clone();
        blocking(move || {
            let speech = resample(&speech, SPEECH_RATE, whisper::RATE);
            models.whisper.with(|whisper| whisper.transcribe(&speech))
        })
        .await
    }

    fn preset(&self, preset: VoicePreset) -> Voiceprint {
        match preset {
            VoicePreset::Feminine => Voiceprint::new(feminine::VOICEPRINT.to_vec()),
        }
    }

    fn fingerprint(&self, speech: &[f32]) -> String {
        fingerprint::of_speech(speech)
    }
}

/// What a conversion keeps between one stretch of speech and the next.
struct Running {
    source: Vec<f32>,
    target: Vec<f32>,
    down: StreamResampler,
    stream: Stream,
    up: StreamResampler,
    spectrogram: Spectrogram,
    /// Speech at the rate of the model, waiting to be converted.
    heard: Vec<f32>,
}

impl SpeechConverter for OnnxVoices {
    fn start(&self, conversion: &Conversion) -> Result<(), Failure> {
        // Loading takes a while, which is better spent here than in the middle of the speech.
        self.models.tone_color.with(|_| Ok(()))?;
        *self.conversion.lock().unwrap() = Some(Running {
            source: conversion.source.values().to_vec(),
            target: conversion.target.values().to_vec(),
            down: StreamResampler::new(SPEECH_RATE, openvoice::RATE, DOWN_BLOCK),
            stream: Stream::default(),
            up: StreamResampler::new(openvoice::RATE, SPEECH_RATE, UP_BLOCK),
            spectrogram: Spectrogram::new(),
            heard: Vec::new(),
        });
        Ok(())
    }

    fn convert(&self, speech: &[f32]) -> Result<Vec<f32>, Failure> {
        let mut conversion = self.conversion.lock().unwrap();
        // With nobody to turn the voice into yet, nothing comes out.
        let Some(running) = conversion.as_mut() else {
            return Ok(Vec::new());
        };

        running.heard.clear();
        running.down.push(speech, &mut running.heard);
        let converted = running.stream.convert(&running.heard, &mut |window| {
            self.models.tone_color.with(|session| {
                openvoice::convert(
                    session,
                    &mut running.spectrogram,
                    window,
                    &running.source,
                    &running.target,
                )
            })
        })?;

        let mut converted_speech = Vec::new();
        running.up.push(&converted, &mut converted_speech);
        Ok(converted_speech)
    }
}

#[cfg(test)]
mod tests {
    use instantpad_application::message::Message;

    use super::*;

    fn empty_folder() -> OnnxVoices {
        OnnxVoices::new(&std::env::temp_dir().join("instantpad-voice-never-installed"))
    }

    #[test]
    fn the_feminine_preset_is_a_voiceprint_of_the_size_the_models_use() {
        let preset = empty_folder().preset(VoicePreset::Feminine);

        assert_eq!(preset.values().len(), 256);
        assert!((preset.similarity(&preset) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn the_fingerprint_of_a_speech_is_its_sha256() {
        assert_eq!(
            empty_folder().fingerprint(&[]),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn models_that_were_not_downloaded_are_not_installed_and_do_not_load() {
        let voices = empty_folder();

        assert!(!voices.is_installed());
        let Failure(Message::Text { key, .. }) = voices
            .start(&Conversion {
                source: voices.preset(VoicePreset::Feminine),
                target: voices.preset(VoicePreset::Feminine),
            })
            .unwrap_err()
        else {
            panic!("expected a translated message");
        };
        assert_eq!(key, TextKey::AiLoadFailed);
    }

    #[tokio::test]
    async fn analyzing_speech_with_models_that_were_not_downloaded_fails_to_load() {
        let voices = empty_folder();

        let voiceprint = voices.voiceprint(vec![0.0; 48_000]).await.unwrap_err();
        let transcript = voices.transcribe(vec![0.0; 48_000]).await.unwrap_err();

        for failure in [voiceprint, transcript] {
            let Failure(Message::Text { key, .. }) = failure else {
                panic!("expected a translated message");
            };
            assert_eq!(key, TextKey::AiLoadFailed);
        }
    }

    #[test]
    fn converting_before_a_conversion_starts_gives_nothing() {
        assert!(empty_folder().convert(&[0.1; 480]).unwrap().is_empty());
    }
}
