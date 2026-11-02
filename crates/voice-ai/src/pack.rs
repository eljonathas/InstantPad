//! The files of the AI voices, which are far bigger than the app and download on demand.
//!
//! Every file is pinned by its size and SHA-256, so what runs is what was reviewed.

use std::error::Error;
use std::path::{Path, PathBuf};
use std::time::Duration;

use instantpad_application::message::{Failure, TextKey};
use reqwest::Client;
use sha2::{Digest, Sha256};
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;
use zip::ZipArchive;

use crate::fingerprint::hex;

pub(crate) const TONE_EXTRACT: &str = "tone_extract.onnx";
pub(crate) const TONE_COLOR: &str = "tone_color.onnx";
pub(crate) const WHISPER_ENCODER: &str = "whisper_encoder.onnx";
pub(crate) const WHISPER_DECODER: &str = "whisper_decoder.onnx";
pub(crate) const TOKENIZER: &str = "tokenizer.json";

const STALLED_AFTER: Duration = Duration::from_secs(30);
const BYTES_PER_MEGABYTE: u64 = 1_000_000;

struct Download {
    /// What the file is called in the model folder.
    name: &'static str,
    url: &'static str,
    size: u64,
    sha256: &'static str,
    /// Set when the download is an archive that holds the file.
    unpack: Option<Unpack>,
}

/// The one file of an archive that is kept.
#[derive(Clone, Copy)]
struct Unpack {
    entry: &'static str,
    size: u64,
}

impl Download {
    /// How big the file is once it is in the model folder.
    fn kept_size(&self) -> u64 {
        self.unpack.as_ref().map_or(self.size, |unpack| unpack.size)
    }
}

// ONNX Runtime 1.23.2 is the last version that has a build for Macs with an Intel processor.
// The library comes from the wheel PyPI serves for Python, which is a zip.
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
static RUNTIME: Option<Download> = Some(Download {
    name: "onnxruntime.dll",
    url: "https://files.pythonhosted.org/packages/c0/b4/569d298f9fc4d286c11c45e85d9ffa9e877af12ace98af8cab52396e8f46/onnxruntime-1.23.2-cp312-cp312-win_amd64.whl",
    size: 13_470_528,
    sha256: "25de5214923ce941a3523739d34a520aac30f21e631de53bba9174dc9c004435",
    unpack: Some(Unpack {
        entry: "onnxruntime/capi/onnxruntime.dll",
        size: 15_709_728,
    }),
});

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
static RUNTIME: Option<Download> = Some(Download {
    name: "libonnxruntime.1.23.2.dylib",
    url: "https://files.pythonhosted.org/packages/1b/9e/f748cd64161213adeef83d0cb16cd0b8ace1e62fa501033acdd9f9341fff57/onnxruntime-1.23.2-cp312-cp312-macosx_13_0_arm64.whl",
    size: 17_195_929,
    sha256: "b8f029a6b98d3cf5be564d52802bb50a8489ab73409fa9db0bf583eabb7c2321",
    unpack: Some(Unpack {
        entry: "onnxruntime/capi/libonnxruntime.1.23.2.dylib",
        size: 27_623_992,
    }),
});

#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
static RUNTIME: Option<Download> = Some(Download {
    name: "libonnxruntime.1.23.2.dylib",
    url: "https://files.pythonhosted.org/packages/91/9d/a81aafd899b900101988ead7fb14974c8a58695338ab6a0f3d6b0100f30b/onnxruntime-1.23.2-cp312-cp312-macosx_13_0_x86_64.whl",
    size: 19_157_705,
    sha256: "218295a8acae83905f6f1aed8cacb8e3eb3bd7513a13fe4ba3b2664a19fc4a6b",
    unpack: Some(Unpack {
        entry: "onnxruntime/capi/libonnxruntime.1.23.2.dylib",
        size: 30_819_512,
    }),
});

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
static RUNTIME: Option<Download> = Some(Download {
    name: "libonnxruntime.so.1.23.2",
    url: "https://files.pythonhosted.org/packages/ef/88/9cc25d2bafe6bc0d4d3c1db3ade98196d5b355c0b273e6a5dc09c5d5d0d5/onnxruntime-1.23.2-cp312-cp312-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl",
    size: 17_382_649,
    sha256: "0f9b4ae77f8e3c9bee50c27bc1beede83f786fe1d52e99ac85aa8d65a01e9b77",
    unpack: Some(Unpack {
        entry: "onnxruntime/capi/libonnxruntime.so.1.23.2",
        size: 22_326_072,
    }),
});

#[cfg(not(any(
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "macos", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64"),
)))]
static RUNTIME: Option<Download> = None;

/// OpenVoice V2 (MIT) tells voices apart and changes one into another.
static OPENVOICE: [Download; 2] = [
    Download {
        name: TONE_EXTRACT,
        url: "https://huggingface.co/Hinotsuba/OpenVoice-ONNX-v2/resolve/74104c72661cc678d7ef1b743f05321d1756a5b4/tone_extract.onnx",
        size: 3_364_792,
        sha256: "e91c2cb696e199d2519ed8b62ca6e3c8e42cb99ca13955dd6e188051486e681c",
        unpack: None,
    },
    Download {
        name: TONE_COLOR,
        url: "https://huggingface.co/Hinotsuba/OpenVoice-ONNX-v2/resolve/74104c72661cc678d7ef1b743f05321d1756a5b4/tone_color.onnx",
        size: 157_196_170,
        sha256: "896195b84b0cb87a828bb8cab06577e9c024356bc9727b1a8f4174154bc0affa",
        unpack: None,
    },
];

/// Whisper base (Apache-2.0), shrunk to 8 bits, writes down the phrase the user reads aloud. The
/// encoder is the unsigned variant, because ONNX Runtime 1.23 cannot run the signed one.
static WHISPER: [Download; 3] = [
    Download {
        name: WHISPER_ENCODER,
        url: "https://huggingface.co/onnx-community/whisper-base/resolve/27a07b4b52b3a4c5886134bb4320e93e1a4bbc7c/onnx/encoder_model_uint8.onnx",
        size: 23_201_314,
        sha256: "5862993336bf33acd23736071aae2b32261d3b1b2f37780194460d4ef974dd46",
        unpack: None,
    },
    Download {
        name: WHISPER_DECODER,
        url: "https://huggingface.co/onnx-community/whisper-base/resolve/27a07b4b52b3a4c5886134bb4320e93e1a4bbc7c/onnx/decoder_model_int8.onnx",
        size: 53_310_178,
        sha256: "c859636f4463dd5a9ea752546c6c7badeb48ab3e22b340cafef08a15552fcb2e",
        unpack: None,
    },
    Download {
        name: TOKENIZER,
        url: "https://huggingface.co/onnx-community/whisper-base/resolve/27a07b4b52b3a4c5886134bb4320e93e1a4bbc7c/tokenizer.json",
        size: 2_480_466,
        sha256: "27fc476bfe7f17299480be2273fc0608e4d5a99aba2ab5dec5374b4482d1a566",
        unpack: None,
    },
];

/// Everything to download, or `None` where there is no ONNX Runtime to run it.
fn files() -> Option<Vec<&'static Download>> {
    let runtime = RUNTIME.as_ref()?;
    Some(
        std::iter::once(runtime)
            .chain(&OPENVOICE)
            .chain(&WHISPER)
            .collect(),
    )
}

pub(crate) fn unsupported() -> String {
    format!(
        "the AI voices do not run on {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

/// Where ONNX Runtime is, once downloaded.
pub(crate) fn library(folder: &Path) -> Option<PathBuf> {
    RUNTIME.as_ref().map(|runtime| folder.join(runtime.name))
}

pub(crate) fn download_megabytes() -> u32 {
    files().map_or(0, |files| {
        let bytes: u64 = files.iter().map(|file| file.size).sum();
        bytes.div_ceil(BYTES_PER_MEGABYTE) as u32
    })
}

/// A cheap check of the sizes, because hashing hundreds of megabytes at every start would not be.
pub(crate) fn is_installed(folder: &Path) -> bool {
    files().is_some_and(|files| files.iter().all(|file| is_kept(folder, file)))
}

fn is_kept(folder: &Path, file: &Download) -> bool {
    std::fs::metadata(folder.join(file.name)).is_ok_and(|kept| kept.len() == file.kept_size())
}

/// Downloads what is not in the folder yet, and reports how much of it is done.
pub(crate) async fn install(
    folder: &Path,
    report: &(dyn Fn(u8) + Send + Sync),
) -> Result<(), Failure> {
    let files = files().ok_or_else(|| failed(unsupported()))?;
    let http = Client::builder()
        .connect_timeout(STALLED_AFTER)
        .read_timeout(STALLED_AFTER)
        .build()
        .map_err(failed)?;
    fs::create_dir_all(folder).await.map_err(failed)?;

    let mut progress = Progress::new(report, files.iter().map(|file| file.size).sum());
    for file in files {
        if is_kept(folder, file) {
            progress.advance(file.size);
        } else {
            fetch(&http, folder, file, &mut progress).await?;
        }
    }
    Ok(())
}

/// Counts the bytes that arrived, and tells when the whole number of percent changes.
struct Progress<'a> {
    report: &'a (dyn Fn(u8) + Send + Sync),
    total: u64,
    done: u64,
    shown: u8,
}

impl<'a> Progress<'a> {
    fn new(report: &'a (dyn Fn(u8) + Send + Sync), total: u64) -> Self {
        report(0);
        Self {
            report,
            total,
            done: 0,
            shown: 0,
        }
    }

    fn advance(&mut self, bytes: u64) {
        self.done += bytes;
        let percent = (self.done * 100 / self.total) as u8;
        if percent != self.shown {
            self.shown = percent;
            (self.report)(percent);
        }
    }
}

/// Downloads to a partial file, which only becomes the real one when it is what was pinned.
async fn fetch(
    http: &Client,
    folder: &Path,
    file: &Download,
    progress: &mut Progress<'_>,
) -> Result<(), Failure> {
    let partial = folder.join(format!("{}.part", file.name));
    let kept = folder.join(file.name);
    let result = async {
        download(http, file, &partial, progress).await?;
        match &file.unpack {
            None => fs::rename(&partial, &kept).await.map_err(failed),
            Some(unpack) => {
                let (archive, kept, unpack) = (partial.clone(), kept.clone(), *unpack);
                tokio::task::spawn_blocking(move || extract(&archive, &unpack, &kept))
                    .await
                    .map_err(failed)?
                    .map_err(failed)
            }
        }
    }
    .await;

    // A download is never picked up where it stopped, and an archive is of no use once opened.
    let _ = fs::remove_file(&partial).await;
    if result.is_err() {
        let _ = fs::remove_file(&kept).await;
    }
    result
}

async fn download(
    http: &Client,
    file: &Download,
    partial: &Path,
    progress: &mut Progress<'_>,
) -> Result<(), Failure> {
    let mut response = http
        .get(file.url)
        .send()
        .await
        .map_err(failed)?
        .error_for_status()
        .map_err(failed)?;
    let mut target = File::create(partial).await.map_err(failed)?;
    let mut hasher = Sha256::new();
    let mut written = 0;
    while let Some(chunk) = response.chunk().await.map_err(failed)? {
        written += chunk.len() as u64;
        // A server that sends more than was pinned is not sending the pinned file.
        if written > file.size {
            return Err(unexpected(file));
        }
        hasher.update(&chunk);
        target.write_all(&chunk).await.map_err(failed)?;
        progress.advance(chunk.len() as u64);
    }
    target.flush().await.map_err(failed)?;

    if written == file.size && hex(&hasher.finalize()) == file.sha256 {
        Ok(())
    } else {
        Err(unexpected(file))
    }
}

/// Copies the one file out of the archive. The archive checks it while reading.
fn extract(
    archive: &Path,
    unpack: &Unpack,
    kept: &Path,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut archive = ZipArchive::new(std::fs::File::open(archive)?)?;
    let mut entry = archive.by_name(unpack.entry)?;
    let mut target = std::fs::File::create(kept)?;
    if std::io::copy(&mut entry, &mut target)? == unpack.size {
        Ok(())
    } else {
        Err(format!("{} is not the size it should be", unpack.entry).into())
    }
}

fn unexpected(file: &Download) -> Failure {
    failed(format!("{} is not the file that was expected", file.name))
}

fn failed(detail: impl ToString) -> Failure {
    Failure::because(TextKey::AiDownloadFailed, detail)
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::Mutex;
    use std::thread;

    use instantpad_application::message::Message;

    use super::*;

    /// Serves `body` to one request and returns the address to ask for it.
    fn serve(body: Vec<u8>) -> &'static str {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}/file", listener.local_addr().unwrap());
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            let _ = stream.read(&mut request).unwrap();
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(head.as_bytes()).unwrap();
            stream.write_all(&body).unwrap();
        });
        Box::leak(address.into_boxed_str())
    }

    fn folder(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("instantpad-voice-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn pinned(url: &'static str, body: &[u8], unpack: Option<Unpack>) -> Download {
        Download {
            name: "model.bin",
            url,
            size: body.len() as u64,
            sha256: Box::leak(hex(&Sha256::digest(body)).into_boxed_str()),
            unpack,
        }
    }

    /// A zip with a deflated file, the way a wheel holds the library.
    fn archive(entry: &str, content: &[u8]) -> Vec<u8> {
        let mut bytes = std::io::Cursor::new(Vec::new());
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
    fn every_pinned_hash_is_a_sha256() {
        for file in RUNTIME.iter().chain(&OPENVOICE).chain(&WHISPER) {
            assert_eq!(file.sha256.len(), 64, "{}", file.name);
            assert!(file.sha256.bytes().all(|byte| byte.is_ascii_hexdigit()));
        }
    }

    #[test]
    fn the_download_is_the_sum_of_the_files_rounded_up() {
        let Some(files) = files() else {
            assert_eq!(download_megabytes(), 0);
            return;
        };

        let bytes: u64 = files.iter().map(|file| file.size).sum();
        let megabytes = u64::from(download_megabytes());

        assert!(megabytes * BYTES_PER_MEGABYTE >= bytes);
        assert!((megabytes - 1) * BYTES_PER_MEGABYTE < bytes);
    }

    #[test]
    fn an_empty_folder_has_nothing_installed() {
        assert!(!is_installed(&folder("empty")));
    }

    #[test]
    fn a_folder_is_installed_when_every_file_has_the_pinned_size() {
        let Some(files) = files() else { return };
        let folder = folder("sizes");
        for file in &files {
            let kept = std::fs::File::create(folder.join(file.name)).unwrap();
            kept.set_len(file.kept_size()).unwrap();
        }
        assert!(is_installed(&folder));

        let last = files.last().unwrap();
        std::fs::File::create(folder.join(last.name))
            .unwrap()
            .set_len(last.size - 1)
            .unwrap();

        assert!(!is_installed(&folder));
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[tokio::test]
    async fn a_pinned_file_is_kept_and_leaves_no_partial_file() {
        let body = b"model bytes".to_vec();
        let file = pinned(serve(body.clone()), &body, None);
        let folder = folder("kept");
        let mut progress = Progress::new(&|_| {}, file.size);

        fetch(&Client::new(), &folder, &file, &mut progress)
            .await
            .unwrap();

        assert_eq!(std::fs::read(folder.join("model.bin")).unwrap(), body);
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 1);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[tokio::test]
    async fn a_file_that_is_not_the_pinned_one_is_thrown_away() {
        let file = pinned(serve(b"something else!".to_vec()), b"model bytes", None);
        let folder = folder("changed");
        let mut progress = Progress::new(&|_| {}, file.size);

        let failure = fetch(&Client::new(), &folder, &file, &mut progress)
            .await
            .unwrap_err();

        assert_eq!(failure, unexpected(&file));
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 0);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[tokio::test]
    async fn a_file_with_the_pinned_hash_but_missing_bytes_is_thrown_away() {
        let file = pinned(serve(b"model".to_vec()), b"model bytes", None);
        let folder = folder("short");
        let mut progress = Progress::new(&|_| {}, file.size);

        let failure = fetch(&Client::new(), &folder, &file, &mut progress)
            .await
            .unwrap_err();

        assert_eq!(failure, unexpected(&file));
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 0);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[tokio::test]
    async fn only_the_library_is_kept_from_an_archive() {
        let library = b"library bytes".repeat(100);
        let wheel = archive("pkg/capi/library.so", &library);
        let unpack = Unpack {
            entry: "pkg/capi/library.so",
            size: library.len() as u64,
        };
        let file = pinned(serve(wheel.clone()), &wheel, Some(unpack));
        let folder = folder("wheel");
        let mut progress = Progress::new(&|_| {}, file.size);

        fetch(&Client::new(), &folder, &file, &mut progress)
            .await
            .unwrap();

        assert_eq!(std::fs::read(folder.join("model.bin")).unwrap(), library);
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 1);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[tokio::test]
    async fn an_archive_without_the_library_is_thrown_away() {
        let wheel = archive("pkg/other.txt", b"nothing");
        let unpack = Unpack {
            entry: "pkg/capi/library.so",
            size: 7,
        };
        let file = pinned(serve(wheel.clone()), &wheel, Some(unpack));
        let folder = folder("no-library");
        let mut progress = Progress::new(&|_| {}, file.size);

        let failure = fetch(&Client::new(), &folder, &file, &mut progress)
            .await
            .unwrap_err();

        let Failure(Message::Text { key, .. }) = failure else {
            panic!("expected a translated message");
        };
        assert_eq!(key, TextKey::AiDownloadFailed);
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 0);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn progress_reports_each_percent_once_and_ends_at_a_hundred() {
        let reported = Mutex::new(Vec::new());
        let report = |percent| reported.lock().unwrap().push(percent);
        let mut progress = Progress::new(&report, 400);

        for _ in 0..8 {
            progress.advance(50);
        }

        assert_eq!(
            *reported.lock().unwrap(),
            [0, 12, 25, 37, 50, 62, 75, 87, 100]
        );
    }
}
