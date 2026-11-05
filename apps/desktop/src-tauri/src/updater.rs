//! Finds, downloads and applies signed releases through the Tauri updater.

use std::sync::Mutex;

use async_trait::async_trait;
use instantpad_application::message::{Failure, TextKey};
use instantpad_application::ports::AppUpdater;
use instantpad_application::sync::lock;
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
/// Only the file name counts, not a folder such as `D:\PortableApps`.
fn is_portable() -> bool {
    std::env::current_exe().is_ok_and(|path| {
        path.file_name()
            .is_some_and(|name| is_portable_name(&name.to_string_lossy()))
    })
}

fn is_portable_name(file_name: &str) -> bool {
    file_name.to_lowercase().contains("portable")
}

/// The share of the download that is done. A server whose Content-Length is too small must not
/// make this wrap around.
fn percent(received: u64, total: u64) -> u8 {
    u8::try_from((received.saturating_mul(100) / total).min(100)).unwrap_or(100)
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
        *lock(&self.found) = update;
        Ok(version)
    }

    async fn download(&self, progress: &(dyn Fn(u8) + Send + Sync)) -> Result<(), Failure> {
        let found = lock(&self.found).take();
        let Some(update) = found else {
            return Ok(());
        };

        let mut received = 0;
        let report = |chunk: usize, total: Option<u64>| {
            received += chunk as u64;
            if let Some(total) = total.filter(|&total| total > 0) {
                progress(percent(received, total));
            }
        };
        let bytes = update
            .download(report, || {})
            .await
            .map_err(|_| Failure::text(TextKey::UpdateDownloadFailed))?;

        *lock(&self.downloaded) = Some((update, bytes));
        Ok(())
    }

    fn apply(&self, restart: bool) -> Result<(), Failure> {
        let downloaded = lock(&self.downloaded).take();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_file_name_makes_a_copy_portable() {
        assert!(is_portable_name("InstantPad-portable.exe"));
        assert!(!is_portable_name("InstantPad.exe"));
    }

    #[test]
    fn a_download_larger_than_announced_stays_at_one_hundred_percent() {
        assert_eq!(percent(50, 200), 25);
        assert_eq!(percent(900, 200), 100);
        assert_eq!(percent(u64::MAX, 1), 100);
    }
}
