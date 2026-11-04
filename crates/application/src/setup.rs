use std::sync::{Arc, Mutex};

use instantpad_domain::hotkeys::{Hotkey, NumericLayout, NumericScheme};
use instantpad_domain::trending;

use crate::board::BoardSession;
use crate::events::{AppEvent, DisplayEvent, SetupView};
use crate::message::{Failure, Message, TextKey};
use crate::ports::{AudioDevice, AudioRouter, DeviceRole, EventSink, SystemAudio, VirtualCable};
use crate::recovery::{Notice, Recovery, Step};
use crate::routing::{self, DefaultDevices, RoutingChoice, SavedRouting};
use crate::settings::{AppSettings, Settings};

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
    /// Also lets one routing change finish before the next starts, so bringing a device back
    /// never undoes what the user just chose.
    recovery: tokio::sync::Mutex<Recovery>,
    /// The device open in place of the chosen one while that one is missing, by role. The
    /// settings keep the chosen one, which comes back as soon as it is there again.
    stand_ins: Mutex<[Option<AudioDevice>; 3]>,
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
            recovery: tokio::sync::Mutex::default(),
            stand_ins: Mutex::default(),
        }
    }

    pub fn view(&self) -> SetupView {
        let microphones = self
            .microphones
            .lock()
            .expect("device lock poisoned")
            .clone();
        let outputs = self.outputs.lock().expect("device lock poisoned").clone();
        let stand_ins = self.stand_ins.lock().expect("device lock poisoned").clone();
        // Before the lock below, which working it out takes too.
        let trending_region = self.settings.trending_region();
        let settings = self.settings.lock();
        let in_use = |role: DeviceRole| match &stand_ins[role as usize] {
            Some(stand_in) => Some(stand_in.id.clone()),
            None => chosen_id(&settings, role).clone(),
        };
        let game_output_id = in_use(DeviceRole::GameOutput);

        let cable = &*self.cable;
        let game_output = outputs
            .iter()
            .find(|device| Some(&device.id) == game_output_id.as_ref());
        let system_output_id = self.system.system_output_id();
        let system_output = outputs
            .iter()
            .find(|device| Some(&device.id) == system_output_id.as_ref());

        SetupView {
            microphones: routing::real_devices(cable, &microphones),
            game_outputs: routing::virtual_devices(cable, &outputs),
            monitor_outputs: routing::real_devices(cable, &outputs),
            microphone_id: in_use(DeviceRole::Microphone),
            game_output_id,
            monitor_output_id: in_use(DeviceRole::MonitorOutput),
            has_cable: cable.find_game_output(&outputs).is_some(),
            cable_name: cable.product_name().to_owned(),
            is_system_output_on_cable: system_output.is_some_and(|device| cable.is_virtual(device)),
            game_microphone_name: cable.game_microphone_name(game_output),
            language: settings.language.clone(),
            trending_region,
            chosen_trending_region: settings.trending_region.clone(),
            numeric_scheme: settings.numeric_scheme,
            numeric_hint_keys: numeric_hint_keys(settings.numeric_scheme),
            voice_keys: voice_keys(settings.numeric_scheme),
            stop_hotkey: settings.stop_hotkey.clone(),
            one_sound_at_a_time: settings.one_sound_at_a_time,
            press_again_to_stop: settings.press_again_to_stop,
            quiet_updates: settings.quiet_updates,
        }
    }

    /// Reads the devices again and reopens all three roles.
    pub async fn load_devices(&self) {
        let mut recovery = self.recovery.lock().await;
        *recovery = Recovery::default();
        *self.stand_ins.lock().expect("device lock poisoned") = Default::default();
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
        self.choose(DeviceRole::Microphone, id).await;
    }

    pub async fn choose_game_output(&self, id: Option<String>) {
        self.choose(DeviceRole::GameOutput, id).await;
    }

    pub async fn choose_monitor_output(&self, id: Option<String>) {
        self.choose(DeviceRole::MonitorOutput, id).await;
    }

    /// Brings back the devices whose stream stopped by itself, such as a headset that
    /// reconnected or a device whose format changed in the system settings. The app calls it
    /// every tick, far from the audio callbacks, and it waits longer after each failure.
    pub async fn watch_devices(&self) {
        let mut recovery = self.recovery.lock().await;
        for (role, stop) in self.router.take_stopped() {
            let Some(notice) = recovery.stopped(role, stop) else {
                continue;
            };
            let key = match notice {
                Notice::Reconnecting => TextKey::DeviceLost,
                Notice::GaveUp => TextKey::DeviceGaveUp,
                Notice::Failed => TextKey::DeviceStopped,
            };
            self.events
                .emit(AppEvent::error(Message::with(key, [role_name(role)])));
        }

        for role in DeviceRole::ALL {
            match recovery.tick(role) {
                Step::Wait => {}
                Step::Reopen => self.bring_back(&mut recovery, role, false).await,
                Step::LookForChosen => self.bring_back(&mut recovery, role, true).await,
            }
        }
    }

    async fn choose(&self, role: DeviceRole, id: Option<String>) {
        let mut recovery = self.recovery.lock().await;
        recovery.chosen(role);
        self.stand_ins.lock().expect("device lock poisoned")[role as usize] = None;
        *chosen_id_mut(&mut self.settings.lock(), role) = id.clone();
        self.report(self.open(role, id.as_deref()).await);
        self.after_routing_change();
    }

    /// Opens the role on the chosen device, or on the one that would stand in for it when it is
    /// missing. With `only_chosen`, a role already running on a stand-in moves only once the
    /// chosen device is back.
    async fn bring_back(&self, recovery: &mut Recovery, role: DeviceRole, only_chosen: bool) {
        let chosen = chosen_id(&self.settings.lock(), role).clone();
        // Before the lists are read again, which no longer have a device that left.
        let chosen_name = self.name_of(role, chosen.as_deref());
        let device = self.pick_devices().of(role).cloned();
        let is_chosen = device.as_ref().map(|device| &device.id) == chosen.as_ref();
        if only_chosen && !is_chosen {
            return;
        }

        let opened = match &device {
            Some(device) => self.open(role, Some(&device.id)).await.is_ok(),
            None => false,
        };
        let Some(device) = device.filter(|_| opened) else {
            if recovery.failed(role) {
                self.stand_ins.lock().expect("device lock poisoned")[role as usize] = None;
                self.events.emit(AppEvent::error(Message::with(
                    TextKey::DeviceGaveUp,
                    [role_name(role)],
                )));
            }
            return;
        };

        recovery.reopened(role, !is_chosen);
        let message = if is_chosen {
            Message::with(TextKey::DeviceBack, [role_name(role)])
        } else {
            Message::with(
                TextKey::DeviceStandIn,
                [Message::plain(device.name.clone()), chosen_name],
            )
        };
        self.stand_ins.lock().expect("device lock poisoned")[role as usize] =
            (!is_chosen).then_some(device);
        self.events.emit(AppEvent::show(message));
        self.publish();
    }

    async fn open(&self, role: DeviceRole, id: Option<&str>) -> Result<(), Failure> {
        match role {
            DeviceRole::Microphone => self.router.set_microphone(id).await,
            DeviceRole::GameOutput => self.router.set_game_output(id).await,
            DeviceRole::MonitorOutput => self.router.set_monitor_output(id).await,
        }
    }

    /// The name of a device as last listed, or what the role is called when it is not there.
    fn name_of(&self, role: DeviceRole, id: Option<&str>) -> Message {
        let devices = match role {
            DeviceRole::Microphone => &self.microphones,
            DeviceRole::GameOutput | DeviceRole::MonitorOutput => &self.outputs,
        };
        devices
            .lock()
            .expect("device lock poisoned")
            .iter()
            .find(|device| Some(device.id.as_str()) == id)
            .map_or_else(|| role_name(role), |device| Message::plain(&device.name))
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

    pub fn set_quiet_updates(&self, enabled: bool) {
        self.settings.lock().quiet_updates = enabled;
        self.save();
        self.publish();
    }

    pub fn set_language(&self, language: Option<String>) {
        self.settings.lock().language = language;
        self.save();
        self.publish();
    }

    /// `None` follows the system. A country the settings do not offer is ignored.
    pub fn set_trending_region(&self, region: Option<String>) {
        if region
            .as_deref()
            .is_some_and(|region| !trending::is_trending_region(region))
        {
            return;
        }
        self.settings.lock().trending_region = region;
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

    /// Lists the devices again and saves what each role takes, in place of the saved devices that
    /// are missing.
    fn refresh_devices(&self) -> RoutingChoice {
        let choice = self.pick_devices();
        let mut settings = self.settings.lock();
        for role in DeviceRole::ALL {
            *chosen_id_mut(&mut settings, role) = choice.of(role).map(|device| device.id.clone());
        }
        choice
    }

    /// Lists the devices again and picks one for each role: the saved one, or one in its place.
    fn pick_devices(&self) -> RoutingChoice {
        let microphones = self.router.list_microphones();
        let outputs = self.router.list_outputs();
        let default_microphone = self.system.default_microphone_id();
        let default_output = self.system.default_output_id();

        let settings = self.settings.lock();
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
        self.events.emit(AppEvent::Setup(Box::new(view)));
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

/// The device the user chose for `role`, which the settings keep.
fn chosen_id(settings: &AppSettings, role: DeviceRole) -> &Option<String> {
    match role {
        DeviceRole::Microphone => &settings.microphone_id,
        DeviceRole::GameOutput => &settings.game_output_id,
        DeviceRole::MonitorOutput => &settings.monitor_output_id,
    }
}

fn chosen_id_mut(settings: &mut AppSettings, role: DeviceRole) -> &mut Option<String> {
    match role {
        DeviceRole::Microphone => &mut settings.microphone_id,
        DeviceRole::GameOutput => &mut settings.game_output_id,
        DeviceRole::MonitorOutput => &mut settings.monitor_output_id,
    }
}

/// What the role is called in a sentence, such as "the microphone".
fn role_name(role: DeviceRole) -> Message {
    Message::text(match role {
        DeviceRole::Microphone => TextKey::RoleMicrophone,
        DeviceRole::GameOutput => TextKey::RoleGameOutput,
        DeviceRole::MonitorOutput => TextKey::RoleMonitorOutput,
    })
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
