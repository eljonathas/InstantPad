//! The one place that knows which implementation goes behind each port.

use std::sync::{Arc, Once};

use instantpad_application::board::BoardSession;
use instantpad_application::events::MeterFrame;
use instantpad_application::favorites::Favorites;
use instantpad_application::hotkeys::{HotkeyCommand, HotkeyService};
use instantpad_application::library::SoundLibrary;
use instantpad_application::mixer::MixerSession;
use instantpad_application::ports::{EventSink, HotkeyHost};
use instantpad_application::rows::SoundRows;
use instantpad_application::settings::Settings;
use instantpad_application::setup::SetupSession;
use instantpad_application::updates::UpdateSession;
use instantpad_application::voice::VoiceSession;
use instantpad_audio::AudioEngine;
use instantpad_catalog::MyInstantsCatalog;
use instantpad_storage::{
    AppDataFolder, FolderSoundStore, JsonBoardRepository, JsonFavoritesRepository,
    JsonSettingsRepository,
};
use tauri::{AppHandle, Manager};

use crate::events::TauriEvents;
use crate::updater::TauriUpdater;

pub struct Core {
    pub board: Arc<BoardSession>,
    pub favorites: Arc<Favorites>,
    pub hotkeys: Arc<HotkeyService>,
    pub library: Arc<SoundLibrary>,
    pub mixer: MixerSession,
    pub rows: SoundRows,
    pub setup: SetupSession,
    pub updates: UpdateSession,
    pub voice: VoiceSession,
    pub settings: Arc<Settings>,
    /// The interface can start more than once, as it does after a reload.
    pub background_work: Once,
}

impl Core {
    pub async fn on_hotkey(&self, command: HotkeyCommand) {
        match command {
            HotkeyCommand::ToggleVoice => self.voice.toggle(),
            HotkeyCommand::NextVoice => self.voice.select_next(),
            other => self.board.on_hotkey(other).await,
        }
    }

    pub fn meter_frame(&self) -> MeterFrame {
        let (mic_level, output_level) = self.mixer.levels();
        MeterFrame {
            mic_level,
            output_level,
            playing_slots: self.board.playing_slots(),
        }
    }

    /// What must happen before the app closes.
    pub fn shut_down(&self) {
        self.board.save();
        // The app is closing, so there is no one left to tell.
        let _ = self.settings.save();
        self.library.clear_previews();
    }
}

pub fn build(app: &AppHandle, hotkey_host: Arc<dyn HotkeyHost>) -> tauri::Result<Core> {
    let events: Arc<dyn EventSink> = Arc::new(TauriEvents(app.clone()));

    let folder = AppDataFolder::resolve(app.path().app_data_dir()?)?;
    let files = Arc::new(FolderSoundStore::new(&folder));
    let engine = Arc::new(AudioEngine::new(events.clone()));

    let settings = Arc::new(Settings::load(Arc::new(JsonSettingsRepository::new(
        &folder,
    ))));
    let library = Arc::new(SoundLibrary::new(
        Arc::new(MyInstantsCatalog::new()),
        files.clone(),
    ));
    // A crash leaves them behind, so they also go when the app opens.
    library.clear_previews();
    let favorites = Arc::new(Favorites::new(
        Arc::new(JsonFavoritesRepository::new(&folder)),
        library.clone(),
    ));
    let hotkeys = Arc::new(HotkeyService::new(hotkey_host, events.clone()));
    let board = Arc::new(BoardSession::new(
        Arc::new(JsonBoardRepository::new(&folder)),
        engine.clone(),
        files,
        favorites.clone(),
        hotkeys.clone(),
        settings.clone(),
        events.clone(),
    ));

    Ok(Core {
        mixer: MixerSession::new(engine.clone(), settings.clone()),
        voice: VoiceSession::new(engine.clone(), settings.clone(), events.clone()),
        rows: SoundRows::new(
            board.clone(),
            library.clone(),
            favorites.clone(),
            engine.clone(),
            events.clone(),
        ),
        setup: SetupSession::new(
            engine,
            instantpad_platform::system_audio(),
            instantpad_platform::virtual_cable(),
            settings.clone(),
            board.clone(),
            events.clone(),
        ),
        updates: UpdateSession::new(Arc::new(TauriUpdater::new(app.clone())), events),
        board,
        favorites,
        hotkeys,
        library,
        settings,
        background_work: Once::new(),
    })
}
