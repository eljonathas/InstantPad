use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::cloning::{TrainedModel, Voiceprint};

const THIN_SEMITONES: (f64, f64) = (2.0, 9.0);
const DEEP_SEMITONES: (f64, f64) = (-2.0, -9.0);
const THIN_FORMANT: (f64, f64) = (0.0, 5.0);
const DEEP_FORMANT: (f64, f64) = (0.0, -5.0);
const CYCLE_SECONDS: (f64, f64) = (0.2, 6.0);
const CYCLE_REACH: f64 = 12.0;
/// A woman's voice sits about half an octave above a man's, and her resonances a little higher.
const FEMININE_SEMITONES: (f64, f64) = (3.0, 7.0);
const FEMININE_FORMANT: (f64, f64) = (1.0, 4.0);
const FEMININE_BRIGHTNESS: f32 = 0.5;
const GHOST_SEMITONES: (f64, f64) = (-4.0, 4.0);
const GHOST_TREMOLO: f32 = 0.3;
const OGRE_SEMITONES: (f64, f64) = (-4.0, -10.0);
const OGRE_FORMANT: (f64, f64) = (-2.0, -7.0);
/// The AI keeps the melody of the voice, so a higher pitch first makes the result convincing.
const AI_SEMITONES: (f64, f64) = (0.0, 8.0);
const CLONE_SEMITONES: (f64, f64) = (-8.0, 8.0);
/// A trained model already moves the voice to its own pitch, and this moves it further.
const TRAINED_SEMITONES: (f64, f64) = (-12.0, 12.0);

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
    Feminine,
    Ghost,
    Ogre,
    FeminineAi,
    /// One of the user's voices, cloned or designed, named by [`VoiceSettings::clone_id`].
    Cloned,
}

impl VoiceEffect {
    /// The effects that come with the app, in the order they are shown.
    pub const BUILT_IN: [Self; 8] = [
        Self::Natural,
        Self::Thin,
        Self::Deep,
        Self::PitchCycle,
        Self::Feminine,
        Self::Ghost,
        Self::Ogre,
        Self::FeminineAi,
    ];

    /// Runs through the AI models, which must be downloaded first.
    pub fn needs_ai(self) -> bool {
        matches!(self, Self::FeminineAi | Self::Cloned)
    }
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
    FeminineIntensity,
    FeminineFormant,
    GhostWhisper,
    GhostEcho,
    GhostPitch,
    OgreDepth,
    OgreGrowl,
    OgreFormant,
    FeminineAiPitch,
    ClonePitch,
    /// The pitch of a trained model, from its own.
    TrainedPitch,
}

impl VoiceParam {
    pub fn default_value(self) -> f64 {
        match self {
            Self::CycleTime => 0.3,
            Self::GhostWhisper | Self::GhostEcho => 0.6,
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

/// The character added after the pitch. Every amount goes from 0, untouched, to 1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VoiceColor {
    /// Replaces the tone of the voice with breath.
    pub whisper: f32,
    /// A long, hollow reverb.
    pub echo: f32,
    /// A slow wobble of the volume.
    pub tremolo: f32,
    /// Distortion and a second voice an octave below.
    pub growl: f32,
    /// Lifts the high frequencies.
    pub brightness: f32,
}

/// What the audio engine does to the voice, before any AI conversion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoiceChain {
    pub pitch: PitchShape,
    /// Moves the resonances of the voice apart from its pitch, in semitones.
    pub formant: f32,
    pub color: VoiceColor,
}

impl VoiceChain {
    /// The voice as it is, for a trained model, which sets the pitch itself.
    pub fn untouched() -> Self {
        Self {
            pitch: PitchShape::Fixed { semitones: 0.0 },
            formant: 0.0,
            color: VoiceColor::default(),
        }
    }
}

/// A voice the app ships for the AI. The masculine one only moves the timbre of designed voices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoicePreset {
    Feminine,
    Masculine,
}

/// Whose voice the AI turns the user's into.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VoiceTarget {
    Preset(VoicePreset),
    /// One of the user's voices, cloned or designed.
    Own(String),
}

impl VoiceTarget {
    /// What the settings and the library know the voice by.
    pub fn key(&self) -> String {
        match self {
            Self::Preset(VoicePreset::Feminine) => "feminine".to_owned(),
            Self::Preset(VoicePreset::Masculine) => "masculine".to_owned(),
            Self::Own(id) => id.clone(),
        }
    }
}

/// How the AI speaks with a voice.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum VoiceEngine {
    /// Takes the timbre of the voice from a few seconds of it and puts it on the user's, as the
    /// voice is being spoken. Runs on any computer, sounds like the voice only roughly.
    #[default]
    Instant,
    /// A model trained on the voice, which sounds far more like it and needs a faster processor.
    Trained,
}

/// What the AI does to the voice.
#[derive(Clone, Debug, PartialEq)]
pub enum Conversion {
    /// The timbre of `source`, the user, replaced with that of `target`.
    Timbre {
        source: Voiceprint,
        target: Voiceprint,
    },
    /// The trained model speaks, at the pitch of its voice moved by `semitones`.
    Trained { model: TrainedModel, semitones: f32 },
}

impl Conversion {
    /// Whether the two speak with the same voice and differ at most in pitch, which a
    /// conversion follows without starting over.
    pub fn same_voice(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Trained { model, .. }, Self::Trained { model: other, .. }) => model == other,
            _ => self == other,
        }
    }

    /// How far a trained model moves the pitch, which stays out of the comparison above.
    pub fn semitones(&self) -> Option<f32> {
        match self {
            Self::Trained { semitones, .. } => Some(*semitones),
            Self::Timbre { .. } => None,
        }
    }
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
    /// The cloned voice that [`VoiceEffect::Cloned`] speaks with, kept while another effect is on.
    pub clone_id: Option<String>,
    values: BTreeMap<VoiceParam, f64>,
    /// The engine picked for each voice that has a trained model, by [`VoiceTarget::key`].
    engines: BTreeMap<String, VoiceEngine>,
}

impl Default for VoiceSettings {
    fn default() -> Self {
        Self {
            effect: VoiceEffect::Natural,
            last_effect: VoiceEffect::Thin,
            clone_id: None,
            values: BTreeMap::new(),
            engines: BTreeMap::new(),
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

    /// The engine the AI speaks with `target` through. Only a voice with a trained model can
    /// use it, and does unless the user picked the instant one: it is why the model was added.
    pub fn engine(&self, target: &VoiceTarget, has_trained_model: bool) -> VoiceEngine {
        if !has_trained_model {
            return VoiceEngine::Instant;
        }
        self.engines
            .get(&target.key())
            .copied()
            .unwrap_or(VoiceEngine::Trained)
    }

    pub fn set_engine(&mut self, target: &VoiceTarget, engine: VoiceEngine) {
        self.engines.insert(target.key(), engine);
    }

    /// Picks a built-in effect. A cloned voice goes through [`Self::select_clone`].
    pub fn select(&mut self, effect: VoiceEffect) {
        if effect == VoiceEffect::Cloned && self.clone_id.is_none() {
            return;
        }
        self.effect = effect;
        if effect != VoiceEffect::Natural {
            self.last_effect = effect;
        }
    }

    pub fn select_clone(&mut self, id: &str) {
        self.clone_id = Some(id.to_owned());
        self.select(VoiceEffect::Cloned);
    }

    /// Forgets a cloned voice that was deleted, so nothing points at it anymore.
    pub fn forget_clone(&mut self, id: &str) {
        self.engines.remove(id);
        if self.clone_id.as_deref() != Some(id) {
            return;
        }
        self.clone_id = None;
        if self.effect == VoiceEffect::Cloned {
            self.effect = VoiceEffect::Natural;
        }
        if self.last_effect == VoiceEffect::Cloned {
            self.last_effect = Self::default().last_effect;
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

    /// The voice after the current one, leaving out the natural voice. The built-in effects
    /// come first, then the cloned voices in `clones`.
    pub fn select_next(&mut self, clones: &[&str]) {
        let built_in = VoiceEffect::BUILT_IN[1..]
            .iter()
            .map(|&effect| (effect, None));
        let cloned = clones.iter().map(|&id| (VoiceEffect::Cloned, Some(id)));
        let voices: Vec<_> = built_in.chain(cloned).collect();

        let current = (self.effect, self.clone_id.as_deref());
        let next = voices
            .iter()
            .position(|voice| match voice {
                (VoiceEffect::Cloned, id) => current.0 == VoiceEffect::Cloned && *id == current.1,
                (effect, _) => *effect == current.0,
            })
            .map_or(0, |index| (index + 1) % voices.len());

        match voices[next] {
            (_, Some(id)) => self.select_clone(id),
            (effect, None) => self.select(effect),
        }
    }

    /// `None` is the natural voice, which goes through untouched.
    pub fn chain(&self) -> Option<VoiceChain> {
        let semitones = |range, param| along(range, self.get(param)) as f32;
        let amount = |param| self.get(param) as f32;
        let fixed = |semitones| PitchShape::Fixed { semitones };
        let plain = |pitch, formant| VoiceChain {
            pitch,
            formant,
            color: VoiceColor::default(),
        };

        Some(match self.effect {
            VoiceEffect::Natural => return None,
            VoiceEffect::Thin => plain(
                fixed(semitones(THIN_SEMITONES, VoiceParam::ThinIntensity)),
                semitones(THIN_FORMANT, VoiceParam::ThinFormant),
            ),
            VoiceEffect::Deep => plain(
                fixed(semitones(DEEP_SEMITONES, VoiceParam::DeepIntensity)),
                semitones(DEEP_FORMANT, VoiceParam::DeepFormant),
            ),
            VoiceEffect::PitchCycle => plain(
                PitchShape::Cycle {
                    low: -(CYCLE_REACH * self.get(VoiceParam::CycleLow)) as f32,
                    high: (CYCLE_REACH * self.get(VoiceParam::CycleHigh)) as f32,
                    seconds: along(CYCLE_SECONDS, self.get(VoiceParam::CycleTime)) as f32,
                },
                0.0,
            ),
            VoiceEffect::Feminine => VoiceChain {
                pitch: fixed(semitones(FEMININE_SEMITONES, VoiceParam::FeminineIntensity)),
                formant: semitones(FEMININE_FORMANT, VoiceParam::FeminineFormant),
                color: VoiceColor {
                    brightness: FEMININE_BRIGHTNESS,
                    ..VoiceColor::default()
                },
            },
            VoiceEffect::Ghost => VoiceChain {
                pitch: fixed(semitones(GHOST_SEMITONES, VoiceParam::GhostPitch)),
                formant: 0.0,
                color: VoiceColor {
                    whisper: amount(VoiceParam::GhostWhisper),
                    echo: amount(VoiceParam::GhostEcho),
                    tremolo: GHOST_TREMOLO,
                    ..VoiceColor::default()
                },
            },
            VoiceEffect::Ogre => VoiceChain {
                pitch: fixed(semitones(OGRE_SEMITONES, VoiceParam::OgreDepth)),
                formant: semitones(OGRE_FORMANT, VoiceParam::OgreFormant),
                color: VoiceColor {
                    growl: amount(VoiceParam::OgreGrowl),
                    ..VoiceColor::default()
                },
            },
            VoiceEffect::FeminineAi => plain(
                fixed(semitones(AI_SEMITONES, VoiceParam::FeminineAiPitch)),
                0.0,
            ),
            VoiceEffect::Cloned => self.own_voice_chain(),
        })
    }

    /// What the engine does before the AI speaks with one of the user's voices.
    pub fn own_voice_chain(&self) -> VoiceChain {
        let semitones = along(CLONE_SEMITONES, self.get(VoiceParam::ClonePitch)) as f32;
        VoiceChain {
            pitch: PitchShape::Fixed { semitones },
            formant: 0.0,
            color: VoiceColor::default(),
        }
    }

    /// How far a trained model moves the pitch from that of its voice.
    pub fn trained_semitones(&self) -> f32 {
        along(TRAINED_SEMITONES, self.get(VoiceParam::TrainedPitch)) as f32
    }

    /// Whose voice the AI speaks with, or `None` when the effect needs no AI.
    pub fn target(&self) -> Option<VoiceTarget> {
        match self.effect {
            VoiceEffect::FeminineAi => Some(VoiceTarget::Preset(VoicePreset::Feminine)),
            VoiceEffect::Cloned => self.clone_id.clone().map(VoiceTarget::Own),
            _ => None,
        }
    }

    /// The value of a setting as the user reads it.
    pub fn describe(&self, param: VoiceParam) -> String {
        let value = self.get(param);
        let semitones = |number: f64| format!("{number:+.1} st");
        let percent = || format!("{:.0}%", value * 100.0);
        match param {
            VoiceParam::ThinIntensity => semitones(along(THIN_SEMITONES, value)),
            VoiceParam::DeepIntensity => semitones(along(DEEP_SEMITONES, value)),
            VoiceParam::ThinFormant => semitones(along(THIN_FORMANT, value)),
            VoiceParam::DeepFormant => semitones(along(DEEP_FORMANT, value)),
            VoiceParam::CycleTime => format!("{:.1} s", along(CYCLE_SECONDS, value)),
            VoiceParam::CycleHigh => semitones(CYCLE_REACH * value),
            VoiceParam::CycleLow => semitones(-CYCLE_REACH * value),
            VoiceParam::FeminineIntensity => semitones(along(FEMININE_SEMITONES, value)),
            VoiceParam::FeminineFormant => semitones(along(FEMININE_FORMANT, value)),
            VoiceParam::GhostPitch => semitones(along(GHOST_SEMITONES, value)),
            VoiceParam::OgreDepth => semitones(along(OGRE_SEMITONES, value)),
            VoiceParam::OgreFormant => semitones(along(OGRE_FORMANT, value)),
            VoiceParam::FeminineAiPitch => semitones(along(AI_SEMITONES, value)),
            VoiceParam::ClonePitch => semitones(along(CLONE_SEMITONES, value)),
            VoiceParam::TrainedPitch => semitones(along(TRAINED_SEMITONES, value)),
            VoiceParam::GhostWhisper | VoiceParam::GhostEcho | VoiceParam::OgreGrowl => percent(),
        }
    }
}
