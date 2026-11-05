//! What is known about VB-CABLE and its vendor, kept free of Windows calls so any OS can test it.

use instantpad_application::ports::AudioDevice;
use sha2::{Digest, Sha256};

/// The one driver pack this version trusts. A newer pack is not followed from the product page:
/// it only becomes the one installed by a release of the app that pins it.
pub const DRIVER_PACK_URL: &str =
    "https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack45.zip";
/// The SHA-256 of that pack, as the macOS package pins BlackHole's. Taken from the vendor's
/// download over TLS on 2026-11-05; compare it with an independent copy before each release.
const DRIVER_PACK_SHA256: &str = "b950e39f01af1d04ea623c8f6d8eb9b6ea5c477c637295fabf20631c85116bfb";
/// The setup that is run, and the only programs that may be unpacked next to it.
pub const SETUP_FILE: &str = "VBCABLE_Setup_x64.exe";
const OTHER_PROGRAM: &str = "VBCABLE_ControlPanel.exe";
/// Driver files, their catalogs and the notes: what the setup reads from its folder.
const DRIVER_EXTENSIONS: [&str; 5] = ["inf", "cat", "sys", "ico", "txt"];

const VENDOR_NAME: &str = "BUREL VINCENT";

const DEFAULT_GAME_MICROPHONE: &str = "CABLE Output";
const VOICEMEETER_NAME: &str = "VoiceMeeter";
const HIFI_CABLE_NAME: &str = "Hi-Fi";

/// Render side of each known virtual cable: what this app plays into.
const RENDER_NAMES: [&str; 5] = [
    "CABLE Input",
    "CABLE In ",
    "VoiceMeeter Input",
    "VoiceMeeter AUX Input",
    "Hi-Fi Cable Input",
];

/// Capture side: what the game reads from.
const CAPTURE_NAMES: [&str; 4] = [
    "CABLE Output",
    "VoiceMeeter Output",
    "VoiceMeeter AUX Output",
    "Hi-Fi Cable Output",
];

pub fn find_game_output(outputs: &[AudioDevice]) -> Option<AudioDevice> {
    RENDER_NAMES
        .iter()
        .find_map(|name| outputs.iter().find(|device| has_name(device, name)))
        .cloned()
}

pub fn is_virtual(device: &AudioDevice) -> bool {
    RENDER_NAMES
        .iter()
        .chain(&CAPTURE_NAMES)
        .any(|name| has_name(device, name))
}

pub fn game_microphone_name(game_output: Option<&AudioDevice>) -> &'static str {
    match game_output {
        Some(device) if has_name(device, VOICEMEETER_NAME) => "VoiceMeeter Output",
        Some(device) if has_name(device, HIFI_CABLE_NAME) => "Hi-Fi Cable Output",
        _ => DEFAULT_GAME_MICROPHONE,
    }
}

/// Whether `pack` is exactly the pinned driver pack.
pub fn is_pinned_pack(pack: &[u8]) -> bool {
    let digest = Sha256::digest(pack);
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    hex == DRIVER_PACK_SHA256
}

/// Whether a file of the pack is one that gets unpacked: a flat name, either one of the two
/// known programs or a driver file. A library next to an elevated program could be loaded by it.
pub fn is_expected_entry(name: &str) -> bool {
    if name.contains(['/', '\\']) {
        return false;
    }
    if name.eq_ignore_ascii_case(SETUP_FILE) || name.eq_ignore_ascii_case(OTHER_PROGRAM) {
        return true;
    }
    name.rsplit_once('.').is_some_and(|(_, extension)| {
        DRIVER_EXTENSIONS
            .iter()
            .any(|known| extension.eq_ignore_ascii_case(known))
    })
}

/// Whether `name`, the common name of the certificate that signed the setup, is the vendor's.
/// The whole name must match: a longer one, or one only containing it, is someone else.
pub fn is_vendor_name(name: &str) -> bool {
    name.eq_ignore_ascii_case(VENDOR_NAME)
}

fn has_name(device: &AudioDevice, name: &str) -> bool {
    device
        .name
        .to_ascii_lowercase()
        .contains(&name.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(name: &str) -> AudioDevice {
        AudioDevice {
            id: format!("wasapi:{name}"),
            name: name.to_owned(),
        }
    }

    #[test]
    fn every_known_cable_side_is_virtual_and_a_real_device_is_not() {
        for name in RENDER_NAMES.iter().chain(&CAPTURE_NAMES) {
            assert!(is_virtual(&device(name)), "{name}");
        }
        assert!(is_virtual(&device("cable input (vb-audio virtual cable)")));
        assert!(!is_virtual(&device("Speakers (Realtek Audio)")));
    }

    #[test]
    fn the_game_output_follows_the_order_of_the_known_cables() {
        let outputs = [
            device("VoiceMeeter Input"),
            device("CABLE Input (VB-Audio Virtual Cable)"),
        ];

        assert_eq!(find_game_output(&outputs), Some(outputs[1].clone()));
        assert_eq!(find_game_output(&[device("Speakers")]), None);
    }

    #[test]
    fn the_game_microphone_is_the_capture_side_of_the_chosen_cable() {
        let name = |output: &str| game_microphone_name(Some(&device(output)));

        assert_eq!(name("VoiceMeeter AUX Input"), "VoiceMeeter Output");
        assert_eq!(name("Hi-Fi Cable Input"), "Hi-Fi Cable Output");
        assert_eq!(name("CABLE Input"), "CABLE Output");
        assert_eq!(game_microphone_name(None), "CABLE Output");
    }

    #[test]
    fn only_the_pinned_pack_is_accepted() {
        assert!(!is_pinned_pack(b"a pack someone tampered with"));
        assert!(!is_pinned_pack(b""));
    }

    #[test]
    fn the_pack_comes_from_the_vendor_over_https() {
        assert!(DRIVER_PACK_URL.starts_with("https://download.vb-audio.com/"));
    }

    #[test]
    fn the_pin_is_a_sha256_in_lower_case_hex() {
        assert_eq!(DRIVER_PACK_SHA256.len(), 64);
        assert!(
            DRIVER_PACK_SHA256
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
        );
    }

    #[test]
    fn only_the_setup_and_the_driver_files_are_unpacked() {
        for name in [
            "VBCABLE_Setup_x64.exe",
            "vbaudio_cable64_win10.sys",
            "vbaudio_cable64_win10.cat",
            "vbMmeCable64_win10.inf",
            "readme.txt",
        ] {
            assert!(is_expected_entry(name), "{name}");
        }
        for name in [
            "version.dll",
            "dwmapi.DLL",
            "other.exe",
            "../vbaudio.sys",
            "sub\\vbaudio.sys",
            "folder/vbaudio.sys",
            "noextension",
        ] {
            assert!(!is_expected_entry(name), "{name}");
        }
    }

    #[test]
    fn only_the_exact_vendor_name_is_trusted_whatever_its_case() {
        assert!(is_vendor_name("BUREL VINCENT"));
        assert!(is_vendor_name("burel vincent"));
        assert!(!is_vendor_name("BUREL VINCENTE"));
        assert!(!is_vendor_name("Burel Vincent Holdings Ltd"));
        assert!(!is_vendor_name("OU=\"CN=BUREL VINCENT\", CN=Mallory"));
        assert!(!is_vendor_name(""));
    }
}
