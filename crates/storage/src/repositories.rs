use std::path::PathBuf;

use instantpad_application::message::Failure;
use instantpad_application::ports::{BoardRepository, FavoritesRepository, SettingsRepository};
use instantpad_application::settings::AppSettings;
use instantpad_domain::board::PadBoard;
use instantpad_domain::sounds::SoundSource;

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
