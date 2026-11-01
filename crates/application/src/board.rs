use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use instantpad_domain::board::{InterfaceLayout, PadBoard, Sound};
use instantpad_domain::hotkeys::{Hotkey, NumericLayout};
use instantpad_domain::palette;

use crate::events::{AppEvent, BoardView, DisplayEvent, InterfaceTab, PadView, SoundView};
use crate::favorites::Favorites;
use crate::hotkeys::{HotkeyCommand, HotkeyService, resolve};
use crate::message::{Failure, Message, TextKey};
use crate::ports::{
    BoardRepository, EventSink, NowPlaying, PlaybackTag, SoundFileStore, SoundPlayer,
};
use crate::settings::Settings;

/// A pad of one interface, so an action that takes a while still lands where it was aimed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PadTarget {
    pub interface_id: String,
    pub slot: u32,
}

struct State {
    board: PadBoard,
    active_id: String,
    target_slot: Option<u32>,
    /// Pads with a sound on its way. A pad appears once per sound.
    loading: Vec<PadTarget>,
}

/// The pads on screen, the interface tabs and what happens when they are used.
pub struct BoardSession {
    state: Mutex<State>,
    repository: Arc<dyn BoardRepository>,
    player: Arc<dyn SoundPlayer>,
    files: Arc<dyn SoundFileStore>,
    favorites: Arc<Favorites>,
    hotkeys: Arc<HotkeyService>,
    settings: Arc<Settings>,
    events: Arc<dyn EventSink>,
    was_playing: AtomicBool,
}

pub fn pad_label(slot: u32) -> String {
    format!("{:02}", slot + 1)
}

fn sound_tag(sound: &Sound) -> PlaybackTag {
    PlaybackTag::of(&sound.id)
}

impl BoardSession {
    pub fn new(
        repository: Arc<dyn BoardRepository>,
        player: Arc<dyn SoundPlayer>,
        files: Arc<dyn SoundFileStore>,
        favorites: Arc<Favorites>,
        hotkeys: Arc<HotkeyService>,
        settings: Arc<Settings>,
        events: Arc<dyn EventSink>,
    ) -> Self {
        Self {
            state: Mutex::new(State {
                board: repository.load(),
                active_id: String::new(),
                target_slot: None,
                loading: Vec::new(),
            }),
            repository,
            player,
            files,
            favorites,
            hotkeys,
            settings,
            events,
            was_playing: AtomicBool::new(false),
        }
    }

    /// Runs once the interface knows its language, which names the first interface.
    pub fn start(&self, default_interface_name: &str) {
        {
            let mut state = self.lock();
            state.board.ensure_interface(default_interface_name);

            let saved = self.settings.lock().active_interface_id.clone();
            let id = saved
                .filter(|id| state.board.interface(id).is_some())
                .unwrap_or_else(|| state.board.interfaces()[0].id.clone());
            self.activate(&mut state, id);
        }
        self.apply_hotkeys();
        self.save();
    }

    pub async fn preload_sounds(&self) {
        let paths: Vec<String> = {
            let state = self.lock();
            state
                .board
                .sounds()
                .iter()
                .map(|sound| sound.path.clone())
                .collect()
        };
        for path in paths {
            // Only an optimization: the error shows up if the user presses the pad.
            let _ = self.player.preload(&path).await;
        }
    }

    pub fn save(&self) {
        if let Err(Failure(reason)) = self.repository.save(&self.lock().board) {
            self.events.emit(AppEvent::error(reason));
        }
    }

    pub fn view(&self) -> BoardView {
        let state = self.lock();
        let settings = self.settings.lock();
        let layout = NumericLayout::for_scheme(settings.numeric_scheme);
        let stop = settings.stop_hotkey.as_ref().or(layout.stop_all.as_ref());

        BoardView {
            interfaces: state
                .board
                .interfaces()
                .iter()
                .map(|item| InterfaceTab {
                    id: item.id.clone(),
                    name: item.layout.name.clone(),
                    rows: item.layout.rows,
                    columns: item.layout.columns,
                    accent: item.layout.accent.clone(),
                    sound_count: state.board.sounds_of(&item.id).count() as u32,
                })
                .collect(),
            active_id: state.active_id.clone(),
            pads: self.pads(&state, &layout),
            target_slot: state.target_slot,
            next_accent: palette::accent_for(state.board.interfaces().len()).to_owned(),
            stop_shortcut: stop.map(Hotkey::to_string),
        }
    }

    /// Called on every frame to light the pads that are sounding.
    pub fn playing_slots(&self) -> Vec<u32> {
        let state = self.lock();
        state
            .board
            .sounds_of(&state.active_id)
            .filter(|sound| self.player.is_playing(sound_tag(sound)))
            .map(|sound| sound.slot)
            .collect()
    }

    /// Called on every frame, which is also how the display learns that the sound has ended.
    pub fn now_playing(&self) -> Option<NowPlaying> {
        let playing = self.player.now_playing();
        let was_playing = self.was_playing.swap(playing.is_some(), Ordering::Relaxed);
        if was_playing && playing.is_none() {
            self.events.emit(AppEvent::Display(DisplayEvent::Silent));
        }
        playing
    }

    pub fn apply_hotkeys(&self) {
        let bindings = {
            let state = self.lock();
            let settings = self.settings.lock();
            let layout = NumericLayout::for_scheme(settings.numeric_scheme);
            resolve(state.board.sounds(), &layout, settings.stop_hotkey.as_ref())
        };
        self.hotkeys.apply(bindings);
    }

    pub async fn on_hotkey(&self, command: HotkeyCommand) {
        match command {
            HotkeyCommand::PlaySound { sound_id } => {
                let sound = self.find_sound(&sound_id);
                if let Some(sound) = sound {
                    self.trigger(&sound).await;
                }
            }
            HotkeyCommand::PlayPad { index } => self.play_pad(index as u32).await,
            HotkeyCommand::NextInterface => self.step_interface(1),
            HotkeyCommand::PreviousInterface => self.step_interface(-1),
            HotkeyCommand::StopAll => self.stop_all(),
            // The voice session answers these.
            HotkeyCommand::ToggleVoice | HotkeyCommand::NextVoice => {}
        }
    }

    /// Makes way for a sound about to play, if the user wants one at a time.
    pub fn stop_others(&self) {
        if self.settings.lock().one_sound_at_a_time {
            self.player.stop_all();
        }
    }

    pub fn stop_all(&self) {
        self.player.stop_all();
        self.show(Message::text(TextKey::Stopped));
    }

    pub fn step_interface(&self, offset: i32) {
        let name = {
            let mut state = self.lock();
            let Some(next) = state.board.neighbor(&state.active_id, offset) else {
                return;
            };
            let (id, name) = (next.id.clone(), next.layout.name.clone());
            self.activate(&mut state, id);
            name
        };
        self.show(Message::plain(name));
        self.publish();
    }

    pub fn select_interface(&self, id: &str) {
        let name = {
            let mut state = self.lock();
            let Some(name) = state
                .board
                .interface(id)
                .map(|item| item.layout.name.clone())
            else {
                return;
            };
            if id == state.active_id {
                return;
            }
            self.activate(&mut state, id.to_owned());
            name
        };
        self.show(Message::plain(name));
        self.publish();
    }

    pub fn add_interface(&self, layout: InterfaceLayout) {
        let name = layout.name.clone();
        {
            let mut state = self.lock();
            let Ok(id) = state.board.add_interface(layout) else {
                return;
            };
            self.activate(&mut state, id);
        }
        self.save();
        self.show(Message::plain(name));
        self.publish();
    }

    pub fn configure_interface(&self, id: &str, layout: InterfaceLayout) {
        {
            let mut state = self.lock();
            if state.board.reconfigure(id, layout).is_err() {
                return;
            }
            if state.active_id == id {
                state.target_slot = None;
            }
        }
        self.after_board_edit();
    }

    pub fn move_interface(&self, id: &str, offset: i32) {
        self.lock().board.move_interface(id, offset);
        self.after_board_edit();
    }

    pub fn delete_interface(&self, id: &str) {
        let sounds = {
            let mut state = self.lock();
            let sounds: Vec<Sound> = state.board.sounds_of(id).cloned().collect();
            let Ok(next) = state.board.delete_interface(id) else {
                return;
            };
            for sound in &sounds {
                self.player.stop(sound_tag(sound));
            }
            if state.active_id == id {
                self.activate(&mut state, next);
            }
            sounds
        };
        self.apply_hotkeys();
        self.after_board_edit();
        for sound in sounds {
            self.release_file(&sound.path);
        }
    }

    /// Deletes a sound file once no pad and no favorite use it anymore.
    pub fn release_file(&self, path: &str) {
        let on_a_pad = self
            .lock()
            .board
            .sounds()
            .iter()
            .any(|sound| sound.path == path);
        if on_a_pad || self.favorites.is_favorite(path) {
            return;
        }
        self.player.forget(path);
        self.files.delete(path);
    }

    pub async fn press_pad(&self, slot: u32) {
        match self.sound_in_active(slot) {
            Some(sound) => self.trigger(&sound).await,
            None => {
                self.lock().target_slot = Some(slot);
                self.show(Message::with(
                    TextKey::PadSelected,
                    [Message::plain(pad_label(slot))],
                ));
                self.publish();
            }
        }
    }

    pub fn pad_on_screen(&self, slot: u32) -> PadTarget {
        PadTarget {
            interface_id: self.lock().active_id.clone(),
            slot,
        }
    }

    /// The pad a new sound goes to: the one the user picked, or else the first free one.
    pub fn pad_for_new_sound(&self) -> Option<PadTarget> {
        let state = self.lock();
        let slot = state
            .target_slot
            .or_else(|| state.board.first_free_slot(&state.active_id))?;
        Some(PadTarget {
            interface_id: state.active_id.clone(),
            slot,
        })
    }

    /// Lights the loading state of a pad while `work` runs, however it ends.
    pub async fn while_loading<T>(&self, target: &PadTarget, work: impl Future<Output = T>) -> T {
        self.lock().loading.push(target.clone());
        self.publish();

        let result = work.await;

        let mut state = self.lock();
        if let Some(index) = state.loading.iter().position(|item| item == target) {
            state.loading.remove(index);
        }
        drop(state);
        self.publish();
        result
    }

    pub async fn place(
        &self,
        target: &PadTarget,
        name: &str,
        path: &str,
        color: &str,
    ) -> Result<(), Failure> {
        self.player.forget(path);
        self.player.preload(path).await?;

        let replaced = {
            let mut state = self.lock();
            let PadTarget { interface_id, slot } = target;
            // The interface may have been deleted or made smaller while the sound was loading.
            let fits = state
                .board
                .interface(interface_id)
                .is_some_and(|item| *slot < item.layout.capacity());
            if !fits {
                return Ok(());
            }

            let replaced = state
                .board
                .sound_at(interface_id, *slot)
                .map(|sound| sound.path.clone());
            state.board.place(interface_id, *slot, name, path, color);
            if state.active_id == *interface_id {
                state.target_slot = None;
            }
            replaced
        };
        self.after_board_edit();
        if let Some(path) = replaced {
            self.release_file(&path);
        }
        self.show(Message::plain(format!("{} {name}", pad_label(target.slot))));
        Ok(())
    }

    pub async fn import_file(&self, source_path: &str, slot: u32) {
        let target = self.pad_on_screen(slot);
        let placed = async {
            let path = self.files.import(source_path)?;
            let name = file_stem(source_path);
            self.place(&target, &name, &path, palette::pad_color_for(slot))
                .await
        };
        if let Err(Failure(reason)) = self.while_loading(&target, placed).await {
            self.events.emit(AppEvent::error(reason));
        }
    }

    pub fn swap_pads(&self, from_slot: u32, to_slot: u32) {
        {
            let mut state = self.lock();
            let active = state.active_id.clone();
            state.board.swap(&active, from_slot, to_slot);
            state.target_slot = None;
        }
        self.after_board_edit();
    }

    pub fn send_to_interface(&self, slot: u32, target_id: &str) {
        let Some(sound) = self.sound_in_active(slot) else {
            return;
        };
        let name = self
            .lock()
            .board
            .interface(target_id)
            .map(|item| item.layout.name.clone())
            .unwrap_or_default();

        if self.lock().board.send_to(&sound.id, target_id) {
            self.after_board_edit();
            self.show(Message::with(TextKey::SentTo, [Message::plain(name)]));
        } else {
            self.events.emit(AppEvent::error(Message::with(
                TextKey::InterfaceFull,
                [Message::plain(name)],
            )));
        }
    }

    pub fn rename(&self, slot: u32, name: &str) {
        let name = name.trim();
        if !name.is_empty() {
            self.edit_sound(slot, |sound| sound.name = name.to_owned());
        }
    }

    pub fn set_volume(&self, slot: u32, volume: f64) {
        self.edit_sound(slot, |sound| sound.volume = volume);
    }

    pub fn set_color(&self, slot: u32, color: &str) {
        self.edit_sound(slot, |sound| sound.color = color.to_owned());
    }

    pub fn set_hotkey(&self, slot: u32, hotkey: Hotkey) {
        let Some(target) = self.sound_in_active(slot) else {
            return;
        };
        {
            let mut state = self.lock();
            let holders: Vec<String> = state
                .board
                .sounds()
                .iter()
                .filter(|sound| sound.hotkey.as_ref() == Some(&hotkey))
                .map(|sound| sound.id.clone())
                .collect();
            for id in holders {
                if let Some(sound) = state.board.sound_mut(&id) {
                    sound.hotkey = None;
                }
            }
            if let Some(sound) = state.board.sound_mut(&target.id) {
                sound.hotkey = Some(hotkey);
            }
        }
        self.apply_hotkeys();
        self.after_board_edit();
    }

    pub fn clear_hotkey(&self, slot: u32) {
        self.edit_sound(slot, |sound| sound.hotkey = None);
        self.apply_hotkeys();
    }

    pub fn remove(&self, slot: u32) {
        let Some(sound) = self.sound_in_active(slot) else {
            return;
        };
        self.player.stop(sound_tag(&sound));
        self.lock().board.remove(&sound.id);
        self.apply_hotkeys();
        self.after_board_edit();
        self.release_file(&sound.path);
    }

    /// Redraws the pads after something that changes them from outside, such as a favorite.
    pub fn publish(&self) {
        self.events.emit(AppEvent::Board(self.view()));
    }

    async fn play_pad(&self, slot: u32) {
        match self.sound_in_active(slot) {
            Some(sound) => self.trigger(&sound).await,
            None => self.show(Message::with(
                TextKey::EmptyPad,
                [Message::plain(pad_label(slot))],
            )),
        }
    }

    async fn trigger(&self, sound: &Sound) {
        let tag = sound_tag(sound);
        if self.settings.lock().press_again_to_stop && self.player.is_playing(tag) {
            self.player.stop(tag);
            return;
        }
        self.stop_others();
        let played = self
            .player
            .play(&sound.path, sound.volume as f32, sound_tag(sound))
            .await;
        match played {
            Ok(()) => self.events.emit(AppEvent::sound(Message::plain(format!(
                "{} {}",
                pad_label(sound.slot),
                sound.name
            )))),
            Err(Failure(reason)) => self.events.emit(AppEvent::error(Message::with(
                TextKey::FailedSound,
                [Message::plain(sound.name.clone()), reason],
            ))),
        }
    }

    fn pads(&self, state: &State, layout: &NumericLayout) -> Vec<PadView> {
        let capacity = state
            .board
            .interface(&state.active_id)
            .map_or(0, |item| item.layout.capacity());

        (0..capacity)
            .map(|slot| {
                let sound = state.board.sound_at(&state.active_id, slot);
                let numeric = layout.pads.get(slot as usize);
                let shortcut = sound.and_then(|sound| sound.hotkey.as_ref()).or(numeric);

                PadView {
                    slot,
                    label: pad_label(slot),
                    shortcut: shortcut.map(Hotkey::to_string).unwrap_or_default(),
                    is_favorite: sound.is_some_and(|sound| self.favorites.is_favorite(&sound.path)),
                    is_loading: state
                        .loading
                        .iter()
                        .any(|item| item.interface_id == state.active_id && item.slot == slot),
                    sound: sound.map(|sound| SoundView {
                        id: sound.id.clone(),
                        name: sound.name.clone(),
                        path: sound.path.clone(),
                        color: sound.color.clone(),
                        volume: sound.volume,
                        has_hotkey: sound.hotkey.is_some(),
                        is_light: palette::is_light(&sound.color),
                    }),
                }
            })
            .collect()
    }

    fn activate(&self, state: &mut State, id: String) {
        self.settings.lock().active_interface_id = Some(id.clone());
        state.active_id = id;
        state.target_slot = None;
    }

    fn sound_in_active(&self, slot: u32) -> Option<Sound> {
        let state = self.lock();
        state.board.sound_at(&state.active_id, slot).cloned()
    }

    fn find_sound(&self, id: &str) -> Option<Sound> {
        let state = self.lock();
        state
            .board
            .sounds()
            .iter()
            .find(|sound| sound.id == id)
            .cloned()
    }

    fn edit_sound(&self, slot: u32, edit: impl FnOnce(&mut Sound)) {
        let Some(id) = self.sound_in_active(slot).map(|sound| sound.id) else {
            return;
        };
        if let Some(sound) = self.lock().board.sound_mut(&id) {
            edit(sound);
        }
        self.after_board_edit();
    }

    fn after_board_edit(&self) {
        self.save();
        self.publish();
    }

    fn show(&self, message: Message) {
        self.events.emit(AppEvent::show(message));
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().expect("board lock poisoned")
    }
}

fn file_stem(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}
