use std::process::Command;

use async_trait::async_trait;
use instantpad_application::message::{Failure, Message, TextKey};
use instantpad_application::ports::{AudioDevice, VirtualCable};

const SINK: &str = "InstantPad";
const SOURCE: &str = "InstantPad_Mic";

/// The app plays into a null sink, and the game reads its monitor through a remapped source,
/// which apps list as a microphone (a bare monitor is hidden from them).
#[derive(Default)]
pub struct PulseCable {
    /// Ids of the modules this cable loaded, unloaded when it is dropped.
    modules: Vec<String>,
}

impl PulseCable {
    /// Without `pactl`, or when it refuses, the cable simply finds nothing.
    #[cfg(target_os = "linux")]
    pub fn load() -> Self {
        let modules = [
            pactl(&[
                "load-module",
                "module-null-sink",
                &format!("sink_name={SINK}"),
                &format!("sink_properties=device.description={SINK}"),
            ]),
            pactl(&[
                "load-module",
                "module-remap-source",
                &format!("master={SINK}.monitor"),
                &format!("source_name={SOURCE}"),
                &format!("source_properties=device.description={SOURCE}"),
            ]),
        ];
        Self {
            modules: modules.into_iter().flatten().collect(),
        }
    }
}

impl Drop for PulseCable {
    fn drop(&mut self) {
        for module in self.modules.iter().rev() {
            pactl(&["unload-module", module]);
        }
    }
}

#[async_trait]
impl VirtualCable for PulseCable {
    fn find_game_output(&self, outputs: &[AudioDevice]) -> Option<AudioDevice> {
        outputs
            .iter()
            .find(|device| self.is_virtual(device))
            .cloned()
    }

    fn is_virtual(&self, device: &AudioDevice) -> bool {
        device.name.contains(SINK)
    }

    fn game_microphone_name(&self, _game_output: Option<&AudioDevice>) -> String {
        SOURCE.to_owned()
    }

    /// The cable is created by the app itself, so there is nothing to download.
    async fn install(&self, _progress: &(dyn Fn(Message) + Send + Sync)) -> Result<(), Failure> {
        if pactl(&["--version"]).is_some() {
            Ok(())
        } else {
            Err(Failure::because(TextKey::DriverFailed, "pactl not found"))
        }
    }
}

/// The output of a `pactl` call that succeeded.
fn pactl(args: &[&str]) -> Option<String> {
    let output = Command::new("pactl").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

#[cfg(test)]
mod tests {
    use instantpad_test_support::contract;
    use instantpad_test_support::fakes::device;

    use super::*;

    #[test]
    fn the_pulse_cable_keeps_the_cable_contract() {
        contract::virtual_cable(
            &PulseCable::default(),
            &device("pulseaudio:in", "InstantPad"),
            &device("pulseaudio:out", "InstantPad_Mic"),
            &device("pulseaudio:real", "Built-in Audio Analog Stereo"),
        );
    }

    #[test]
    fn the_game_reads_the_remapped_source() {
        assert_eq!(
            PulseCable::default().game_microphone_name(None),
            "InstantPad_Mic"
        );
    }
}
