//! Finds, downloads and applies signed releases through the Tauri updater.

use std::sync::Mutex;

use async_trait::async_trait;
use instantpad_application::message::{Failure, TextKey};
use instantpad_application::ports::AppUpdater;
use tauri::AppHandle;
use tauri_plugin_updater::{Update, UpdaterExt};

pub struct TauriUpdater {
    app: AppHandle,
    found: Mutex<Option<Update>>,
    downloaded: Mutex<Option<(Update, Vec<u8>)>>,
}

impl TauriUpdater {
    pub fn new(app: AppHandle) -> Self {
        Self {
            app,
            found: Mutex::default(),
            downloaded: Mutex::default(),
        }
    }
}

/// The portable copy is named so on purpose: nothing installed it, so nothing can replace it.
fn is_portable() -> bool {
    std::env::current_exe()
        .is_ok_and(|path| path.to_string_lossy().to_lowercase().contains("portable"))
}

#[async_trait]
impl AppUpdater for TauriUpdater {
    fn current_version(&self) -> String {
        self.app.package_info().version.to_string()
    }

    fn is_release_build(&self) -> bool {
        !cfg!(debug_assertions) && !is_portable()
    }

    /// A failed check is not worth a message: the app looks again the next time it opens.
    async fn find_update(&self) -> Result<Option<String>, Failure> {
        let Ok(updater) = self.app.updater() else {
            return Ok(None);
        };
        let update = updater.check().await.ok().flatten();
        let version = update.as_ref().map(|update| update.version.clone());
        *self.found.lock().expect("updater lock poisoned") = update;
        Ok(version)
    }

    async fn download(&self, progress: &(dyn Fn(u8) + Send + Sync)) -> Result<(), Failure> {
        let found = self.found.lock().expect("updater lock poisoned").take();
        let Some(update) = found else {
            return Ok(());
        };

        let mut received = 0;
        let report = |chunk: usize, total: Option<u64>| {
            received += chunk as u64;
            if let Some(total) = total.filter(|&total| total > 0) {
                progress((received * 100 / total) as u8);
            }
        };
        let bytes = update
            .download(report, || {})
            .await
            .map_err(|_| Failure::text(TextKey::UpdateDownloadFailed))?;

        *self.downloaded.lock().expect("updater lock poisoned") = Some((update, bytes));
        Ok(())
    }

    fn apply(&self, restart: bool) -> Result<(), Failure> {
        let downloaded = self
            .downloaded
            .lock()
            .expect("updater lock poisoned")
            .take();
        let Some((update, bytes)) = downloaded else {
            return Ok(());
        };

        update
            .restart_after_install(restart)
            .install(bytes)
            .map_err(|_| Failure::text(TextKey::UpdateDownloadFailed))?;

        // On Windows the installer has already ended the process and reopens the app itself.
        if restart {
            self.app.restart();
        }
        Ok(())
    }
}
