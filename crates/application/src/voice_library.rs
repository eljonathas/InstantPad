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

    /// Changes the library and saves it under one lock, so no other change slips in between.
    /// A failed save undoes the change: what is not on disk must not be used either.
    pub fn change<R>(&self, change: impl FnOnce(&mut VoiceLibrary) -> R) -> Result<R, Failure> {
        let mut library = self.lock();
        let before = library.clone();
        let result = change(&mut library);
        if *library != before
            && let Err(failure) = self.repository.save(&library)
        {
            *library = before;
            return Err(failure);
        }
        Ok(result)
    }
}
