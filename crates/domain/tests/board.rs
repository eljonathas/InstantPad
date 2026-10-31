use instantpad_domain::board::{BoardError, InterfaceLayout, PadBoard};
use instantpad_domain::palette;

fn layout(rows: u32, columns: u32) -> InterfaceLayout {
    InterfaceLayout {
        name: "Memes".into(),
        rows,
        columns,
        accent: "#ffb03a".into(),
    }
}

fn board() -> (PadBoard, String) {
    let mut board = PadBoard::default();
    board.ensure_interface("Main");
    let first = board.interfaces()[0].id.clone();
    (board, first)
}

#[test]
fn an_empty_board_starts_with_one_interface() {
    let (board, _) = board();

    assert_eq!(board.interfaces().len(), 1);
    assert_eq!(board.interfaces()[0].layout.capacity(), 24);
}

#[test]
fn a_smaller_grid_moves_sounds_into_free_pads() {
    let (mut board, target) = board();
    board.place(&target, 0, "a", "a.mp3", "#ff0000");
    board.place(&target, 5, "b", "b.mp3", "#ff0000");
    board.place(&target, 23, "c", "c.mp3", "#ff0000");

    board.reconfigure(&target, layout(2, 2)).unwrap();

    let mut slots: Vec<u32> = board.sounds_of(&target).map(|sound| sound.slot).collect();
    slots.sort_unstable();
    assert_eq!(slots, [0, 1, 2]);
}

#[test]
fn a_grid_that_cannot_fit_the_sounds_is_refused() {
    let (mut board, target) = board();
    board.place(&target, 0, "a", "a.mp3", "#ff0000");
    board.place(&target, 1, "b", "b.mp3", "#ff0000");

    assert_eq!(
        board.reconfigure(&target, layout(1, 1)),
        Err(BoardError::GridTooSmall {
            capacity: 1,
            sounds: 2
        })
    );
}

#[test]
fn a_grid_past_the_limits_is_refused() {
    let (mut board, target) = board();

    assert_eq!(
        board.reconfigure(&target, layout(9, 10)),
        Err(BoardError::InvalidGrid)
    );
    assert_eq!(
        board.add_interface(layout(8, 11)),
        Err(BoardError::InvalidGrid)
    );
}

#[test]
fn placing_on_an_occupied_pad_replaces_the_sound() {
    let (mut board, target) = board();
    board.place(&target, 0, "a", "a.mp3", "#ff0000");

    let placed = board.place(&target, 0, "b", "b.mp3", "#ff0000").id.clone();

    assert_eq!(board.sounds().len(), 1);
    assert_eq!(board.sounds()[0].id, placed);
}

#[test]
fn a_nearly_black_pad_takes_a_palette_color() {
    let (mut board, target) = board();

    let sound = board.place(&target, 1, "a", "a.mp3", "#000000");

    assert_eq!(sound.color, palette::pad_color_for(1));
}

#[test]
fn swapping_exchanges_two_pads() {
    let (mut board, target) = board();
    let first = board.place(&target, 0, "a", "a.mp3", "#ff0000").id.clone();
    let second = board.place(&target, 1, "b", "b.mp3", "#ff0000").id.clone();

    board.swap(&target, 0, 1);

    assert_eq!(board.sound_at(&target, 1).unwrap().id, first);
    assert_eq!(board.sound_at(&target, 0).unwrap().id, second);
}

#[test]
fn swapping_moves_a_sound_to_an_empty_pad() {
    let (mut board, target) = board();
    let sound = board.place(&target, 0, "a", "a.mp3", "#ff0000").id.clone();

    board.swap(&target, 0, 7);

    assert_eq!(board.sound_at(&target, 7).unwrap().id, sound);
    assert!(board.sound_at(&target, 0).is_none());
}

#[test]
fn sending_uses_the_first_free_pad() {
    let (mut board, source) = board();
    let destination = board.add_interface(layout(1, 2)).unwrap();
    board.place(&destination, 0, "a", "a.mp3", "#ff0000");
    let sound = board.place(&source, 9, "b", "b.mp3", "#ff0000").id.clone();

    assert!(board.send_to(&sound, &destination));

    let moved = board.sound_at(&destination, 1).unwrap();
    assert_eq!(moved.id, sound);
}

#[test]
fn sending_to_a_full_interface_changes_nothing() {
    let (mut board, source) = board();
    let destination = board.add_interface(layout(1, 1)).unwrap();
    board.place(&destination, 0, "a", "a.mp3", "#ff0000");
    let sound = board.place(&source, 9, "b", "b.mp3", "#ff0000").id.clone();

    assert!(!board.send_to(&sound, &destination));

    assert_eq!(board.sound_at(&source, 9).unwrap().id, sound);
}

#[test]
fn deleting_an_interface_removes_its_sounds_and_picks_a_neighbor() {
    let (mut board, first) = board();
    let second = board.add_interface(layout(4, 6)).unwrap();
    board.place(&second, 0, "a", "a.mp3", "#ff0000");

    let next = board.delete_interface(&second).unwrap();

    assert_eq!(next, first);
    assert!(board.sounds().is_empty());
}

#[test]
fn the_last_interface_cannot_be_deleted() {
    let (mut board, first) = board();

    assert_eq!(
        board.delete_interface(&first),
        Err(BoardError::LastInterface)
    );
}

#[test]
fn neighbors_wrap_around() {
    let (mut board, first) = board();
    let second = board.add_interface(layout(4, 6)).unwrap();

    assert_eq!(board.neighbor(&second, 1).unwrap().id, first);
    assert_eq!(board.neighbor(&first, -1).unwrap().id, second);
}

#[test]
fn interfaces_can_change_places() {
    let (mut board, first) = board();
    let second = board.add_interface(layout(4, 6)).unwrap();

    board.move_interface(&second, -1);
    board.move_interface(&second, -1);

    let order: Vec<&str> = board
        .interfaces()
        .iter()
        .map(|item| item.id.as_str())
        .collect();
    assert_eq!(order, [second.as_str(), first.as_str()]);
}
