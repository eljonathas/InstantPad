use std::sync::{Arc, Mutex};

use instantpad_domain::sounds::SoundSource;

use crate::library::SoundLibrary;
use crate::message::Failure;
use crate::ports::FavoritesRepository;

/// Favorites live apart from the pads, so they survive when a pad or interface is removed.
pub struct Favorites {
    repository: Arc<dyn FavoritesRepository>,
    library: Arc<SoundLibrary>,
    favorites: Mutex<Vec<SoundSource>>,
}

impl Favorites {
    pub fn new(repository: Arc<dyn FavoritesRepository>, library: Arc<SoundLibrary>) -> Self {
        Self {
            favorites: Mutex::new(repository.load()),
            repository,
            library,
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
        let add = !self.is_favorite(&source.path);
        if add {
            // Saved locally so a favorite survives the sound leaving the site.
            self.library.ensure_downloaded(source).await?;
        }

        let mut favorites = self.lock();
        if add {
            favorites.push(source.clone());
        } else {
            favorites.retain(|favorite| !favorite.is_same_sound_as(&source.path));
        }
        self.repository.save(&favorites)?;
        Ok(add)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<SoundSource>> {
        self.favorites.lock().expect("favorites lock poisoned")
    }
}
