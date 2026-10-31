//! What is known about VB-CABLE and its vendor, kept free of Windows calls so any OS can test it.

use instantpad_application::ports::AudioDevice;

const FALLBACK_DRIVER_PACK: &str =
    "https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack45.zip";
const DRIVER_PACK_PREFIX: &str = "https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack";
const DRIVER_PACK_SUFFIX: &str = ".zip";

const VENDOR_SUBJECT: &str = "CN=BUREL VINCENT";

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

/// The newest driver pack linked on the product page, or the fixed pack when it links none.
pub fn driver_pack_url(page: &str) -> String {
    page.match_indices(DRIVER_PACK_PREFIX)
        .filter_map(|(start, _)| {
            let rest = &page[start + DRIVER_PACK_PREFIX.len()..];
            let digits = rest.find(|c: char| !c.is_ascii_digit())?;
            let version: u32 = rest[..digits].parse().ok()?;
            let link_end = start + DRIVER_PACK_PREFIX.len() + digits;
            page[link_end..]
                .starts_with(DRIVER_PACK_SUFFIX)
                .then(|| (version, &page[start..link_end + DRIVER_PACK_SUFFIX.len()]))
        })
        .max_by_key(|(version, _)| *version)
        .map_or_else(
            || FALLBACK_DRIVER_PACK.to_owned(),
            |(_, url)| url.to_owned(),
        )
}

pub fn is_vendor_subject(subject: &str) -> bool {
    subject
        .to_ascii_lowercase()
        .contains(&VENDOR_SUBJECT.to_ascii_lowercase())
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
    fn the_newest_driver_pack_on_the_page_wins() {
        let page = r#"
            <a href="https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack43.zip">old</a>
            <a href="https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack100.zip">new</a>
            <a href="https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack45.zip">mid</a>
        "#;

        assert_eq!(
            driver_pack_url(page),
            "https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack100.zip"
        );
    }

    #[test]
    fn a_page_with_no_driver_link_uses_the_fixed_pack() {
        let fallback = "https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_Pack45.zip";

        assert_eq!(driver_pack_url("<html></html>"), fallback);
        assert_eq!(driver_pack_url(""), fallback);
        assert_eq!(
            driver_pack_url(
                "https://download.vb-audio.com/Download_CABLE/VBCABLE_Driver_PackX.zip"
            ),
            fallback
        );
    }

    #[test]
    fn only_the_vendor_subject_is_trusted_whatever_its_case() {
        assert!(is_vendor_subject("CN=BUREL VINCENT, O=BUREL VINCENT, C=FR"));
        assert!(is_vendor_subject("cn=burel vincent"));
        assert!(!is_vendor_subject("CN=SOMEONE ELSE, O=BUREL VINCENT"));
    }
}
