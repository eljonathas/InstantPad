use std::sync::{Arc, Mutex};

use crate::events::MixerView;
use crate::ports::AudioMix;
use crate::settings::Settings;

const METER_DECAY: f32 = 0.85;

/// The knobs, the meters and the mic switch.
pub struct MixerSession {
    mix: Arc<dyn AudioMix>,
    settings: Arc<Settings>,
    peaks: Mutex<(f32, f32)>,
}

impl MixerSession {
    pub fn new(mix: Arc<dyn AudioMix>, settings: Arc<Settings>) -> Self {
        {
            let current = settings.lock();
            mix.set_mic_volume(current.mic_volume as f32);
            mix.set_game_volume(current.game_volume as f32);
            mix.set_monitor_volume(current.monitor_volume as f32);
            mix.set_mic_enabled(current.mic_enabled);
        }
        Self {
            mix,
            settings,
            peaks: Mutex::default(),
        }
    }

    pub fn view(&self) -> MixerView {
        let settings = self.settings.lock();
        MixerView {
            mic_volume: settings.mic_volume,
            game_volume: settings.game_volume,
            monitor_volume: settings.monitor_volume,
            mic_enabled: settings.mic_enabled,
        }
    }

    pub fn set_mic_volume(&self, volume: f64) {
        let volume = volume.clamp(0.0, 1.0);
        self.settings.lock().mic_volume = volume;
        self.mix.set_mic_volume(volume as f32);
    }

    pub fn set_game_volume(&self, volume: f64) {
        let volume = volume.clamp(0.0, 1.0);
        self.settings.lock().game_volume = volume;
        self.mix.set_game_volume(volume as f32);
    }

    pub fn set_monitor_volume(&self, volume: f64) {
        let volume = volume.clamp(0.0, 1.0);
        self.settings.lock().monitor_volume = volume;
        self.mix.set_monitor_volume(volume as f32);
    }

    pub fn set_mic_enabled(&self, enabled: bool) {
        self.settings.lock().mic_enabled = enabled;
        self.mix.set_mic_enabled(enabled);
    }

    /// Mic and output levels for one frame. Peaks fall slowly, like the needle of a real meter.
    pub fn levels(&self) -> (f32, f32) {
        let mut peaks = self.peaks.lock().expect("meter lock poisoned");
        peaks.0 = self.mix.mic_level().max(peaks.0 * METER_DECAY);
        peaks.1 = self.mix.game_output_level().max(peaks.1 * METER_DECAY);

        // The square root spreads quiet levels over more LEDs.
        (peaks.0.sqrt(), peaks.1.sqrt())
    }
}
