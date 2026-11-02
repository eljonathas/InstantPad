//! The AI voices: downloads the models on demand and runs them with ONNX Runtime.

mod fingerprint;
mod openvoice;
mod pack;
mod presets;
mod resample;
mod runtime;
mod stft;
mod stream;
mod whisper;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use async_trait::async_trait;
use instantpad_application::message::Failure;
use instantpad_application::ports::{SpeechConverter, VoiceAnalyzer, VoiceModels};
use instantpad_domain::cloning::Voiceprint;
use instantpad_domain::voice::{Conversion, VoicePreset};

use crate::openvoice::Spectrogram;
use crate::resample::{SPEECH_RATE, StreamResampler, resample};
use crate::runtime::{Cached, load_failed, session};
use crate::stream::Stream;
use crate::whisper::Whisper;

// The smallest blocks of speech that 48 kHz and 22.05 kHz divide into, one way and the other.
const DOWN_BLOCK: usize = 320;
const UP_BLOCK: usize = 147;

pub struct OnnxVoices {
    folder: PathBuf,
    /// The only model that stays loaded, because the conversion runs all the time. The others
    /// serve a calibration or a clone now and then, and are loaded for each.
    tone_color: Cached,
    requested: Mutex<Option<Conversion>>,
    /// The conversion in progress, which only the thread that converts uses.
    conversion: Mutex<Option<Running>>,
}

impl OnnxVoices {
    pub fn new(folder: &Path) -> Self {
        Self {
            folder: folder.to_owned(),
            tone_color: Cached::new(folder, pack::TONE_COLOR),
            requested: Mutex::new(None),
            conversion: Mutex::new(None),
        }
    }

    /// The speakers asked for since the last time, if any.
    fn take_request(&self) -> Option<Conversion> {
        self.requested.lock().unwrap().take()
    }
}

/// Runs heavy work off the threads that serve the interface.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, Failure> + Send + 'static,
) -> Result<T, Failure> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(load_failed)?
}

#[async_trait]
impl VoiceModels for OnnxVoices {
    fn is_installed(&self) -> bool {
        pack::is_installed(&self.folder)
    }

    fn download_megabytes(&self) -> u32 {
        pack::download_megabytes(&self.folder)
    }

    async fn install(&self, progress: &(dyn Fn(u8) + Send + Sync)) -> Result<(), Failure> {
        pack::install(&self.folder, progress).await
    }
}

#[async_trait]
impl VoiceAnalyzer for OnnxVoices {
    async fn voiceprint(&self, speech: Vec<f32>) -> Result<Voiceprint, Failure> {
        let folder = self.folder.clone();
        blocking(move || {
            let speech = resample(&speech, SPEECH_RATE, openvoice::RATE);
            let mut model = session(&folder, pack::TONE_EXTRACT)?;
            let tone = openvoice::voiceprint(&mut model, &speech).map_err(load_failed)?;
            Ok(Voiceprint::new(tone))
        })
        .await
    }

    async fn transcribe(&self, speech: Vec<f32>) -> Result<String, Failure> {
        let folder = self.folder.clone();
        blocking(move || {
            let speech = resample(&speech, SPEECH_RATE, whisper::RATE);
            Whisper::load(&folder)?
                .transcribe(&speech)
                .map_err(load_failed)
        })
        .await
    }

    fn preset(&self, preset: VoicePreset) -> Voiceprint {
        let values = match preset {
            VoicePreset::Feminine => presets::FEMININE,
            VoicePreset::Masculine => presets::MASCULINE,
        };
        Voiceprint::new(values.to_vec())
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
    /// The speech at the rate of the model, which every call empties and fills again.
    heard: Vec<f32>,
}

impl Running {
    fn new(conversion: &Conversion) -> Self {
        Self {
            source: conversion.source.values().to_vec(),
            target: conversion.target.values().to_vec(),
            down: StreamResampler::new(SPEECH_RATE, openvoice::RATE, DOWN_BLOCK),
            stream: Stream::default(),
            up: StreamResampler::new(openvoice::RATE, SPEECH_RATE, UP_BLOCK),
            spectrogram: Spectrogram::new(),
            heard: Vec::new(),
        }
    }
}

impl SpeechConverter for OnnxVoices {
    /// Only notes the speakers, because the interface thread calls it and the conversion may be
    /// in the middle of a stretch. The thread that converts starts over at its next call, and
    /// that is where the model loads.
    fn start(&self, conversion: &Conversion) {
        *self.requested.lock().unwrap() = Some(conversion.clone());
    }

    fn convert(&self, speech: &[f32]) -> Result<Vec<f32>, Failure> {
        let mut conversion = self.conversion.lock().unwrap();
        if let Some(speakers) = self.take_request() {
            *conversion = Some(Running::new(&speakers));
        }
        // With nobody to turn the voice into yet, nothing comes out.
        let Some(running) = conversion.as_mut() else {
            return Ok(Vec::new());
        };

        running.heard.clear();
        running.down.push(speech, &mut running.heard);
        let converted = running.stream.convert(&running.heard, &mut |window| {
            self.tone_color.with(|session| {
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
    use instantpad_application::message::{Message, TextKey};

    use super::*;

    fn empty_folder() -> OnnxVoices {
        OnnxVoices::new(&std::env::temp_dir().join("instantpad-voice-never-installed"))
    }

    fn conversion(source: VoicePreset, target: VoicePreset) -> Conversion {
        let voices = empty_folder();
        Conversion {
            source: voices.preset(source),
            target: voices.preset(target),
        }
    }

    #[test]
    fn the_presets_are_different_voiceprints_of_the_size_the_models_use() {
        let voices = empty_folder();

        let feminine = voices.preset(VoicePreset::Feminine);
        let masculine = voices.preset(VoicePreset::Masculine);

        for preset in [&feminine, &masculine] {
            assert_eq!(preset.values().len(), 256);
            assert!((preset.similarity(preset) - 1.0).abs() < 1e-6);
        }
        assert!(!feminine.is_same_speaker(&masculine));
    }

    #[test]
    fn the_fingerprint_of_a_speech_is_its_sha256() {
        assert_eq!(
            empty_folder().fingerprint(&[]),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn analyzing_speech_with_models_that_were_not_downloaded_fails_to_load() {
        let voices = empty_folder();
        assert!(!voices.is_installed());

        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let voiceprint = runtime.block_on(voices.voiceprint(vec![0.0; 48_000]));
        let transcript = runtime.block_on(voices.transcribe(vec![0.0; 48_000]));

        for failure in [voiceprint.unwrap_err(), transcript.unwrap_err()] {
            let Failure(Message::Text { key, .. }) = failure else {
                panic!("expected a translated message");
            };
            assert_eq!(key, TextKey::AiLoadFailed);
        }
    }

    #[test]
    fn starting_only_notes_the_speakers_and_loading_fails_in_the_conversion() {
        let voices = empty_folder();
        let speakers = conversion(VoicePreset::Masculine, VoicePreset::Feminine);

        voices.start(&speakers);
        // Enough speech for one window of the model.
        let failure = voices.convert(&vec![0.0; 24_000]).unwrap_err();

        let Failure(Message::Text { key, .. }) = failure else {
            panic!("expected a translated message");
        };
        assert_eq!(key, TextKey::AiLoadFailed);
    }

    #[test]
    fn every_start_asks_the_conversion_to_start_over_even_with_the_same_speakers() {
        let voices = empty_folder();
        let first = conversion(VoicePreset::Masculine, VoicePreset::Feminine);
        let swapped = conversion(VoicePreset::Feminine, VoicePreset::Masculine);
        assert_eq!(voices.take_request(), None);

        voices.start(&first);
        assert_eq!(voices.take_request(), Some(first.clone()));
        assert_eq!(voices.take_request(), None);

        voices.start(&first);
        assert_eq!(voices.take_request(), Some(first));

        voices.start(&swapped);
        assert_eq!(voices.take_request(), Some(swapped));
    }

    #[test]
    fn converting_before_a_conversion_starts_gives_nothing() {
        assert!(empty_folder().convert(&[0.1; 480]).unwrap().is_empty());
    }
}
