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

    /// Gives the sound a permanent file, reusing the one it may have from being tried out.
    pub async fn ensure_downloaded(&self, source: &SoundSource) -> Result<(), Failure> {
        if self.files.exists(&source.path) {
            return Ok(());
        }
        let url = source
            .sound_url
            .as_deref()
            .ok_or(Failure::text(TextKey::SoundFileMissing))?;

        let preview = self.files.path_for_preview(url);
        if self.files.exists(&preview) {
            return self.files.keep_preview(&preview, &source.path);
        }
        self.catalog.download(url, &source.path).await
    }

    /// A file to listen to without keeping it: the permanent one if the sound has it, or else
    /// a temporary one. Only `ensure_downloaded` makes a sound stay on disk.
    pub async fn file_to_hear(&self, source: &SoundSource) -> Result<String, Failure> {
        if self.files.exists(&source.path) {
            return Ok(source.path.clone());
        }
        let url = source
            .sound_url
            .as_deref()
            .ok_or(Failure::text(TextKey::SoundFileMissing))?;

        let preview = self.files.path_for_preview(url);
        if !self.files.exists(&preview) {
            self.catalog.download(url, &preview).await?;
        }
        Ok(preview)
    }

    pub fn clear_previews(&self) {
        self.files.clear_previews();
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
