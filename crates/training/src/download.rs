//! Downloads pinned files, picking up where a download stopped, since the biggest is 3 GB. The
//! download itself is the one in `instantpad-download`.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use instantpad_download::{Pin, sha256_file};

pub(crate) use instantpad_download::{Interrupted, client};

use crate::pins::Pinned;

/// Whether `target` is there with the pinned size. Only a guess of what is left to download:
/// [`fetch`] looks at the hash before it trusts a file.
pub(crate) fn is_there(target: &Path, file: &Pinned) -> bool {
    std::fs::metadata(target).is_ok_and(|kept| kept.len() == file.size)
}

/// Downloads `file` to `target`, reporting the bytes as they come. A file that is already there
/// is kept only if it is the pinned one, since it is installed without a hash check of its own.
pub(crate) async fn fetch(
    http: &reqwest::Client,
    file: &Pinned,
    target: &Path,
    stop: &AtomicBool,
    arrived: &mut (dyn FnMut(u64) + Send),
) -> Result<(), Interrupted> {
    if is_there(target, file) {
        let kept = target.to_owned();
        let hash = tokio::task::spawn_blocking(move || sha256_file(&kept)).await??;
        if hash == file.sha256 {
            arrived(file.size);
            return Ok(());
        }
        tokio::fs::remove_file(target).await?;
    }
    let pin = Pin {
        url: &file.url,
        size: file.size,
        sha256: &file.sha256,
    };
    instantpad_download::fetch(http, pin, target, stop, arrived).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pinned(size: u64, sha256: &str) -> Pinned {
        Pinned {
            url: "https://example.com/a".to_owned(),
            size,
            sha256: sha256.to_owned(),
        }
    }

    #[test]
    fn a_file_counts_as_there_only_with_its_pinned_size() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("a");
        std::fs::write(&path, b"abc").unwrap();

        assert!(is_there(&path, &pinned(3, "")));
        assert!(!is_there(&path, &pinned(4, "")));
    }

    #[tokio::test]
    async fn a_file_of_the_right_size_with_another_hash_is_deleted_and_not_trusted() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("a");
        std::fs::write(&path, b"abc").unwrap();
        // Nothing to download from: the address is refused before any request is made.
        let http = client().unwrap();
        let file = Pinned {
            url: "http://127.0.0.1:9/a".to_owned(),
            size: 3,
            sha256: "0".repeat(64),
        };

        let fetched = fetch(&http, &file, &path, &AtomicBool::new(false), &mut |_| {}).await;

        assert!(fetched.is_err());
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn a_file_with_the_pinned_hash_is_kept_without_downloading() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("a");
        std::fs::write(&path, b"abc").unwrap();
        let file = Pinned {
            url: "http://127.0.0.1:9/a".to_owned(),
            size: 3,
            sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad".to_owned(),
        };
        let mut arrived = 0;

        fetch(
            &client().unwrap(),
            &file,
            &path,
            &AtomicBool::new(false),
            &mut |bytes| arrived += bytes,
        )
        .await
        .unwrap();

        assert_eq!(arrived, 3);
    }
}
