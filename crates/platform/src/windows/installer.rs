use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use std::time::Duration;

use instantpad_application::message::{Failure, Message, TextKey};
use zip::ZipArchive;

use super::shell::{self, Refusal};
use super::vb_audio::driver_pack_url;

const PRODUCT_PAGE: &str = "https://vb-audio.com/Cable/";
const SETUP_FILE: &str = "VBCABLE_Setup_x64.exe";
const STALLED_AFTER: Duration = Duration::from_secs(30);

/// Windows only loads Microsoft-signed audio drivers, so the cable is VB-CABLE. It is downloaded
/// from the vendor and installed by its own signed setup, after the signature is checked.
pub async fn install(progress: &(dyn Fn(Message) + Send + Sync)) -> Result<(), Failure> {
    let http = reqwest::Client::builder()
        .connect_timeout(STALLED_AFTER)
        .read_timeout(STALLED_AFTER)
        .build()
        .map_err(failed)?;

    progress(Message::text(TextKey::DriverSearching));
    // The direct link still works when only the product page is down.
    let page = page(&http).await.unwrap_or_default();
    let pack = driver_pack_url(&page);

    progress(Message::text(TextKey::DriverDownloading));
    let archive = download(&http, &pack).await.map_err(failed)?;
    let setup = tokio::task::spawn_blocking(move || extract_pack(archive))
        .await
        .map_err(failed)??;

    progress(Message::text(TextKey::DriverVerifying));
    super::signature::verify(&setup)?;

    progress(Message::text(TextKey::DriverOpeningSetup));
    shell::open_elevated(&setup).map_err(|refusal| match refusal {
        Refusal::Declined => Failure::text(TextKey::DriverInstallCanceled),
        Refusal::Failed(code) => failed(format!("ShellExecute error {code}")),
    })
}

async fn page(http: &reqwest::Client) -> reqwest::Result<String> {
    http.get(PRODUCT_PAGE)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await
}

async fn download(http: &reqwest::Client, url: &str) -> reqwest::Result<Vec<u8>> {
    let bytes = http
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    Ok(bytes.to_vec())
}

/// Unpacks everything, because the setup installs the driver files that sit next to it.
fn extract_pack(archive: Vec<u8>) -> Result<PathBuf, Failure> {
    let mut archive = ZipArchive::new(Cursor::new(archive)).map_err(failed)?;

    let folder = std::env::temp_dir().join(format!(
        "InstantPad-VBCABLE-{}",
        uuid::Uuid::new_v4().simple()
    ));
    fs::create_dir_all(&folder).map_err(failed)?;
    archive.extract(&folder).map_err(failed)?;

    let setup = folder.join(SETUP_FILE);
    if setup.is_file() {
        Ok(setup)
    } else {
        Err(Failure::because(TextKey::DriverSetupMissing, SETUP_FILE))
    }
}

fn failed(detail: impl ToString) -> Failure {
    Failure::because(TextKey::DriverFailed, detail)
}
