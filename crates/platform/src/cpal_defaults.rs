//! The default devices as cpal reports them, for the OSes that have a single default.

use cpal::traits::{DeviceTrait, HostTrait};

pub fn default_microphone_id() -> Option<String> {
    let device = cpal::default_host().default_input_device()?;
    Some(device.id().ok()?.to_string())
}

pub fn default_output_id() -> Option<String> {
    let device = cpal::default_host().default_output_device()?;
    Some(device.id().ok()?.to_string())
}
