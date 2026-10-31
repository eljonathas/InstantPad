//! Every port the use cases depend on. Adapters live in other crates.

use async_trait::async_trait;
use instantpad_domain::board::PadBoard;
use instantpad_domain::hotkeys::Hotkey;
use instantpad_domain::sounds::SoundSource;
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

/// Plays sounds.
#[async_trait]
pub trait SoundPlayer: Send + Sync {
    /// Decodes ahead of time, so the first press of a pad has no delay.
    async fn preload(&self, path: &str) -> Result<(), Failure>;

    /// Plays for the game and for you. Pressing again restarts the sound.
    async fn play(&self, path: &str, volume: f32, tag: PlaybackTag) -> Result<(), Failure>;

    /// Plays only for you, to try a sound out.
    async fn preview(&self, path: &str, volume: f32, tag: PlaybackTag) -> Result<(), Failure>;

    /// Drops the decoded audio, for when the file on disk has changed.
    fn forget(&self, path: &str);

    fn stop(&self, tag: PlaybackTag);

    fn stop_all(&self);

    fn is_playing(&self, tag: PlaybackTag) -> bool;
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

    async fn trending(&self, page: u32) -> Result<CatalogPage, Failure>;

    async fn download(&self, sound_url: &str, destination: &str) -> Result<(), Failure>;
}

/// The folder where the app keeps its audio files.
pub trait SoundFileStore: Send + Sync {
    fn path_for_download(&self, sound_url: &str) -> String;

    /// Copies an outside file into the store and returns its new path.
    fn import(&self, source_path: &str) -> Result<String, Failure>;

    /// Copies a stored sound to a place the user chose.
    fn export(&self, stored_path: &str, destination: &str) -> Result<(), Failure>;

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
