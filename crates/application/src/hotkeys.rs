use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use instantpad_domain::board::Sound;
use instantpad_domain::hotkeys::{Hotkey, NumericLayout};

use crate::events::AppEvent;
use crate::message::{Message, TextKey};
use crate::ports::{EventSink, HotkeyHost};

/// What a key does when pressed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HotkeyCommand {
    PlaySound { sound_id: String },
    PlayPad { index: usize },
    NextInterface,
    PreviousInterface,
    StopAll,
}

pub type Binding = (Hotkey, HotkeyCommand);

/// A key can only do one thing, so the first binding wins: keys set by hand on a pad, then the
/// stop key, then the numeric scheme.
pub fn resolve(
    sounds: &[Sound],
    layout: &NumericLayout,
    custom_stop: Option<&Hotkey>,
) -> Vec<Binding> {
    let by_hand = sounds.iter().filter_map(|sound| {
        let command = HotkeyCommand::PlaySound {
            sound_id: sound.id.clone(),
        };
        sound.hotkey.clone().map(|hotkey| (hotkey, command))
    });
    let stop = custom_stop
        .or(layout.stop_all.as_ref())
        .map(|hotkey| (hotkey.clone(), HotkeyCommand::StopAll));
    let pads = layout
        .pads
        .iter()
        .enumerate()
        .map(|(index, hotkey)| (hotkey.clone(), HotkeyCommand::PlayPad { index }));
    let navigation = [
        (&layout.next_interface, HotkeyCommand::NextInterface),
        (&layout.previous_interface, HotkeyCommand::PreviousInterface),
    ]
    .into_iter()
    .filter_map(|(hotkey, command)| hotkey.clone().map(|hotkey| (hotkey, command)));

    let mut taken = HashSet::new();
    by_hand
        .chain(stop)
        .chain(pads)
        .chain(navigation)
        .filter(|(hotkey, _)| taken.insert(hotkey.clone()))
        .collect()
}

#[derive(Default)]
struct State {
    pause_count: u32,
    bindings: Vec<Binding>,
}

/// Keeps the system-wide keys in sync with the board and the numeric scheme.
pub struct HotkeyService {
    host: Arc<dyn HotkeyHost>,
    events: Arc<dyn EventSink>,
    state: Mutex<State>,
}

impl HotkeyService {
    pub fn new(host: Arc<dyn HotkeyHost>, events: Arc<dyn EventSink>) -> Self {
        Self {
            host,
            events,
            state: Mutex::default(),
        }
    }

    pub fn apply(&self, bindings: Vec<Binding>) {
        let mut state = self.lock();
        state.bindings = bindings;
        if state.pause_count == 0 {
            self.register_all(&state.bindings);
        }
    }

    /// Global keys swallow their presses, so they must be off while the user types.
    pub fn pause(&self) {
        self.lock().pause_count += 1;
        self.host.clear();
    }

    pub fn resume(&self) {
        let mut state = self.lock();
        state.pause_count = state.pause_count.saturating_sub(1);
        if state.pause_count == 0 {
            self.register_all(&state.bindings);
        }
    }

    fn register_all(&self, bindings: &[Binding]) {
        self.host.clear();

        let failed: Vec<String> = bindings
            .iter()
            .filter(|(hotkey, command)| !self.host.register(hotkey, command.clone()))
            .map(|(hotkey, _)| hotkey.to_string())
            .collect();

        if !failed.is_empty() {
            let keys = Message::plain(failed.join(", "));
            self.events
                .emit(AppEvent::error(Message::with(TextKey::HotkeyInUse, [keys])));
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().expect("hotkey lock poisoned")
    }
}
