use std::path::PathBuf;

use instantpad_application::message::Failure;
use instantpad_application::ports::{
    BoardRepository, FavoritesRepository, SettingsRepository, TrainingRepository,
    VoiceLibraryRepository,
};
use instantpad_application::settings::AppSettings;
use instantpad_domain::board::PadBoard;
use instantpad_domain::cloning::VoiceLibrary;
use instantpad_domain::sounds::SoundSource;
use instantpad_domain::training::TrainingJob;

use crate::data_folder::AppDataFolder;
use crate::json_file;

pub struct JsonBoardRepository(PathBuf);

impl JsonBoardRepository {
    pub fn new(folder: &AppDataFolder) -> Self {
        Self(folder.file("board.json"))
    }
}

impl BoardRepository for JsonBoardRepository {
    fn load(&self) -> PadBoard {
        json_file::load(&self.0)
    }

    fn save(&self, board: &PadBoard) -> Result<(), Failure> {
        json_file::save(&self.0, board)
    }
}

pub struct JsonFavoritesRepository(PathBuf);

impl JsonFavoritesRepository {
    pub fn new(folder: &AppDataFolder) -> Self {
        Self(folder.file("favorites.json"))
    }
}

impl FavoritesRepository for JsonFavoritesRepository {
    fn load(&self) -> Vec<SoundSource> {
        json_file::load(&self.0)
    }

    fn save(&self, favorites: &[SoundSource]) -> Result<(), Failure> {
        json_file::save(&self.0, favorites)
    }
}

pub struct JsonSettingsRepository(PathBuf);

impl JsonSettingsRepository {
    pub fn new(folder: &AppDataFolder) -> Self {
        Self(folder.file("settings.json"))
    }
}

impl SettingsRepository for JsonSettingsRepository {
    fn load(&self) -> AppSettings {
        json_file::load(&self.0)
    }

    fn save(&self, settings: &AppSettings) -> Result<(), Failure> {
        json_file::save(&self.0, settings)
    }
}

/// The user's voiceprint and the cloned voices. No recording is ever kept here.
pub struct JsonVoiceLibraryRepository(PathBuf);

impl JsonVoiceLibraryRepository {
    pub fn new(folder: &AppDataFolder) -> Self {
        Self(folder.file("voices.json"))
    }
}

impl VoiceLibraryRepository for JsonVoiceLibraryRepository {
    fn load(&self) -> VoiceLibrary {
        json_file::load(&self.0)
    }

    fn save(&self, library: &VoiceLibrary) -> Result<(), Failure> {
        json_file::save(&self.0, library)
    }
}

/// The training in progress, if there is one. Its takes are files of the training's own.
pub struct JsonTrainingRepository(PathBuf);

impl JsonTrainingRepository {
    pub fn new(folder: &AppDataFolder) -> Self {
        Self(folder.file("training.json"))
    }
}

impl TrainingRepository for JsonTrainingRepository {
    fn load(&self) -> Option<TrainingJob> {
        json_file::load(&self.0)
    }

    fn save(&self, job: Option<&TrainingJob>) -> Result<(), Failure> {
        json_file::save(&self.0, &job)
    }
}
