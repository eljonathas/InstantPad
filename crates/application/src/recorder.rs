use std::sync::{Arc, Mutex};

use crate::message::Failure;
use crate::ports::VoiceRecorder;

/// Who is using the microphone recorder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Recording {
    Calibration,
    Cloning,
    Training,
}

/// The one recorder, shared by the learning of the user's voice, cloning and training. Only one of them records at a time, and
/// only the one that started a recording can stop it.
pub struct SharedRecorder {
    recorder: Arc<dyn VoiceRecorder>,
    owner: Mutex<Option<Recording>>,
}

impl SharedRecorder {
    pub fn new(recorder: Arc<dyn VoiceRecorder>) -> Self {
        Self {
            recorder,
            owner: Mutex::new(None),
        }
    }

    /// Returns false when the other one is recording.
    pub(crate) fn start(&self, owner: Recording) -> bool {
        let mut current = self.owner.lock().expect("recorder lock poisoned");
        if current.is_some_and(|current| current != owner) {
            return false;
        }
        *current = Some(owner);
        self.recorder.start_recording();
        true
    }

    /// Mono at 48 kHz. Empty when `owner` was not recording.
    pub(crate) fn stop(&self, owner: Recording) -> Vec<f32> {
        let mut current = self.owner.lock().expect("recorder lock poisoned");
        if *current != Some(owner) {
            return Vec::new();
        }
        *current = None;
        self.recorder.stop_recording()
    }

    /// What `owner` has recorded so far. Empty when it was not recording.
    pub(crate) fn peek(&self, owner: Recording) -> Vec<f32> {
        let current = self.owner.lock().expect("recorder lock poisoned");
        if *current != Some(owner) {
            return Vec::new();
        }
        self.recorder.recorded()
    }

    pub(crate) async fn decode_file(&self, path: &str) -> Result<Vec<f32>, Failure> {
        self.recorder.decode_file(path).await
    }
}
