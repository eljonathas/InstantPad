use std::path::{Path, PathBuf};

use instantpad_application::message::{Failure, TextKey};
use instantpad_application::ports::SoundFileStore;
use instantpad_domain::sounds::safe_file_name;
use percent_encoding::percent_decode_str;

use crate::data_folder::AppDataFolder;

pub struct FolderSoundStore {
    folder: PathBuf,
    previews: PathBuf,
}

impl FolderSoundStore {
    pub fn new(folder: &AppDataFolder) -> Self {
        Self {
            folder: folder.sounds().to_path_buf(),
            previews: folder.previews().to_path_buf(),
        }
    }

    fn stored_path(&self, name: &str) -> PathBuf {
        self.folder.join(disk_name(name))
    }

    /// `path` itself if nothing is there, or else the first of "name (2)", "name (3)"... that
    /// is free, so an import never replaces a sound that pads still use.
    fn free_name(path: PathBuf) -> PathBuf {
        let mut candidate = path.clone();
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        let extension = path.extension().map(|e| e.to_string_lossy());
        let mut copy = 2;
        while candidate.exists() {
            let name = match &extension {
                Some(extension) => format!("{stem} ({copy}).{extension}"),
                None => format!("{stem} ({copy})"),
            };
            candidate = path.with_file_name(name);
            copy += 1;
        }
        candidate
    }
}

/// The last segment of the URL path, as a name that is safe on disk.
fn file_name_of(sound_url: &str) -> String {
    // The query and the fragment are not part of the name. An encoded `?` still is.
    let path = sound_url.split(['?', '#']).next().unwrap_or_default();
    let last = path.rsplit('/').next().unwrap_or_default();
    disk_name(&percent_decode_str(last).decode_utf8_lossy())
}

/// A name that Windows accepts as well: no forbidden letters, no empty name, no dots-only
/// name, no trailing dot or space, and none of its device names such as `CON` or `NUL`.
fn disk_name(name: &str) -> String {
    let safe = safe_file_name(name);
    let trimmed = safe.trim_end_matches(['.', ' ']);
    if trimmed.is_empty() {
        return "sound".to_owned();
    }
    let device = trimmed
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let is_device = matches!(device.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|prefix| {
            device
                .strip_prefix(prefix)
                .is_some_and(|number| matches!(number.as_bytes(), [b'1'..=b'9']))
        });
    if is_device {
        format!("_{trimmed}")
    } else {
        trimmed.to_owned()
    }
}

fn same_file(first: &Path, second: &Path) -> bool {
    matches!(
        (first.canonicalize(), second.canonicalize()),
        (Ok(first), Ok(second)) if first == second
    )
}

impl SoundFileStore for FolderSoundStore {
    fn path_for_download(&self, sound_url: &str) -> String {
        self.stored_path(&file_name_of(sound_url))
            .to_string_lossy()
            .into_owned()
    }

    fn path_for_preview(&self, sound_url: &str) -> String {
        self.previews
            .join(file_name_of(sound_url))
            .to_string_lossy()
            .into_owned()
    }

    fn keep_preview(&self, preview_path: &str, stored_path: &str) -> Result<(), Failure> {
        std::fs::rename(preview_path, stored_path)
            .map_err(|error| Failure::because(TextKey::SoundCopyFailed, error))
    }

    /// Whatever cannot be deleted now goes on the next clean-up.
    fn clear_previews(&self) {
        let Ok(entries) = std::fs::read_dir(&self.previews) else {
            return;
        };
        for entry in entries.flatten() {
            let _ = std::fs::remove_file(entry.path());
        }
    }

    fn import(&self, source_path: &str) -> Result<String, Failure> {
        let source = Path::new(source_path);
        let name = source.file_name().ok_or_else(|| {
            Failure::because(TextKey::SoundCopyFailed, "the path has no file name")
        })?;
        let destination = self.stored_path(&name.to_string_lossy());
        // Copying a file onto itself would truncate it.
        if same_file(source, &destination) {
            return Ok(destination.to_string_lossy().into_owned());
        }
        let destination = Self::free_name(destination);
        std::fs::copy(source, &destination)
            .map_err(|error| Failure::because(TextKey::SoundCopyFailed, error))?;
        Ok(destination.to_string_lossy().into_owned())
    }

    fn export(&self, stored_path: &str, destination: &str) -> Result<(), Failure> {
        std::fs::copy(stored_path, destination)
            .map(drop)
            .map_err(|error| Failure::because(TextKey::SoundSaveFailed, error))
    }

    fn delete(&self, stored_path: &str) {
        let path = Path::new(stored_path);
        if path.parent() == Some(self.folder.as_path()) {
            // Whatever cannot be deleted now only takes a little space.
            let _ = std::fs::remove_file(path);
        }
    }

    fn exists(&self, path: &str) -> bool {
        Path::new(path).is_file()
    }

    fn is_stored(&self, path: &str) -> bool {
        Path::new(path).parent() == Some(self.folder.as_path())
    }
}
