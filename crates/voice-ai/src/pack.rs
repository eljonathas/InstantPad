//! The files of the AI voices, which are far bigger than the app and download on demand.
//!
//! Every file is pinned by its size and SHA-256, so what runs is what was reviewed.

use std::error::Error;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
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
const BLOCK: usize = 64 * 1024;

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
    sha256: &'static str,
}

impl Download {
    /// How big the file is once it is in the model folder.
    fn kept_size(&self) -> u64 {
        self.unpack.as_ref().map_or(self.size, |unpack| unpack.size)
    }

    fn kept_sha256(&self) -> &'static str {
        self.unpack
            .as_ref()
            .map_or(self.sha256, |unpack| unpack.sha256)
    }
}

/// ONNX Runtime for one kind of computer.
struct Runtime {
    os: &'static str,
    arch: &'static str,
    download: Download,
}

// ONNX Runtime 1.23.2 is the last version that has a build for Macs with an Intel processor.
// The library comes from the wheel PyPI serves for Python, which is a zip.
static RUNTIMES: [Runtime; 4] = [
    Runtime {
        os: "windows",
        arch: "x86_64",
        download: Download {
            name: "onnxruntime.dll",
            url: "https://files.pythonhosted.org/packages/c0/b4/569d298f9fc4d286c11c45e85d9ffa9e877af12ace98af8cab52396e8f46/onnxruntime-1.23.2-cp312-cp312-win_amd64.whl",
            size: 13_470_528,
            sha256: "25de5214923ce941a3523739d34a520aac30f21e631de53bba9174dc9c004435",
            unpack: Some(Unpack {
                entry: "onnxruntime/capi/onnxruntime.dll",
                size: 15_709_728,
                sha256: "462ea8e85b881b26388c595f96b1b9b39e57565aa4725fe29136695c1ead179f",
            }),
        },
    },
    Runtime {
        os: "macos",
        arch: "aarch64",
        download: Download {
            name: "libonnxruntime.1.23.2.dylib",
            url: "https://files.pythonhosted.org/packages/1b/9e/f748cd64161213adeef83d0cb16cb8ace1e62fa501033acdd9f9341fff57/onnxruntime-1.23.2-cp312-cp312-macosx_13_0_arm64.whl",
            size: 17_195_929,
            sha256: "b8f029a6b98d3cf5be564d52802bb50a8489ab73409fa9db0bf583eabb7c2321",
            unpack: Some(Unpack {
                entry: "onnxruntime/capi/libonnxruntime.1.23.2.dylib",
                size: 27_623_992,
                sha256: "92500010659b052368797e30c1841956b0efd699a8061ef8b3b25f27449c86e7",
            }),
        },
    },
    Runtime {
        os: "macos",
        arch: "x86_64",
        download: Download {
            name: "libonnxruntime.1.23.2.dylib",
            url: "https://files.pythonhosted.org/packages/91/9d/a81aafd899b900101988ead7fb14974c8a58695338ab6a0f3d6b0100f30b/onnxruntime-1.23.2-cp312-cp312-macosx_13_0_x86_64.whl",
            size: 19_157_705,
            sha256: "218295a8acae83905f6f1aed8cacb8e3eb3bd7513a13fe4ba3b2664a19fc4a6b",
            unpack: Some(Unpack {
                entry: "onnxruntime/capi/libonnxruntime.1.23.2.dylib",
                size: 30_819_512,
                sha256: "41e7960eccef34f60ca21d091d37bc97d0e70a0b4ce902babf5f795a76e1af74",
            }),
        },
    },
    Runtime {
        os: "linux",
        arch: "x86_64",
        download: Download {
            name: "libonnxruntime.so.1.23.2",
            url: "https://files.pythonhosted.org/packages/ef/88/9cc25d2bafe6bc0d4d3c1db3ade98196d5b355c0b273e6a5dc09c5d5d0d5/onnxruntime-1.23.2-cp312-cp312-manylinux_2_27_x86_64.manylinux_2_28_x86_64.whl",
            size: 17_382_649,
            sha256: "0f9b4ae77f8e3c9bee50c27bc1beede83f786fe1d52e99ac85aa8d65a01e9b77",
            unpack: Some(Unpack {
                entry: "onnxruntime/capi/libonnxruntime.so.1.23.2",
                size: 22_326_072,
                sha256: "718c3fb0785d5ea06061b92c32ce4ad35be1bd4c71ae6192efe06f1ab21f0f8a",
            }),
        },
    },
];

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

/// ONNX Runtime for this computer, or why there is none.
fn runtime() -> Result<&'static Download, String> {
    let (os, arch) = (std::env::consts::OS, std::env::consts::ARCH);
    let runtime = RUNTIMES
        .iter()
        .find(|runtime| runtime.os == os && runtime.arch == arch)
        .ok_or_else(|| format!("the AI voices do not run on {os} {arch}"))?;
    if os == "macos" && !macos_runs_the_runtime() {
        return Err("the AI voices need macOS 13.4 or later".to_owned());
    }
    Ok(&runtime.download)
}

/// The builds of ONNX Runtime for the Mac need macOS 13.4, though the app runs on older ones.
fn macos_runs_the_runtime() -> bool {
    static SUPPORTED: OnceLock<bool> = OnceLock::new();
    *SUPPORTED.get_or_init(|| {
        Command::new("sw_vers")
            .arg("-productVersion")
            .output()
            .ok()
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .is_none_or(|version| is_macos_13_4_or_later(&version))
    })
}

/// A version that cannot be read is let through, because the runtime says what it cannot run.
fn is_macos_13_4_or_later(version: &str) -> bool {
    let mut numbers = version.trim().split('.').map(str::parse::<u32>);
    match (numbers.next(), numbers.next()) {
        (Some(Ok(major)), minor) => (major, minor.map_or(0, Result::unwrap_or_default)) >= (13, 4),
        _ => true,
    }
}

/// Everything to download.
fn files() -> Result<Vec<&'static Download>, String> {
    Ok(std::iter::once(runtime()?)
        .chain(&OPENVOICE)
        .chain(&WHISPER)
        .collect())
}

/// Where ONNX Runtime is, once downloaded.
pub(crate) fn library(folder: &Path) -> Result<PathBuf, String> {
    runtime().map(|runtime| folder.join(runtime.name))
}

/// What is still to download, rounded up.
pub(crate) fn download_megabytes(folder: &Path) -> u32 {
    files().map_or(0, |files| {
        let bytes: u64 = files
            .iter()
            .filter(|file| !is_kept(folder, file))
            .map(|file| file.size)
            .sum();
        bytes.div_ceil(BYTES_PER_MEGABYTE) as u32
    })
}

/// A cheap check of the sizes, because hashing hundreds of megabytes at every start would not be.
pub(crate) fn is_installed(folder: &Path) -> bool {
    files().is_ok_and(|files| files.iter().all(|file| is_kept(folder, file)))
}

fn is_kept(folder: &Path, file: &Download) -> bool {
    std::fs::metadata(folder.join(file.name)).is_ok_and(|kept| kept.len() == file.kept_size())
}

/// Downloads what is not in the folder yet or is not what was pinned, and reports how much of
/// it is done. Installing runs in the background, so it can afford to hash what is kept.
pub(crate) async fn install(
    folder: &Path,
    report: &(dyn Fn(u8) + Send + Sync),
) -> Result<(), Failure> {
    let files = files().map_err(failed)?;
    let http = Client::builder()
        .connect_timeout(STALLED_AFTER)
        .read_timeout(STALLED_AFTER)
        .build()
        .map_err(failed)?;
    fs::create_dir_all(folder).await.map_err(failed)?;

    let mut progress = Progress::new(report, files.iter().map(|file| file.size).sum());
    for file in files {
        if is_intact(folder, file).await {
            progress.advance(file.size);
        } else {
            fetch(&http, folder, file, &mut progress).await?;
        }
    }
    Ok(())
}

/// Whether the file is there and is the pinned one. A damaged file is deleted, so nothing
/// takes it for installed.
async fn is_intact(folder: &Path, file: &Download) -> bool {
    if !is_kept(folder, file) {
        return false;
    }
    let kept = folder.join(file.name);
    let path = kept.clone();
    let intact = tokio::task::spawn_blocking(move || sha256_of(&path))
        .await
        .is_ok_and(|hash| hash.is_ok_and(|hash| hash == file.kept_sha256()));
    if !intact {
        let _ = fs::remove_file(&kept).await;
    }
    intact
}

fn sha256_of(path: &Path) -> std::io::Result<String> {
    copy_hashed(&mut std::fs::File::open(path)?, &mut std::io::sink()).map(|(_, hash)| hash)
}

/// Copies everything and returns how much it was and its SHA-256.
fn copy_hashed(source: &mut impl Read, target: &mut impl Write) -> std::io::Result<(u64, String)> {
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

/// Counts the bytes that arrived, and tells when the whole number of percent changes.
struct Progress<'a> {
    report: &'a (dyn Fn(u8) + Send + Sync),
    total: u64,
    done: u64,
    shown: u8,
}

impl<'a> Progress<'a> {
    fn new(report: &'a (dyn Fn(u8) + Send + Sync), total: u64) -> Self {
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
    let archive = folder.join(format!("{}.zip.part", file.name));
    let result = async {
        match &file.unpack {
            None => {
                download(http, file, &partial, progress).await?;
            }
            Some(unpack) => {
                download(http, file, &archive, progress).await?;
                let (archive, partial, unpack) = (archive.clone(), partial.clone(), *unpack);
                tokio::task::spawn_blocking(move || extract(&archive, &unpack, &partial))
                    .await
                    .map_err(failed)?
                    .map_err(failed)?;
            }
        }
        fs::rename(&partial, folder.join(file.name))
            .await
            .map_err(failed)
    }
    .await;

    // A download is never picked up where it stopped, and an archive is of no use once opened.
    let _ = fs::remove_file(&archive).await;
    let _ = fs::remove_file(&partial).await;
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
            return Err(failed(unexpected(file.name)));
        }
        hasher.update(&chunk);
        target.write_all(&chunk).await.map_err(failed)?;
        progress.advance(chunk.len() as u64);
    }
    target.flush().await.map_err(failed)?;

    if written == file.size && hex(&hasher.finalize()) == file.sha256 {
        Ok(())
    } else {
        Err(failed(unexpected(file.name)))
    }
}

/// Copies the one file out of the archive, and checks it is the pinned one.
fn extract(
    archive: &Path,
    unpack: &Unpack,
    partial: &Path,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut archive = ZipArchive::new(std::fs::File::open(archive)?)?;
    let mut entry = archive.by_name(unpack.entry)?;
    let mut target = std::fs::File::create(partial)?;
    let (size, hash) = copy_hashed(&mut entry, &mut target)?;
    if size == unpack.size && hash == unpack.sha256 {
        Ok(())
    } else {
        Err(unexpected(unpack.entry).into())
    }
}

fn unexpected(name: &str) -> String {
    format!("{name} is not the file that was expected")
}

fn failed(detail: impl ToString) -> Failure {
    Failure::because(TextKey::AiDownloadFailed, detail)
}

#[cfg(test)]
mod tests {
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

    fn sha256_text(body: &[u8]) -> &'static str {
        Box::leak(hex(&Sha256::digest(body)).into_boxed_str())
    }

    fn pinned(url: &'static str, body: &[u8], unpack: Option<Unpack>) -> Download {
        Download {
            name: "model.bin",
            url,
            size: body.len() as u64,
            sha256: sha256_text(body),
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

    fn all_downloads() -> impl Iterator<Item = &'static Download> {
        RUNTIMES
            .iter()
            .map(|runtime| &runtime.download)
            .chain(&OPENVOICE)
            .chain(&WHISPER)
    }

    #[test]
    fn every_pinned_hash_is_a_sha256() {
        for file in all_downloads() {
            for hash in [file.sha256, file.kept_sha256()] {
                assert_eq!(hash.len(), 64, "{}", file.name);
                assert!(hash.bytes().all(|byte| byte.is_ascii_hexdigit()));
            }
        }
    }

    #[test]
    fn every_kind_of_computer_has_one_runtime() {
        for (index, runtime) in RUNTIMES.iter().enumerate() {
            let same = RUNTIMES
                .iter()
                .skip(index + 1)
                .any(|other| (other.os, other.arch) == (runtime.os, runtime.arch));
            assert!(!same, "{} {}", runtime.os, runtime.arch);
            assert!(runtime.download.unpack.is_some());
        }
    }

    #[test]
    fn the_download_is_what_is_missing_rounded_up() {
        let Ok(files) = files() else {
            assert_eq!(download_megabytes(&folder("unsupported")), 0);
            return;
        };
        let folder = folder("missing");
        let bytes: u64 = files.iter().map(|file| file.size).sum();

        let everything = u64::from(download_megabytes(&folder));
        assert!(everything * BYTES_PER_MEGABYTE >= bytes);
        assert!((everything - 1) * BYTES_PER_MEGABYTE < bytes);

        let first = files[0];
        std::fs::File::create(folder.join(first.name))
            .unwrap()
            .set_len(first.kept_size())
            .unwrap();
        let rest = u64::from(download_megabytes(&folder));
        assert!(rest * BYTES_PER_MEGABYTE >= bytes - first.size);
        assert!(rest < everything);

        for file in files {
            std::fs::File::create(folder.join(file.name))
                .unwrap()
                .set_len(file.kept_size())
                .unwrap();
        }
        assert_eq!(download_megabytes(&folder), 0);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn an_empty_folder_has_nothing_installed() {
        let never_made = std::env::temp_dir().join("instantpad-voice-never-made");

        assert!(!is_installed(&never_made));
    }

    #[test]
    fn a_folder_is_installed_when_every_file_has_the_pinned_size() {
        let Ok(files) = files() else { return };
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

    #[test]
    fn the_runtime_builds_for_the_mac_need_macos_13_4() {
        assert!(!is_macos_13_4_or_later("10.15.7\n"));
        assert!(!is_macos_13_4_or_later("12.7.6"));
        assert!(!is_macos_13_4_or_later("13.3.1"));
        assert!(is_macos_13_4_or_later("13.4"));
        assert!(is_macos_13_4_or_later("13.6.1"));
        assert!(is_macos_13_4_or_later("14"));
        assert!(is_macos_13_4_or_later("26.0.1"));
        assert!(is_macos_13_4_or_later("not a version"));
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

        assert_eq!(failure, failed(unexpected("model.bin")));
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 0);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[tokio::test]
    async fn a_file_with_missing_bytes_is_thrown_away() {
        let file = pinned(serve(b"model".to_vec()), b"model bytes", None);
        let folder = folder("short");
        let mut progress = Progress::new(&|_| {}, file.size);

        let failure = fetch(&Client::new(), &folder, &file, &mut progress)
            .await
            .unwrap_err();

        assert_eq!(failure, failed(unexpected("model.bin")));
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 0);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    fn library_in_archive(entry: &str, library: &[u8]) -> Download {
        let wheel = archive(entry, library);
        let unpack = Unpack {
            entry: "pkg/capi/library.so",
            size: library.len() as u64,
            sha256: sha256_text(library),
        };
        pinned(serve(wheel.clone()), &wheel, Some(unpack))
    }

    #[tokio::test]
    async fn only_the_library_is_kept_from_an_archive() {
        let library = b"library bytes".repeat(100);
        let file = library_in_archive("pkg/capi/library.so", &library);
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
    async fn a_library_that_is_not_the_pinned_one_is_thrown_away() {
        let mut file = library_in_archive("pkg/capi/library.so", b"library bytes");
        file.unpack.as_mut().unwrap().sha256 = sha256_text(b"another library");
        let folder = folder("wrong-library");
        let mut progress = Progress::new(&|_| {}, file.size);

        let failure = fetch(&Client::new(), &folder, &file, &mut progress)
            .await
            .unwrap_err();

        assert_eq!(failure, failed(unexpected("pkg/capi/library.so")));
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 0);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[tokio::test]
    async fn an_archive_without_the_library_is_thrown_away() {
        let file = library_in_archive("pkg/other.txt", b"nothing");
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

    #[tokio::test]
    async fn a_damaged_file_of_the_right_size_is_deleted_so_it_downloads_again() {
        let folder = folder("damaged");
        let file = pinned("", b"model bytes", None);
        std::fs::write(folder.join("model.bin"), b"model bytez").unwrap();
        assert!(is_kept(&folder, &file));

        assert!(!is_intact(&folder, &file).await);

        assert!(!folder.join("model.bin").exists());
        std::fs::write(folder.join("model.bin"), b"model bytes").unwrap();
        assert!(is_intact(&folder, &file).await);
        assert!(folder.join("model.bin").exists());
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

        assert_eq!(*reported.lock().unwrap(), [12, 25, 37, 50, 62, 75, 87, 100]);
    }
}
