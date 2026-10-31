use std::io;
use std::path::{Path, PathBuf};

/// Where everything the app keeps lives.
#[derive(Clone, Debug)]
pub struct AppDataFolder {
    root: PathBuf,
    sounds: PathBuf,
}

impl AppDataFolder {
    pub fn new(root: PathBuf) -> io::Result<Self> {
        let sounds = root.join("sounds");
        std::fs::create_dir_all(&sounds)?;
        Ok(Self { root, sounds })
    }

    /// `INSTANTPAD_DATA` keeps development runs away from the real data.
    pub fn resolve(default_root: PathBuf) -> io::Result<Self> {
        match std::env::var_os("INSTANTPAD_DATA") {
            Some(root) if !root.is_empty() => Self::new(PathBuf::from(root)),
            _ => Self::new(default_root),
        }
    }

    pub(crate) fn file(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    pub(crate) fn sounds(&self) -> &Path {
        &self.sounds
    }
}
