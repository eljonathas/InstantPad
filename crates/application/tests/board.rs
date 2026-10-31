mod support;

use instantpad_application::hotkeys::HotkeyCommand;
use instantpad_application::ports::PlaybackTag;
use instantpad_domain::board::InterfaceLayout;
use instantpad_domain::hotkeys::{Hotkey, HotkeyModifiers};
use support::{World, shown};

fn layout(name: &str) -> InterfaceLayout {
    InterfaceLayout {
        name: name.to_owned(),
        rows: 2,
        columns: 2,
        accent: "#35f27a".to_owned(),
    }
}

async fn fill(world: &World, slot: u32, name: &str) {
    let path = format!("{name}.mp3");
    let target = world.board.pad_on_screen(slot);
    world
        .board
        .place(&target, name, &path, "#ff0000")
        .await
        .unwrap();
}

#[tokio::test]
async fn a_hotkey_plays_the_active_interface_without_any_interface_code() {
    let world = World::new();
    fill(&world, 0, "first").await;

    world
        .board
        .on_hotkey(HotkeyCommand::PlayPad { index: 0 })
        .await;

    assert_eq!(*world.player.played.lock().unwrap(), ["first.mp3"]);
    assert_eq!(world.last_display(), Some(shown("01 first")));
}

#[tokio::test]
async fn the_numeric_keys_follow_the_interface_on_screen() {
    let world = World::new();
    fill(&world, 0, "main").await;
    world.board.add_interface(layout("Memes"));
    fill(&world, 0, "memes").await;

    world.board.on_hotkey(HotkeyCommand::NextInterface).await;
    world
        .board
        .on_hotkey(HotkeyCommand::PlayPad { index: 0 })
        .await;

    assert_eq!(*world.player.played.lock().unwrap(), ["main.mp3"]);
}

#[tokio::test]
async fn a_broken_sound_is_reported_and_nothing_stops() {
    let world = World::new();
    fill(&world, 0, "bad").await;
    world
        .player
        .broken
        .lock()
        .unwrap()
        .insert("bad.mp3".to_owned());

    world.board.press_pad(0).await;

    assert!(matches!(
        world.last_display(),
        Some(instantpad_application::events::DisplayEvent::Error { .. })
    ));
}

#[tokio::test]
async fn pressing_an_empty_pad_waits_for_a_sound_and_placing_clears_it() {
    let world = World::new();

    world.board.press_pad(4).await;
    assert_eq!(world.board.view().target_slot, Some(4));
    assert_eq!(world.board.pad_for_new_sound().unwrap().slot, 4);

    fill(&world, 4, "a").await;
    assert_eq!(world.board.view().target_slot, None);
    assert_eq!(world.board.pad_for_new_sound().unwrap().slot, 0);
}

#[tokio::test]
async fn placing_reads_the_file_again_because_it_may_have_changed() {
    let world = World::new();

    fill(&world, 0, "a").await;

    assert_eq!(*world.player.forgotten.lock().unwrap(), ["a.mp3"]);
}

#[tokio::test]
async fn a_hotkey_moves_to_the_last_pad_that_claimed_it() {
    let world = World::new();
    fill(&world, 0, "a").await;
    fill(&world, 1, "b").await;
    let key = Hotkey::new("F2", HotkeyModifiers::NONE);

    world.board.set_hotkey(0, key.clone());
    world.board.set_hotkey(1, key);

    let pads = world.board.view().pads;
    assert!(!pads[0].sound.as_ref().unwrap().has_hotkey);
    assert!(pads[1].sound.as_ref().unwrap().has_hotkey);
    assert_eq!(pads[1].shortcut, "F2");
}

#[tokio::test]
async fn the_first_ten_pads_show_the_numeric_keys() {
    let world = World::new();

    let pads = world.board.view().pads;

    assert_eq!(pads[0].shortcut, "Num1");
    assert_eq!(pads[9].shortcut, "Num0");
    assert_eq!(pads[10].shortcut, "");
}

#[tokio::test]
async fn removing_a_pad_stops_its_sound() {
    let world = World::new();
    fill(&world, 0, "a").await;
    world.board.press_pad(0).await;
    assert_eq!(world.board.playing_slots(), [0]);

    world.board.remove(0);

    assert!(world.board.playing_slots().is_empty());
    assert!(world.player.playing.lock().unwrap().is_empty());
}

#[tokio::test]
async fn deleting_the_interface_on_screen_shows_a_neighbor_and_stops_its_sounds() {
    let world = World::new();
    world.board.add_interface(layout("Memes"));
    fill(&world, 0, "a").await;
    world.board.press_pad(0).await;
    let memes = world.board.view().active_id;

    world.board.delete_interface(&memes);

    let view = world.board.view();
    assert_ne!(view.active_id, memes);
    assert_eq!(view.interfaces.len(), 1);
    assert!(world.player.playing.lock().unwrap().is_empty());
}

#[tokio::test]
async fn sending_to_a_full_interface_keeps_the_pad_and_reports_it() {
    let world = World::new();
    let main = world.board.view().active_id;
    fill(&world, 0, "a").await;
    world.board.add_interface(InterfaceLayout {
        rows: 1,
        columns: 1,
        ..layout("Small")
    });
    let small = world.board.view().active_id;
    fill(&world, 0, "b").await;
    world.board.select_interface(&main);

    world.board.send_to_interface(0, &small);

    assert!(world.board.view().pads[0].sound.is_some());
    assert!(matches!(
        world.last_display(),
        Some(instantpad_application::events::DisplayEvent::Error { .. })
    ));
}

#[tokio::test]
async fn every_edit_reaches_the_repository() {
    let world = World::new();
    fill(&world, 0, "a").await;

    world.board.rename(0, "  renamed  ");
    world.board.set_volume(0, 0.5);

    let saved = world.repository.0.lock().unwrap().clone();
    assert_eq!(saved.sounds()[0].name, "renamed");
    assert_eq!(saved.sounds()[0].volume, 0.5);
}

#[tokio::test]
async fn a_blank_name_is_ignored() {
    let world = World::new();
    fill(&world, 0, "a").await;

    world.board.rename(0, "   ");

    assert_eq!(world.board.view().pads[0].sound.as_ref().unwrap().name, "a");
}

#[tokio::test]
async fn the_active_interface_survives_a_restart() {
    let world = World::new();
    world.board.add_interface(layout("Memes"));

    let saved = world.settings.lock().active_interface_id.clone();

    assert_eq!(saved, Some(world.board.view().active_id));
}

#[tokio::test]
async fn stop_keys_stop_everything() {
    let world = World::new();
    fill(&world, 0, "a").await;
    world.board.press_pad(0).await;

    world.board.on_hotkey(HotkeyCommand::StopAll).await;

    assert!(
        !world
            .player
            .playing
            .lock()
            .unwrap()
            .contains(&PlaybackTag(0))
    );
    assert!(world.player.playing.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_sound_lands_on_the_interface_it_was_aimed_at_even_if_the_user_moved_on() {
    let world = World::new();
    let main = world.board.pad_on_screen(5);
    world.board.add_interface(layout("Memes"));
    let memes = world.board.view().active_id;
    assert_ne!(main.interface_id, memes);

    world
        .board
        .place(&main, "a", "a.mp3", "#ff0000")
        .await
        .unwrap();

    assert!(
        world
            .board
            .view()
            .pads
            .iter()
            .all(|pad| pad.sound.is_none())
    );
    world.board.select_interface(&main.interface_id);
    assert!(world.board.view().pads[5].sound.is_some());
}

#[tokio::test]
async fn a_pad_that_stopped_existing_while_loading_takes_nothing() {
    let world = World::new();
    let far = world.board.pad_on_screen(20);
    let id = far.interface_id.clone();
    world.board.configure_interface(&id, layout("Small"));

    world
        .board
        .place(&far, "a", "a.mp3", "#ff0000")
        .await
        .unwrap();

    assert_eq!(world.board.view().interfaces[0].sound_count, 0);
}

#[tokio::test]
async fn swapping_pads_forgets_the_pad_that_was_waiting_for_a_sound() {
    let world = World::new();
    fill(&world, 0, "a").await;
    world.board.press_pad(1).await;

    world.board.swap_pads(0, 1);

    assert_eq!(world.board.view().target_slot, None);
    assert_eq!(world.board.pad_for_new_sound().unwrap().slot, 0);
}

#[tokio::test]
async fn resizing_the_interface_forgets_the_pad_that_was_waiting_for_a_sound() {
    let world = World::new();
    world.board.press_pad(20).await;
    let id = world.board.view().active_id;

    world.board.configure_interface(&id, layout("Small"));

    assert_eq!(world.board.view().target_slot, None);
}
