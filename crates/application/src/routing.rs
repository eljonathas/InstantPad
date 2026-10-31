use crate::ports::{AudioDevice, VirtualCable};

#[derive(Clone, Copy, Debug, Default)]
pub struct SavedRouting<'a> {
    pub microphone_id: Option<&'a str>,
    pub game_output_id: Option<&'a str>,
    pub monitor_output_id: Option<&'a str>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultDevices<'a> {
    pub microphone_id: Option<&'a str>,
    pub output_id: Option<&'a str>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RoutingChoice {
    pub microphone: Option<AudioDevice>,
    pub game_output: Option<AudioDevice>,
    pub monitor_output: Option<AudioDevice>,
}

/// Each role excludes the mistakes that wreck the sound: a mic reading the cable feeds the mix
/// back into itself, and a real device as the game output sends the mic and every sound to
/// your headphones a second time.
pub fn real_devices(cable: &dyn VirtualCable, devices: &[AudioDevice]) -> Vec<AudioDevice> {
    devices
        .iter()
        .filter(|device| !cable.is_virtual(device))
        .cloned()
        .collect()
}

pub fn virtual_devices(cable: &dyn VirtualCable, devices: &[AudioDevice]) -> Vec<AudioDevice> {
    devices
        .iter()
        .filter(|device| cable.is_virtual(device))
        .cloned()
        .collect()
}

pub fn choose(
    cable: &dyn VirtualCable,
    microphones: &[AudioDevice],
    outputs: &[AudioDevice],
    saved: SavedRouting<'_>,
    defaults: DefaultDevices<'_>,
) -> RoutingChoice {
    let usable_microphones = real_devices(cable, microphones);
    let game_outputs = virtual_devices(cable, outputs);
    let monitor_outputs = real_devices(cable, outputs);

    RoutingChoice {
        microphone: with_id(&usable_microphones, saved.microphone_id)
            .or_else(|| with_id(&usable_microphones, defaults.microphone_id))
            .or_else(|| usable_microphones.first().cloned()),
        game_output: with_id(&game_outputs, saved.game_output_id)
            .or_else(|| cable.find_game_output(&game_outputs)),
        monitor_output: with_id(&monitor_outputs, saved.monitor_output_id)
            .or_else(|| with_id(&monitor_outputs, defaults.output_id))
            .or_else(|| monitor_outputs.first().cloned()),
    }
}

fn with_id(devices: &[AudioDevice], id: Option<&str>) -> Option<AudioDevice> {
    let id = id?;
    devices.iter().find(|device| device.id == id).cloned()
}
