use std::process::Command;
use std::thread;

use instantpad_application::ports::SystemAudio;

use crate::cpal_defaults;

const SETTINGS_TOOLS: &[(&str, &[&str])] = &[
    ("gnome-control-center", &["sound"]),
    ("systemsettings", &["kcm_pulseaudio"]),
    ("pavucontrol", &[]),
];

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
        // No desktop answers to a common address for this, so each one's own tool is tried.
        for (program, args) in SETTINGS_TOOLS {
            if let Ok(mut child) = Command::new(program).args(*args).spawn() {
                // Waiting on another thread, so the window is not held and no zombie is left.
                thread::spawn(move || child.wait());
                return;
            }
        }
    }
}
