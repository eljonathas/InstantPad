use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use instantpad_domain::palette;
use instantpad_domain::voice::{VoiceEffect, VoiceParam, VoiceSettings};

use crate::events::{AppEvent, EffectView, ParamView, VoiceIcon, VoiceView};
use crate::message::{Message, TextKey};
use crate::ports::{EventSink, VoiceChanger};
use crate::settings::Settings;

struct EffectInfo {
    effect: VoiceEffect,
    name: TextKey,
    icon: VoiceIcon,
    color: &'static str,
    params: &'static [(VoiceParam, TextKey)],
}

const EFFECTS: [EffectInfo; 4] = [
    EffectInfo {
        effect: VoiceEffect::Natural,
        name: TextKey::EffectNatural,
        icon: VoiceIcon::Mic,
        color: palette::PADS[4],
        params: &[],
    },
    EffectInfo {
        effect: VoiceEffect::Thin,
        name: TextKey::EffectThin,
        icon: VoiceIcon::Feather,
        color: palette::PADS[4],
        params: &[
            (VoiceParam::ThinIntensity, TextKey::ParamIntensity),
            (VoiceParam::ThinFormant, TextKey::ParamFormant),
        ],
    },
    EffectInfo {
        effect: VoiceEffect::Deep,
        name: TextKey::EffectDeep,
        icon: VoiceIcon::Mountain,
        color: palette::PADS[6],
        params: &[
            (VoiceParam::DeepIntensity, TextKey::ParamIntensity),
            (VoiceParam::DeepFormant, TextKey::ParamFormant),
        ],
    },
    EffectInfo {
        effect: VoiceEffect::PitchCycle,
        name: TextKey::EffectCycle,
        icon: VoiceIcon::Wave,
        color: palette::PADS[3],
        params: &[
            (VoiceParam::CycleTime, TextKey::ParamTime),
            (VoiceParam::CycleHigh, TextKey::ParamHigh),
            (VoiceParam::CycleLow, TextKey::ParamLow),
        ],
    },
];

fn effect_view(info: &EffectInfo, voice: &VoiceSettings) -> EffectView {
    EffectView {
        effect: info.effect,
        name: info.name,
        icon: info.icon,
        color: info.color.to_owned(),
        params: info
            .params
            .iter()
            .map(|&(param, label)| ParamView {
                param,
                label,
                value: voice.get(param),
                default_value: param.default_value(),
                text: voice.describe(param),
            })
            .collect(),
    }
}

/// The effect on the voice, which keeps working on both screens of the app.
pub struct VoiceSession {
    changer: Arc<dyn VoiceChanger>,
    settings: Arc<Settings>,
    events: Arc<dyn EventSink>,
    /// Not saved: a speaker near the mic would howl as soon as the app opens.
    is_hearing: AtomicBool,
}

impl VoiceSession {
    pub fn new(
        changer: Arc<dyn VoiceChanger>,
        settings: Arc<Settings>,
        events: Arc<dyn EventSink>,
    ) -> Self {
        changer.set_voice(settings.lock().voice.shift());
        Self {
            changer,
            settings,
            events,
            is_hearing: AtomicBool::new(false),
        }
    }

    pub fn view(&self) -> VoiceView {
        let settings = self.settings.lock();
        VoiceView {
            active: settings.voice.effect,
            effects: EFFECTS
                .iter()
                .map(|info| effect_view(info, &settings.voice))
                .collect(),
            is_hearing: self.is_hearing.load(Ordering::Relaxed),
        }
    }

    pub fn select(&self, effect: VoiceEffect) {
        self.settings.lock().voice.select(effect);
        self.apply();
    }

    pub fn set_param(&self, param: VoiceParam, value: f64) {
        self.settings.lock().voice.set(param, value);
        self.apply();
    }

    pub fn set_hearing(&self, enabled: bool) {
        self.is_hearing.store(enabled, Ordering::Relaxed);
        self.changer.set_hearing_voice(enabled);
        self.publish();
    }

    /// For the key that switches the effect on and off from inside a game.
    pub fn toggle(&self) {
        self.settings.lock().voice.toggle();
        self.apply();
        self.announce();
    }

    /// For the key that moves to the next effect from inside a game.
    pub fn select_next(&self) {
        self.settings.lock().voice.select_next();
        self.apply();
        self.announce();
    }

    fn apply(&self) {
        let shift = self.settings.lock().voice.shift();
        self.changer.set_voice(shift);
        self.publish();
    }

    fn announce(&self) {
        let active = self.settings.lock().voice.effect;
        if let Some(info) = EFFECTS.iter().find(|info| info.effect == active) {
            self.events.emit(AppEvent::show(Message::text(info.name)));
        }
    }

    fn publish(&self) {
        self.events.emit(AppEvent::Voice(self.view()));
    }
}
