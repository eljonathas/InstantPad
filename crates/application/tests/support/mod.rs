// Each test binary uses its own part of the world.
#![allow(dead_code)]

use std::sync::Arc;

use instantpad_application::board::BoardSession;
use instantpad_application::events::{AppEvent, DisplayEvent};
use instantpad_application::favorites::Favorites;
use instantpad_application::hotkeys::HotkeyService;
use instantpad_application::library::SoundLibrary;
use instantpad_application::message::Message;
use instantpad_application::settings::Settings;
use instantpad_test_support::fakes::{
    FakeCatalog, FakeFileStore, FakeHotkeyHost, FakePlayer, InMemoryBoard, InMemoryFavorites,
    InMemorySettings, RecordingEvents,
};

pub struct World {
    pub player: Arc<FakePlayer>,
    pub files: Arc<FakeFileStore>,
    pub catalog: Arc<FakeCatalog>,
    pub host: Arc<FakeHotkeyHost>,
    pub repository: Arc<InMemoryBoard>,
    pub events: Arc<RecordingEvents>,
    pub settings: Arc<Settings>,
    pub library: Arc<SoundLibrary>,
    pub favorites: Arc<Favorites>,
    pub board: Arc<BoardSession>,
    pub hotkeys: Arc<HotkeyService>,
}

impl World {
    pub fn new() -> Self {
        let player = Arc::new(FakePlayer::default());
        let files = Arc::new(FakeFileStore::default());
        let catalog = Arc::new(FakeCatalog::default());
        let host = Arc::new(FakeHotkeyHost::default());
        let repository = Arc::new(InMemoryBoard::default());
        let events = Arc::new(RecordingEvents::default());
        let settings = Arc::new(Settings::load(Arc::new(InMemorySettings::default())));
        let library = Arc::new(SoundLibrary::new(catalog.clone(), files.clone()));
        let favorites = Arc::new(Favorites::new(
            Arc::new(InMemoryFavorites::default()),
            library.clone(),
        ));
        let hotkeys = Arc::new(HotkeyService::new(host.clone(), events.clone()));
        let board = Arc::new(BoardSession::new(
            repository.clone(),
            player.clone(),
            files.clone(),
            favorites.clone(),
            hotkeys.clone(),
            settings.clone(),
            events.clone(),
        ));
        board.start("Main");

        Self {
            player,
            files,
            catalog,
            host,
            repository,
            events,
            settings,
            library,
            favorites,
            board,
            hotkeys,
        }
    }

    pub fn hotkeys(&self) -> Arc<HotkeyService> {
        self.hotkeys.clone()
    }

    pub fn last_display(&self) -> Option<DisplayEvent> {
        self.events
            .0
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find_map(|event| match event {
                AppEvent::Display(display) => Some(display.clone()),
                _ => None,
            })
    }
}

pub fn shown(text: &str) -> DisplayEvent {
    DisplayEvent::Show {
        message: Message::plain(text),
    }
}
