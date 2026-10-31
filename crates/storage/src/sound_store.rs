use std::path::{Path, PathBuf};

use instantpad_application::message::{Failure, TextKey};
use instantpad_application::ports::SoundFileStore;
use instantpad_domain::sounds::safe_file_name;
use percent_encoding::percent_decode_str;

use crate::data_folder::AppDataFolder;

pub struct FolderSoundStore {
    folder: PathBuf,
}

impl FolderSoundStore {
    pub fn new(folder: &AppDataFolder) -> Self {
        Self {
            folder: folder.sounds().to_path_buf(),
        }
    }

    fn stored_path(&self, name: &str) -> PathBuf {
        self.folder.join(safe_file_name(name))
    }
}

impl SoundFileStore for FolderSoundStore {
    fn path_for_download(&self, sound_url: &str) -> String {
        let last = sound_url.rsplit('/').next().unwrap_or_default();
        let name = percent_decode_str(last).decode_utf8_lossy();
        self.stored_path(&name).to_string_lossy().into_owned()
    }

    fn import(&self, source_path: &str) -> Result<String, Failure> {
        let source = Path::new(source_path);
        let name = source.file_name().ok_or_else(|| {
            Failure::because(TextKey::SoundCopyFailed, "the path has no file name")
        })?;
        let destination = self.stored_path(&name.to_string_lossy());
        let destination_text = destination.to_string_lossy().into_owned();
        // Copying a file onto itself would truncate it.
        if !destination_text.eq_ignore_ascii_case(source_path) {
            std::fs::copy(source, &destination)
                .map_err(|error| Failure::because(TextKey::SoundCopyFailed, error))?;
        }
        Ok(destination_text)
    }

    fn export(&self, stored_path: &str, destination: &str) -> Result<(), Failure> {
        std::fs::copy(stored_path, destination)
            .map(drop)
            .map_err(|error| Failure::because(TextKey::SoundSaveFailed, error))
    }

    fn exists(&self, path: &str) -> bool {
        Path::new(path).is_file()
    }
}
