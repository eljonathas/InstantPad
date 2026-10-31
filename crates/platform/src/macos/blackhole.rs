use std::process::Command;

use async_trait::async_trait;
use instantpad_application::message::{Failure, Message, TextKey};
use instantpad_application::ports::{AudioDevice, VirtualCable};

const DOWNLOAD_PAGE: &str = "https://existential.audio/blackhole/";
const NAME: &str = "BlackHole";
const STEREO_NAME: &str = "BlackHole 2ch";

/// A macOS driver is not installed behind the user's back, so `install` only opens the
/// download page.
pub struct BlackHole;

#[async_trait]
impl VirtualCable for BlackHole {
    fn product_name(&self) -> &'static str {
        NAME
    }

    fn find_game_output(&self, outputs: &[AudioDevice]) -> Option<AudioDevice> {
        let named = |wanted: &str| outputs.iter().find(|device| device.name.contains(wanted));
        named(STEREO_NAME).or_else(|| named(NAME)).cloned()
    }

    fn is_virtual(&self, device: &AudioDevice) -> bool {
        device.name.contains(NAME)
    }

    fn game_microphone_name(&self, game_output: Option<&AudioDevice>) -> String {
        game_output
            .map_or(STEREO_NAME, |device| &device.name)
            .to_owned()
    }

    async fn install(&self, progress: &(dyn Fn(Message) + Send + Sync)) -> Result<(), Failure> {
        progress(Message::text(TextKey::DriverOpeningPage));
        let status = Command::new("/usr/bin/open")
            .arg(DOWNLOAD_PAGE)
            .status()
            .map_err(|error| Failure::because(TextKey::DriverFailed, error))?;
        if status.success() {
            Ok(())
        } else {
            Err(Failure::because(TextKey::DriverFailed, status))
        }
    }
}

#[cfg(test)]
mod tests {
    use instantpad_test_support::contract;
    use instantpad_test_support::fakes::device;

    use super::*;

    #[test]
    fn blackhole_keeps_the_cable_contract() {
        contract::virtual_cable(
            &BlackHole,
            &device("coreaudio:in", "BlackHole 2ch"),
            &device("coreaudio:out", "BlackHole 2ch"),
            &device("coreaudio:real", "MacBook Pro Speakers"),
        );
    }

    #[test]
    fn the_stereo_blackhole_is_preferred_over_the_others() {
        let outputs = [device("a", "BlackHole 16ch"), device("b", "BlackHole 2ch")];

        assert_eq!(
            BlackHole.find_game_output(&outputs),
            Some(outputs[1].clone())
        );
        assert_eq!(
            BlackHole.find_game_output(&outputs[..1]),
            Some(outputs[0].clone())
        );
    }

    #[test]
    fn the_game_microphone_is_the_chosen_device_or_the_stereo_blackhole() {
        assert_eq!(
            BlackHole.game_microphone_name(Some(&device("a", "BlackHole 16ch"))),
            "BlackHole 16ch"
        );
        assert_eq!(BlackHole.game_microphone_name(None), "BlackHole 2ch");
    }
}
