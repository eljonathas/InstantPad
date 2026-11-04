//! Every port the use cases depend on. Adapters live in other crates.

use std::path::PathBuf;

use async_trait::async_trait;
use instantpad_domain::board::PadBoard;
use instantpad_domain::cloning::{TrainedModel, VoiceLibrary, Voiceprint};
use instantpad_domain::cuts::Cut;
use instantpad_domain::hotkeys::Hotkey;
use instantpad_domain::sounds::SoundSource;
use instantpad_domain::training::{
    GraphicsCard, System, TrainingDevice, TrainingJob, TrainingPlan,
};
use instantpad_domain::voice::{Conversion, VoiceChain, VoicePreset};
use serde::Serialize;
use specta::Type;

use crate::events::AppEvent;
use crate::hotkeys::HotkeyCommand;
use crate::message::{Failure, Message};
use crate::settings::AppSettings;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Type)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
}

/// Identifies a playback so it can be stopped or queried later.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlaybackTag(pub u64);

impl PlaybackTag {
    /// FNV-1a, so the same id always gives the same tag.
    pub fn of(id: &str) -> Self {
        Self(id.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        }))
    }
}

/// A sound as the cut editor draws it.
#[derive(Clone, Debug, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Waveform {
    pub duration_ms: u32,
    /// From 0 to 1 of the loudest moment of the whole sound.
    #[specta(type = Vec<u32>)]
    pub peaks: Vec<f32>,
}

/// The sound the display follows.
#[derive(Clone, Debug, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct NowPlaying {
    /// How far the sound has played, in seconds.
    #[specta(type = u32)]
    pub position: f32,
    /// How long the sound lasts, in seconds.
    #[specta(type = u32)]
    pub duration: f32,
    /// The loudest moment of each stretch of the sound, from 0 to 1.
    #[specta(type = Vec<u32>)]
    pub peaks: Vec<f32>,
}

/// Plays sounds.
#[async_trait]
pub trait SoundPlayer: Send + Sync {
    /// Decodes ahead of time, so the first press of a pad has no delay.
    async fn preload(&self, path: &str) -> Result<(), Failure>;

    /// Plays for the game and for you, only `cut` when there is one. Pressing again restarts
    /// the sound.
    async fn play(
        &self,
        path: &str,
        volume: f32,
        tag: PlaybackTag,
        cut: Option<Cut>,
    ) -> Result<(), Failure>;

    /// Plays only for you, to try a sound out, only `cut` when there is one.
    async fn preview(
        &self,
        path: &str,
        volume: f32,
        tag: PlaybackTag,
        cut: Option<Cut>,
    ) -> Result<(), Failure>;

    /// How long the sound lasts, and how loud each of `bars` stretches from `start_ms` to
    /// `end_ms` gets, at the scale of the whole sound, for drawing it.
    async fn waveform(
        &self,
        path: &str,
        start_ms: u32,
        end_ms: u32,
        bars: u32,
    ) -> Result<Waveform, Failure>;

    /// How loud the sound is along the way, one value from 0 to 1 per `frame_ms`.
    async fn levels(&self, path: &str, frame_ms: u32) -> Result<Vec<f32>, Failure>;

    /// Drops the decoded audio, for when the file on disk has changed.
    fn forget(&self, path: &str);

    fn stop(&self, tag: PlaybackTag);

    fn stop_all(&self);

    fn is_playing(&self, tag: PlaybackTag) -> bool;

    /// The sound started last among the ones still playing.
    fn now_playing(&self) -> Option<NowPlaying>;
}

/// The three places the audio flows through, each on a device of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DeviceRole {
    Microphone,
    GameOutput,
    MonitorOutput,
}

impl DeviceRole {
    pub const ALL: [Self; 3] = [Self::Microphone, Self::GameOutput, Self::MonitorOutput];
}

/// Why the stream of a role stopped without being asked to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamStop {
    /// The device went away or the system reconfigured it, as when a headset reconnects or its
    /// format changes. Opening it again may work.
    Lost,
    /// Anything else, which opening it again would not fix.
    Failed,
}

/// Chooses which physical devices the audio flows through.
#[async_trait]
pub trait AudioRouter: Send + Sync {
    fn list_microphones(&self) -> Vec<AudioDevice>;

    fn list_outputs(&self) -> Vec<AudioDevice>;

    /// A driver in a bad state can block forever, so these give up after a few seconds
    /// with `AudioDeviceNotResponding`, and one stuck device never holds up the others.
    async fn set_microphone(&self, device_id: Option<&str>) -> Result<(), Failure>;

    async fn set_game_output(&self, device_id: Option<&str>) -> Result<(), Failure>;

    async fn set_monitor_output(&self, device_id: Option<&str>) -> Result<(), Failure>;

    /// The roles whose stream stopped by itself since the last call, each told once. A role
    /// closed or opened again on purpose tells nothing.
    fn take_stopped(&self) -> Vec<(DeviceRole, StreamStop)>;
}

/// Live mix controls and levels. Device failures reach the user through [`EventSink`].
pub trait AudioMix: Send + Sync {
    fn set_mic_volume(&self, volume: f32);

    fn set_game_volume(&self, volume: f32);

    fn set_monitor_volume(&self, volume: f32);

    fn set_mic_enabled(&self, enabled: bool);

    fn mic_level(&self) -> f32;

    fn game_output_level(&self) -> f32;
}

/// Changes the voice on its way to the game.
pub trait VoiceChanger: Send + Sync {
    /// `None` and `None` is the natural voice, which gets through with no added delay. A
    /// `conversion` hands the voice to the AI after the chain.
    fn set_voice(&self, chain: Option<VoiceChain>, conversion: Option<Conversion>);

    /// Sends the voice to the headphones too, so the user can hear what the effect does.
    fn set_hearing_voice(&self, enabled: bool);
}

/// Keeps what the microphone hears, before any effect, for the AI to learn a voice from.
#[async_trait]
pub trait VoiceRecorder: Send + Sync {
    /// Starts over, dropping anything recorded before.
    fn start_recording(&self);

    /// Mono at 48 kHz. Empty when no microphone was open.
    fn stop_recording(&self) -> Vec<f32>;

    /// What has been recorded so far, mono at 48 kHz, while the recording goes on.
    fn recorded(&self) -> Vec<f32>;

    /// Reads an audio file the user chose, as mono at 48 kHz.
    async fn decode_file(&self, path: &str) -> Result<Vec<f32>, Failure>;
}

/// The AI voice models. They are far bigger than the app, so they download on demand.
#[async_trait]
pub trait VoiceModels: Send + Sync {
    fn is_installed(&self) -> bool;

    /// What the user is about to download, rounded up.
    fn download_megabytes(&self) -> u32;

    /// Downloads and checks every file. Reports 0 to 100.
    async fn install(&self, progress: &(dyn Fn(u8) + Send + Sync)) -> Result<(), Failure>;
}

/// Models trained on a voice with the kit in `tools/voice-training`, which the user adds to one
/// of the AI voices.
#[async_trait]
pub trait TrainedVoiceModels: Send + Sync {
    /// Checks the voice the kit exported to `folder` and keeps a copy of it as the model `id`.
    /// The AI models must be installed, because the check loads it.
    async fn add(&self, folder: PathBuf, id: String) -> Result<TrainedModel, Failure>;

    /// Deletes the files of a model no voice uses anymore.
    fn remove(&self, id: &str);
}

/// The tools a training needs, for one kind of device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrainingTools {
    pub is_installed: bool,
    /// What is still to download, rounded up.
    pub download_megabytes: u32,
    /// What they take on disk once installed.
    pub installed_megabytes: u32,
}

/// What a training is doing, as the kit reports it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum TrainingStage {
    /// Cutting the speech into pieces.
    Cutting,
    /// Finding the pitch of each piece.
    Pitch,
    /// Finding the words of each piece.
    Words,
    /// Going through the pieces, epoch after epoch.
    Learning,
    /// Writing and checking the voice for the app.
    Exporting,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TrainingProgress {
    Stage(TrainingStage),
    /// `epoch` is done; the first one reported may be where a stopped training went on from.
    Epoch {
        epoch: u32,
        seconds: f32,
    },
}

/// How a long piece of work ended without failing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome<T> {
    Done(T),
    /// [`VoiceTrainer::stop`] ended it, and it can go on later.
    Stopped,
}

/// A training the trainer runs.
#[derive(Clone, Debug, PartialEq)]
pub struct TrainingRun {
    /// The folder of the training, from [`instantpad_domain::training::TrainingJob::id`].
    pub job: String,
    pub plan: TrainingPlan,
    /// The name the model gets.
    pub title: String,
}

/// Trains models of voices on this computer, with tools it downloads when the user agrees. See
/// ADR 0007. Speech is mono at 48 kHz.
#[async_trait]
pub trait VoiceTrainer: Send + Sync {
    /// Whether the app trains voices on this system at all.
    fn is_supported(&self) -> bool;

    /// The system the training runs on, which sets the oldest driver of a card that works.
    fn system(&self) -> System;

    /// The NVIDIA card, when the system has one with a driver.
    async fn graphics_card(&self) -> Option<GraphicsCard>;

    fn tools(&self, device: TrainingDevice) -> TrainingTools;

    /// How much room the installed tools take, to tell the user what removing them frees.
    fn installed_megabytes(&self) -> u32;

    /// Free room where the training lives, or `None` when the system does not say.
    fn free_megabytes(&self) -> Option<u64>;

    /// Downloads and checks the tools for `device`, picking up a download that stopped.
    /// Reports 0 to 100.
    async fn install(
        &self,
        device: TrainingDevice,
        progress: &(dyn Fn(u8) + Send + Sync),
    ) -> Result<Outcome<()>, Failure>;

    /// Ends the install or the training that runs, which then returns [`Outcome::Stopped`].
    fn stop(&self);

    /// Deletes the tools. A training of a voice keeps its own folder.
    fn remove_tools(&self);

    /// Writes a take of speech to the folder of the training `job`.
    async fn keep_take(&self, job: &str, take: &str, speech: Vec<f32>) -> Result<(), Failure>;

    fn remove_take(&self, job: &str, take: &str);

    /// Trains the voice from the takes of the job, going on from its last save, and exports it.
    /// Returns the folder `TrainedVoiceModels::add` takes.
    async fn train(
        &self,
        run: &TrainingRun,
        progress: &(dyn Fn(TrainingProgress) + Send + Sync),
    ) -> Result<Outcome<PathBuf>, Failure>;

    /// Deletes everything of the training `job`: its takes, what RVC made of them and its model.
    fn discard(&self, job: &str);
}

/// Keeps the training in progress, so it goes on after the app closes.
pub trait TrainingRepository: Send + Sync {
    fn load(&self) -> Option<TrainingJob>;

    fn save(&self, job: Option<&TrainingJob>) -> Result<(), Failure>;
}

/// Listens to speech with the AI models, which must be installed. Speech is mono at 48 kHz.
#[async_trait]
pub trait VoiceAnalyzer: Send + Sync {
    async fn voiceprint(&self, speech: Vec<f32>) -> Result<Voiceprint, Failure>;

    /// The voiceprint of a voice that ships with the app.
    fn preset(&self, preset: VoicePreset) -> Voiceprint;
}

/// Turns one voice into another, a stretch at a time. It is far slower than the audio
/// callbacks, so the engine runs it on a thread of its own. Speech is mono at 48 kHz.
pub trait SpeechConverter: Send + Sync {
    /// Starts over with new speakers, dropping anything half converted. Cheap: the work, and
    /// any failure, comes with the next `convert`.
    fn start(&self, conversion: &Conversion);

    /// Moves the pitch of the trained voice that speaks, without starting over, which a knob
    /// asks for at every step.
    fn retune(&self, _semitones: f32) {}

    /// Takes the next stretch of voice and returns the converted voice that is ready, which
    /// comes out later and in larger pieces than it went in.
    fn convert(&self, speech: &[f32]) -> Result<Vec<f32>, Failure>;
}

/// How the operating system itself has its audio configured.
pub trait SystemAudio: Send + Sync {
    fn default_microphone_id(&self) -> Option<String>;

    fn default_output_id(&self) -> Option<String>;

    /// Where the system sends the sound of every other app.
    fn system_output_id(&self) -> Option<String>;

    fn open_sound_settings(&self);
}

/// The virtual cable that carries the mix into the game.
#[async_trait]
pub trait VirtualCable: Send + Sync {
    /// What the user installs to get the cable.
    fn product_name(&self) -> &'static str;

    fn find_game_output(&self, outputs: &[AudioDevice]) -> Option<AudioDevice>;

    fn is_virtual(&self, device: &AudioDevice) -> bool;

    /// The device the player must choose as microphone in the game.
    fn game_microphone_name(&self, game_output: Option<&AudioDevice>) -> String;

    /// Downloads the driver and opens its setup, which asks the user for elevation.
    async fn install(&self, progress: &(dyn Fn(Message) + Send + Sync)) -> Result<(), Failure>;
}

/// Registers system-wide keys. The adapter hands pressed commands to the composition root.
pub trait HotkeyHost: Send + Sync {
    /// Returns false when another program already owns the key.
    fn register(&self, hotkey: &Hotkey, command: HotkeyCommand) -> bool;

    fn clear(&self);
}

pub trait BoardRepository: Send + Sync {
    fn load(&self) -> PadBoard;

    fn save(&self, board: &PadBoard) -> Result<(), Failure>;
}

pub trait FavoritesRepository: Send + Sync {
    fn load(&self) -> Vec<SoundSource>;

    fn save(&self, favorites: &[SoundSource]) -> Result<(), Failure>;
}

pub trait SettingsRepository: Send + Sync {
    fn load(&self) -> AppSettings;

    fn save(&self, settings: &AppSettings) -> Result<(), Failure>;
}

pub trait VoiceLibraryRepository: Send + Sync {
    fn load(&self) -> VoiceLibrary;

    fn save(&self, library: &VoiceLibrary) -> Result<(), Failure>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogSound {
    pub name: String,
    pub sound_url: String,
    pub color: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogPage {
    pub sounds: Vec<CatalogSound>,
    pub has_more: bool,
}

/// The online sound library.
#[async_trait]
pub trait SoundCatalog: Send + Sync {
    async fn search(&self, query: &str, page: u32) -> Result<CatalogPage, Failure>;

    /// The sounds trending in `region`, a lower-case country code such as "br".
    async fn trending(&self, region: &str, page: u32) -> Result<CatalogPage, Failure>;

    async fn download(&self, sound_url: &str, destination: &str) -> Result<(), Failure>;
}

/// The folder where the app keeps its audio files, and the one for sounds only heard so far.
pub trait SoundFileStore: Send + Sync {
    fn path_for_download(&self, sound_url: &str) -> String;

    /// Where a sound that is only being tried out lives until the next clean-up.
    fn path_for_preview(&self, sound_url: &str) -> String;

    /// Moves a sound being tried out to where it stays.
    fn keep_preview(&self, preview_path: &str, stored_path: &str) -> Result<(), Failure>;

    /// Deletes the sounds that were only tried out.
    fn clear_previews(&self);

    /// Copies an outside file into the store and returns its new path.
    fn import(&self, source_path: &str) -> Result<String, Failure>;

    /// Copies a stored sound to a place the user chose.
    fn export(&self, stored_path: &str, destination: &str) -> Result<(), Failure>;

    /// Deletes a stored sound. Anything outside the store is left alone.
    fn delete(&self, stored_path: &str);

    fn exists(&self, path: &str) -> bool;
}

/// Finds, downloads and applies new versions of the app.
#[async_trait]
pub trait AppUpdater: Send + Sync {
    fn current_version(&self) -> String;

    /// Local builds must not replace themselves.
    fn is_release_build(&self) -> bool;

    /// Returns the version to install, or `None` when the app is up to date.
    async fn find_update(&self) -> Result<Option<String>, Failure>;

    /// Keeps the new version ready without touching the running app. Reports 0 to 100.
    async fn download(&self, progress: &(dyn Fn(u8) + Send + Sync)) -> Result<(), Failure>;

    /// Applying closes the app, so this only runs on a restart or on exit.
    fn apply(&self, restart: bool) -> Result<(), Failure>;
}

/// Tells the interface what changed.
pub trait EventSink: Send + Sync {
    fn emit(&self, event: AppEvent);
}
