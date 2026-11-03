//! Rules for training a model of a cloned voice inside the app: how much speech it learns from,
//! where it trains, for how long, and how much room it needs. See ADR 0007.

use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

/// Below this RVC does not learn the voice well.
pub const MIN_TRAINING_SECONDS: u32 = 10 * 60;
/// Above this the voice gets little better and the training only takes longer.
pub const MAX_TRAINING_SECONDS: u32 = 40 * 60;
/// A take longer than this would not fit in memory comfortably, so it is cut here.
pub const MAX_TAKE_SECONDS: u32 = 10 * 60;
/// A take shorter than this is a slip of the button.
pub const MIN_TAKE_SECONDS: u32 = 5;

/// A card with less memory than this cannot hold RVC's training, even a piece at a time.
pub const MIN_CARD_MEGABYTES: u32 = 4 * 1024;
/// The oldest NVIDIA drivers that run the CUDA 12 builds of PyTorch.
const MIN_DRIVER_WINDOWS: (u32, u32) = (528, 33);
const MIN_DRIVER_LINUX: (u32, u32) = (525, 60);

/// Passes over the recordings. On a card it takes about an hour for 40 minutes of speech, which
/// is what the developer's voices train with. On the processor half of it already takes more than
/// a day.
const CARD_EPOCHS: u32 = 200;
const PROCESSOR_EPOCHS: u32 = 100;
/// How often RVC saves, which is where a stopped training goes on from. An epoch on the processor
/// takes minutes, so it saves at every one.
const CARD_SAVE_EVERY: u32 = 5;
const PROCESSOR_SAVE_EVERY: u32 = 1;
/// Pieces learned at once on the processor, RVC's default.
const PROCESSOR_BATCH: u32 = 8;
/// The length of the pieces RVC cuts the speech into.
const PIECE_SECONDS: f32 = 3.7;
/// One step of 8 pieces on four cores of a 2.1 GHz processor, measured with the kit.
const PROCESSOR_STEP_SECONDS: f32 = 7.0;
/// An epoch of a minute of speech on a recent NVIDIA card, from the hour the kit takes.
const CARD_EPOCH_SECONDS_PER_MINUTE: f32 = 0.45;
/// Cutting the speech and finding its pitch and words, for each minute of it.
const CARD_PREPARE_SECONDS_PER_MINUTE: f32 = 3.0;
const PROCESSOR_PREPARE_SECONDS_PER_MINUTE: f32 = 60.0;
/// What a training writes besides the tools: the pieces, their pitch and words, RVC's saves and
/// the exported voice.
pub const TRAINING_WORK_MEGABYTES: u32 = 3 * 1024;

/// What the system says about an NVIDIA card.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GraphicsCard {
    pub name: String,
    pub memory_megabytes: u32,
    /// Such as "610.88".
    pub driver: String,
}

/// The system the app runs on, which sets the oldest driver that works.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum System {
    Windows,
    Linux,
}

/// Why a training runs on the processor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ProcessorReason {
    /// No NVIDIA card, which includes every AMD and Intel one.
    NoCard,
    SmallCard,
    OldDriver,
}

/// Where the training runs, which also sets which build of PyTorch it needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TrainingDevice {
    Card,
    Processor { reason: ProcessorReason },
}

impl TrainingDevice {
    /// The card when it can hold the training and its driver runs CUDA 12, the processor
    /// otherwise.
    pub fn choose(card: Option<&GraphicsCard>, system: System) -> Self {
        let Some(card) = card else {
            return Self::Processor {
                reason: ProcessorReason::NoCard,
            };
        };
        if card.memory_megabytes < MIN_CARD_MEGABYTES {
            return Self::Processor {
                reason: ProcessorReason::SmallCard,
            };
        }
        let oldest = match system {
            System::Windows => MIN_DRIVER_WINDOWS,
            System::Linux => MIN_DRIVER_LINUX,
        };
        if driver_version(&card.driver).is_none_or(|version| version < oldest) {
            return Self::Processor {
                reason: ProcessorReason::OldDriver,
            };
        }
        Self::Card
    }

    pub fn is_card(self) -> bool {
        self == Self::Card
    }
}

/// "610.88" as (610, 88). `None` when it cannot be read, which counts as too old.
fn driver_version(driver: &str) -> Option<(u32, u32)> {
    let mut numbers = driver.trim().split('.');
    let major = numbers.next()?.parse().ok()?;
    let minor = numbers.next().map_or(Some(0), |minor| minor.parse().ok())?;
    Some((major, minor))
}

/// How a voice is trained.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TrainingPlan {
    pub device: TrainingDevice,
    pub epochs: u32,
    /// Pieces learned at once, as many as the memory of the card holds.
    pub batch: u32,
    pub save_every: u32,
}

impl TrainingPlan {
    pub fn new(device: TrainingDevice, card: Option<&GraphicsCard>) -> Self {
        match device {
            TrainingDevice::Card => Self {
                device,
                epochs: CARD_EPOCHS,
                batch: card.map_or(4, |card| card_batch(card.memory_megabytes)),
                save_every: CARD_SAVE_EVERY,
            },
            TrainingDevice::Processor { .. } => Self {
                device,
                epochs: PROCESSOR_EPOCHS,
                batch: PROCESSOR_BATCH,
                save_every: PROCESSOR_SAVE_EVERY,
            },
        }
    }

    /// How long the whole training takes for `speech_seconds` of speech, before it starts. Once
    /// it runs, [`TrainingPlan::remaining_seconds`] knows better.
    pub fn estimated_seconds(&self, speech_seconds: u32) -> u32 {
        let minutes = speech_seconds as f32 / 60.0;
        let (prepare, epoch) = match self.device {
            TrainingDevice::Card => (
                CARD_PREPARE_SECONDS_PER_MINUTE * minutes,
                CARD_EPOCH_SECONDS_PER_MINUTE * minutes,
            ),
            TrainingDevice::Processor { .. } => {
                let steps = speech_seconds as f32 / PIECE_SECONDS / PROCESSOR_BATCH as f32;
                (
                    PROCESSOR_PREPARE_SECONDS_PER_MINUTE * minutes,
                    steps.ceil() * PROCESSOR_STEP_SECONDS,
                )
            }
        };
        (prepare + epoch * self.epochs as f32).round() as u32
    }

    /// What is left after `epoch`, from how long the epochs took so far.
    pub fn remaining_seconds(&self, epoch: u32, seconds_per_epoch: f32) -> u32 {
        (self.epochs.saturating_sub(epoch) as f32 * seconds_per_epoch.max(0.0)).round() as u32
    }
}

/// RVC holds about a piece in memory for each gigabyte past the first two.
fn card_batch(memory_megabytes: u32) -> u32 {
    (memory_megabytes / 1024).saturating_sub(2).clamp(2, 8)
}

/// What a training needs on disk, in megabytes: the tools when they are missing, the download
/// while it unpacks, and the work of the training.
pub fn needed_megabytes(download: u32, installed: u32, tools_are_installed: bool) -> u32 {
    let tools = if tools_are_installed {
        0
    } else {
        download + installed
    };
    tools + TRAINING_WORK_MEGABYTES
}

/// How many megabytes are missing for a training, if any.
pub fn missing_megabytes(needed: u32, free: Option<u64>) -> Option<u32> {
    // A disk that cannot be asked is not refused: the training says itself when it runs out.
    let free = free?;
    let missing = u64::from(needed).saturating_sub(free);
    (missing > 0).then(|| u32::try_from(missing).unwrap_or(u32::MAX))
}

/// A stretch of speech the voice learns from, recorded or taken from a file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Take {
    /// Also the name of its file in the training's folder.
    pub id: String,
    pub seconds: u32,
}

/// Why a take was not kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TakeRefusal {
    TooShort,
    /// The training has all the speech it takes.
    Full,
}

/// A training of a cloned voice, from its first take to the model. It is saved, so a training
/// stopped by closing the app goes on when the user asks.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainingJob {
    /// The folder of the training, which holds its takes and what RVC makes of them.
    pub id: String,
    /// The cloned voice the model is for.
    pub clone_id: String,
    pub plan: TrainingPlan,
    pub takes: Vec<Take>,
    /// Set once RVC has started: the takes can no longer change.
    pub is_started: bool,
    /// The last epoch RVC finished.
    pub epoch: u32,
}

impl TrainingJob {
    pub fn new(clone_id: &str, plan: TrainingPlan) -> Self {
        Self {
            id: format!("voice-{}", Uuid::new_v4().simple()),
            clone_id: clone_id.to_owned(),
            plan,
            takes: Vec::new(),
            is_started: false,
            epoch: 0,
        }
    }

    pub fn seconds(&self) -> u32 {
        self.takes.iter().map(|take| take.seconds).sum()
    }

    /// How much of a take of `seconds` is kept: all of it, or what still fits. Never more than
    /// [`MAX_TAKE_SECONDS`].
    pub fn room_for(&self, seconds: u32) -> Result<u32, TakeRefusal> {
        let room = MAX_TRAINING_SECONDS.saturating_sub(self.seconds());
        if room == 0 {
            return Err(TakeRefusal::Full);
        }
        let kept = seconds.min(room).min(MAX_TAKE_SECONDS);
        if kept < MIN_TAKE_SECONDS {
            return Err(TakeRefusal::TooShort);
        }
        Ok(kept)
    }

    /// Names a take of `seconds`, which the caller has checked with [`TrainingJob::room_for`].
    pub fn add_take(&mut self, seconds: u32) -> Take {
        let take = Take {
            id: format!("take-{}", Uuid::new_v4().simple()),
            seconds,
        };
        self.takes.push(take.clone());
        take
    }

    /// Takes a take out, and returns it, while the training has not started.
    pub fn remove_take(&mut self, id: &str) -> Option<Take> {
        if self.is_started {
            return None;
        }
        let index = self.takes.iter().position(|take| take.id == id)?;
        Some(self.takes.remove(index))
    }

    pub fn has_enough(&self) -> bool {
        self.seconds() >= MIN_TRAINING_SECONDS
    }
}
