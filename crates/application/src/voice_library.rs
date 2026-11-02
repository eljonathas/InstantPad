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
    /// Voices cloned under older terms are deleted for good as the library loads.
    pub fn load(repository: Arc<dyn VoiceLibraryRepository>) -> Self {
        let mut library = repository.load();
        // They are gone from memory either way, and a failed save tries again at the next start.
        if library.drop_outdated() {
            let _ = repository.save(&library);
        }
        Self {
            current: Mutex::new(library),
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
