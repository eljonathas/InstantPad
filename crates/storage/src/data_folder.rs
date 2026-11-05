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
        std::fs::create_dir_all(&sounds)?;
        std::fs::create_dir_all(&previews)?;
        std::fs::create_dir_all(&local)?;
        let voice_models = voice_models_folder(&root, &local);
        std::fs::create_dir_all(&voice_models)?;
        Ok(Self {
            root,
            local,
            sounds,
            previews,
            voice_models,
        })
    }

    /// `INSTANTPAD_DATA` keeps development runs away from the real data. Only a debug build
    /// honours it: a release build is the user's, and must not be moved to another folder by a
    /// variable that something else set.
    pub fn resolve(default_root: PathBuf, default_local: PathBuf) -> io::Result<Self> {
        #[cfg(debug_assertions)]
        if let Some(root) = std::env::var_os("INSTANTPAD_DATA").filter(|root| !root.is_empty()) {
            return Self::new(PathBuf::from(root));
        }
        Self::with_local(default_root, default_local)
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

/// The models are big, and a native library among them is loaded by the app, so they belong in
/// the local folder, which a roaming profile does not copy (see ADR 0007). Models downloaded
/// before that are moved there once. A folder that cannot be renamed (another volume, a file in
/// use) is left where it is and still used, since losing hundreds of megabytes the user cannot
/// always download again is worse than keeping them in the old place.
fn voice_models_folder(root: &Path, local: &Path) -> PathBuf {
    let old = root.join("voice-models");
    let new = local.join("voice-models");
    if old == new || !old.is_dir() {
        return new;
    }
    // An empty folder from an earlier start would stop the rename on Windows.
    let _ = std::fs::remove_dir(&new);
    if new.exists() {
        // Models already live in the new place, so the old ones are not in the way.
        return new;
    }
    match std::fs::rename(&old, &new) {
        Ok(()) => new,
        Err(_) => old,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folders() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("roaming");
        let local = base.path().join("local");
        std::fs::create_dir_all(&root).unwrap();
        (base, root, local)
    }

    #[test]
    fn the_voice_models_live_in_the_local_folder() {
        let (_base, root, local) = folders();
        let folder = AppDataFolder::with_local(root, local.clone()).unwrap();
        assert_eq!(folder.voice_models(), local.join("voice-models"));
    }

    #[test]
    fn models_downloaded_before_move_to_the_local_folder() {
        let (_base, root, local) = folders();
        std::fs::create_dir_all(root.join("voice-models")).unwrap();
        std::fs::write(root.join("voice-models/tone_color.onnx"), b"model").unwrap();
        let folder = AppDataFolder::with_local(root.clone(), local.clone()).unwrap();
        assert_eq!(folder.voice_models(), local.join("voice-models"));
        assert!(local.join("voice-models/tone_color.onnx").is_file());
        assert!(!root.join("voice-models").exists());
    }

    #[test]
    fn old_models_are_kept_when_the_new_folder_already_has_models() {
        let (_base, root, local) = folders();
        std::fs::create_dir_all(root.join("voice-models")).unwrap();
        std::fs::write(root.join("voice-models/old.onnx"), b"old").unwrap();
        std::fs::create_dir_all(local.join("voice-models")).unwrap();
        std::fs::write(local.join("voice-models/new.onnx"), b"new").unwrap();
        let folder = AppDataFolder::with_local(root.clone(), local.clone()).unwrap();
        assert_eq!(folder.voice_models(), local.join("voice-models"));
        assert!(root.join("voice-models/old.onnx").is_file());
        assert!(local.join("voice-models/new.onnx").is_file());
    }
}
