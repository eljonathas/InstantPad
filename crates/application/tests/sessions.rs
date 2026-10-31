mod support;

use std::sync::Arc;

use instantpad_application::events::{AppEvent, UpdateView};
use instantpad_application::message::{Message, TextKey};
use instantpad_application::mixer::MixerSession;
use instantpad_application::ports::{CatalogPage, CatalogSound, SoundFileStore};
use instantpad_application::rows::SoundRows;
use instantpad_application::settings::Settings;
use instantpad_application::setup::SetupSession;
use instantpad_application::updates::UpdateSession;
use instantpad_domain::hotkeys::NumericScheme;
use instantpad_domain::sounds::SoundSource;
use instantpad_test_support::fakes::{
    FakeCable, FakeMix, FakeRouter, FakeSystemAudio, FakeUpdater, InMemorySettings,
    RecordingEvents, device,
};
use support::World;

fn online() -> SoundSource {
    SoundSource {
        name: "a".to_owned(),
        color: "#ffffff".to_owned(),
        path: "store/a.mp3".to_owned(),
        sound_url: Some("https://site/a.mp3".to_owned()),
    }
}

fn rows(world: &World) -> SoundRows {
    SoundRows::new(
        world.board.clone(),
        world.library.clone(),
        world.favorites.clone(),
        world.player.clone(),
        world.events.clone(),
    )
}

#[tokio::test]
async fn a_preview_stays_on_the_headphones() {
    let world = World::new();

    rows(&world).preview(online()).await;

    assert_eq!(*world.player.previewed.lock().unwrap(), ["previews/a.mp3"]);
    assert!(world.player.played.lock().unwrap().is_empty());
    assert_eq!(world.catalog.downloaded.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn a_sound_only_heard_never_reaches_the_store() {
    let world = World::new();

    rows(&world).preview(online()).await;
    rows(&world).play_in_game(online()).await;

    assert!(!world.files.exists("store/a.mp3"));
}

#[tokio::test]
async fn a_sound_only_heard_does_not_stay_decoded_in_memory() {
    let world = World::new();

    rows(&world).preview(online()).await;

    assert_eq!(*world.player.forgotten.lock().unwrap(), ["previews/a.mp3"]);
}

#[tokio::test]
async fn hearing_a_sound_already_kept_uses_its_file_and_downloads_nothing() {
    let world = World::new();
    world
        .files
        .files
        .lock()
        .unwrap()
        .insert("store/a.mp3".to_owned());

    rows(&world).preview(online()).await;

    assert_eq!(*world.player.previewed.lock().unwrap(), ["store/a.mp3"]);
    assert!(world.catalog.downloaded.lock().unwrap().is_empty());
    assert!(world.player.forgotten.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_sound_heard_first_is_not_downloaded_again_when_it_goes_to_a_pad() {
    let world = World::new();
    world
        .files
        .files
        .lock()
        .unwrap()
        .insert("previews/a.mp3".to_owned());

    rows(&world).add_to_pad(online(), Some(0)).await;

    assert!(world.catalog.downloaded.lock().unwrap().is_empty());
    assert!(world.files.exists("store/a.mp3"));
    assert!(!world.files.exists("previews/a.mp3"));
}

#[tokio::test]
async fn a_sound_played_in_the_game_does_not_take_a_pad() {
    let world = World::new();

    rows(&world).play_in_game(online()).await;

    assert_eq!(*world.player.played.lock().unwrap(), ["previews/a.mp3"]);
    assert!(
        world
            .board
            .view()
            .pads
            .iter()
            .all(|pad| pad.sound.is_none())
    );
}

#[tokio::test]
async fn a_row_goes_to_the_pad_the_user_picked() {
    let world = World::new();
    world.board.press_pad(3).await;

    rows(&world).add_to_pad(online(), None).await;

    assert!(world.board.view().pads[3].sound.is_some());
}

fn loading_seen(world: &World, slot: usize) -> Vec<bool> {
    let events = world.events.0.lock().unwrap();
    events
        .iter()
        .filter_map(|event| match event {
            AppEvent::Board(view) => Some(view.pads[slot].is_loading),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn a_pad_shows_it_is_loading_until_its_sound_arrives() {
    let world = World::new();

    rows(&world).add_to_pad(online(), Some(2)).await;

    let seen = loading_seen(&world, 2);
    assert_eq!(seen.first(), Some(&true));
    assert_eq!(seen.last(), Some(&false));
    assert!(world.board.view().pads[2].sound.is_some());
}

#[tokio::test]
async fn a_download_that_fails_leaves_no_pad_loading() {
    let world = World::new();
    let gone = SoundSource {
        sound_url: None,
        ..online()
    };

    rows(&world).add_to_pad(gone, Some(2)).await;

    assert_eq!(loading_seen(&world, 2).last(), Some(&false));
    assert!(world.board.view().pads[2].sound.is_none());
}

#[tokio::test]
async fn one_sound_at_a_time_also_covers_the_rows() {
    let world = World::new();
    let target = world.board.pad_on_screen(0);
    world
        .board
        .place(&target, "a", "a.mp3", "#ff0000")
        .await
        .unwrap();
    world.board.press_pad(0).await;
    world.settings.lock().one_sound_at_a_time = true;

    rows(&world).play_in_game(online()).await;

    assert!(world.board.playing_slots().is_empty());
}

#[tokio::test]
async fn favoriting_a_row_lights_the_mark_on_its_pad() {
    let world = World::new();
    let rows = rows(&world);
    rows.add_to_pad(online(), Some(0)).await;

    rows.toggle_favorite(online()).await;

    assert!(world.board.view().pads[0].is_favorite);
}

#[tokio::test]
async fn unfavoriting_deletes_the_file_of_a_sound_on_no_pad() {
    let world = World::new();
    world
        .files
        .files
        .lock()
        .unwrap()
        .insert("store/a.mp3".to_owned());
    let rows = rows(&world);
    rows.toggle_favorite(online()).await;

    rows.toggle_favorite(online()).await;

    assert!(!world.files.exists("store/a.mp3"));
}

#[tokio::test]
async fn unfavoriting_keeps_the_file_of_a_sound_on_a_pad() {
    let world = World::new();
    world
        .files
        .files
        .lock()
        .unwrap()
        .insert("store/a.mp3".to_owned());
    let rows = rows(&world);
    rows.add_to_pad(online(), Some(0)).await;
    rows.toggle_favorite(online()).await;

    rows.toggle_favorite(online()).await;

    assert!(world.files.exists("store/a.mp3"));
}

#[tokio::test]
async fn saving_a_sound_copies_it_to_the_chosen_place() {
    let world = World::new();

    rows(&world)
        .save_to_computer(online(), "/music/a.mp3")
        .await;

    let exported = world.files.exported.lock().unwrap();
    assert_eq!(
        exported[0],
        ("store/a.mp3".to_owned(), "/music/a.mp3".to_owned())
    );
}

#[tokio::test]
async fn catalog_results_point_at_the_store() {
    let world = World::new();
    *world.catalog.page.lock().unwrap() = Some(CatalogPage {
        sounds: vec![CatalogSound {
            name: "a".to_owned(),
            sound_url: "https://site/a.mp3".to_owned(),
            color: "#fff".to_owned(),
        }],
        has_more: true,
    });

    let page = world.library.search("a", 1).await.unwrap();

    assert_eq!(page.sounds[0].path, "store/a.mp3");
    assert!(page.has_more);
}

fn mixer() -> (Arc<FakeMix>, MixerSession) {
    let mix = Arc::new(FakeMix::default());
    let settings = Arc::new(Settings::load(Arc::new(InMemorySettings::default())));
    (mix.clone(), MixerSession::new(mix, settings))
}

#[test]
fn the_mix_starts_from_the_saved_settings() {
    let (mix, _) = mixer();

    assert_eq!(*mix.game_volume.lock().unwrap(), 0.8);
    assert!(*mix.mic_enabled.lock().unwrap());
}

#[test]
fn the_mic_is_never_boosted() {
    let (mix, session) = mixer();

    session.set_mic_volume(1.5);

    assert_eq!(*mix.mic_volume.lock().unwrap(), 1.0);
    assert_eq!(session.view().mic_volume, 1.0);
}

#[test]
fn meters_fall_slowly_after_a_peak() {
    let (mix, session) = mixer();
    *mix.mic.lock().unwrap() = 1.0;
    assert_eq!(session.levels().0, 1.0);

    *mix.mic.lock().unwrap() = 0.0;
    let (level, _) = session.levels();

    assert!(level > 0.9 && level < 1.0, "{level}");
}

struct Setup {
    world: World,
    router: Arc<FakeRouter>,
    system: Arc<FakeSystemAudio>,
    session: SetupSession,
}

fn setup() -> Setup {
    let world = World::new();
    let router = Arc::new(FakeRouter::default());
    let system = Arc::new(FakeSystemAudio::default());
    *router.microphones.lock().unwrap() = vec![device("mic", "USB Mic")];
    *router.outputs.lock().unwrap() = vec![
        device("phones", "Headphones"),
        device("cable", "CABLE Input"),
    ];

    let session = SetupSession::new(
        router.clone(),
        system.clone(),
        Arc::new(FakeCable),
        world.settings.clone(),
        world.board.clone(),
        world.events.clone(),
    );
    Setup {
        world,
        router,
        system,
        session,
    }
}

#[tokio::test]
async fn loading_devices_opens_every_role() {
    let setup = setup();

    setup.session.load_devices().await;

    let opened = setup.router.opened.lock().unwrap();
    assert_eq!(*opened, ["mic:mic", "game:cable", "monitor:phones"]);
    assert!(setup.session.view().has_cable);
}

#[tokio::test]
async fn the_system_output_on_the_cable_is_flagged() {
    let setup = setup();
    *setup.system.system_output.lock().unwrap() = Some("cable".to_owned());

    setup.session.load_devices().await;

    assert!(setup.session.view().is_system_output_on_cable);
}

#[tokio::test]
async fn choosing_a_device_is_remembered() {
    let setup = setup();

    setup
        .session
        .choose_monitor_output(Some("other".to_owned()))
        .await;

    assert_eq!(
        setup.world.settings.lock().monitor_output_id.as_deref(),
        Some("other")
    );
    assert_eq!(*setup.router.opened.lock().unwrap(), ["monitor:other"]);
}

#[tokio::test]
async fn one_sound_at_a_time_is_remembered_and_shown() {
    let setup = setup();

    setup.session.set_one_sound_at_a_time(true);

    assert!(setup.session.view().one_sound_at_a_time);
    assert!(setup.world.settings.lock().one_sound_at_a_time);
}

#[tokio::test]
async fn changing_the_numeric_scheme_updates_keys_and_labels() {
    let setup = setup();

    setup.session.set_numeric_scheme(NumericScheme::Off);

    assert!(setup.world.host.registered.lock().unwrap().is_empty());
    assert_eq!(setup.world.board.view().pads[0].shortcut, "");
}

#[tokio::test]
async fn the_driver_install_reports_its_progress() {
    let setup = setup();

    setup.session.install_driver().await;

    let events = setup.world.events.0.lock().unwrap();
    let expected = AppEvent::DriverStatus(Message::text(TextKey::DriverSearching));
    assert!(events.contains(&expected));
}

fn updates(
    is_release: bool,
    available: Option<&str>,
) -> (Arc<FakeUpdater>, Arc<RecordingEvents>, UpdateSession) {
    let updater = Arc::new(FakeUpdater {
        is_release,
        available: available.map(str::to_owned).into(),
        ..FakeUpdater::default()
    });
    let events = Arc::new(RecordingEvents::default());
    let session = UpdateSession::new(updater.clone(), events.clone());
    (updater, events, session)
}

fn last_update(events: &RecordingEvents) -> UpdateView {
    let events = events.0.lock().unwrap();
    events
        .iter()
        .rev()
        .find_map(|event| match event {
            AppEvent::Update(view) => Some(view.clone()),
            _ => None,
        })
        .expect("an update event")
}

#[tokio::test]
async fn a_local_build_never_replaces_itself() {
    let (updater, events, session) = updates(false, Some("2.0.0"));

    session.check_and_download().await;
    session.apply_on_exit();

    assert_eq!(
        last_update(&events).status,
        Message::text(TextKey::DevelopmentVersion)
    );
    assert!(updater.applied.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_downloaded_version_waits_for_the_user() {
    let (updater, events, session) = updates(true, Some("2.0.0"));

    session.check_and_download().await;

    assert!(last_update(&events).is_ready_to_restart);
    assert!(updater.applied.lock().unwrap().is_empty());
}

#[tokio::test]
async fn restarting_applies_the_version_once_even_though_it_also_exits() {
    let (updater, _, session) = updates(true, Some("2.0.0"));
    session.check_and_download().await;

    session.restart_now();
    session.apply_on_exit();

    assert_eq!(*updater.applied.lock().unwrap(), [true]);
}

#[tokio::test]
async fn closing_installs_the_version_without_opening_the_app_again() {
    let (updater, _, session) = updates(true, Some("2.0.0"));
    session.check_and_download().await;

    session.apply_on_exit();

    assert_eq!(*updater.applied.lock().unwrap(), [false]);
}

#[tokio::test]
async fn closing_without_a_new_version_does_nothing() {
    let (updater, _, session) = updates(true, None);
    session.check_and_download().await;

    session.apply_on_exit();

    assert!(updater.applied.lock().unwrap().is_empty());
}
