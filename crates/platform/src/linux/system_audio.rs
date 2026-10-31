use std::process::Command;

use instantpad_application::ports::SystemAudio;

use crate::cpal_defaults;

pub struct LinuxSystemAudio;

impl SystemAudio for LinuxSystemAudio {
    fn default_microphone_id(&self) -> Option<String> {
        cpal_defaults::default_microphone_id()
    }

    fn default_output_id(&self) -> Option<String> {
        cpal_defaults::default_output_id()
    }

    fn system_output_id(&self) -> Option<String> {
        cpal_defaults::default_output_id()
    }

    fn open_sound_settings(&self) {
        let _ = Command::new("xdg-open").arg("settings://sound").status();
    }
}
