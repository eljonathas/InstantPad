use std::io;
use std::path::{Path, PathBuf};

/// Where everything the app keeps lives.
#[derive(Clone, Debug)]
pub struct AppDataFolder {
    root: PathBuf,
    /// For what is big and belongs to this computer, which a roaming profile must not copy.
    local: PathBuf,
    sounds: PathBuf,
    previews: PathBuf,
    voice_models: PathBuf,
}

impl AppDataFolder {
    pub fn new(root: PathBuf) -> io::Result<Self> {
        Self::with_local(root.clone(), root)
    }

    fn with_local(root: PathBuf, local: PathBuf) -> io::Result<Self> {
        let sounds = root.join("sounds");
        let previews = root.join("previews");
        let voice_models = root.join("voice-models");
        std::fs::create_dir_all(&sounds)?;
        std::fs::create_dir_all(&previews)?;
        std::fs::create_dir_all(&voice_models)?;
        Ok(Self {
            root,
            local,
            sounds,
            previews,
            voice_models,
        })
    }

    /// `INSTANTPAD_DATA` keeps development runs away from the real data.
    pub fn resolve(default_root: PathBuf, default_local: PathBuf) -> io::Result<Self> {
        match std::env::var_os("INSTANTPAD_DATA") {
            Some(root) if !root.is_empty() => Self::new(PathBuf::from(root)),
            _ => Self::with_local(default_root, default_local),
        }
    }

    pub(crate) fn file(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    pub(crate) fn sounds(&self) -> &Path {
        &self.sounds
    }

    pub(crate) fn previews(&self) -> &Path {
        &self.previews
    }

    /// Where the tools that train voices are installed, and the trainings work. Made only when
    /// the user installs them. The name is short because Windows loads no library from a path
    /// longer than 260 characters, and PyTorch's go deep.
    pub fn voice_training(&self) -> PathBuf {
        self.local.join("training")
    }

    /// Where the AI voice models are downloaded to.
    pub fn voice_models(&self) -> &Path {
        &self.voice_models
    }
}
