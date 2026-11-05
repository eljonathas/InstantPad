use std::sync::{Arc, Mutex};

use instantpad_domain::sounds::SoundSource;

use crate::library::SoundLibrary;
use crate::message::Failure;
use crate::ports::FavoritesRepository;
use crate::sync::lock;

/// Favorites live apart from the pads, so they survive when a pad or interface is removed.
pub struct Favorites {
    repository: Arc<dyn FavoritesRepository>,
    library: Arc<SoundLibrary>,
    favorites: Mutex<Vec<SoundSource>>,
    /// Makes saves follow each other, so the disk always ends with the latest list.
    saving: Mutex<()>,
}

impl Favorites {
    pub fn new(repository: Arc<dyn FavoritesRepository>, library: Arc<SoundLibrary>) -> Self {
        Self {
            favorites: Mutex::new(repository.load()),
            repository,
            library,
            saving: Mutex::new(()),
        }
    }

    pub fn list(&self) -> Vec<SoundSource> {
        self.lock().clone()
    }

    pub fn is_favorite(&self, path: &str) -> bool {
        self.lock()
            .iter()
            .any(|favorite| favorite.is_same_sound_as(path))
    }

    /// Returns true when the sound is a favorite after the call.
    pub async fn toggle(&self, source: &SoundSource) -> Result<bool, Failure> {
        let source = &self.library.trusted(source)?;
        let add = !self.is_favorite(&source.path);
        if add {
            // Saved locally so a favorite survives the sound leaving the site.
            self.library.ensure_downloaded(source).await?;
        }

        {
            // The download above let other clicks in, so the list is checked again here.
            let mut favorites = self.lock();
            favorites.retain(|favorite| !favorite.is_same_sound_as(&source.path));
            if add {
                favorites.push(source.clone());
            }
        }
        // The list is written outside its lock, because the board view reads it.
        let _saving = lock(&self.saving);
        self.repository.save(&self.list())?;
        Ok(add)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<SoundSource>> {
        lock(&self.favorites)
    }
}
