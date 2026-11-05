//! Rules for cloning a voice: only from a sample long enough to be faithful. Voices designed
//! from cloned ones live and die with them.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

use crate::voice::{VoicePreset, VoiceTarget};

/// Below this, the sample is too short for the AI to catch the voice.
pub const MIN_SAMPLE_SECONDS: u32 = 20;
/// Above this, the sample adds nothing and only takes longer to analyze.
pub const MAX_SAMPLE_SECONDS: u32 = 300;
/// What the AI needs to know the user's own voice.
pub const MIN_CALIBRATION_SECONDS: u32 = 8;
/// How far the timbre of a designed voice moves toward the deep or the high preset at the end of
/// its range. Further, nothing is left of the voice it started from.
const TIMBRE_REACH: f32 = 0.6;
/// The timbre of the voice a design starts from, halfway between deeper and higher.
pub const DEFAULT_TIMBRE: f32 = 0.5;
/// A blend this small keeps the voice it starts from recognizable.
pub const DEFAULT_BLEND: f32 = 0.3;

/// What the AI hears as the identity of a voice.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Voiceprint(Vec<f32>);

impl Voiceprint {
    pub fn new(values: Vec<f32>) -> Self {
        Self(values)
    }

    pub fn values(&self) -> &[f32] {
        &self.0
    }

    /// The cosine of the angle between the two, from -1 to 1.
    pub fn similarity(&self, other: &Self) -> f32 {
        let dot: f32 = self.0.iter().zip(&other.0).map(|(a, b)| a * b).sum();
        let length = |values: &[f32]| values.iter().map(|value| value * value).sum::<f32>().sqrt();
        let lengths = length(&self.0) * length(&other.0);
        if lengths == 0.0 { 0.0 } else { dot / lengths }
    }

    /// `amount` of the way from this voice to `other`, from 0 to 1.
    pub fn toward(&self, other: &Self, amount: f32) -> Self {
        let amount = amount.clamp(0.0, 1.0);
        let values = self.0.iter().zip(&other.0);
        Self(values.map(|(a, b)| a + (b - a) * amount).collect())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClonedVoice {
    pub id: String,
    pub name: String,
    pub voiceprint: Voiceprint,
}

/// How a voice is designed from the cloned ones.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceRecipe {
    /// The cloned voice it starts from.
    pub base_id: String,
    // `#[specta(type = u32)]` exports these floats as a plain `number`, without the `| null`
    // specta adds for NaN, which they never are.
    /// From 0, deeper, to 1, higher. [`DEFAULT_TIMBRE`] leaves the voice as it is.
    #[specta(type = u32)]
    pub timbre: f32,
    /// Another cloned voice mixed in.
    pub blend_id: Option<String>,
    /// How much of the blended voice, from 0 to 1.
    #[specta(type = u32)]
    pub blend: f32,
}

/// A voice designed from cloned ones. It keeps the recipe rather than a voiceprint, so it
/// goes when the voices it is made of go.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesignedVoice {
    pub id: String,
    pub name: String,
    pub recipe: VoiceRecipe,
}

impl DesignedVoice {
    fn uses(&self, clone_id: &str) -> bool {
        let recipe = &self.recipe;
        recipe.base_id == clone_id || recipe.blend_id.as_deref() == Some(clone_id)
    }
}

/// Why a voice could not be designed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesignRefusal {
    MissingName,
    /// The recipe names a cloned voice that is not there.
    UnknownVoice,
}

/// A model trained on one voice, which speaks with it far more like it than a voiceprint does,
/// and needs a faster processor. The maintainers train it with the kit in `tools/voice-training`,
/// and the user adds it to one of the AI voices.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainedModel {
    /// The folder of its files, among the models of the app.
    pub id: String,
    /// What the training called it.
    pub name: String,
    /// Where its recordings came from.
    pub credit: String,
    /// The pitch its voice usually speaks at, which the user's voice is moved to.
    pub pitch_hz: f32,
}

impl TrainedModel {
    /// A folder name for a model being added, which no other model has.
    pub fn new_id() -> String {
        Uuid::new_v4().simple().to_string()
    }
}

/// The voices the AI knows: the user's own, the ones they cloned and the ones they designed.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct VoiceLibrary {
    /// The user's voice, which the AI removes before adding another.
    pub own: Option<Voiceprint>,
    pub clones: Vec<ClonedVoice>,
    pub designs: Vec<DesignedVoice>,
    /// The trained model of each voice that has one, by [`VoiceTarget::key`].
    pub trained: BTreeMap<String, TrainedModel>,
}

impl VoiceLibrary {
    fn clone_voice(&self, id: &str) -> Option<&ClonedVoice> {
        self.clones.iter().find(|voice| voice.id == id)
    }

    fn design(&self, id: &str) -> Option<&DesignedVoice> {
        self.designs.iter().find(|voice| voice.id == id)
    }

    /// The name of a cloned or designed voice.
    pub fn name(&self, id: &str) -> Option<&str> {
        let cloned = self.clone_voice(id).map(|voice| &voice.name);
        cloned
            .or_else(|| self.design(id).map(|voice| &voice.name))
            .map(String::as_str)
    }

    /// Who a cloned or designed voice sounds like. `preset` gives the voices that ship with the
    /// app, which move the timbre of a designed voice.
    pub fn voiceprint(
        &self,
        id: &str,
        preset: impl Fn(VoicePreset) -> Voiceprint,
    ) -> Option<Voiceprint> {
        match self.clone_voice(id) {
            Some(voice) => Some(voice.voiceprint.clone()),
            None => self.design_voiceprint(&self.design(id)?.recipe, preset),
        }
    }

    /// Who a recipe sounds like, or `None` when it names a cloned voice that is not there.
    pub fn design_voiceprint(
        &self,
        recipe: &VoiceRecipe,
        preset: impl Fn(VoicePreset) -> Voiceprint,
    ) -> Option<Voiceprint> {
        let mut voiceprint = self.clone_voice(&recipe.base_id)?.voiceprint.clone();
        if let Some(blend_id) = &recipe.blend_id {
            let blended = &self.clone_voice(blend_id)?.voiceprint;
            voiceprint = voiceprint.toward(blended, recipe.blend);
        }
        let offset = recipe.timbre - DEFAULT_TIMBRE;
        let toward = if offset < 0.0 {
            VoicePreset::Masculine
        } else {
            VoicePreset::Feminine
        };
        let amount = offset.abs() / DEFAULT_TIMBRE * TIMBRE_REACH;
        Some(voiceprint.toward(&preset(toward), amount))
    }

    /// The cloned voices, then the designed ones, each in the order it was made.
    pub fn ids(&self) -> Vec<&str> {
        let cloned = self.clones.iter().map(|voice| voice.id.as_str());
        cloned
            .chain(self.designs.iter().map(|voice| voice.id.as_str()))
            .collect()
    }

    /// Checks a recipe and names the voice. The caller adds it to `designs`.
    pub fn design_voice(
        &self,
        name: &str,
        recipe: VoiceRecipe,
    ) -> Result<DesignedVoice, DesignRefusal> {
        let name = name.trim();
        if name.is_empty() {
            return Err(DesignRefusal::MissingName);
        }
        let blend_is_known = recipe
            .blend_id
            .as_deref()
            .is_none_or(|id| self.clone_voice(id).is_some());
        if self.clone_voice(&recipe.base_id).is_none() || !blend_is_known {
            return Err(DesignRefusal::UnknownVoice);
        }
        // Mixing a voice with itself changes nothing.
        let blend_id = recipe.blend_id.filter(|id| *id != recipe.base_id);
        Ok(DesignedVoice {
            id: Uuid::new_v4().simple().to_string(),
            name: name.to_owned(),
            recipe: VoiceRecipe {
                base_id: recipe.base_id,
                timbre: recipe.timbre.clamp(0.0, 1.0),
                blend_id,
                blend: recipe.blend.clamp(0.0, 1.0),
            },
        })
    }

    /// Deletes a voice and every voice designed from it. Returns the ids of all that went.
    pub fn remove(&mut self, id: &str) -> Vec<String> {
        let mut removed = Vec::new();
        if self.clone_voice(id).is_some() {
            removed.push(id.to_owned());
        }
        self.clones.retain(|voice| voice.id != id);
        self.designs.retain(|voice| {
            let goes = voice.id == id || voice.uses(id);
            if goes {
                removed.push(voice.id.clone());
            }
            !goes
        });
        for id in &removed {
            self.trained.remove(id);
        }
        removed
    }

    /// The trained model of a voice, when it has one.
    pub fn trained_model(&self, target: &VoiceTarget) -> Option<&TrainedModel> {
        self.trained.get(&target.key())
    }

    /// Whether a trained model can be added to the voice: a voice that is there and is not
    /// designed, because a design is a recipe of other voices rather than a voice of its own.
    pub fn takes_trained_model(&self, target: &VoiceTarget) -> bool {
        match target {
            VoiceTarget::Preset(preset) => *preset == VoicePreset::Feminine,
            VoiceTarget::Own(id) => self.clone_voice(id).is_some(),
        }
    }
}

/// Why a voice could not be cloned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloneRefusal {
    MissingName,
}

impl ClonedVoice {
    /// Names the voice of a sample, which the caller adds to `clones`.
    pub fn clone_from(name: &str, sample: Voiceprint) -> Result<Self, CloneRefusal> {
        let name = name.trim();
        if name.is_empty() {
            return Err(CloneRefusal::MissingName);
        }
        Ok(Self {
            id: Uuid::new_v4().simple().to_string(),
            name: name.to_owned(),
            voiceprint: sample,
        })
    }
}
