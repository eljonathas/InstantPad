use std::path::Path;

use serde::{Deserialize, Serialize};
use specta::Type;

const INVALID_FILE_NAME_CHARS: &str = "<>:\"/\\|?*";

/// A sound that is not on a pad yet: a catalog result or a favorite.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SoundSource {
    pub name: String,
    pub color: String,
    /// Where the audio file lives, or will live once downloaded. Also identifies the sound.
    pub path: String,
    /// `None` for files the user imported.
    pub sound_url: Option<String>,
}

impl SoundSource {
    pub fn is_same_sound_as(&self, path: &str) -> bool {
        self.path.to_lowercase() == path.to_lowercase()
    }

    /// A readable file name for saving the sound outside the app.
    pub fn suggested_file_name(&self) -> String {
        let extension = Path::new(&self.path)
            .extension()
            .map(|extension| format!(".{}", extension.to_string_lossy()))
            .unwrap_or_default();
        format!("{}{extension}", safe_file_name(&self.name).trim())
    }
}

/// Replaces what Windows refuses in a file name, which is also safe on the other systems.
pub fn safe_file_name(name: &str) -> String {
    name.chars()
        .map(|letter| {
            if letter.is_control() || INVALID_FILE_NAME_CHARS.contains(letter) {
                '_'
            } else {
                letter
            }
        })
        .collect()
}
