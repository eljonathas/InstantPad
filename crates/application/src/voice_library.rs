use std::sync::{Arc, Mutex, MutexGuard};

use instantpad_domain::cloning::VoiceLibrary;

use crate::message::Failure;
use crate::ports::VoiceLibraryRepository;

/// The voices the AI knows, in memory, shared by the voice and cloning use cases.
pub struct VoiceLibraryStore {
    current: Mutex<VoiceLibrary>,
    repository: Arc<dyn VoiceLibraryRepository>,
}

impl VoiceLibraryStore {
    pub fn load(repository: Arc<dyn VoiceLibraryRepository>) -> Self {
        Self {
            current: Mutex::new(repository.load()),
            repository,
        }
    }

    pub fn lock(&self) -> MutexGuard<'_, VoiceLibrary> {
        self.current.lock().expect("voice library lock poisoned")
    }

    pub fn save(&self) -> Result<(), Failure> {
        self.repository.save(&self.lock())
    }
}
