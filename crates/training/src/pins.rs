//! What the training downloads, pinned by size and SHA-256, and the kit it runs. The wheels come
//! from `tools/voice-training/environments`, which `pin.py` writes.

use serde::Deserialize;

/// A file to download, which is only kept when it is the one pinned here.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub(crate) struct Pinned {
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

/// The Python packages for one kind of device.
#[derive(Debug, Deserialize)]
pub(crate) struct Environment {
    /// The build of Python uv installs, which uv checks against its own hashes.
    pub python: String,
    pub wheels: Vec<Pinned>,
}

impl Environment {
    pub fn parse(text: &str) -> Self {
        serde_json::from_str(text).expect("the pinned environments are valid")
    }

    pub fn wheel_bytes(&self) -> u64 {
        self.wheels.iter().map(|wheel| wheel.size).sum()
    }
}

/// uv itself, and where its program is in the archive.
pub(crate) struct Uv {
    pub file: Pinned,
    pub entry: &'static str,
}

/// The kit, which the app writes next to the tools before each run, so a newer app brings a
/// newer kit.
pub(crate) const VOICEKIT: &str = include_str!("../../../tools/voice-training/voicekit.py");

/// What `voicekit.py setup` downloads: RVC's archive, its base models and the silence it mixes
/// in, as listed in the kit.
pub(crate) const KIT_DOWNLOAD_BYTES: u64 =
    821_646 + 1_492 + 225 + 189_206_711 + 181_184_272 + 73_106_273 + 142_875_703 + 312_273;

/// A build of Python 3.12 for uv to install, about as big on every system.
pub(crate) const PYTHON_BYTES: u64 = 35_000_000;

const UV_VERSION: &str = "0.11.29";

#[cfg(target_os = "windows")]
mod system {
    pub(super) const CARD: &str =
        include_str!("../../../tools/voice-training/environments/windows-cuda.json");
    pub(super) const PROCESSOR: &str =
        include_str!("../../../tools/voice-training/environments/windows-cpu.json");
    pub(super) const UV_FILE: &str = "uv-x86_64-pc-windows-msvc.zip";
    pub(super) const UV_SIZE: u64 = 25_534_683;
    pub(super) const UV_SHA256: &str =
        "a047d55651bc3e0ca24595b25ec4cfcb10f9dca9fb56514e661269b37d4fae68";
    pub(super) const UV_ENTRY: &str = "uv.exe";
    /// What the tools take once installed. The card's was measured on a fresh install (7.6 GB);
    /// the processor's is the card's less PyTorch's CUDA libraries, with room to spare.
    pub(super) const CARD_INSTALLED_MEGABYTES: u32 = 7_800;
    pub(super) const PROCESSOR_INSTALLED_MEGABYTES: u32 = 2_700;
}

#[cfg(not(target_os = "windows"))]
mod system {
    pub(super) const CARD: &str =
        include_str!("../../../tools/voice-training/environments/linux-cuda.json");
    pub(super) const PROCESSOR: &str =
        include_str!("../../../tools/voice-training/environments/linux-cpu.json");
    pub(super) const UV_FILE: &str = "uv-x86_64-unknown-linux-gnu.tar.gz";
    pub(super) const UV_SIZE: u64 = 26_200_011;
    pub(super) const UV_SHA256: &str =
        "04f8b82f5d47f0512dcd32c67a4a6f16a0ea27c81537c338fd0ad6b23cebe829";
    pub(super) const UV_ENTRY: &str = "uv-x86_64-unknown-linux-gnu/uv";
    /// Measured on the install `setup.sh` makes, which holds the same packages (8.7 GB).
    pub(super) const CARD_INSTALLED_MEGABYTES: u32 = 8_900;
    pub(super) const PROCESSOR_INSTALLED_MEGABYTES: u32 = 2_700;
}

/// The packages for training on the card, with CUDA, or on the processor.
pub(crate) fn environment(on_card: bool) -> Environment {
    Environment::parse(if on_card {
        system::CARD
    } else {
        system::PROCESSOR
    })
}

/// A fingerprint of what an install of these packages is, to tell when the app pins others.
/// The pinned Python is part of the file it hashes.
pub(crate) fn fingerprint(on_card: bool) -> String {
    use sha2::{Digest, Sha256};

    let text = if on_card {
        system::CARD
    } else {
        system::PROCESSOR
    };
    let digest = Sha256::digest(text.as_bytes());
    instantpad_download::hex(&digest[..8])
}

/// Which uv is pinned, to tell when the app pins another.
pub(crate) fn uv_id() -> String {
    format!("{UV_VERSION}-{}", system::UV_SHA256)
}

pub(crate) fn installed_megabytes(on_card: bool) -> u32 {
    if on_card {
        system::CARD_INSTALLED_MEGABYTES
    } else {
        system::PROCESSOR_INSTALLED_MEGABYTES
    }
}

pub(crate) fn uv() -> Uv {
    Uv {
        file: Pinned {
            url: format!(
                "https://github.com/astral-sh/uv/releases/download/{UV_VERSION}/{}",
                system::UV_FILE
            ),
            size: system::UV_SIZE,
            sha256: system::UV_SHA256.to_owned(),
        },
        entry: system::UV_ENTRY,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_wheel_is_pinned_by_its_size_and_sha256() {
        for on_card in [true, false] {
            let environment = environment(on_card);
            assert!(environment.python.starts_with("3.12."));
            assert!(!environment.wheels.is_empty());
            for wheel in &environment.wheels {
                assert!(wheel.url.starts_with("https://"), "{}", wheel.url);
                assert!(wheel.url.ends_with(".whl"), "{}", wheel.url);
                assert!(wheel.size > 0);
                assert_eq!(wheel.sha256.len(), 64);
            }
        }
    }

    #[test]
    fn the_card_needs_pytorch_with_cuda_and_the_processor_does_not() {
        let has_cuda = |on_card| {
            environment(on_card)
                .wheels
                .iter()
                .any(|wheel| wheel.url.contains("cu128"))
        };

        assert!(has_cuda(true));
        assert!(!has_cuda(false));
        assert!(environment(true).wheel_bytes() > environment(false).wheel_bytes());
    }

    #[test]
    fn the_sizes_the_app_counts_for_the_kit_are_the_ones_the_kit_pins() {
        // The pinned files are listed between RVC's archive and the dataset, and each size is a
        // number alone on its line.
        let pins = VOICEKIT
            .split_once("RVC_ARCHIVE = (")
            .and_then(|(_, rest)| rest.split_once("# Hi-Fi TTS"))
            .map(|(pins, _)| pins)
            .unwrap();
        let sizes: u64 = pins
            .lines()
            .filter_map(|line| {
                line.trim()
                    .strip_suffix(',')?
                    .replace('_', "")
                    .parse::<u64>()
                    .ok()
            })
            .sum();

        assert_eq!(sizes, KIT_DOWNLOAD_BYTES);
    }

    #[test]
    fn the_kit_built_into_the_app_reports_events() {
        assert!(VOICEKIT.contains("@instantpad "));
        assert!(VOICEKIT.contains("--events"));
    }
}
