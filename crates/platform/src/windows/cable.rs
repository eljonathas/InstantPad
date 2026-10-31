use async_trait::async_trait;
use instantpad_application::message::{Failure, Message};
use instantpad_application::ports::{AudioDevice, VirtualCable};

use super::{installer, vb_audio};

pub struct VbCable;

#[async_trait]
impl VirtualCable for VbCable {
    fn find_game_output(&self, outputs: &[AudioDevice]) -> Option<AudioDevice> {
        vb_audio::find_game_output(outputs)
    }

    fn is_virtual(&self, device: &AudioDevice) -> bool {
        vb_audio::is_virtual(device)
    }

    fn game_microphone_name(&self, game_output: Option<&AudioDevice>) -> String {
        vb_audio::game_microphone_name(game_output).to_owned()
    }

    async fn install(&self, progress: &(dyn Fn(Message) + Send + Sync)) -> Result<(), Failure> {
        installer::install(progress).await
    }
}

#[cfg(test)]
mod tests {
    use instantpad_test_support::contract;
    use instantpad_test_support::fakes::device;

    use super::*;

    #[test]
    fn the_vb_cable_keeps_the_cable_contract() {
        contract::virtual_cable(
            &VbCable,
            &device("wasapi:in", "CABLE Input (VB-Audio Virtual Cable)"),
            &device("wasapi:out", "CABLE Output (VB-Audio Virtual Cable)"),
            &device("wasapi:real", "Speakers (Realtek Audio)"),
        );
    }
}
