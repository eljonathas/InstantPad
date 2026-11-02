// Each test binary uses its own part of the world.
#![allow(dead_code)]

use std::sync::{Arc, Mutex};

use instantpad_application::board::BoardSession;
use instantpad_application::events::{AiStatus, AppEvent, DisplayEvent};
use instantpad_application::favorites::Favorites;
use instantpad_application::hotkeys::HotkeyService;
use instantpad_application::library::SoundLibrary;
use instantpad_application::message::Message;
use instantpad_application::settings::{AppSettings, Settings};
use instantpad_application::voice::{VoicePorts, VoiceSession};
use instantpad_application::voice_library::VoiceLibraryStore;
use instantpad_domain::cloning::{ClonedVoice, Consent, TERMS_VERSION, VoiceLibrary, Voiceprint};
use instantpad_domain::voice::VoiceSettings;
use instantpad_test_support::fakes::{
    FakeAnalyzer, FakeCatalog, FakeFileStore, FakeHotkeyHost, FakeModels, FakePlayer, FakeRecorder,
    FakeVoiceChanger, InMemoryBoard, InMemoryFavorites, InMemorySettings, InMemoryVoiceLibrary,
    RecordingEvents,
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

/// The voice use cases with the fakes of everything they talk to.
pub struct VoiceWorld {
    pub changer: Arc<FakeVoiceChanger>,
    pub models: Arc<FakeModels>,
    pub analyzer: Arc<FakeAnalyzer>,
    pub recorder: Arc<FakeRecorder>,
    pub repository: Arc<InMemoryVoiceLibrary>,
    pub library: Arc<VoiceLibraryStore>,
    pub settings: Arc<Settings>,
    pub events: Arc<RecordingEvents>,
    pub voice: Arc<VoiceSession>,
}

impl VoiceWorld {
    /// No AI models and no voices: the app on its first run.
    pub fn new() -> Self {
        Self::resuming(VoiceSettings::default(), VoiceLibrary::default(), false)
    }

    /// The models installed and the user's own voice known, so AI voices can speak.
    pub fn ready() -> Self {
        let library = VoiceLibrary {
            own: Some(own_voice()),
            ..VoiceLibrary::default()
        };
        Self::resuming(VoiceSettings::default(), library, true)
    }

    /// The app opening with what an earlier run saved.
    pub fn resuming(voice: VoiceSettings, library: VoiceLibrary, has_models: bool) -> Self {
        let changer = Arc::new(FakeVoiceChanger::default());
        let models = Arc::new(FakeModels::default());
        *models.is_installed.lock().unwrap() = has_models;
        let analyzer = Arc::new(FakeAnalyzer::default());
        let recorder = Arc::new(FakeRecorder::default());
        let repository = Arc::new(InMemoryVoiceLibrary {
            library: Mutex::new(library),
            ..InMemoryVoiceLibrary::default()
        });
        let events = Arc::new(RecordingEvents::default());
        let settings = Arc::new(Settings::load(Arc::new(InMemorySettings(Mutex::new(
            AppSettings {
                voice,
                ..AppSettings::default()
            },
        )))));
        let library = Arc::new(VoiceLibraryStore::load(repository.clone()));
        let voice = Arc::new(VoiceSession::new(
            VoicePorts {
                changer: changer.clone(),
                models: models.clone(),
                analyzer: analyzer.clone(),
                recorder: recorder.clone(),
            },
            library.clone(),
            settings.clone(),
            events.clone(),
        ));

        Self {
            changer,
            models,
            analyzer,
            recorder,
            repository,
            library,
            settings,
            events,
            voice,
        }
    }

    /// Adds a cloned voice, as if it had been cloned earlier.
    pub fn keep_clone(&self, id: &str, name: &str) {
        self.library.lock().clones.push(cloned_voice(id, name));
    }

    pub fn saw(&self, event: &AppEvent) -> bool {
        self.events.0.lock().unwrap().contains(event)
    }

    /// The state of the AI as the screen was told, in order.
    pub fn ai_statuses(&self) -> Vec<AiStatus> {
        self.events
            .0
            .lock()
            .unwrap()
            .iter()
            .filter_map(|event| match event {
                AppEvent::Voice(view) => Some(view.ai.status.clone()),
                _ => None,
            })
            .collect()
    }
}

/// The user's own voice, as the fake analyzer would hear it.
pub fn own_voice() -> Voiceprint {
    FakeAnalyzer::voice([1.0, 0.0])
}

/// A voice that sounds like nobody else's in the fakes.
pub fn cloned_voice(id: &str, name: &str) -> ClonedVoice {
    ClonedVoice {
        id: id.to_owned(),
        name: name.to_owned(),
        voiceprint: FakeAnalyzer::voice([0.6, 0.8]),
        consent: Consent {
            owner_name: "Ana".to_owned(),
            terms_version: TERMS_VERSION,
            accepted_at: 0,
            similarity: 1.0,
            sample_sha256: "abc".to_owned(),
        },
    }
}

/// Mono speech at 48 kHz lasting `seconds`.
pub fn speech(seconds: u32) -> Vec<f32> {
    vec![0.0; seconds as usize * 48_000]
}
