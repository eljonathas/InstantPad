//! The AI voices: downloads the models on demand and runs them with ONNX Runtime.

mod openvoice;
mod pack;
mod presets;
mod resample;
mod runtime;
mod rvc;
mod stft;
mod stream;
mod trained;
mod yin;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use async_trait::async_trait;
use instantpad_application::message::{Failure, TextKey};
use instantpad_application::ports::{
    SpeechConverter, TrainedVoiceModels, VoiceAnalyzer, VoiceModels,
};
use instantpad_application::sync::lock;
use instantpad_domain::cloning::{TrainedModel, Voiceprint};
use instantpad_domain::voice::{Conversion, VoicePreset};
use ort::session::Session;
use ort::value::{Tensor, TensorRef};

use crate::openvoice::Spectrogram;
use crate::resample::{SPEECH_RATE, StreamResampler, resample};
use crate::runtime::{Cached, load_failed, session};
use crate::rvc::{FEATURES, FEATURES_OF_FLOW, Models, TrainedStream, Words};
use crate::stream::Stream;

// The smallest blocks of speech that 48 kHz and 22.05 kHz divide into, one way and the other.
const DOWN_BLOCK: usize = 320;
const UP_BLOCK: usize = 147;

pub struct OnnxVoices {
    folder: PathBuf,
    /// Stays loaded while a conversion of the timbre runs, because it runs all the time, and is
    /// freed when another kind of conversion starts. The one that hears a voice serves a
    /// calibration or a clone now and then, and is loaded for each.
    tone_color: Cached,
    /// What the trained voices hear with, which stays loaded while one of them speaks and is
    /// freed when a conversion of the timbre starts.
    content: Cached,
    requested: Mutex<Option<Conversion>>,
    /// A new pitch for the trained voice that speaks, asked for since the last time.
    retuned: Mutex<Option<f32>>,
    /// The conversion in progress, which only the thread that converts uses.
    conversion: Mutex<Option<Running>>,
}

impl OnnxVoices {
    pub fn new(folder: &Path) -> Self {
        pack::remove_retired(folder);
        Self {
            folder: folder.to_owned(),
            tone_color: Cached::new(folder, pack::TONE_COLOR),
            content: Cached::new(folder, rvc::CONTENT),
            requested: Mutex::new(None),
            retuned: Mutex::new(None),
            conversion: Mutex::new(None),
        }
    }

    /// The speakers asked for since the last time, if any.
    fn take_request(&self) -> Option<Conversion> {
        lock(&self.requested).take()
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

    fn preset(&self, preset: VoicePreset) -> Voiceprint {
        let values = match preset {
            VoicePreset::Feminine => presets::FEMININE,
            VoicePreset::Masculine => presets::MASCULINE,
        };
        Voiceprint::new(values.to_vec())
    }
}

#[async_trait]
impl TrainedVoiceModels for OnnxVoices {
    async fn add(&self, folder: PathBuf, id: String) -> Result<TrainedModel, Failure> {
        let models = self.folder.clone();
        blocking(move || {
            let model = trained::add(&models, &folder, &id)?;
            // Loading it is the only way to know the runtime can run it.
            if let Err(failure) = session(&models, &synthesizer_path(&model)) {
                trained::remove(&models, &id);
                return Err(failure);
            }
            Ok(model)
        })
        .await
    }

    fn remove(&self, id: &str) {
        trained::remove(&self.folder, id);
    }
}

/// What a conversion keeps between one stretch of speech and the next.
/// Both kinds keep resamplers far larger than the rest, so they are boxed.
enum Running {
    Timbre(Box<Timbre>),
    Trained {
        stream: Box<TrainedStream>,
        synthesizer: Session,
    },
}

impl Running {
    /// Loads what the conversion needs that is not loaded yet.
    fn start(folder: &Path, conversion: &Conversion) -> Result<Self, Failure> {
        Ok(match conversion {
            Conversion::Timbre { source, target } => {
                Self::Timbre(Box::new(Timbre::new(source, target)))
            }
            Conversion::Trained { model, semitones } => {
                // The model comes from the settings file, which anyone can edit.
                if !trained::is_plain_id(&model.id) {
                    return Err(Failure::because(
                        TextKey::TrainedModelInvalid,
                        "not a name the app made",
                    ));
                }
                Self::Trained {
                    stream: Box::new(TrainedStream::new(model.pitch_hz, *semitones)),
                    synthesizer: session(folder, &synthesizer_path(model))?,
                }
            }
        })
    }
}

/// The synthesizer of a trained model, from the model folder.
fn synthesizer_path(model: &TrainedModel) -> String {
    format!("{}/{}/{}", rvc::TRAINED, model.id, rvc::SYNTHESIZER)
}

/// What a conversion of the timbre keeps.
struct Timbre {
    source: Vec<f32>,
    target: Vec<f32>,
    down: StreamResampler,
    stream: Stream,
    up: StreamResampler,
    spectrogram: Spectrogram,
    /// The speech at the rate of the model, which every call empties and fills again.
    heard: Vec<f32>,
}

impl Timbre {
    fn new(source: &Voiceprint, target: &Voiceprint) -> Self {
        Self {
            source: source.values().to_vec(),
            target: target.values().to_vec(),
            down: StreamResampler::new(SPEECH_RATE, openvoice::RATE, DOWN_BLOCK),
            stream: Stream::default(),
            up: StreamResampler::new(openvoice::RATE, SPEECH_RATE, UP_BLOCK),
            spectrogram: Spectrogram::new(),
            heard: Vec::new(),
        }
    }

    fn convert(&mut self, tone_color: &Cached, speech: &[f32]) -> Result<Vec<f32>, Failure> {
        self.heard.clear();
        self.down.push(speech, &mut self.heard);
        let converted = self.stream.convert(&self.heard, &mut |window| {
            tone_color.with(|session| {
                openvoice::convert(
                    session,
                    &mut self.spectrogram,
                    window,
                    &self.source,
                    &self.target,
                )
            })
        })?;

        let mut converted_speech = Vec::new();
        self.up.push(&converted, &mut converted_speech);
        Ok(converted_speech)
    }
}

/// The models of a trained voice as ONNX Runtime runs them.
struct TrainedModels<'a> {
    content: &'a Cached,
    synthesizer: &'a mut Session,
}

impl Models for TrainedModels<'_> {
    fn content(&mut self, speech: &[f32]) -> Result<Vec<f32>, Failure> {
        self.content.with(|session| {
            let outputs = session.run(ort::inputs![
                "audio" => TensorRef::from_array_view(([1, speech.len()], speech))?,
            ])?;
            let (_, features) = outputs["features"].try_extract_tensor::<f32>()?;
            Ok(features.to_vec())
        })
    }

    fn speak(&mut self, words: &Words) -> Result<Vec<f32>, Failure> {
        let frames = words.frames();
        let spoken = words.pitchf.len();
        let flow = words.noise.len() / FEATURES_OF_FLOW;
        let mut run = || -> ort::Result<Vec<f32>> {
            let outputs = self.synthesizer.run(ort::inputs![
                "phone" => TensorRef::from_array_view(([1, frames, FEATURES], words.phone))?,
                "phone_lengths" => Tensor::from_array(([1], vec![frames as i64]))?,
                "pitch" => TensorRef::from_array_view(([1, frames], words.pitch))?,
                "pitchf" => TensorRef::from_array_view(([1, spoken], words.pitchf))?,
                "sid" => Tensor::from_array(([1], vec![0_i64]))?,
                "noise" => TensorRef::from_array_view(([1, FEATURES_OF_FLOW, flow], words.noise))?,
            ])?;
            let (_, audio) = outputs["audio"].try_extract_tensor::<f32>()?;
            Ok(audio.to_vec())
        };
        run().map_err(load_failed)
    }
}

impl SpeechConverter for OnnxVoices {
    /// Only notes the speakers, because the interface thread calls it and the conversion may be
    /// in the middle of a stretch. The thread that converts starts over at its next call, and
    /// that is where the model loads.
    fn start(&self, conversion: &Conversion) {
        // Cleared first, so a conversion that starts in between never takes the pitch of the
        // one before.
        *lock(&self.retuned) = None;
        *lock(&self.requested) = Some(conversion.clone());
    }

    fn retune(&self, semitones: f32) {
        *lock(&self.retuned) = Some(semitones);
    }

    fn convert(&self, speech: &[f32]) -> Result<Vec<f32>, Failure> {
        let mut conversion = lock(&self.conversion);
        if let Some(speakers) = self.take_request() {
            *conversion = None;
            // What the new conversion does not use is hundreds of megabytes it would keep.
            match speakers {
                Conversion::Timbre { .. } => self.content.unload(),
                Conversion::Trained { .. } => self.tone_color.unload(),
            }
            let mut running = Running::start(&self.folder, &speakers)?;
            // Both models load and run once now, in the first call of the conversion, which
            // may take long. Loading HuBERT at the first block instead, once the conversion
            // counts as warm, would pile up a second of voice and be taken for falling behind.
            if let Running::Trained { synthesizer, .. } = &mut running {
                rvc::warm_up(&mut TrainedModels {
                    content: &self.content,
                    synthesizer,
                })?;
            }
            *conversion = Some(running);
        }
        let retuned = lock(&self.retuned).take();
        // With nobody to turn the voice into yet, nothing comes out.
        match conversion.as_mut() {
            None => Ok(Vec::new()),
            Some(Running::Timbre(timbre)) => timbre.convert(&self.tone_color, speech),
            Some(Running::Trained {
                stream,
                synthesizer,
            }) => {
                if let Some(semitones) = retuned {
                    stream.retune(semitones);
                }
                let mut models = TrainedModels {
                    content: &self.content,
                    synthesizer,
                };
                stream.convert(speech, &mut models)
            }
        }
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
        Conversion::Timbre {
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
        assert!(feminine.similarity(&masculine) < 0.8);
    }

    #[test]
    fn hearing_a_voice_with_models_that_were_not_downloaded_fails_to_load() {
        let voices = empty_folder();
        assert!(!voices.is_installed());

        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let failure = runtime
            .block_on(voices.voiceprint(vec![0.0; 48_000]))
            .unwrap_err();

        let Failure(Message::Text { key, .. }) = failure else {
            panic!("expected a translated message");
        };
        assert_eq!(key, TextKey::AiLoadFailed);
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

    #[test]
    fn a_trained_model_with_an_id_that_leads_out_of_the_models_is_not_loaded() {
        let model = TrainedModel {
            id: "../../elsewhere".to_owned(),
            name: "Voice".to_owned(),
            credit: String::new(),
            pitch_hz: 150.0,
        };
        let conversion = Conversion::Trained {
            model,
            semitones: 0.0,
        };

        let failure = Running::start(Path::new("/nowhere"), &conversion)
            .err()
            .unwrap();

        let Failure(Message::Text { key, .. }) = failure else {
            panic!("expected a translated message");
        };
        assert_eq!(key, TextKey::TrainedModelInvalid);
    }
}
