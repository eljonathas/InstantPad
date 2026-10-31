use std::path::{Path, PathBuf};

use instantpad_application::message::{Failure, TextKey};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// A missing file is a first run. A damaged one is kept as `.bak` so the next save
/// does not destroy what the user might still recover.
pub fn load<T: DeserializeOwned + Default>(path: &Path) -> T {
    let Ok(text) = std::fs::read_to_string(path) else {
        return T::default();
    };
    serde_json::from_str(&text).unwrap_or_else(|_| {
        // If the backup fails too, the app still has to start with the default.
        let _ = std::fs::copy(path, appended(path, "bak"));
        T::default()
    })
}

/// Writes next to the target and renames, so a crash never leaves half a document.
pub fn save<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<(), Failure> {
    let write = || -> std::io::Result<()> {
        let part = appended(path, "part");
        std::fs::write(&part, serde_json::to_vec_pretty(value)?)?;
        std::fs::rename(&part, path)
    };
    write().map_err(|error| Failure::because(TextKey::SaveFailed, error))
}

fn appended(path: &Path, extension: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".");
    name.push(extension);
    PathBuf::from(name)
}
