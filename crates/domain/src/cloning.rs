//! Rules for cloning a voice: only with the consent of its owner, who proves it by speaking a
//! random phrase into the microphone, and only from a sample long enough to be faithful. Voices
//! designed from cloned ones live and die with them.

use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

use crate::voice::VoicePreset;

/// Raise it when the terms change, which deletes every voice cloned under older terms.
pub const TERMS_VERSION: u32 = 1;
/// Below this, the sample is too short for the AI to catch the voice.
pub const MIN_SAMPLE_SECONDS: u32 = 20;
/// Above this, the sample adds nothing and only takes longer to analyze.
pub const MAX_SAMPLE_SECONDS: u32 = 300;
/// What the AI needs to know the user's own voice.
pub const MIN_CALIBRATION_SECONDS: u32 = 8;
/// Above this, a recording of the phrase is someone who never stopped it.
pub const MAX_PHRASE_SECONDS: u32 = 20;
const CHALLENGE_LENGTH: usize = 6;
/// Two recordings of one person score above this, two people almost never do.
const SAME_SPEAKER: f32 = 0.80;
/// A digit the speech recognition missed or misheard is forgiven.
const PHRASE_TOLERANCE: usize = 1;
/// How far the timbre of a designed voice moves toward the deep or the high preset at the end of
/// its range. Further, nothing is left of the voice it started from.
const TIMBRE_REACH: f32 = 0.6;

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

    pub fn is_same_speaker(&self, other: &Self) -> bool {
        self.similarity(other) >= SAME_SPEAKER
    }

    /// `amount` of the way from this voice to `other`, from 0 to 1.
    pub fn toward(&self, other: &Self, amount: f32) -> Self {
        let amount = amount.clamp(0.0, 1.0);
        let values = self.0.iter().zip(&other.0);
        Self(values.map(|(a, b)| a + (b - a) * amount).collect())
    }
}

/// The record that the owner of a voice agreed to have it cloned.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Consent {
    pub owner_name: String,
    pub terms_version: u32,
    /// Seconds since the Unix epoch.
    pub accepted_at: u64,
    /// How alike the sample and the spoken phrase were, which proves the owner was there.
    pub similarity: f32,
    /// SHA-256 of the sample, which is deleted once the voice is cloned.
    pub sample_sha256: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClonedVoice {
    pub id: String,
    pub name: String,
    pub voiceprint: Voiceprint,
    pub consent: Consent,
}

impl ClonedVoice {
    /// The owner agreed to the terms in force.
    pub fn has_current_consent(&self) -> bool {
        self.consent.terms_version == TERMS_VERSION
    }
}

/// How a voice is designed from the cloned ones.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct VoiceRecipe {
    /// The cloned voice it starts from.
    pub base_id: String,
    /// From -1, deeper, to 1, higher.
    #[specta(type = u32)]
    pub timbre: f32,
    /// Another cloned voice mixed in.
    pub blend_id: Option<String>,
    /// How much of the blended voice, from 0 to 1.
    #[specta(type = u32)]
    pub blend: f32,
}

/// A voice designed from cloned ones. It keeps the recipe rather than a voiceprint, so it
/// follows the consent of the voices it is made of.
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

/// The voices the AI knows: the user's own, the ones they cloned and the ones they designed.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct VoiceLibrary {
    /// The user's voice, which the AI removes before adding another.
    pub own: Option<Voiceprint>,
    pub clones: Vec<ClonedVoice>,
    pub designs: Vec<DesignedVoice>,
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
        let toward = if recipe.timbre < 0.0 {
            VoicePreset::Masculine
        } else {
            VoicePreset::Feminine
        };
        Some(voiceprint.toward(&preset(toward), recipe.timbre.abs() * TIMBRE_REACH))
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
        Ok(DesignedVoice {
            id: Uuid::new_v4().simple().to_string(),
            name: name.to_owned(),
            recipe: VoiceRecipe {
                timbre: recipe.timbre.clamp(-1.0, 1.0),
                blend: recipe.blend.clamp(0.0, 1.0),
                ..recipe
            },
        })
    }

    /// Deletes the voices cloned under older terms, since their owners never agreed to the new
    /// ones, and the voices designed from them. Returns whether any was deleted.
    pub fn drop_outdated(&mut self) -> bool {
        let outdated: Vec<String> = self
            .clones
            .iter()
            .filter(|voice| !voice.has_current_consent())
            .map(|voice| voice.id.clone())
            .collect();
        for id in &outdated {
            self.remove(id);
        }
        !outdated.is_empty()
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
        removed
    }
}

/// Why a voice could not be cloned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloneRefusal {
    /// The person who spoke the phrase is not the one in the sample.
    NotTheSameSpeaker,
    /// The phrase heard is not the one asked for.
    WrongPhrase,
    MissingName,
}

/// Everything gathered while cloning, checked together before the voice is kept.
pub struct CloneRequest<'a> {
    pub name: &'a str,
    pub owner_name: &'a str,
    pub sample: &'a Voiceprint,
    pub sample_sha256: &'a str,
    pub phrase: &'a Voiceprint,
    pub challenge: &'a Challenge,
    pub transcript: &'a str,
    /// Seconds since the Unix epoch.
    pub now: u64,
}

impl CloneRequest<'_> {
    /// Checks the proof of consent before anything is kept.
    pub fn verify(&self) -> Result<(), CloneRefusal> {
        if !self.challenge.is_answered_by(self.transcript) {
            return Err(CloneRefusal::WrongPhrase);
        }
        if !self.sample.is_same_speaker(self.phrase) {
            return Err(CloneRefusal::NotTheSameSpeaker);
        }
        Ok(())
    }

    pub fn clone_voice(&self) -> Result<ClonedVoice, CloneRefusal> {
        self.verify()?;
        let (name, owner_name) = (self.name.trim(), self.owner_name.trim());
        if name.is_empty() || owner_name.is_empty() {
            return Err(CloneRefusal::MissingName);
        }
        Ok(ClonedVoice {
            id: Uuid::new_v4().simple().to_string(),
            name: name.to_owned(),
            voiceprint: self.sample.clone(),
            consent: Consent {
                owner_name: owner_name.to_owned(),
                terms_version: TERMS_VERSION,
                accepted_at: self.now,
                similarity: self.sample.similarity(self.phrase),
                sample_sha256: self.sample_sha256.to_owned(),
            },
        })
    }
}

/// Random digits the owner of the voice reads aloud. A recording of them cannot exist yet, so
/// whoever says them is at the microphone now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Challenge(String);

impl Challenge {
    /// Digits from 1 to 9 with no digit twice in a row, which speech recognition tends to merge.
    pub fn new() -> Self {
        let mut digits = String::with_capacity(CHALLENGE_LENGTH);
        let mut random = Uuid::new_v4().as_u128();
        while digits.len() < CHALLENGE_LENGTH {
            let digit = char::from(b'1' + (random % 9) as u8);
            random /= 9;
            if random == 0 {
                random = Uuid::new_v4().as_u128();
            }
            if !digits.ends_with(digit) {
                digits.push(digit);
            }
        }
        Self(digits)
    }

    pub fn digits(&self) -> &str {
        &self.0
    }

    pub fn is_answered_by(&self, transcript: &str) -> bool {
        edit_distance(&spoken_digits(transcript), &self.0) <= PHRASE_TOLERANCE
    }
}

impl Default for Challenge {
    fn default() -> Self {
        Self::new()
    }
}

/// The digits in what was heard, written as numbers or as words in Portuguese or English.
fn spoken_digits(transcript: &str) -> String {
    let word_digit = |word: &str| match word {
        "zero" => Some('0'),
        "um" | "uma" | "one" => Some('1'),
        "dois" | "duas" | "two" => Some('2'),
        "três" | "tres" | "three" => Some('3'),
        "quatro" | "four" => Some('4'),
        "cinco" | "five" => Some('5'),
        "seis" | "meia" | "six" => Some('6'),
        "sete" | "seven" => Some('7'),
        "oito" | "eight" => Some('8'),
        "nove" | "nine" => Some('9'),
        _ => None,
    };

    let lower = transcript.to_lowercase();
    let mut digits = String::new();
    let mut word = String::new();
    for character in lower.chars().chain([' ']) {
        if character.is_alphabetic() {
            word.push(character);
            continue;
        }
        digits.extend(word_digit(&word));
        word.clear();
        if character.is_ascii_digit() {
            digits.push(character);
        }
    }
    digits
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, x) in a.chars().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, &y) in b.iter().enumerate() {
            let substitution = diagonal + usize::from(x != y);
            diagonal = row[j + 1];
            row[j + 1] = substitution.min(row[j] + 1).min(diagonal + 1);
        }
    }
    row[b.len()]
}
