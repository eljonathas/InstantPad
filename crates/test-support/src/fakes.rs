use std::collections::HashSet;
use std::sync::Mutex;

use async_trait::async_trait;
use instantpad_application::events::AppEvent;
use instantpad_application::hotkeys::HotkeyCommand;
use instantpad_application::message::{Failure, Message, TextKey};
use instantpad_application::ports::{
    AppUpdater, AudioDevice, AudioMix, AudioRouter, BoardRepository, CatalogPage, EventSink,
    FavoritesRepository, HotkeyHost, NowPlaying, PlaybackTag, SettingsRepository, SoundCatalog,
    SoundFileStore, SoundPlayer, SystemAudio, VirtualCable, VoiceChanger,
};
use instantpad_application::settings::AppSettings;
use instantpad_domain::board::PadBoard;
use instantpad_domain::hotkeys::Hotkey;
use instantpad_domain::sounds::SoundSource;
use instantpad_domain::voice::VoiceShift;

pub fn device(id: &str, name: &str) -> AudioDevice {
    AudioDevice {
        id: id.to_owned(),
        name: name.to_owned(),
    }
}

#[derive(Default)]
pub struct FakePlayer {
    pub playing: Mutex<HashSet<PlaybackTag>>,
    pub previewed: Mutex<Vec<String>>,
    pub played: Mutex<Vec<String>>,
    pub forgotten: Mutex<Vec<String>>,
    /// Paths that fail like a file that cannot be decoded.
    pub broken: Mutex<HashSet<String>>,
}

impl FakePlayer {
    fn check(&self, path: &str) -> Result<(), Failure> {
        if self.broken.lock().unwrap().contains(path) {
            return Err(Failure::text(TextKey::SoundHasNoAudio));
        }
        Ok(())
    }
}

#[async_trait]
impl SoundPlayer for FakePlayer {
    async fn preload(&self, path: &str) -> Result<(), Failure> {
        self.check(path)
    }

    async fn play(&self, path: &str, _volume: f32, tag: PlaybackTag) -> Result<(), Failure> {
        self.check(path)?;
        self.played.lock().unwrap().push(path.to_owned());
        self.playing.lock().unwrap().insert(tag);
        Ok(())
    }

    async fn preview(&self, path: &str, _volume: f32, tag: PlaybackTag) -> Result<(), Failure> {
        self.check(path)?;
        self.previewed.lock().unwrap().push(path.to_owned());
        self.playing.lock().unwrap().insert(tag);
        Ok(())
    }

    fn forget(&self, path: &str) {
        self.forgotten.lock().unwrap().push(path.to_owned());
    }

    fn stop(&self, tag: PlaybackTag) {
        self.playing.lock().unwrap().remove(&tag);
    }

    fn stop_all(&self) {
        self.playing.lock().unwrap().clear();
    }

    fn is_playing(&self, tag: PlaybackTag) -> bool {
        self.playing.lock().unwrap().contains(&tag)
    }

    fn now_playing(&self) -> Option<NowPlaying> {
        let is_silent = self.playing.lock().unwrap().is_empty();
        (!is_silent).then(|| NowPlaying {
            position: 0.0,
            duration: 1.0,
            peaks: Vec::new(),
        })
    }
}

#[derive(Default)]
pub struct FakeRouter {
    pub microphones: Mutex<Vec<AudioDevice>>,
    pub outputs: Mutex<Vec<AudioDevice>>,
    /// Every device opened, as `"role:id"`.
    pub opened: Mutex<Vec<String>>,
}

impl FakeRouter {
    fn open(&self, role: &str, device_id: Option<&str>) -> Result<(), Failure> {
        let id = device_id.unwrap_or("none");
        self.opened.lock().unwrap().push(format!("{role}:{id}"));
        Ok(())
    }
}

#[async_trait]
impl AudioRouter for FakeRouter {
    fn list_microphones(&self) -> Vec<AudioDevice> {
        self.microphones.lock().unwrap().clone()
    }

    fn list_outputs(&self) -> Vec<AudioDevice> {
        self.outputs.lock().unwrap().clone()
    }

    async fn set_microphone(&self, device_id: Option<&str>) -> Result<(), Failure> {
        self.open("mic", device_id)
    }

    async fn set_game_output(&self, device_id: Option<&str>) -> Result<(), Failure> {
        self.open("game", device_id)
    }

    async fn set_monitor_output(&self, device_id: Option<&str>) -> Result<(), Failure> {
        self.open("monitor", device_id)
    }
}

#[derive(Default)]
pub struct FakeMix {
    pub mic_volume: Mutex<f32>,
    pub game_volume: Mutex<f32>,
    pub monitor_volume: Mutex<f32>,
    pub mic_enabled: Mutex<bool>,
    pub mic: Mutex<f32>,
    pub output: Mutex<f32>,
}

impl AudioMix for FakeMix {
    fn set_mic_volume(&self, volume: f32) {
        *self.mic_volume.lock().unwrap() = volume;
    }

    fn set_game_volume(&self, volume: f32) {
        *self.game_volume.lock().unwrap() = volume;
    }

    fn set_monitor_volume(&self, volume: f32) {
        *self.monitor_volume.lock().unwrap() = volume;
    }

    fn set_mic_enabled(&self, enabled: bool) {
        *self.mic_enabled.lock().unwrap() = enabled;
    }

    fn mic_level(&self) -> f32 {
        *self.mic.lock().unwrap()
    }

    fn game_output_level(&self) -> f32 {
        *self.output.lock().unwrap()
    }
}

#[derive(Default)]
pub struct FakeVoiceChanger {
    pub shift: Mutex<Option<VoiceShift>>,
    pub is_hearing: Mutex<bool>,
}

impl VoiceChanger for FakeVoiceChanger {
    fn set_voice(&self, shift: Option<VoiceShift>) {
        *self.shift.lock().unwrap() = shift;
    }

    fn set_hearing_voice(&self, enabled: bool) {
        *self.is_hearing.lock().unwrap() = enabled;
    }
}

#[derive(Default)]
pub struct FakeSystemAudio {
    pub default_microphone: Mutex<Option<String>>,
    pub default_output: Mutex<Option<String>>,
    pub system_output: Mutex<Option<String>>,
}

impl SystemAudio for FakeSystemAudio {
    fn default_microphone_id(&self) -> Option<String> {
        self.default_microphone.lock().unwrap().clone()
    }

    fn default_output_id(&self) -> Option<String> {
        self.default_output.lock().unwrap().clone()
    }

    fn system_output_id(&self) -> Option<String> {
        self.system_output.lock().unwrap().clone()
    }

    fn open_sound_settings(&self) {}
}

/// Treats any device with "CABLE" in its name as virtual.
#[derive(Default)]
pub struct FakeCable;

#[async_trait]
impl VirtualCable for FakeCable {
    fn product_name(&self) -> &'static str {
        "CABLE"
    }

    fn find_game_output(&self, outputs: &[AudioDevice]) -> Option<AudioDevice> {
        outputs
            .iter()
            .find(|device| self.is_virtual(device))
            .cloned()
    }

    fn is_virtual(&self, device: &AudioDevice) -> bool {
        device.name.contains("CABLE")
    }

    fn game_microphone_name(&self, _game_output: Option<&AudioDevice>) -> String {
        "CABLE Output".to_owned()
    }

    async fn install(&self, progress: &(dyn Fn(Message) + Send + Sync)) -> Result<(), Failure> {
        progress(Message::text(TextKey::DriverSearching));
        Ok(())
    }
}

#[derive(Default)]
pub struct FakeHotkeyHost {
    pub registered: Mutex<Vec<(Hotkey, HotkeyCommand)>>,
    /// Keys that behave as owned by another program.
    pub taken: Mutex<HashSet<Hotkey>>,
}

impl HotkeyHost for FakeHotkeyHost {
    fn register(&self, hotkey: &Hotkey, command: HotkeyCommand) -> bool {
        if self.taken.lock().unwrap().contains(hotkey) {
            return false;
        }
        self.registered
            .lock()
            .unwrap()
            .push((hotkey.clone(), command));
        true
    }

    fn clear(&self) {
        self.registered.lock().unwrap().clear();
    }
}

#[derive(Default)]
pub struct InMemoryBoard(pub Mutex<PadBoard>);

impl BoardRepository for InMemoryBoard {
    fn load(&self) -> PadBoard {
        self.0.lock().unwrap().clone()
    }

    fn save(&self, board: &PadBoard) -> Result<(), Failure> {
        *self.0.lock().unwrap() = board.clone();
        Ok(())
    }
}

#[derive(Default)]
pub struct InMemoryFavorites(pub Mutex<Vec<SoundSource>>);

impl FavoritesRepository for InMemoryFavorites {
    fn load(&self) -> Vec<SoundSource> {
        self.0.lock().unwrap().clone()
    }

    fn save(&self, favorites: &[SoundSource]) -> Result<(), Failure> {
        *self.0.lock().unwrap() = favorites.to_vec();
        Ok(())
    }
}

#[derive(Default)]
pub struct InMemorySettings(pub Mutex<AppSettings>);

impl SettingsRepository for InMemorySettings {
    fn load(&self) -> AppSettings {
        self.0.lock().unwrap().clone()
    }

    fn save(&self, settings: &AppSettings) -> Result<(), Failure> {
        *self.0.lock().unwrap() = settings.clone();
        Ok(())
    }
}

#[derive(Default)]
pub struct FakeCatalog {
    pub page: Mutex<Option<CatalogPage>>,
    pub downloaded: Mutex<Vec<String>>,
}

#[async_trait]
impl SoundCatalog for FakeCatalog {
    async fn search(&self, _query: &str, _page: u32) -> Result<CatalogPage, Failure> {
        self.page
            .lock()
            .unwrap()
            .clone()
            .ok_or(Failure::text(TextKey::CatalogNoConnection))
    }

    async fn trending(&self, page: u32) -> Result<CatalogPage, Failure> {
        self.search("", page).await
    }

    async fn download(&self, sound_url: &str, _destination: &str) -> Result<(), Failure> {
        self.downloaded.lock().unwrap().push(sound_url.to_owned());
        Ok(())
    }
}

#[derive(Default)]
pub struct FakeFileStore {
    pub files: Mutex<HashSet<String>>,
    pub exported: Mutex<Vec<(String, String)>>,
}

impl SoundFileStore for FakeFileStore {
    fn path_for_download(&self, sound_url: &str) -> String {
        let name = sound_url.rsplit('/').next().unwrap_or(sound_url);
        format!("store/{name}")
    }

    fn path_for_preview(&self, sound_url: &str) -> String {
        let name = sound_url.rsplit('/').next().unwrap_or(sound_url);
        format!("previews/{name}")
    }

    fn keep_preview(&self, preview_path: &str, stored_path: &str) -> Result<(), Failure> {
        let mut files = self.files.lock().unwrap();
        files.remove(preview_path);
        files.insert(stored_path.to_owned());
        Ok(())
    }

    fn clear_previews(&self) {
        let mut files = self.files.lock().unwrap();
        files.retain(|path| !path.starts_with("previews/"));
    }

    fn import(&self, source_path: &str) -> Result<String, Failure> {
        let name = source_path.rsplit('/').next().unwrap_or(source_path);
        let path = format!("store/{name}");
        self.files.lock().unwrap().insert(path.clone());
        Ok(path)
    }

    fn export(&self, stored_path: &str, destination: &str) -> Result<(), Failure> {
        let pair = (stored_path.to_owned(), destination.to_owned());
        self.exported.lock().unwrap().push(pair);
        Ok(())
    }

    fn delete(&self, stored_path: &str) {
        self.files.lock().unwrap().remove(stored_path);
    }

    fn exists(&self, path: &str) -> bool {
        self.files.lock().unwrap().contains(path)
    }
}

#[derive(Default)]
pub struct FakeUpdater {
    pub is_release: bool,
    pub available: Mutex<Option<String>>,
    pub applied: Mutex<Vec<bool>>,
}

#[async_trait]
impl AppUpdater for FakeUpdater {
    fn current_version(&self) -> String {
        "1.0.0".to_owned()
    }

    fn is_release_build(&self) -> bool {
        self.is_release
    }

    async fn find_update(&self) -> Result<Option<String>, Failure> {
        Ok(self.available.lock().unwrap().clone())
    }

    async fn download(&self, progress: &(dyn Fn(u8) + Send + Sync)) -> Result<(), Failure> {
        progress(100);
        Ok(())
    }

    fn apply(&self, restart: bool) -> Result<(), Failure> {
        self.applied.lock().unwrap().push(restart);
        Ok(())
    }
}

#[derive(Default)]
pub struct RecordingEvents(pub Mutex<Vec<AppEvent>>);

impl EventSink for RecordingEvents {
    fn emit(&self, event: AppEvent) {
        self.0.lock().unwrap().push(event);
    }
}
