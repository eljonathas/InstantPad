use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use specta::Type;

const THIN_SEMITONES: (f64, f64) = (2.0, 9.0);
const DEEP_SEMITONES: (f64, f64) = (-2.0, -9.0);
const THIN_FORMANT: (f64, f64) = (0.0, 5.0);
const DEEP_FORMANT: (f64, f64) = (0.0, -5.0);
const CYCLE_SECONDS: (f64, f64) = (0.2, 6.0);
const CYCLE_REACH: f64 = 12.0;

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "camelCase")]
pub enum VoiceEffect {
    #[default]
    Natural,
    Thin,
    Deep,
    PitchCycle,
}

impl VoiceEffect {
    pub const ALL: [Self; 4] = [Self::Natural, Self::Thin, Self::Deep, Self::PitchCycle];
}

/// A setting of an effect. Every value goes from 0 to 1, which is what a knob gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum VoiceParam {
    ThinIntensity,
    ThinFormant,
    DeepIntensity,
    DeepFormant,
    CycleTime,
    CycleHigh,
    CycleLow,
}

impl VoiceParam {
    pub fn default_value(self) -> f64 {
        match self {
            Self::CycleTime => 0.3,
            _ => 0.5,
        }
    }
}

/// How the pitch moves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PitchShape {
    Fixed {
        semitones: f32,
    },
    /// Rises to `high` and falls to `low` once every `seconds`.
    Cycle {
        low: f32,
        high: f32,
        seconds: f32,
    },
}

/// What the audio engine does to the voice.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoiceShift {
    pub pitch: PitchShape,
    /// Moves the resonances of the voice apart from its pitch, in semitones.
    pub formant: f32,
}

fn along((from, to): (f64, f64), position: f64) -> f64 {
    from + (to - from) * position
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct VoiceSettings {
    pub effect: VoiceEffect,
    /// What switching the effects back on returns to.
    pub last_effect: VoiceEffect,
    values: BTreeMap<VoiceParam, f64>,
}

impl Default for VoiceSettings {
    fn default() -> Self {
        Self {
            effect: VoiceEffect::Natural,
            last_effect: VoiceEffect::Thin,
            values: BTreeMap::new(),
        }
    }
}

impl VoiceSettings {
    pub fn get(&self, param: VoiceParam) -> f64 {
        self.values
            .get(&param)
            .copied()
            .unwrap_or(param.default_value())
    }

    pub fn set(&mut self, param: VoiceParam, value: f64) {
        self.values.insert(param, value.clamp(0.0, 1.0));
    }

    pub fn select(&mut self, effect: VoiceEffect) {
        self.effect = effect;
        if effect != VoiceEffect::Natural {
            self.last_effect = effect;
        }
    }

    /// Off goes back to the last effect, and any effect goes off.
    pub fn toggle(&mut self) {
        let next = match self.effect {
            VoiceEffect::Natural => self.last_effect,
            _ => VoiceEffect::Natural,
        };
        self.select(next);
    }

    /// The effect after the current one, leaving out the natural voice.
    pub fn select_next(&mut self) {
        let effects = &VoiceEffect::ALL[1..];
        let current = effects.iter().position(|effect| *effect == self.effect);
        let next = current.map_or(0, |index| (index + 1) % effects.len());
        self.select(effects[next]);
    }

    /// `None` is the natural voice, which goes through untouched.
    pub fn shift(&self) -> Option<VoiceShift> {
        let semitones = |range, param| along(range, self.get(param)) as f32;
        let fixed = |semitones| PitchShape::Fixed { semitones };

        match self.effect {
            VoiceEffect::Natural => None,
            VoiceEffect::Thin => Some(VoiceShift {
                pitch: fixed(semitones(THIN_SEMITONES, VoiceParam::ThinIntensity)),
                formant: semitones(THIN_FORMANT, VoiceParam::ThinFormant),
            }),
            VoiceEffect::Deep => Some(VoiceShift {
                pitch: fixed(semitones(DEEP_SEMITONES, VoiceParam::DeepIntensity)),
                formant: semitones(DEEP_FORMANT, VoiceParam::DeepFormant),
            }),
            VoiceEffect::PitchCycle => Some(VoiceShift {
                pitch: PitchShape::Cycle {
                    low: -(CYCLE_REACH * self.get(VoiceParam::CycleLow)) as f32,
                    high: (CYCLE_REACH * self.get(VoiceParam::CycleHigh)) as f32,
                    seconds: along(CYCLE_SECONDS, self.get(VoiceParam::CycleTime)) as f32,
                },
                formant: 0.0,
            }),
        }
    }

    /// The value of a setting as the user reads it.
    pub fn describe(&self, param: VoiceParam) -> String {
        let value = self.get(param);
        let semitones = |number: f64| format!("{number:+.1} st");
        match param {
            VoiceParam::ThinIntensity => semitones(along(THIN_SEMITONES, value)),
            VoiceParam::DeepIntensity => semitones(along(DEEP_SEMITONES, value)),
            VoiceParam::ThinFormant => semitones(along(THIN_FORMANT, value)),
            VoiceParam::DeepFormant => semitones(along(DEEP_FORMANT, value)),
            VoiceParam::CycleTime => format!("{:.1} s", along(CYCLE_SECONDS, value)),
            VoiceParam::CycleHigh => semitones(CYCLE_REACH * value),
            VoiceParam::CycleLow => semitones(-CYCLE_REACH * value),
        }
    }
}
