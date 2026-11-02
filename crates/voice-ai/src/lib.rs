//! The AI voices: downloads the models on demand and runs them with ONNX Runtime.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use instantpad_application::message::Failure;
use instantpad_application::ports::{SpeechConverter, VoiceAnalyzer, VoiceModels};
use instantpad_domain::cloning::Voiceprint;
use instantpad_domain::voice::{Conversion, VoicePreset};

pub struct OnnxVoices {
    folder: PathBuf,
}

impl OnnxVoices {
    pub fn new(folder: &Path) -> Self {
        Self {
            folder: folder.to_owned(),
        }
    }
}

#[async_trait]
impl VoiceModels for OnnxVoices {
    fn is_installed(&self) -> bool {
        todo!("{:?}", self.folder)
    }

    fn download_megabytes(&self) -> u32 {
        todo!()
    }

    async fn install(&self, _progress: &(dyn Fn(u8) + Send + Sync)) -> Result<(), Failure> {
        todo!()
    }
}

#[async_trait]
impl VoiceAnalyzer for OnnxVoices {
    async fn voiceprint(&self, _speech: Vec<f32>) -> Result<Voiceprint, Failure> {
        todo!()
    }

    async fn transcribe(&self, _speech: Vec<f32>) -> Result<String, Failure> {
        todo!()
    }

    fn preset(&self, _preset: VoicePreset) -> Voiceprint {
        todo!()
    }

    fn fingerprint(&self, _speech: &[f32]) -> String {
        todo!()
    }
}

impl SpeechConverter for OnnxVoices {
    fn start(&self, _conversion: &Conversion) -> Result<(), Failure> {
        todo!()
    }

    fn convert(&self, _speech: &[f32]) -> Result<Vec<f32>, Failure> {
        todo!()
    }
}
