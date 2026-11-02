//! ONNX Runtime, loaded from the library the model pack brings and started on first use.

use std::num::NonZero;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::thread;

use instantpad_application::message::{Failure, TextKey};
use ort::environment::Environment;
use ort::session::Session;

use crate::pack;

/// The runtime lives once per process, and it cannot be started again after it was stopped.
static ENVIRONMENT: Mutex<Option<Environment>> = Mutex::new(None);

/// Something loaded from the model folder when it is first needed, and kept.
pub(crate) struct Lazy<T> {
    folder: PathBuf,
    load: fn(&Path) -> Result<T, Failure>,
    loaded: Mutex<Option<T>>,
}

impl<T> Lazy<T> {
    pub(crate) fn new(folder: &Path, load: fn(&Path) -> Result<T, Failure>) -> Self {
        Self {
            folder: folder.to_owned(),
            load,
            loaded: Mutex::new(None),
        }
    }

    /// Runs `work` with exclusive use of the value, which a first call loads.
    pub(crate) fn with<R>(
        &self,
        work: impl FnOnce(&mut T) -> ort::Result<R>,
    ) -> Result<R, Failure> {
        let mut loaded = self.loaded.lock().unwrap();
        if loaded.is_none() {
            *loaded = Some((self.load)(&self.folder)?);
        }
        let value = loaded.as_mut().expect("the value was just loaded");
        work(value).map_err(|error| Failure::because(TextKey::AiLoadFailed, error))
    }
}

/// A model of the pack, ready to run.
pub(crate) fn session(folder: &Path, model: &str) -> Result<Session, Failure> {
    let failed =
        |detail: &dyn ToString| Failure::because(TextKey::AiLoadFailed, detail.to_string());
    let library = pack::library(folder).ok_or_else(|| failed(&pack::unsupported()))?;
    let environment = environment(&library)?;
    open(&environment, &folder.join(model)).map_err(|error| failed(&error))
}

fn open(environment: &Environment, model: &Path) -> ort::Result<Session> {
    Session::builder(environment)?
        .with_intra_threads(threads())?
        .commit_from_file(model)
}

fn environment(library: &Path) -> Result<Environment, Failure> {
    let failed =
        |detail: &dyn ToString| Failure::because(TextKey::AiLoadFailed, detail.to_string());
    let mut started = ENVIRONMENT.lock().unwrap();
    if let Some(environment) = started.as_ref() {
        return Ok(environment.clone());
    }
    let environment = ort::init_from(library)
        .map_err(|error| failed(&error))?
        .with_name("instantpad")
        .with_telemetry(false)
        .build()
        .map_err(|error| failed(&error))?;
    *started = Some(environment.clone());
    Ok(environment)
}

/// The cores the models may use, leaving one for the audio and the game.
fn threads() -> usize {
    let cores = thread::available_parallelism().map_or(1, NonZero::get);
    (cores - 1).clamp(1, 4)
}
