use std::sync::Arc;

use instantpad_domain::sounds::SoundSource;
use serde::Serialize;
use specta::Type;

use crate::message::{Failure, TextKey};
use crate::ports::{CatalogPage, CatalogSound, SoundCatalog, SoundFileStore};

#[derive(Clone, Debug, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SoundPage {
    pub sounds: Vec<SoundSource>,
    pub has_more: bool,
}

/// Turns catalog results into sounds with a local file behind them.
pub struct SoundLibrary {
    catalog: Arc<dyn SoundCatalog>,
    files: Arc<dyn SoundFileStore>,
}

impl SoundLibrary {
    pub fn new(catalog: Arc<dyn SoundCatalog>, files: Arc<dyn SoundFileStore>) -> Self {
        Self { catalog, files }
    }

    pub async fn search(&self, query: &str, page: u32) -> Result<SoundPage, Failure> {
        Ok(self.to_page(self.catalog.search(query, page).await?))
    }

    pub async fn trending(&self, page: u32) -> Result<SoundPage, Failure> {
        Ok(self.to_page(self.catalog.trending(page).await?))
    }

    pub async fn ensure_downloaded(&self, source: &SoundSource) -> Result<(), Failure> {
        if self.files.exists(&source.path) {
            return Ok(());
        }
        let url = source
            .sound_url
            .as_deref()
            .ok_or(Failure::text(TextKey::SoundFileMissing))?;
        self.catalog.download(url, &source.path).await
    }

    /// Saves the audio file on the user's computer, outside the app.
    pub async fn save_to_computer(
        &self,
        source: &SoundSource,
        destination: &str,
    ) -> Result<(), Failure> {
        self.ensure_downloaded(source).await?;
        self.files.export(&source.path, destination)
    }

    fn to_page(&self, page: CatalogPage) -> SoundPage {
        SoundPage {
            sounds: page
                .sounds
                .into_iter()
                .map(|sound| self.to_source(sound))
                .collect(),
            has_more: page.has_more,
        }
    }

    fn to_source(&self, sound: CatalogSound) -> SoundSource {
        SoundSource {
            name: sound.name,
            color: sound.color,
            path: self.files.path_for_download(&sound.sound_url),
            sound_url: Some(sound.sound_url),
        }
    }
}
