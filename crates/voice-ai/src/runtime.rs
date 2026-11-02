//! ONNX Runtime, loaded from the library the model pack brings and started on first use.

use std::num::NonZero;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::thread;

use instantpad_application::message::{Failure, TextKey};
use ort::environment::Environment;
use ort::error::ErrorCode;
use ort::session::Session;

use crate::pack;

/// The runtime lives once per process, and it cannot be started again after it was stopped.
static ENVIRONMENT: Mutex<Option<Environment>> = Mutex::new(None);

pub(crate) fn load_failed(detail: impl ToString) -> Failure {
    Failure::because(TextKey::AiLoadFailed, detail)
}

/// A model that stays loaded after its first use, for what runs all the time.
pub(crate) struct Cached {
    folder: PathBuf,
    model: &'static str,
    session: Mutex<Option<Session>>,
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
        let mut loaded = self.session.lock().unwrap();
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
    let environment = environment(&library)?;
    let file = folder.join(model);
    open(&environment, &file).map_err(|error| {
        // A model that cannot be read is not there as far as the app knows, so the interface
        // offers the download again.
        if matches!(
            error.code(),
            ErrorCode::InvalidProtobuf | ErrorCode::InvalidGraph | ErrorCode::NoModel
        ) {
            let _ = std::fs::remove_file(&file);
        }
        load_failed(error)
    })
}

fn open(environment: &Environment, model: &Path) -> ort::Result<Session> {
    Session::builder(environment)?
        .with_intra_threads(threads())?
        .commit_from_file(model)
}

fn environment(library: &Path) -> Result<Environment, Failure> {
    let mut started = ENVIRONMENT.lock().unwrap();
    if let Some(environment) = started.as_ref() {
        return Ok(environment.clone());
    }
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
