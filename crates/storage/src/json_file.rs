use std::collections::HashSet;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use instantpad_application::message::{Failure, TextKey};
use serde::Serialize;
use serde::de::DeserializeOwned;

static NEXT_WRITE: AtomicU64 = AtomicU64::new(0);

/// Files that exist but could not be read. Saving over them would replace the user's data
/// with the defaults the app started with, so they are left alone until the next start.
static UNREADABLE: LazyLock<Mutex<HashSet<PathBuf>>> = LazyLock::new(Mutex::default);

/// A missing file is a first run. A damaged one is kept as a backup so the next save
/// does not destroy what the user might still recover. One that cannot be read at all
/// (locked by another program, no permission) is never saved over.
pub fn load<T: DeserializeOwned + Default>(path: &Path) -> T {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => return T::default(),
        Err(_) => {
            // A copy may work where reading did not. If not, the file is still untouched.
            let _ = std::fs::copy(path, backup_name(path));
            unreadable().insert(path.to_path_buf());
            return T::default();
        }
    };
    serde_json::from_slice(&bytes).unwrap_or_else(|_| {
        // If the backup fails too, the app still has to start with the default.
        let _ = std::fs::copy(path, backup_name(path));
        T::default()
    })
}

/// Writes next to the target and renames, so a crash never leaves half a document, and
/// waits for the disk first, so a power cut cannot leave an empty one.
pub fn save<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<(), Failure> {
    if unreadable().contains(path) {
        return Err(Failure::because(
            TextKey::SaveFailed,
            "the saved file could not be read, so it is not replaced",
        ));
    }
    let part = appended(
        path,
        &format!("{}.part", NEXT_WRITE.fetch_add(1, Ordering::Relaxed)),
    );
    let write = || -> std::io::Result<()> {
        let mut file = create_private(&part)?;
        file.write_all(&serde_json::to_vec_pretty(value)?)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&part, path)?;
        sync_folder(path);
        Ok(())
    };
    write().map_err(|error| {
        let _ = std::fs::remove_file(&part);
        Failure::because(TextKey::SaveFailed, error)
    })
}

fn unreadable() -> std::sync::MutexGuard<'static, HashSet<PathBuf>> {
    UNREADABLE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Readable by the user only: the voices file holds the voiceprint.
fn create_private(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)
}

/// Makes the rename itself durable. Windows has no way to do this for a folder.
fn sync_folder(path: &Path) {
    #[cfg(unix)]
    if let Some(folder) = path.parent()
        && let Ok(folder) = std::fs::File::open(folder)
    {
        let _ = folder.sync_all();
    }
    #[cfg(not(unix))]
    let _ = path;
}

/// `.bak`, or a stamped name when an earlier backup exists that must not be lost.
fn backup_name(path: &Path) -> PathBuf {
    let backup = appended(path, "bak");
    if !backup.exists() {
        return backup;
    }
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    appended(path, &format!("{seconds}.bak"))
}

fn appended(path: &Path, extension: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".");
    name.push(extension);
    PathBuf::from(name)
}
