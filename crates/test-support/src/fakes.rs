use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;

use async_trait::async_trait;
use instantpad_application::events::AppEvent;
use instantpad_application::hotkeys::HotkeyCommand;
use instantpad_application::message::{Failure, Message, TextKey};
use instantpad_application::ports::{
    AppUpdater, AudioDevice, AudioMix, AudioRouter, BoardRepository, CatalogPage, DeviceRole,
    EventSink, FavoritesRepository, HotkeyHost, NowPlaying, Outcome, PlaybackTag,
    SettingsRepository, SoundCatalog, SoundFileStore, SoundPlayer, SpeechConverter, StreamStop,
    SystemAudio, TrainedVoiceModels, TrainingProgress, TrainingRepository, TrainingRun,
    TrainingTools, VirtualCable, VoiceAnalyzer, VoiceChanger, VoiceLibraryRepository, VoiceModels,
    VoiceRecorder, VoiceTrainer, Waveform,
};
use instantpad_application::settings::AppSettings;
use instantpad_domain::board::PadBoard;
use instantpad_domain::cloning::{TrainedModel, VoiceLibrary, Voiceprint};
use instantpad_domain::cuts::Cut;
use instantpad_domain::hotkeys::Hotkey;
use instantpad_domain::sounds::SoundSource;
use instantpad_domain::training::{GraphicsCard, System, TrainingDevice, TrainingJob};
use instantpad_domain::voice::{Conversion, VoiceChain, VoicePreset};

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
    /// The cut each play and preview asked for, in order.
    pub cuts: Mutex<Vec<Option<Cut>>>,
    /// How long each sound lasts. A sound not here lasts ten seconds.
    pub durations: Mutex<HashMap<String, u32>>,
    /// How loud each sound is along the way, one value per 10 ms. Silent when not here.
    pub levels: Mutex<HashMap<String, Vec<f32>>>,
    /// Runs while a sound is being loaded, for what the user does meanwhile.
    pub meanwhile: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
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
        if let Some(meanwhile) = &*self.meanwhile.lock().unwrap() {
            meanwhile();
        }
        self.check(path)
    }

    async fn play(
        &self,
        path: &str,
        _volume: f32,
        tag: PlaybackTag,
        cut: Option<Cut>,
    ) -> Result<(), Failure> {
        self.check(path)?;
        self.cuts.lock().unwrap().push(cut);
        self.played.lock().unwrap().push(path.to_owned());
        self.playing.lock().unwrap().insert(tag);
        Ok(())
    }

    async fn preview(
        &self,
        path: &str,
        _volume: f32,
        tag: PlaybackTag,
        cut: Option<Cut>,
    ) -> Result<(), Failure> {
        self.check(path)?;
        self.cuts.lock().unwrap().push(cut);
        self.previewed.lock().unwrap().push(path.to_owned());
        self.playing.lock().unwrap().insert(tag);
        Ok(())
    }

    async fn waveform(
        &self,
        path: &str,
        _start_ms: u32,
        _end_ms: u32,
        bars: u32,
    ) -> Result<Waveform, Failure> {
        self.check(path)?;
        let duration_ms = self
            .durations
            .lock()
            .unwrap()
            .get(path)
            .copied()
            .unwrap_or(10_000);
        Ok(Waveform {
            duration_ms,
            peaks: vec![0.5; bars as usize],
        })
    }

    async fn levels(&self, path: &str, _frame_ms: u32) -> Result<Vec<f32>, Failure> {
        self.check(path)?;
        Ok(self
            .levels
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .unwrap_or_default())
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
    /// Every device opened, as `"role:id"`, including the ones that failed to open.
    pub opened: Mutex<Vec<String>>,
    /// Devices that fail to open, though they may be listed.
    pub broken: Mutex<HashSet<String>>,
    /// The streams that stopped by themselves and were not told yet.
    pub stopped: Mutex<Vec<(DeviceRole, StreamStop)>>,
    /// Held by a test, the microphone takes as long to open as a slow driver.
    pub slow_microphone: tokio::sync::Mutex<()>,
}

impl FakeRouter {
    fn open(&self, role: &str, device_id: Option<&str>) -> Result<(), Failure> {
        let id = device_id.unwrap_or("none");
        self.opened.lock().unwrap().push(format!("{role}:{id}"));
        if self.broken.lock().unwrap().contains(id) {
            return Err(Failure::because(TextKey::DeviceUnavailable, "broken"));
        }
        Ok(())
    }

    /// The stream of `role` stops, as the audio callback would see it.
    pub fn stop(&self, role: DeviceRole, stop: StreamStop) {
        self.stopped.lock().unwrap().push((role, stop));
    }

    /// The device leaves the lists, as an unplugged one does.
    pub fn unplug(&self, id: &str) {
        self.microphones
            .lock()
            .unwrap()
            .retain(|device| device.id != id);
        self.outputs
            .lock()
            .unwrap()
            .retain(|device| device.id != id);
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
        let _driver = self.slow_microphone.lock().await;
        self.open("mic", device_id)
    }

    async fn set_game_output(&self, device_id: Option<&str>) -> Result<(), Failure> {
        self.open("game", device_id)
    }

    async fn set_monitor_output(&self, device_id: Option<&str>) -> Result<(), Failure> {
        self.open("monitor", device_id)
    }

    fn take_stopped(&self) -> Vec<(DeviceRole, StreamStop)> {
        std::mem::take(&mut *self.stopped.lock().unwrap())
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
    pub chain: Mutex<Option<VoiceChain>>,
    pub conversion: Mutex<Option<Conversion>>,
    pub is_hearing: Mutex<bool>,
}

impl VoiceChanger for FakeVoiceChanger {
    fn set_voice(&self, chain: Option<VoiceChain>, conversion: Option<Conversion>) {
        *self.chain.lock().unwrap() = chain;
        *self.conversion.lock().unwrap() = conversion;
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
    /// The countries whose trending sounds were asked for.
    pub trending_regions: Mutex<Vec<String>>,
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

    async fn trending(&self, region: &str, page: u32) -> Result<CatalogPage, Failure> {
        self.trending_regions
            .lock()
            .unwrap()
            .push(region.to_owned());
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

/// Hands back whatever `next` holds when a recording stops.
#[derive(Default)]
pub struct FakeRecorder {
    pub is_recording: Mutex<bool>,
    /// How many times a recording started, which also counts starting over.
    pub starts: Mutex<u32>,
    pub next: Mutex<Vec<f32>>,
    /// What a file decodes to, or `None` for a file that cannot be read.
    pub file: Mutex<Option<Vec<f32>>>,
    /// Runs while a file is being read, for what the user does meanwhile.
    pub meanwhile: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
}

#[async_trait]
impl VoiceRecorder for FakeRecorder {
    fn start_recording(&self) {
        *self.starts.lock().unwrap() += 1;
        *self.is_recording.lock().unwrap() = true;
    }

    fn stop_recording(&self) -> Vec<f32> {
        *self.is_recording.lock().unwrap() = false;
        std::mem::take(&mut *self.next.lock().unwrap())
    }

    fn recorded(&self) -> Vec<f32> {
        self.next.lock().unwrap().clone()
    }

    async fn decode_file(&self, _path: &str) -> Result<Vec<f32>, Failure> {
        if let Some(meanwhile) = &*self.meanwhile.lock().unwrap() {
            meanwhile();
        }
        self.file
            .lock()
            .unwrap()
            .clone()
            .ok_or(Failure::text(TextKey::SoundOpenFailed))
    }
}

#[derive(Default)]
pub struct FakeModels {
    pub is_installed: Mutex<bool>,
    /// Makes the next install fail.
    pub is_offline: Mutex<bool>,
}

#[async_trait]
impl VoiceModels for FakeModels {
    fn is_installed(&self) -> bool {
        *self.is_installed.lock().unwrap()
    }

    fn download_megabytes(&self) -> u32 {
        260
    }

    async fn install(&self, progress: &(dyn Fn(u8) + Send + Sync)) -> Result<(), Failure> {
        if *self.is_offline.lock().unwrap() {
            return Err(Failure::because(TextKey::AiDownloadFailed, "offline"));
        }
        progress(50);
        progress(100);
        *self.is_installed.lock().unwrap() = true;
        Ok(())
    }
}

/// Hears the voiceprint that tests prepare, whatever the speech.
#[derive(Default)]
pub struct FakeAnalyzer {
    /// What the next calls to `voiceprint` give, one each. The analyzer fails when it runs out.
    pub voiceprints: Mutex<Vec<Voiceprint>>,
    /// Runs while a voiceprint is being taken, for what the user does meanwhile.
    pub meanwhile: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
}

impl FakeAnalyzer {
    /// A voiceprint of two numbers, enough to tell voices apart in tests.
    pub fn voice(values: [f32; 2]) -> Voiceprint {
        Voiceprint::new(values.to_vec())
    }
}

#[async_trait]
impl VoiceAnalyzer for FakeAnalyzer {
    async fn voiceprint(&self, _speech: Vec<f32>) -> Result<Voiceprint, Failure> {
        if let Some(meanwhile) = &*self.meanwhile.lock().unwrap() {
            meanwhile();
        }
        let mut voiceprints = self.voiceprints.lock().unwrap();
        if voiceprints.is_empty() {
            return Err(Failure::because(
                TextKey::AiLoadFailed,
                "no voiceprint prepared",
            ));
        }
        Ok(voiceprints.remove(0))
    }

    fn preset(&self, preset: VoicePreset) -> Voiceprint {
        match preset {
            VoicePreset::Feminine => Self::voice([0.0, 1.0]),
            VoicePreset::Masculine => Self::voice([0.0, -1.0]),
        }
    }
}

/// Gives the speech straight back.
#[derive(Default)]
pub struct FakeConverter {
    pub started: Mutex<Vec<Conversion>>,
    pub retuned: Mutex<Vec<f32>>,
}

impl SpeechConverter for FakeConverter {
    fn start(&self, conversion: &Conversion) {
        self.started.lock().unwrap().push(conversion.clone());
    }

    fn retune(&self, semitones: f32) {
        self.retuned.lock().unwrap().push(semitones);
    }

    fn convert(&self, speech: &[f32]) -> Result<Vec<f32>, Failure> {
        Ok(speech.to_vec())
    }
}

/// Keeps what it was asked to add, and adds a model named after the folder.
#[derive(Default)]
pub struct FakeTrainedModels {
    pub added: Mutex<Vec<(PathBuf, String)>>,
    pub removed: Mutex<Vec<String>>,
    /// Makes adding fail, like a folder the kit did not export.
    pub fails: Mutex<Option<Failure>>,
    /// Runs once, while the next model is being checked, for what happens meanwhile.
    pub meanwhile: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

#[async_trait]
impl TrainedVoiceModels for FakeTrainedModels {
    async fn add(&self, folder: PathBuf, id: String) -> Result<TrainedModel, Failure> {
        let meanwhile = self.meanwhile.lock().unwrap().take();
        if let Some(meanwhile) = meanwhile {
            meanwhile();
        }
        if let Some(failure) = self.fails.lock().unwrap().clone() {
            return Err(failure);
        }
        self.added
            .lock()
            .unwrap()
            .push((folder.clone(), id.clone()));
        let name = folder
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        Ok(TrainedModel {
            id,
            name,
            credit: "Recordings".to_owned(),
            pitch_hz: 210.0,
        })
    }

    fn remove(&self, id: &str) {
        self.removed.lock().unwrap().push(id.to_owned());
    }
}

#[derive(Default)]
pub struct InMemoryVoiceLibrary {
    pub library: Mutex<VoiceLibrary>,
    /// Makes saving fail, like a disk that cannot be written.
    pub fails: Mutex<bool>,
}

impl VoiceLibraryRepository for InMemoryVoiceLibrary {
    fn load(&self) -> VoiceLibrary {
        self.library.lock().unwrap().clone()
    }

    fn save(&self, library: &VoiceLibrary) -> Result<(), Failure> {
        if *self.fails.lock().unwrap() {
            return Err(Failure::text(TextKey::SaveFailed));
        }
        *self.library.lock().unwrap() = library.clone();
        Ok(())
    }
}

/// Trains in no time: it reports the progress tests prepare and ends as they say.
pub struct FakeTrainer {
    pub is_supported: Mutex<bool>,
    pub card: Mutex<Option<GraphicsCard>>,
    /// The device whose tools are installed.
    pub tools: Mutex<Option<TrainingDevice>>,
    pub free_megabytes: Mutex<Option<u64>>,
    /// Makes the next install fail.
    pub install_fails: Mutex<Option<Failure>>,
    /// Every take written: the training, the take and how many samples it had.
    pub takes: Mutex<Vec<(String, String, usize)>>,
    pub removed_takes: Mutex<Vec<(String, String)>>,
    pub discarded: Mutex<Vec<String>>,
    pub runs: Mutex<Vec<TrainingRun>>,
    /// What the next training reports before it ends.
    pub progress: Mutex<Vec<TrainingProgress>>,
    /// How the next training ends; by default with a folder named after the title.
    pub ending: Mutex<Option<Result<Outcome<PathBuf>, Failure>>>,
    /// Runs while an install or a training is in progress, for what the user does meanwhile.
    pub meanwhile: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
    /// Set by `stop`, which ends the work in progress.
    pub is_stopped: Mutex<bool>,
    pub tools_removed: Mutex<bool>,
}

impl Default for FakeTrainer {
    fn default() -> Self {
        Self {
            is_supported: Mutex::new(true),
            card: Mutex::new(None),
            tools: Mutex::new(None),
            free_megabytes: Mutex::new(Some(100_000)),
            install_fails: Mutex::new(None),
            takes: Mutex::new(Vec::new()),
            removed_takes: Mutex::new(Vec::new()),
            discarded: Mutex::new(Vec::new()),
            runs: Mutex::new(Vec::new()),
            progress: Mutex::new(Vec::new()),
            ending: Mutex::new(None),
            meanwhile: Mutex::new(None),
            is_stopped: Mutex::new(false),
            tools_removed: Mutex::new(false),
        }
    }
}

impl FakeTrainer {
    pub const CARD_DOWNLOAD: u32 = 4_200;
    pub const PROCESSOR_DOWNLOAD: u32 = 1_200;

    /// Runs what the test does meanwhile, and tells whether it stopped the work.
    fn while_working(&self) -> bool {
        *self.is_stopped.lock().unwrap() = false;
        if let Some(meanwhile) = &*self.meanwhile.lock().unwrap() {
            meanwhile();
        }
        std::mem::take(&mut *self.is_stopped.lock().unwrap())
    }
}

#[async_trait]
impl VoiceTrainer for FakeTrainer {
    fn is_supported(&self) -> bool {
        *self.is_supported.lock().unwrap()
    }

    fn system(&self) -> System {
        System::Windows
    }

    async fn graphics_card(&self) -> Option<GraphicsCard> {
        self.card.lock().unwrap().clone()
    }

    fn tools(&self, device: TrainingDevice) -> TrainingTools {
        let is_installed = *self.tools.lock().unwrap() == Some(device);
        let (download, installed) = if device.is_card() {
            (Self::CARD_DOWNLOAD, 9_000)
        } else {
            (Self::PROCESSOR_DOWNLOAD, 3_000)
        };
        TrainingTools {
            is_installed,
            download_megabytes: if is_installed { 0 } else { download },
            installed_megabytes: installed,
        }
    }

    fn installed_megabytes(&self) -> u32 {
        let device = *self.tools.lock().unwrap();
        device.map_or(0, |device| self.tools(device).installed_megabytes)
    }

    fn free_megabytes(&self) -> Option<u64> {
        *self.free_megabytes.lock().unwrap()
    }

    async fn install(
        &self,
        device: TrainingDevice,
        progress: &(dyn Fn(u8) + Send + Sync),
    ) -> Result<Outcome<()>, Failure> {
        progress(40);
        if self.while_working() {
            return Ok(Outcome::Stopped);
        }
        if let Some(failure) = self.install_fails.lock().unwrap().take() {
            return Err(failure);
        }
        progress(100);
        *self.tools.lock().unwrap() = Some(device);
        Ok(Outcome::Done(()))
    }

    fn stop(&self) {
        *self.is_stopped.lock().unwrap() = true;
    }

    fn remove_tools(&self) {
        *self.tools.lock().unwrap() = None;
        *self.tools_removed.lock().unwrap() = true;
    }

    async fn keep_take(&self, job: &str, take: &str, speech: Vec<f32>) -> Result<(), Failure> {
        self.takes
            .lock()
            .unwrap()
            .push((job.to_owned(), take.to_owned(), speech.len()));
        Ok(())
    }

    fn remove_take(&self, job: &str, take: &str) {
        self.removed_takes
            .lock()
            .unwrap()
            .push((job.to_owned(), take.to_owned()));
    }

    async fn train(
        &self,
        run: &TrainingRun,
        progress: &(dyn Fn(TrainingProgress) + Send + Sync),
    ) -> Result<Outcome<PathBuf>, Failure> {
        self.runs.lock().unwrap().push(run.clone());
        if !self.tools(run.plan.device).is_installed {
            return Err(Failure::because(TextKey::TrainingFailed, "no tools"));
        }
        let steps = std::mem::take(&mut *self.progress.lock().unwrap());
        for step in steps {
            progress(step);
        }
        if self.while_working() {
            return Ok(Outcome::Stopped);
        }
        let ending = self.ending.lock().unwrap().take();
        ending.unwrap_or_else(|| Ok(Outcome::Done(PathBuf::from(&run.title))))
    }

    fn discard(&self, job: &str) {
        self.discarded.lock().unwrap().push(job.to_owned());
    }
}

#[derive(Default)]
pub struct InMemoryTraining {
    pub job: Mutex<Option<TrainingJob>>,
    /// Makes saving fail, like a disk that cannot be written.
    pub fails: Mutex<bool>,
}

impl TrainingRepository for InMemoryTraining {
    fn load(&self) -> Option<TrainingJob> {
        self.job.lock().unwrap().clone()
    }

    fn save(&self, job: Option<&TrainingJob>) -> Result<(), Failure> {
        if *self.fails.lock().unwrap() {
            return Err(Failure::text(TextKey::SaveFailed));
        }
        *self.job.lock().unwrap() = job.cloned();
        Ok(())
    }
}
