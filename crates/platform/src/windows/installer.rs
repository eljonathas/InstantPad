use std::fs::{self, File};
use std::io::{self, Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use instantpad_application::message::{Failure, Message, TextKey};
use zip::ZipArchive;

use super::shell::{self, Refusal};
use super::signature;
use super::vb_audio::{DRIVER_PACK_URL, SETUP_FILE, is_expected_entry, is_pinned_pack};

const STALLED_AFTER: Duration = Duration::from_secs(30);
/// The pack is 1.3 MB.
const MAX_PACK_BYTES: u64 = 20 * 1024 * 1024;
/// What all the files of the pack may take on disk, which keeps a zip bomb from filling it.
const MAX_UNPACKED_BYTES: u64 = 20 * 1024 * 1024;

/// Windows only loads Microsoft-signed audio drivers, so the cable is VB-CABLE. It is downloaded
/// from the vendor and installed by its own signed setup, after the pack matches the one this
/// version pins and the signature is checked.
pub async fn install(progress: &(dyn Fn(Message) + Send + Sync)) -> Result<(), Failure> {
    let http = reqwest::Client::builder()
        .connect_timeout(STALLED_AFTER)
        .read_timeout(STALLED_AFTER)
        .build()
        .map_err(failed)?;

    progress(Message::text(TextKey::DriverDownloading));
    let archive = download(&http, DRIVER_PACK_URL).await?;
    progress(Message::text(TextKey::DriverVerifying));
    if !is_pinned_pack(&archive) {
        return Err(failed(
            "the download is not the driver pack this version trusts",
        ));
    }
    let folder = tokio::task::spawn_blocking(move || extract_pack(archive))
        .await
        .map_err(failed)??;

    progress(Message::text(TextKey::DriverOpeningSetup));
    // The setup decides when the driver is in, so the install ends when it closes. The folder
    // goes with it, however that ends.
    tokio::task::spawn_blocking(move || {
        let setup = folder.path().join(SETUP_FILE);
        signature::verify(&setup)?;
        shell::run_elevated(&setup, folder.path()).map_err(|refusal| match refusal {
            Refusal::Declined => Failure::text(TextKey::DriverInstallCanceled),
            Refusal::Failed(code) => failed(format!("ShellExecute error {code:#010x}")),
        })
    })
    .await
    .map_err(failed)?
}

async fn download(http: &reqwest::Client, url: &str) -> Result<Vec<u8>, Failure> {
    let mut response = http
        .get(url)
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(failed)?;
    if response
        .content_length()
        .is_some_and(|size| size > MAX_PACK_BYTES)
    {
        return Err(failed("the driver pack is bigger than expected"));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(failed)? {
        if bytes.len() as u64 + chunk.len() as u64 > MAX_PACK_BYTES {
            return Err(failed("the driver pack is bigger than expected"));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

/// A folder made for one install, deleted when it is dropped.
struct Workspace(PathBuf);

impl Workspace {
    /// Always a new folder: `create_dir` fails on one that already exists, so nobody can have
    /// prepared it.
    fn new() -> io::Result<Self> {
        let folder = std::env::temp_dir().join(format!(
            "InstantPad-VBCABLE-{}",
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir(&folder)?;
        Ok(Self(folder))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Unpacks the setup and the driver files it installs, and nothing else.
fn extract_pack(archive: Vec<u8>) -> Result<Workspace, Failure> {
    let mut archive = ZipArchive::new(Cursor::new(archive)).map_err(failed)?;
    let workspace = Workspace::new().map_err(failed)?;

    let mut unpacked = 0;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(failed)?;
        // A name that is not valid text is not one of the expected files.
        let Ok(name) = entry.name().map(|name| name.into_owned()) else {
            continue;
        };
        if entry.is_dir() || !is_expected_entry(&name) {
            continue;
        }
        let mut target = File::create_new(workspace.path().join(&name)).map_err(failed)?;
        let room = MAX_UNPACKED_BYTES - unpacked;
        // One byte more than the room left tells that the file would not fit.
        let copied = io::copy(&mut (&mut entry).take(room + 1), &mut target).map_err(failed)?;
        if copied > room {
            return Err(failed("the driver pack unpacks to more than expected"));
        }
        unpacked += copied;
    }

    if workspace.path().join(SETUP_FILE).is_file() {
        Ok(workspace)
    } else {
        Err(Failure::because(TextKey::DriverSetupMissing, SETUP_FILE))
    }
}

fn failed(detail: impl ToString) -> Failure {
    Failure::because(TextKey::DriverFailed, detail)
}
