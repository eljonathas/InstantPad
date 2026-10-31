mod support;

use instantpad_application::hotkeys::{HotkeyCommand, resolve};
use instantpad_domain::board::PadBoard;
use instantpad_domain::hotkeys::{Hotkey, HotkeyModifiers, NumericLayout, NumericScheme};
use support::World;

fn key(code: &str) -> Hotkey {
    Hotkey::new(code, HotkeyModifiers::NONE)
}

fn command_for(bindings: &[(Hotkey, HotkeyCommand)], hotkey: &Hotkey) -> Vec<HotkeyCommand> {
    bindings
        .iter()
        .filter(|(bound, _)| bound == hotkey)
        .map(|(_, command)| command.clone())
        .collect()
}

#[test]
fn a_key_set_on_a_pad_takes_priority_over_the_numeric_scheme() {
    let mut board = PadBoard::default();
    board.ensure_interface("Main");
    let id = board.interfaces()[0].id.clone();
    let sound = board.place(&id, 0, "a", "a.mp3", "#ff0000").id.clone();
    board.sound_mut(&sound).unwrap().hotkey = Some(key("Numpad1"));
    let layout = NumericLayout::for_scheme(NumericScheme::Numpad);

    let bindings = resolve(board.sounds(), &layout, None);

    assert_eq!(
        command_for(&bindings, &key("Numpad1")),
        [HotkeyCommand::PlaySound { sound_id: sound }]
    );
}

#[test]
fn a_custom_stop_key_replaces_the_one_of_the_scheme() {
    let layout = NumericLayout::for_scheme(NumericScheme::Numpad);

    let bindings = resolve(&[], &layout, Some(&key("F9")));

    assert_eq!(command_for(&bindings, &key("F9")), [HotkeyCommand::StopAll]);
    assert!(command_for(&bindings, &key("NumpadDecimal")).is_empty());
}

#[test]
fn the_off_scheme_binds_only_what_the_user_set() {
    let layout = NumericLayout::for_scheme(NumericScheme::Off);

    assert!(resolve(&[], &layout, None).is_empty());
}

#[test]
fn pausing_releases_the_keys_until_every_pause_ends() {
    let world = World::new();
    let hotkeys = world.hotkeys();

    hotkeys.pause();
    hotkeys.pause();
    assert!(world.host.registered.lock().unwrap().is_empty());

    hotkeys.resume();
    assert!(world.host.registered.lock().unwrap().is_empty());

    hotkeys.resume();
    assert!(!world.host.registered.lock().unwrap().is_empty());
}

#[test]
fn bindings_applied_during_a_pause_wait_for_the_end_of_it() {
    let world = World::new();
    let hotkeys = world.hotkeys();

    hotkeys.pause();
    hotkeys.apply(vec![(key("F2"), HotkeyCommand::StopAll)]);
    assert!(world.host.registered.lock().unwrap().is_empty());

    hotkeys.resume();
    assert_eq!(world.host.registered.lock().unwrap().len(), 1);
}

#[test]
fn a_key_owned_by_another_program_is_reported() {
    let world = World::new();
    world.host.taken.lock().unwrap().insert(key("F2"));

    world
        .hotkeys()
        .apply(vec![(key("F2"), HotkeyCommand::StopAll)]);

    assert!(matches!(
        world.last_display(),
        Some(instantpad_application::events::DisplayEvent::Error { .. })
    ));
}
