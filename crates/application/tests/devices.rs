//! A device whose stream stopped by itself, as when Windows invalidates it, comes back without
//! the user doing anything.

mod support;

use std::sync::Arc;

use instantpad_application::events::{AppEvent, DisplayEvent};
use instantpad_application::message::{Message, TextKey};
use instantpad_application::ports::{DeviceRole, StreamStop};
use instantpad_application::setup::SetupSession;
use instantpad_test_support::fakes::{FakeCable, FakeRouter, FakeSystemAudio, device};
use support::World;

struct Devices {
    world: World,
    router: Arc<FakeRouter>,
    session: SetupSession,
}

impl Devices {
    /// A USB mic chosen over the laptop one, the cable for the game and headphones, all open.
    async fn open() -> Self {
        let world = World::new();
        let router = Arc::new(FakeRouter::default());
        *router.microphones.lock().unwrap() =
            vec![device("laptop", "Laptop Mic"), device("usb", "USB Mic")];
        *router.outputs.lock().unwrap() = vec![
            device("phones", "Headphones"),
            device("cable", "CABLE Input"),
        ];
        world.settings.lock().microphone_id = Some("usb".to_owned());
        let session = SetupSession::new(
            router.clone(),
            Arc::new(FakeSystemAudio::default()),
            Arc::new(FakeCable),
            world.settings.clone(),
            world.board.clone(),
            world.events.clone(),
        );
        session.load_devices().await;
        router.opened.lock().unwrap().clear();
        world.events.0.lock().unwrap().clear();
        Self {
            world,
            router,
            session,
        }
    }

    async fn checks(&self, times: usize) {
        for _ in 0..times {
            self.session.watch_devices().await;
        }
    }

    fn opened(&self) -> Vec<String> {
        self.router.opened.lock().unwrap().clone()
    }

    fn displays(&self) -> Vec<DisplayEvent> {
        self.world
            .events
            .0
            .lock()
            .unwrap()
            .iter()
            .filter_map(|event| match event {
                AppEvent::Display(display) => Some(display.clone()),
                _ => None,
            })
            .collect()
    }

    fn saw(&self, display: DisplayEvent) -> bool {
        self.displays().contains(&display)
    }
}

fn error(key: TextKey, role: TextKey) -> DisplayEvent {
    DisplayEvent::Error {
        message: Message::with(key, [Message::text(role)]),
    }
}

fn shown(message: Message) -> DisplayEvent {
    DisplayEvent::Show { message }
}

#[tokio::test]
async fn a_microphone_that_windows_invalidated_is_opened_again_by_itself() {
    let devices = Devices::open().await;

    devices
        .router
        .stop(DeviceRole::Microphone, StreamStop::Lost);
    devices.checks(1).await;

    assert_eq!(devices.opened(), ["mic:usb"]);
    assert!(devices.saw(error(TextKey::DeviceLost, TextKey::RoleMicrophone)));
    assert_eq!(
        devices.world.last_display(),
        Some(DisplayEvent::Routing {
            status: Message::with(
                TextKey::RoutingStatus,
                [
                    Message::text(TextKey::Ok),
                    Message::text(TextKey::Ok),
                    Message::text(TextKey::Ok)
                ]
            ),
            hint: Message::with(
                TextKey::GameMicrophoneHint,
                [Message::plain("CABLE Output")]
            ),
        })
    );
    assert!(devices.saw(shown(Message::with(
        TextKey::DeviceBack,
        [Message::text(TextKey::RoleMicrophone)]
    ))));
}

#[tokio::test]
async fn the_outputs_are_opened_again_by_themselves_too() {
    let devices = Devices::open().await;

    devices
        .router
        .stop(DeviceRole::GameOutput, StreamStop::Lost);
    devices
        .router
        .stop(DeviceRole::MonitorOutput, StreamStop::Lost);
    devices.checks(1).await;

    assert_eq!(devices.opened(), ["game:cable", "monitor:phones"]);
    assert!(devices.saw(error(TextKey::DeviceLost, TextKey::RoleGameOutput)));
    assert!(devices.saw(error(TextKey::DeviceLost, TextKey::RoleMonitorOutput)));
}

#[tokio::test]
async fn nothing_is_opened_while_every_device_runs() {
    let devices = Devices::open().await;

    devices.checks(500).await;

    assert!(devices.opened().is_empty());
    assert!(devices.displays().is_empty());
}

#[tokio::test]
async fn a_device_that_left_is_replaced_until_it_comes_back_and_stays_chosen() {
    let devices = Devices::open().await;

    devices.router.unplug("usb");
    devices
        .router
        .stop(DeviceRole::Microphone, StreamStop::Lost);
    devices.checks(1).await;

    assert_eq!(devices.opened(), ["mic:laptop"]);
    assert!(devices.saw(shown(Message::with(
        TextKey::DeviceStandIn,
        [Message::plain("Laptop Mic"), Message::plain("USB Mic")]
    ))));
    assert_eq!(
        devices.session.view().microphone_id.as_deref(),
        Some("laptop")
    );
    assert_eq!(
        devices.world.settings.lock().microphone_id.as_deref(),
        Some("usb")
    );

    devices
        .router
        .microphones
        .lock()
        .unwrap()
        .push(device("usb", "USB Mic"));
    devices.checks(10).await;

    assert_eq!(devices.opened(), ["mic:laptop", "mic:usb"]);
    assert_eq!(devices.session.view().microphone_id.as_deref(), Some("usb"));
    assert!(devices.saw(shown(Message::with(
        TextKey::DeviceBack,
        [Message::text(TextKey::RoleMicrophone)]
    ))));
}

#[tokio::test]
async fn a_device_that_is_not_back_yet_is_tried_again_less_and_less_and_then_left_alone() {
    let devices = Devices::open().await;
    devices
        .router
        .broken
        .lock()
        .unwrap()
        .insert("usb".to_owned());

    devices
        .router
        .stop(DeviceRole::Microphone, StreamStop::Lost);
    devices.checks(20).await;
    let early = devices.opened().len();
    devices.checks(500).await;
    let attempts = devices.opened().len();
    devices.checks(500).await;

    assert!(early < 10, "{early} attempts in 20 checks");
    assert_eq!(attempts, 8);
    assert_eq!(devices.opened().len(), attempts, "it stopped trying");
    assert_eq!(
        devices.world.last_display(),
        Some(error(TextKey::DeviceGaveUp, TextKey::RoleMicrophone))
    );
}

#[tokio::test]
async fn a_device_that_takes_a_while_to_come_back_is_opened_once_it_does() {
    let devices = Devices::open().await;
    devices
        .router
        .broken
        .lock()
        .unwrap()
        .insert("usb".to_owned());
    devices
        .router
        .stop(DeviceRole::Microphone, StreamStop::Lost);
    devices.checks(5).await;

    devices.router.broken.lock().unwrap().clear();
    devices.checks(20).await;

    let opened = devices.opened();
    assert!(opened.iter().all(|opened| opened == "mic:usb"));
    devices.checks(100).await;
    assert_eq!(devices.opened(), opened, "nothing more once it is back");
    assert!(devices.saw(shown(Message::with(
        TextKey::DeviceBack,
        [Message::text(TextKey::RoleMicrophone)]
    ))));
}

#[tokio::test]
async fn no_message_carries_what_the_system_said() {
    let devices = Devices::open().await;
    devices
        .router
        .broken
        .lock()
        .unwrap()
        .insert("usb".to_owned());

    devices
        .router
        .stop(DeviceRole::Microphone, StreamStop::Lost);
    devices.checks(500).await;

    let said = format!("{:?}", devices.world.events.0.lock().unwrap());
    assert!(!said.contains("broken"), "{said}");
}

#[tokio::test]
async fn choosing_a_device_stops_bringing_back_the_old_one() {
    let devices = Devices::open().await;
    devices
        .router
        .broken
        .lock()
        .unwrap()
        .insert("usb".to_owned());
    devices
        .router
        .stop(DeviceRole::Microphone, StreamStop::Lost);
    devices.checks(1).await;

    devices
        .session
        .choose_microphone(Some("laptop".to_owned()))
        .await;
    devices.checks(500).await;

    assert_eq!(devices.opened(), ["mic:usb", "mic:laptop"]);
}

#[tokio::test]
async fn a_stream_that_failed_for_another_reason_says_so_and_is_left_to_the_user() {
    let devices = Devices::open().await;

    devices
        .router
        .stop(DeviceRole::Microphone, StreamStop::Failed);
    devices.checks(500).await;

    assert!(devices.opened().is_empty());
    assert_eq!(
        devices.displays(),
        [error(TextKey::DeviceStopped, TextKey::RoleMicrophone)]
    );
}

#[tokio::test]
async fn loading_the_devices_again_starts_over() {
    let devices = Devices::open().await;
    devices
        .router
        .broken
        .lock()
        .unwrap()
        .insert("usb".to_owned());
    devices
        .router
        .stop(DeviceRole::Microphone, StreamStop::Lost);
    devices.checks(500).await;
    devices.router.broken.lock().unwrap().clear();

    devices.session.load_devices().await;
    devices.router.opened.lock().unwrap().clear();
    devices
        .router
        .stop(DeviceRole::Microphone, StreamStop::Lost);
    devices.checks(1).await;

    assert_eq!(devices.opened(), ["mic:usb"]);
}
