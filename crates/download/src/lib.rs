//! Downloads of pinned files, shared by the adapters that fetch big files on demand.
//!
//! Every file is pinned by its size and SHA-256, and only becomes the real file once it is the
//! pinned one. A download that stops can be picked up where it stopped, and can be cancelled.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use reqwest::header::RANGE;
use reqwest::{Client, StatusCode};
use sha2::{Digest, Sha256};
use tokio::fs::{self as async_fs, OpenOptions};
use tokio::io::AsyncWriteExt;
use zip::ZipArchive;

const STALLED_AFTER: Duration = Duration::from_secs(30);
/// How often a download that waits for the network looks whether it was stopped.
const STOP_CHECK: Duration = Duration::from_millis(200);
const BLOCK: usize = 1 << 20;

/// A file to download, which is only kept when it is the one pinned here.
#[derive(Clone, Copy, Debug)]
pub struct Pin<'a> {
    pub url: &'a str,
    pub size: u64,
    pub sha256: &'a str,
}

/// Why a download did not end with the file.
#[derive(Debug)]
pub enum Interrupted {
    Stopped,
    Failed(String),
}

impl<E: std::error::Error> From<E> for Interrupted {
    fn from(error: E) -> Self {
        Self::Failed(error.to_string())
    }
}

/// A client that gives up on a server that stalls, and never follows a redirect to plain http.
pub fn client() -> Result<Client, Interrupted> {
    Ok(Client::builder()
        .connect_timeout(STALLED_AFTER)
        .read_timeout(STALLED_AFTER)
        .https_only(true)
        .build()?)
}

/// The bytes in hex.
pub fn hex(digest: &[u8]) -> String {
    use std::fmt::Write as _;

    digest.iter().fold(String::new(), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}

/// Copies everything and returns how much it was and its SHA-256, so a file is hashed as it is
/// written and not read a second time.
pub fn copy_hashed(source: &mut impl Read, target: &mut impl Write) -> io::Result<(u64, String)> {
    let mut hasher = Sha256::new();
    let mut size = 0;
    let mut block = vec![0; BLOCK];
    loop {
        let read = source.read(&mut block)?;
        if read == 0 {
            return Ok((size, hex(&hasher.finalize())));
        }
        hasher.update(&block[..read]);
        target.write_all(&block[..read])?;
        size += read as u64;
    }
}

/// The SHA-256 of a file in hex.
pub fn sha256_file(path: &Path) -> io::Result<String> {
    copy_hashed(&mut File::open(path)?, &mut io::sink()).map(|(_, hash)| hash)
}

/// The file a download goes to before it is whole.
fn partial_of(target: &Path) -> PathBuf {
    let mut name = target.as_os_str().to_owned();
    name.push(".part");
    PathBuf::from(name)
}

/// Writes `target` through a file of its own that only takes its place once it is whole and on
/// disk, so a crash or a full disk never leaves half of it.
pub fn write_atomic(
    target: &Path,
    write: impl FnOnce(&mut File) -> io::Result<()>,
) -> io::Result<()> {
    let partial = partial_of(target);
    let written = (|| {
        let mut file = File::create(&partial)?;
        write(&mut file)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&partial, target)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&partial);
    }
    written
}

/// Copies the one file `entry` out of a zip to `target`, atomically. With `expected` (its size
/// and SHA-256) the extracted bytes are checked too, not only the archive they came from.
pub fn extract_zip_entry(
    archive: &Path,
    entry: &str,
    expected: Option<(u64, &str)>,
    target: &Path,
) -> Result<(), Interrupted> {
    let mut archive = ZipArchive::new(File::open(archive)?)?;
    let mut entry_file = archive.by_name(entry)?;
    write_atomic(target, |file| {
        let (size, hash) = copy_hashed(&mut entry_file, file)?;
        match expected {
            Some((wanted_size, wanted_hash)) if size != wanted_size || hash != wanted_hash => {
                Err(io::Error::other(unexpected(entry)))
            }
            _ => Ok(()),
        }
    })?;
    Ok(())
}

fn unexpected(name: &str) -> String {
    let name = name.rsplit('/').next().unwrap_or(name);
    format!("{name} is not the file that was expected")
}

/// Downloads `file` to `target`, reporting the bytes as they come (the ones kept from an earlier
/// try first). The download goes to a `.part` file that is kept when it stops, and becomes
/// `target` only once its size and SHA-256 are the pinned ones.
pub async fn fetch(
    http: &Client,
    file: Pin<'_>,
    target: &Path,
    stop: &AtomicBool,
    arrived: &mut (dyn FnMut(u64) + Send),
) -> Result<(), Interrupted> {
    if let Some(parent) = target.parent() {
        async_fs::create_dir_all(parent).await?;
    }
    let partial = partial_of(target);
    let mut have = async_fs::metadata(&partial)
        .await
        .map_or(0, |kept| kept.len());
    if have > file.size {
        async_fs::remove_file(&partial).await?;
        have = 0;
    }

    if have < file.size {
        let mut request = http.get(file.url);
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
                async_fs::remove_file(&partial).await?;
                return Err(Interrupted::Failed(unexpected(file.url)));
            }
            written.write_all(&chunk).await?;
            arrived(chunk.len() as u64);
        }
        written.flush().await?;
    } else {
        arrived(have);
    }

    let checked = partial.clone();
    let hash = tokio::task::spawn_blocking(move || sha256_file(&checked)).await??;
    if have != file.size || hash != file.sha256 {
        async_fs::remove_file(&partial).await?;
        return Err(Interrupted::Failed(unexpected(file.url)));
    }
    async_fs::rename(&partial, target).await?;
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

/// Counts the bytes that arrived out of `total`, and tells when the whole number of percent
/// changes.
pub struct Progress<'a> {
    report: &'a (dyn Fn(u8) + Send + Sync),
    total: u64,
    done: u64,
    shown: u8,
}

impl<'a> Progress<'a> {
    pub fn new(report: &'a (dyn Fn(u8) + Send + Sync), total: u64) -> Self {
        Self {
            report,
            total,
            done: 0,
            shown: 0,
        }
    }

    pub fn advance(&mut self, bytes: u64) {
        self.done = (self.done + bytes).min(self.total);
        let percent = (self.done * 100 / self.total.max(1)) as u8;
        if percent != self.shown {
            self.shown = percent;
            (self.report)(percent);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;
    use std::sync::Mutex;

    use super::*;

    /// Serves `body` to each request, honouring a `Range`, and returns the address.
    fn serve(body: Vec<u8>, ranges: bool) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}/file", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut request = [0; 4096];
                let read = stream.read(&mut request).unwrap_or(0);
                let request = String::from_utf8_lossy(&request[..read]).to_lowercase();
                let from = request
                    .split("range: bytes=")
                    .nth(1)
                    .and_then(|rest| rest.split('-').next())
                    .and_then(|number| number.parse::<usize>().ok())
                    .filter(|_| ranges);
                let (status, slice) = match from {
                    Some(from) => ("206 Partial Content", &body[from..]),
                    None => ("200 OK", &body[..]),
                };
                let head = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    slice.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(slice);
            }
        });
        address
    }

    fn pin<'a>(url: &'a str, body: &[u8], sha: &'a str) -> Pin<'a> {
        Pin {
            url,
            size: body.len() as u64,
            sha256: sha,
        }
    }

    fn sha(body: &[u8]) -> String {
        hex(&Sha256::digest(body))
    }

    #[test]
    fn the_hash_of_a_file_is_its_sha256_in_hex() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("a");
        std::fs::write(&path, b"abc").unwrap();

        assert_eq!(
            sha256_file(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[tokio::test]
    async fn a_pinned_file_is_kept_and_leaves_no_partial_file() {
        let body = b"model bytes".to_vec();
        let url = serve(body.clone(), true);
        let folder = tempfile::tempdir().unwrap();
        let target = folder.path().join("model.bin");
        let digest = sha(&body);

        fetch(
            &Client::new(),
            pin(&url, &body, &digest),
            &target,
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .await
        .unwrap();

        assert_eq!(std::fs::read(&target).unwrap(), body);
        assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 1);
    }

    #[tokio::test]
    async fn a_file_that_is_not_the_pinned_one_is_thrown_away() {
        let url = serve(b"something else!".to_vec(), true);
        let folder = tempfile::tempdir().unwrap();
        let wanted = b"model bytes";
        let digest = sha(wanted);

        let result = fetch(
            &Client::new(),
            pin(&url, wanted, &digest),
            &folder.path().join("model.bin"),
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .await;

        assert!(matches!(result, Err(Interrupted::Failed(_))));
        assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn a_file_with_missing_bytes_is_thrown_away() {
        let url = serve(b"model".to_vec(), true);
        let folder = tempfile::tempdir().unwrap();
        let wanted = b"model bytes";
        let digest = sha(wanted);

        let result = fetch(
            &Client::new(),
            pin(&url, wanted, &digest),
            &folder.path().join("model.bin"),
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .await;

        assert!(matches!(result, Err(Interrupted::Failed(_))));
        assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn a_download_that_stopped_goes_on_from_the_bytes_it_has() {
        let body = b"model bytes".to_vec();
        let url = serve(body.clone(), true);
        let folder = tempfile::tempdir().unwrap();
        let target = folder.path().join("model.bin");
        std::fs::write(folder.path().join("model.bin.part"), &body[..5]).unwrap();
        let digest = sha(&body);
        let mut arrived = Vec::new();

        fetch(
            &Client::new(),
            pin(&url, &body, &digest),
            &target,
            &AtomicBool::new(false),
            &mut |bytes| arrived.push(bytes),
        )
        .await
        .unwrap();

        assert_eq!(std::fs::read(&target).unwrap(), body);
        assert_eq!(arrived.iter().sum::<u64>(), body.len() as u64);
        assert_eq!(arrived[0], 5);
    }

    #[tokio::test]
    async fn a_server_that_sends_the_whole_file_again_starts_the_download_over() {
        let body = b"model bytes".to_vec();
        let url = serve(body.clone(), false);
        let folder = tempfile::tempdir().unwrap();
        let target = folder.path().join("model.bin");
        std::fs::write(folder.path().join("model.bin.part"), &body[..5]).unwrap();
        let digest = sha(&body);

        fetch(
            &Client::new(),
            pin(&url, &body, &digest),
            &target,
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .await
        .unwrap();

        assert_eq!(std::fs::read(&target).unwrap(), body);
    }

    #[tokio::test]
    async fn a_download_asked_to_stop_ends_stopped() {
        let body = b"model bytes".to_vec();
        let url = serve(body.clone(), true);
        let folder = tempfile::tempdir().unwrap();
        let digest = sha(&body);

        let result = fetch(
            &Client::new(),
            pin(&url, &body, &digest),
            &folder.path().join("model.bin"),
            &AtomicBool::new(true),
            &mut |_| {},
        )
        .await;

        assert!(matches!(result, Err(Interrupted::Stopped)));
    }

    #[test]
    fn the_client_refuses_plain_http() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let url = serve(b"x".to_vec(), true);

        let result = runtime.block_on(client().unwrap().get(url).send());

        assert!(result.is_err());
    }

    fn zip_with(entry: &str, content: &[u8]) -> Vec<u8> {
        let mut bytes = io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(&mut bytes);
        zip.start_file(
            entry,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        )
        .unwrap();
        zip.write_all(content).unwrap();
        zip.finish().unwrap();
        bytes.into_inner()
    }

    #[test]
    fn only_the_wanted_entry_is_kept_and_checked_against_its_hash() {
        let folder = tempfile::tempdir().unwrap();
        let archive = folder.path().join("a.zip");
        std::fs::write(&archive, zip_with("pkg/lib.so", b"library bytes")).unwrap();
        let target = folder.path().join("lib.so");
        let digest = sha(b"library bytes");

        extract_zip_entry(&archive, "pkg/lib.so", Some((13, &digest)), &target).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"library bytes");

        let other = folder.path().join("other.so");
        let wrong = sha(b"another library");
        let result = extract_zip_entry(&archive, "pkg/lib.so", Some((13, &wrong)), &other);
        assert!(result.is_err());
        assert!(!other.exists());
        assert!(!folder.path().join("other.so.part").exists());
    }

    #[test]
    fn an_archive_without_the_entry_leaves_nothing() {
        let folder = tempfile::tempdir().unwrap();
        let archive = folder.path().join("a.zip");
        std::fs::write(&archive, zip_with("pkg/other.txt", b"nothing")).unwrap();
        let target = folder.path().join("lib.so");

        assert!(extract_zip_entry(&archive, "pkg/lib.so", None, &target).is_err());
        assert!(!target.exists());
    }

    #[test]
    fn progress_reports_each_percent_once_and_ends_at_a_hundred() {
        let reported = Mutex::new(Vec::new());
        let report = |percent| reported.lock().unwrap().push(percent);
        let mut progress = Progress::new(&report, 400);

        for _ in 0..9 {
            progress.advance(50);
        }

        assert_eq!(*reported.lock().unwrap(), [12, 25, 37, 50, 62, 75, 87, 100]);
    }
}
