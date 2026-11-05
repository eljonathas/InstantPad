use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use instantpad_application::message::{Failure, Message, TextKey};
use instantpad_application::ports::{AudioDevice, VirtualCable};

const SINK: &str = "InstantPad";
const SOURCE: &str = "InstantPad_Mic";
const PACTL_TIMEOUT: Duration = Duration::from_secs(3);

/// The app plays into a null sink, and the game reads its monitor through a remapped source,
/// which apps list as a microphone (a bare monitor is hidden from them). Both stay loaded
/// after the app closes, and the next start reuses them.
pub struct PulseCable;

impl PulseCable {
    /// Without `pactl`, or when it refuses, the cable simply finds nothing.
    #[cfg(target_os = "linux")]
    pub fn load() -> Self {
        // A crash can leave one half behind, and loading it again would show a second device.
        // So each half is looked for on its own.
        if !has_device("sinks", SINK) {
            pactl(&[
                "load-module",
                "module-null-sink",
                &format!("sink_name={SINK}"),
                &format!("sink_properties=device.description={SINK}"),
            ]);
        }
        if !has_device("sources", SOURCE) {
            pactl(&[
                "load-module",
                "module-remap-source",
                &format!("master={SINK}.monitor"),
                &format!("source_name={SOURCE}"),
                &format!("source_properties=device.description={SOURCE}"),
            ]);
        }
        Self
    }
}

/// Whether `pactl list short <kind>` has a device with exactly this name.
fn has_device(kind: &str, name: &str) -> bool {
    pactl(&["list", "short", kind]).is_some_and(|listing| listing_has_name(&listing, name))
}

/// A line of `pactl list short` is "index<TAB>name<TAB>driver...".
fn listing_has_name(listing: &str, name: &str) -> bool {
    listing
        .lines()
        .any(|line| line.split('\t').nth(1) == Some(name))
}

#[async_trait]
impl VirtualCable for PulseCable {
    fn product_name(&self) -> &'static str {
        "PulseAudio"
    }

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

/// The output of a `pactl` call that succeeded. A wedged sound server must not hold the app
/// back, so a call that takes too long is given up on.
fn pactl(args: &[&str]) -> Option<String> {
    let mut child = Command::new("pactl")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let started = Instant::now();
    let status = loop {
        match child.try_wait().ok()? {
            Some(status) => break status,
            None if started.elapsed() > PACTL_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            None => thread::sleep(Duration::from_millis(20)),
        }
    };
    let mut text = String::new();
    child.stdout.take()?.read_to_string(&mut text).ok()?;
    status.success().then(|| text.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use instantpad_test_support::contract;
    use instantpad_test_support::fakes::device;

    use super::*;

    #[test]
    fn the_pulse_cable_keeps_the_cable_contract() {
        contract::virtual_cable(
            &PulseCable,
            &device("pulseaudio:in", "InstantPad"),
            &device("pulseaudio:out", "InstantPad_Mic"),
            &device("pulseaudio:real", "Built-in Audio Analog Stereo"),
        );
    }

    #[test]
    fn the_game_reads_the_remapped_source() {
        assert_eq!(
            PulseCable.game_microphone_name(None),
            "InstantPad_Mic"
        );
    }

    #[test]
    fn a_device_is_found_by_its_exact_name_only() {
        let listing =
            "1\tInstantPad.monitor\tmodule-null-sink.c\n2\tInstantPad_Mic\tmodule-remap-source.c";

        assert!(listing_has_name(listing, "InstantPad_Mic"));
        assert!(!listing_has_name(listing, "InstantPad"));
        assert!(!listing_has_name(listing, "Mic"));
    }
}
