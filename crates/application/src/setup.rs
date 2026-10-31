use std::sync::{Arc, Mutex};

use instantpad_domain::hotkeys::{Hotkey, NumericLayout, NumericScheme};

use crate::board::BoardSession;
use crate::events::{AppEvent, DisplayEvent, SetupView};
use crate::message::{Failure, Message, TextKey};
use crate::ports::{AudioDevice, AudioRouter, EventSink, SystemAudio, VirtualCable};
use crate::routing::{self, DefaultDevices, RoutingChoice, SavedRouting};
use crate::settings::Settings;

/// The settings screen: language, driver, audio routing and numeric keys.
pub struct SetupSession {
    router: Arc<dyn AudioRouter>,
    system: Arc<dyn SystemAudio>,
    cable: Arc<dyn VirtualCable>,
    settings: Arc<Settings>,
    board: Arc<BoardSession>,
    events: Arc<dyn EventSink>,
    outputs: Mutex<Vec<AudioDevice>>,
    microphones: Mutex<Vec<AudioDevice>>,
}

impl SetupSession {
    pub fn new(
        router: Arc<dyn AudioRouter>,
        system: Arc<dyn SystemAudio>,
        cable: Arc<dyn VirtualCable>,
        settings: Arc<Settings>,
        board: Arc<BoardSession>,
        events: Arc<dyn EventSink>,
    ) -> Self {
        Self {
            router,
            system,
            cable,
            settings,
            board,
            events,
            outputs: Mutex::default(),
            microphones: Mutex::default(),
        }
    }

    pub fn view(&self) -> SetupView {
        let microphones = self
            .microphones
            .lock()
            .expect("device lock poisoned")
            .clone();
        let outputs = self.outputs.lock().expect("device lock poisoned").clone();
        let settings = self.settings.lock();

        let cable = &*self.cable;
        let game_output = outputs
            .iter()
            .find(|device| Some(&device.id) == settings.game_output_id.as_ref());
        let system_output_id = self.system.system_output_id();
        let system_output = outputs
            .iter()
            .find(|device| Some(&device.id) == system_output_id.as_ref());

        SetupView {
            microphones: routing::real_devices(cable, &microphones),
            game_outputs: routing::virtual_devices(cable, &outputs),
            monitor_outputs: routing::real_devices(cable, &outputs),
            microphone_id: settings.microphone_id.clone(),
            game_output_id: settings.game_output_id.clone(),
            monitor_output_id: settings.monitor_output_id.clone(),
            has_cable: cable.find_game_output(&outputs).is_some(),
            cable_name: cable.product_name().to_owned(),
            is_system_output_on_cable: system_output.is_some_and(|device| cable.is_virtual(device)),
            game_microphone_name: cable.game_microphone_name(game_output),
            language: settings.language.clone(),
            numeric_scheme: settings.numeric_scheme,
            numeric_hint_keys: numeric_hint_keys(settings.numeric_scheme),
            voice_keys: voice_keys(settings.numeric_scheme),
            stop_hotkey: settings.stop_hotkey.clone(),
            one_sound_at_a_time: settings.one_sound_at_a_time,
            press_again_to_stop: settings.press_again_to_stop,
        }
    }

    /// Reads the devices again and reopens all three roles.
    pub async fn load_devices(&self) {
        let choice = self.refresh_devices();
        self.publish();

        // Separately, so one stuck device does not silence the others.
        let (microphone, game, monitor) = tokio::join!(
            self.router.set_microphone(id_of(&choice.microphone)),
            self.router.set_game_output(id_of(&choice.game_output)),
            self.router
                .set_monitor_output(id_of(&choice.monitor_output)),
        );
        for result in [microphone, game, monitor] {
            self.report(result);
        }
    }

    pub async fn choose_microphone(&self, id: Option<String>) {
        self.settings.lock().microphone_id = id.clone();
        self.report(self.router.set_microphone(id.as_deref()).await);
        self.after_routing_change();
    }

    pub async fn choose_game_output(&self, id: Option<String>) {
        self.settings.lock().game_output_id = id.clone();
        self.report(self.router.set_game_output(id.as_deref()).await);
        self.after_routing_change();
    }

    pub async fn choose_monitor_output(&self, id: Option<String>) {
        self.settings.lock().monitor_output_id = id.clone();
        self.report(self.router.set_monitor_output(id.as_deref()).await);
        self.after_routing_change();
    }

    pub fn set_numeric_scheme(&self, scheme: NumericScheme) {
        self.settings.lock().numeric_scheme = scheme;
        self.after_hotkey_change();
    }

    pub fn set_stop_hotkey(&self, hotkey: Hotkey) {
        self.settings.lock().stop_hotkey = Some(hotkey);
        self.after_hotkey_change();
    }

    pub fn set_one_sound_at_a_time(&self, enabled: bool) {
        self.settings.lock().one_sound_at_a_time = enabled;
        self.save();
        self.publish();
    }

    pub fn set_press_again_to_stop(&self, enabled: bool) {
        self.settings.lock().press_again_to_stop = enabled;
        self.save();
        self.publish();
    }

    pub fn set_language(&self, language: Option<String>) {
        self.settings.lock().language = language;
        self.save();
        self.publish();
    }

    pub async fn install_driver(&self) {
        let events = self.events.clone();
        let progress = move |message| events.emit(AppEvent::DriverStatus(message));

        if let Err(Failure(reason)) = self.cable.install(&progress).await {
            self.events.emit(AppEvent::DriverStatus(reason));
        }
    }

    pub fn open_sound_settings(&self) {
        self.system.open_sound_settings();
    }

    fn refresh_devices(&self) -> RoutingChoice {
        let microphones = self.router.list_microphones();
        let outputs = self.router.list_outputs();
        let default_microphone = self.system.default_microphone_id();
        let default_output = self.system.default_output_id();

        let mut settings = self.settings.lock();
        let choice = routing::choose(
            &*self.cable,
            &microphones,
            &outputs,
            SavedRouting {
                microphone_id: settings.microphone_id.as_deref(),
                game_output_id: settings.game_output_id.as_deref(),
                monitor_output_id: settings.monitor_output_id.as_deref(),
            },
            DefaultDevices {
                microphone_id: default_microphone.as_deref(),
                output_id: default_output.as_deref(),
            },
        );
        settings.microphone_id = choice.microphone.as_ref().map(|device| device.id.clone());
        settings.game_output_id = choice.game_output.as_ref().map(|device| device.id.clone());
        settings.monitor_output_id = choice
            .monitor_output
            .as_ref()
            .map(|device| device.id.clone());
        drop(settings);

        *self.microphones.lock().expect("device lock poisoned") = microphones;
        *self.outputs.lock().expect("device lock poisoned") = outputs;
        choice
    }

    fn after_routing_change(&self) {
        self.save();
        self.publish();
    }

    fn after_hotkey_change(&self) {
        self.save();
        self.board.apply_hotkeys();
        self.board.publish();
        self.publish();
    }

    fn publish(&self) {
        let view = self.view();
        self.events.emit(AppEvent::Display(routing_display(&view)));
        self.events.emit(AppEvent::Setup(view));
    }

    fn save(&self) {
        self.report(self.settings.save());
    }

    fn report(&self, result: Result<(), Failure>) {
        if let Err(Failure(reason)) = result {
            self.events.emit(AppEvent::error(reason));
        }
    }
}

fn numeric_hint_keys(scheme: NumericScheme) -> Vec<String> {
    let layout = NumericLayout::for_scheme(scheme);
    [
        layout.pads.first(),
        layout.pads.last(),
        layout.next_interface.as_ref(),
        layout.previous_interface.as_ref(),
        layout.stop_all.as_ref(),
    ]
    .into_iter()
    .flatten()
    .map(Hotkey::to_string)
    .collect()
}

fn voice_keys(scheme: NumericScheme) -> Vec<String> {
    let layout = NumericLayout::for_scheme(scheme);
    [layout.toggle_voice, layout.next_voice]
        .iter()
        .flatten()
        .map(Hotkey::to_string)
        .collect()
}

fn id_of(device: &Option<AudioDevice>) -> Option<&str> {
    device.as_ref().map(|device| device.id.as_str())
}

fn routing_display(view: &SetupView) -> DisplayEvent {
    let state = |ok: bool, missing: Message| {
        if ok {
            Message::text(TextKey::Ok)
        } else {
            missing
        }
    };
    let absent = || Message::plain("--");

    let status = Message::with(
        TextKey::RoutingStatus,
        [
            state(view.microphone_id.is_some(), absent()),
            state(view.game_output_id.is_some(), absent()),
            state(view.has_cable, Message::text(TextKey::Missing)),
        ],
    );
    let hint = if !view.has_cable {
        Message::text(TextKey::NoDriverHint)
    } else if view.is_system_output_on_cable {
        Message::text(TextKey::SystemOutputOnCableHint)
    } else {
        Message::with(
            TextKey::GameMicrophoneHint,
            [Message::plain(view.game_microphone_name.clone())],
        )
    };

    DisplayEvent::Routing { status, hint }
}
