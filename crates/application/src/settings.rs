use std::sync::{Arc, Mutex, MutexGuard};

use instantpad_domain::hotkeys::{Hotkey, NumericScheme};
use serde::{Deserialize, Serialize};

use crate::message::Failure;
use crate::ports::SettingsRepository;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppSettings {
    pub microphone_id: Option<String>,
    pub game_output_id: Option<String>,
    pub monitor_output_id: Option<String>,
    pub mic_volume: f64,
    pub game_volume: f64,
    pub monitor_volume: f64,
    pub mic_enabled: bool,
    pub active_interface_id: Option<String>,
    /// Language tag such as "en" or "pt". `None` follows the system language.
    pub language: Option<String>,
    pub numeric_scheme: NumericScheme,
    /// Overrides the stop key of the numeric scheme when set.
    pub stop_hotkey: Option<Hotkey>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            microphone_id: None,
            game_output_id: None,
            monitor_output_id: None,
            mic_volume: 1.0,
            game_volume: 0.8,
            monitor_volume: 0.6,
            mic_enabled: true,
            active_interface_id: None,
            language: None,
            numeric_scheme: NumericScheme::Numpad,
            stop_hotkey: None,
        }
    }
}

/// The settings in memory, shared by the use cases.
pub struct Settings {
    current: Mutex<AppSettings>,
    repository: Arc<dyn SettingsRepository>,
}

impl Settings {
    pub fn load(repository: Arc<dyn SettingsRepository>) -> Self {
        Self {
            current: Mutex::new(repository.load()),
            repository,
        }
    }

    pub fn lock(&self) -> MutexGuard<'_, AppSettings> {
        self.current.lock().expect("settings lock poisoned")
    }

    pub fn save(&self) -> Result<(), Failure> {
        self.repository.save(&self.lock())
    }
}
