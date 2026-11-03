//! Adapters that keep the board, favorites, settings, voices, the training in progress and
//! sound files on disk.

mod data_folder;
mod json_file;
mod repositories;
mod sound_store;

pub use data_folder::AppDataFolder;
pub use repositories::{
    JsonBoardRepository, JsonFavoritesRepository, JsonSettingsRepository, JsonTrainingRepository,
    JsonVoiceLibraryRepository,
};
pub use sound_store::FolderSoundStore;
