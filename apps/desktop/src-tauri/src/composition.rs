//! The one place that knows which implementation goes behind each port.

use std::sync::Arc;

use instantpad_application::board::BoardSession;
use instantpad_application::favorites::Favorites;
use instantpad_application::hotkeys::HotkeyService;
use instantpad_application::library::SoundLibrary;
use instantpad_application::mixer::MixerSession;
use instantpad_application::ports::EventSink;
use instantpad_application::rows::SoundRows;
use instantpad_application::settings::Settings;
use instantpad_application::setup::SetupSession;
use instantpad_application::updates::UpdateSession;
use instantpad_test_support::fakes::{
    FakeCable, FakeCatalog, FakeFileStore, FakeHotkeyHost, FakeMix, FakePlayer, FakeRouter,
    FakeSystemAudio, FakeUpdater, InMemoryBoard, InMemoryFavorites, InMemorySettings,
};
use tauri::AppHandle;

use crate::events::TauriEvents;

pub struct Core {
    pub board: Arc<BoardSession>,
    pub favorites: Arc<Favorites>,
    pub hotkeys: Arc<HotkeyService>,
    pub library: Arc<SoundLibrary>,
    pub mixer: MixerSession,
    pub rows: SoundRows,
    pub setup: SetupSession,
    pub updates: UpdateSession,
}

// Phase 2 replaces every fake here with its real adapter.
pub fn build(app: AppHandle) -> Core {
    let events: Arc<dyn EventSink> = Arc::new(TauriEvents(app));
    let player = Arc::new(FakePlayer::default());
    let files = Arc::new(FakeFileStore::default());

    let settings = Arc::new(Settings::load(Arc::new(InMemorySettings::default())));
    let library = Arc::new(SoundLibrary::new(
        Arc::new(FakeCatalog::default()),
        files.clone(),
    ));
    let favorites = Arc::new(Favorites::new(
        Arc::new(InMemoryFavorites::default()),
        library.clone(),
    ));
    let hotkeys = Arc::new(HotkeyService::new(
        Arc::new(FakeHotkeyHost::default()),
        events.clone(),
    ));
    let board = Arc::new(BoardSession::new(
        Arc::new(InMemoryBoard::default()),
        player.clone(),
        files,
        favorites.clone(),
        hotkeys.clone(),
        settings.clone(),
        events.clone(),
    ));

    Core {
        mixer: MixerSession::new(Arc::new(FakeMix::default()), settings.clone()),
        rows: SoundRows::new(
            board.clone(),
            library.clone(),
            favorites.clone(),
            player,
            events.clone(),
        ),
        setup: SetupSession::new(
            Arc::new(FakeRouter::default()),
            Arc::new(FakeSystemAudio::default()),
            Arc::new(FakeCable),
            settings,
            board.clone(),
            events.clone(),
        ),
        updates: UpdateSession::new(Arc::new(FakeUpdater::default()), events),
        board,
        favorites,
        hotkeys,
        library,
    }
}
