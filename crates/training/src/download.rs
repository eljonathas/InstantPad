//! Downloads pinned files, picking up where a download stopped, since the biggest is 3 GB.

use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use reqwest::header::RANGE;
use reqwest::{Client, StatusCode};
use sha2::{Digest, Sha256};
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;

use crate::pins::Pinned;

const STALLED_AFTER: Duration = Duration::from_secs(30);
/// How often a download that waits for the network looks whether it was stopped.
const STOP_CHECK: Duration = Duration::from_millis(200);
const BLOCK: usize = 1 << 20;

/// Why a download did not end with the file.
#[derive(Debug)]
pub(crate) enum Interrupted {
    Stopped,
    Failed(String),
}

impl<E: std::error::Error> From<E> for Interrupted {
    fn from(error: E) -> Self {
        Self::Failed(error.to_string())
    }
}

pub(crate) fn client() -> Result<Client, Interrupted> {
    Ok(Client::builder()
        .connect_timeout(STALLED_AFTER)
        .read_timeout(STALLED_AFTER)
        .build()?)
}

/// Whether `target` is there with the pinned size. Its hash is checked when it is downloaded.
pub(crate) fn is_there(target: &Path, file: &Pinned) -> bool {
    std::fs::metadata(target).is_ok_and(|kept| kept.len() == file.size)
}

/// Downloads `file` to `target`, unless it is there already, reporting the bytes as they come.
/// The download goes to a `.part` file that is kept when it stops, and becomes `target` only
/// once its size and SHA-256 are the pinned ones.
pub(crate) async fn fetch(
    http: &Client,
    file: &Pinned,
    target: &Path,
    stop: &AtomicBool,
    arrived: &mut (dyn FnMut(u64) + Send),
) -> Result<(), Interrupted> {
    if is_there(target, file) {
        arrived(file.size);
        return Ok(());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).await?;
    }
    let partial = target.with_extension("part");
    let mut have = fs::metadata(&partial).await.map_or(0, |kept| kept.len());
    if have > file.size {
        fs::remove_file(&partial).await?;
        have = 0;
    }

    if have < file.size {
        let mut request = http.get(&file.url);
        if have > 0 {
            request = request.header(RANGE, format!("bytes={have}-"));
        }
        let mut response = unless_stopped(stop, request.send())
            .await??
            .error_for_status()?;
        // A server that sends the whole file again is started over.
        if have > 0 && response.status() != StatusCode::PARTIAL_CONTENT {
            have = 0;
        }
        let mut written = OpenOptions::new()
            .create(true)
            .write(true)
            .append(have > 0)
            .truncate(have == 0)
            .open(&partial)
            .await?;
        arrived(have);
        loop {
            let next = match unless_stopped(stop, response.chunk()).await {
                Ok(next) => next?,
                Err(stopped) => {
                    written.flush().await?;
                    return Err(stopped);
                }
            };
            let Some(chunk) = next else {
                break;
            };
            have += chunk.len() as u64;
            // A server that sends more than was pinned is not sending the pinned file.
            if have > file.size {
                drop(written);
                fs::remove_file(&partial).await?;
                return Err(unexpected(&file.url));
            }
            written.write_all(&chunk).await?;
            arrived(chunk.len() as u64);
        }
        written.flush().await?;
    }

    let checked = partial.clone();
    let hash = tokio::task::spawn_blocking(move || sha256_of(&checked)).await??;
    if have != file.size || hash != file.sha256 {
        fs::remove_file(&partial).await?;
        return Err(unexpected(&file.url));
    }
    fs::rename(&partial, target).await?;
    Ok(())
}

/// Waits for `work`, unless `stop` is set meanwhile: a server that stalls answers only after
/// [`STALLED_AFTER`], and the user who stops the download should not wait for it.
async fn unless_stopped<T>(
    stop: &AtomicBool,
    work: impl Future<Output = T>,
) -> Result<T, Interrupted> {
    let mut work = std::pin::pin!(work);
    loop {
        if stop.load(Ordering::Relaxed) {
            return Err(Interrupted::Stopped);
        }
        if let Ok(done) = tokio::time::timeout(STOP_CHECK, work.as_mut()).await {
            return Ok(done);
        }
    }
}

fn unexpected(url: &str) -> Interrupted {
    let name = url.rsplit('/').next().unwrap_or(url);
    Interrupted::Failed(format!("{name} is not the file that was expected"))
}

pub(crate) fn sha256_of(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut block = vec![0; BLOCK];
    loop {
        let read = file.read(&mut block)?;
        if read == 0 {
            return Ok(hasher
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect());
        }
        hasher.update(&block[..read]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_of_a_file_is_its_sha256_in_hex() {
        let path = std::env::temp_dir().join(format!("instantpad-hash-{}", std::process::id()));
        std::fs::write(&path, b"abc").unwrap();

        let hash = sha256_of(&path).unwrap();

        std::fs::remove_file(&path).unwrap();
        assert_eq!(
            hash,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[tokio::test]
    async fn stopping_does_not_wait_for_a_server_that_stalls() {
        let stop = AtomicBool::new(false);
        let started = std::time::Instant::now();

        let waited = unless_stopped(&stop, async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            stop.store(true, Ordering::Relaxed);
            std::future::pending::<()>().await;
        })
        .await;

        assert!(matches!(waited, Err(Interrupted::Stopped)));
        assert!(started.elapsed() < STALLED_AFTER);
    }

    #[tokio::test]
    async fn what_ends_before_a_stop_is_kept() {
        let stop = AtomicBool::new(false);

        let waited = unless_stopped(&stop, async { 7 }).await;

        assert!(matches!(waited, Ok(7)));
    }

    #[test]
    fn a_file_counts_as_there_only_with_its_pinned_size() {
        let path = std::env::temp_dir().join(format!("instantpad-size-{}", std::process::id()));
        std::fs::write(&path, b"abc").unwrap();
        let pinned = |size| Pinned {
            url: "https://example.com/a".to_owned(),
            size,
            sha256: String::new(),
        };

        let right = is_there(&path, &pinned(3));
        let wrong = is_there(&path, &pinned(4));

        std::fs::remove_file(&path).unwrap();
        assert!(right);
        assert!(!wrong);
    }
}
