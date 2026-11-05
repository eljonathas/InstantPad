//! ONNX Runtime, loaded from the library the model pack brings and started on first use.

use std::num::NonZero;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::thread;

use instantpad_application::message::{Failure, TextKey};
use instantpad_application::sync::lock;
use ort::environment::Environment;
use ort::error::ErrorCode;
use ort::session::Session;

use crate::pack;

/// The runtime lives once per process, and it cannot be started again after it was stopped.
static ENVIRONMENT: Mutex<Option<Environment>> = Mutex::new(None);

pub(crate) fn load_failed(detail: impl ToString) -> Failure {
    Failure::because(TextKey::AiLoadFailed, detail)
}

/// A model that stays loaded after its first use, for what runs all the time, until it is
/// unloaded. It is generic only so that tests can stand in for the session.
pub(crate) struct Cached<T = Session> {
    folder: PathBuf,
    model: &'static str,
    session: Mutex<Option<T>>,
}

impl<T> Cached<T> {
    /// Frees the model, which the next use loads again. For when what speaks no longer needs it.
    pub(crate) fn unload(&self) {
        *lock(&self.session) = None;
    }

    #[cfg(test)]
    fn is_loaded(&self) -> bool {
        lock(&self.session).is_some()
    }
}

impl Cached {
    pub(crate) fn new(folder: &Path, model: &'static str) -> Self {
        Self {
            folder: folder.to_owned(),
            model,
            session: Mutex::new(None),
        }
    }

    /// Runs `work` with exclusive use of the model, which a first call loads.
    pub(crate) fn with<R>(
        &self,
        work: impl FnOnce(&mut Session) -> ort::Result<R>,
    ) -> Result<R, Failure> {
        let mut loaded = lock(&self.session);
        let session = match loaded.take() {
            Some(session) => session,
            None => session(&self.folder, self.model)?,
        };
        work(loaded.insert(session)).map_err(load_failed)
    }
}

/// A model of the pack, ready to run.
pub(crate) fn session(folder: &Path, model: &str) -> Result<Session, Failure> {
    let library = pack::library(folder).map_err(load_failed)?;
    let environment = environment(folder, &library)?;
    let file = folder.join(model);
    open(&environment, &file).map_err(|error| {
        if matches!(
            error.code(),
            ErrorCode::InvalidProtobuf | ErrorCode::InvalidGraph | ErrorCode::NoModel
        ) {
            forget_unreadable(folder, model);
        }
        load_failed(error)
    })
}

/// A model of the pack that cannot be read is not there as far as the app knows, so the
/// interface offers the download again. Any other model stays: a trained voice and HuBERT have
/// no download, and a graph the runtime cannot run may only be newer than it, not damaged.
fn forget_unreadable(folder: &Path, model: &str) {
    if pack::is_pack_file(model) {
        let _ = std::fs::remove_file(folder.join(model));
    }
}

fn open(environment: &Environment, model: &Path) -> ort::Result<Session> {
    Session::builder(environment)?
        .with_intra_threads(threads())?
        .commit_from_file(model)
}

fn environment(folder: &Path, library: &Path) -> Result<Environment, Failure> {
    let mut started = lock(&ENVIRONMENT);
    if let Some(environment) = started.as_ref() {
        return Ok(environment.clone());
    }
    // Once per process, before the library is loaded: it is code, and only its size was checked
    // at every start.
    pack::verify_library(folder).map_err(load_failed)?;
    let environment = ort::init_from(library)
        .map_err(load_failed)?
        .with_name("instantpad")
        .with_telemetry(false)
        .build()
        .map_err(load_failed)?;
    *started = Some(environment.clone());
    Ok(environment)
}

/// The cores the models may use, leaving one for the audio and the game.
fn threads() -> usize {
    let cores = thread::available_parallelism().map_or(1, NonZero::get);
    (cores - 1).clamp(1, 4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unreadable_trained_voice_or_hubert_is_left_on_disk() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(folder.path().join("trained/0a1b")).unwrap();
        for model in [
            "content.onnx",
            "trained/0a1b/synthesizer.onnx",
            "tone_color.onnx",
        ] {
            std::fs::write(folder.path().join(model), b"model").unwrap();
            forget_unreadable(folder.path(), model);
        }

        assert!(folder.path().join("content.onnx").exists());
        assert!(folder.path().join("trained/0a1b/synthesizer.onnx").exists());
        assert!(!folder.path().join("tone_color.onnx").exists());
    }

    #[test]
    fn unloading_frees_the_model_and_the_next_use_loads_it_again() {
        let cached = Cached::<String> {
            folder: PathBuf::new(),
            model: "model.onnx",
            session: Mutex::new(Some("loaded".to_owned())),
        };
        assert!(cached.is_loaded());

        cached.unload();

        assert!(!cached.is_loaded());
    }
}
