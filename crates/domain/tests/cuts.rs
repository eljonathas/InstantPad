use instantpad_domain::cuts::{Cut, MIN_CUT_MS, onsets, pieces};

fn cut(start_ms: u32, end_ms: u32) -> Cut {
    Cut { start_ms, end_ms }
}

#[test]
fn a_cut_stays_inside_the_sound() {
    assert_eq!(Cut::within(100, 900, 1000), Some(cut(100, 900)));
    assert_eq!(Cut::within(100, 5000, 1000), Some(cut(100, 1000)));
    assert_eq!(Cut::within(2000, 5000, 1000), None);
}

#[test]
fn a_cut_shorter_than_a_click_is_refused() {
    assert_eq!(Cut::within(0, MIN_CUT_MS - 1, 1000), None);
    assert_eq!(Cut::within(0, MIN_CUT_MS, 1000), Some(cut(0, MIN_CUT_MS)));
    assert_eq!(Cut::within(500, 400, 1000), None);
}

#[test]
fn a_cut_says_where_it_starts_and_ends() {
    assert_eq!(cut(1200, 2500).label(), "0:01.2-0:02.5");
    assert_eq!(cut(61_000, 125_900).label(), "1:01.0-2:05.9");
    assert_eq!(cut(0, 99).label(), "0:00.0-0:00.0");
    assert_eq!(cut(300, 1300).duration_ms(), 1000);
}

#[test]
fn each_cue_starts_a_piece_that_lasts_until_the_next_one() {
    assert_eq!(
        pieces(&[0, 1000, 2500], 4000),
        [cut(0, 1000), cut(1000, 2500), cut(2500, 4000)]
    );
}

#[test]
fn what_comes_before_the_first_cue_is_left_out() {
    assert_eq!(
        pieces(&[500, 1500], 3000),
        [cut(500, 1500), cut(1500, 3000)]
    );
}

#[test]
fn the_cues_can_come_in_any_order() {
    assert_eq!(
        pieces(&[2000, 0, 1000], 3000),
        pieces(&[0, 1000, 2000], 3000)
    );
}

#[test]
fn no_cue_is_no_piece() {
    assert!(pieces(&[], 3000).is_empty());
}

#[test]
fn cues_outside_the_sound_the_same_cue_twice_and_cues_too_close_are_left_out() {
    assert_eq!(
        pieces(&[0, 0, 1000, 1000 + MIN_CUT_MS - 1, 5000], 3000),
        [cut(0, 1000), cut(1000, 3000)]
    );
}

#[test]
fn a_cue_too_close_to_the_end_gives_its_room_to_the_piece_before() {
    assert_eq!(pieces(&[0, 2990], 3000), [cut(0, 3000)]);
    assert!(pieces(&[2990], 3000).is_empty());
}

#[test]
fn every_piece_lasts_at_least_a_click() {
    let cues: Vec<u32> = (0..200).map(|n| n * 17).collect();

    for piece in pieces(&cues, 3000) {
        assert!(piece.duration_ms() >= MIN_CUT_MS, "{piece:?}");
    }
}

/// Levels of 10 ms frames: `true` is loud.
fn levels(pattern: &[(bool, usize)]) -> Vec<f32> {
    pattern
        .iter()
        .flat_map(|&(loud, frames)| std::iter::repeat_n(if loud { 0.5 } else { 0.0 }, frames))
        .collect()
}

#[test]
fn each_sound_after_a_pause_starts_a_cue_a_little_before_it() {
    let recording = levels(&[(true, 30), (false, 40), (true, 20), (false, 40), (true, 10)]);

    assert_eq!(onsets(&recording, 10), [0, 690, 1290]);
}

#[test]
fn a_recording_that_starts_with_silence_finds_its_first_sound_where_it_is() {
    let recording = levels(&[(false, 50), (true, 30)]);

    assert_eq!(onsets(&recording, 10), [490]);
}

#[test]
fn a_short_pause_inside_a_sound_does_not_split_it() {
    let recording = levels(&[(true, 30), (false, 10), (true, 30)]);

    assert_eq!(onsets(&recording, 10), [0]);
}

#[test]
fn quiet_is_relative_to_the_loudest_moment() {
    let loud = levels(&[(true, 30), (false, 40), (true, 30)]);
    let soft: Vec<f32> = loud.iter().map(|level| level * 0.01).collect();

    assert_eq!(onsets(&soft, 10), onsets(&loud, 10));
}

#[test]
fn a_background_far_below_the_loudest_moment_counts_as_quiet() {
    let recording: Vec<f32> = levels(&[(true, 30), (false, 40), (true, 30)])
        .iter()
        .map(|level| level.max(0.01))
        .collect();

    assert_eq!(onsets(&recording, 10), [0, 690]);
}

#[test]
fn silence_has_no_sound_to_find() {
    assert!(onsets(&vec![0.0; 500], 10).is_empty());
    assert!(onsets(&[], 10).is_empty());
    assert!(onsets(&[0.5; 10], 0).is_empty());
}

#[test]
fn a_pad_saved_before_cuts_existed_still_loads_and_plays_the_whole_file() {
    use instantpad_domain::board::Sound;

    let saved = r##"{
        "id": "a", "name": "Bruh", "path": "/s/bruh.mp3", "color": "#ff0000", "volume": 1.0,
        "hotkey": null, "interfaceId": "i", "slot": 3
    }"##;

    let sound: Sound = serde_json::from_str(saved).unwrap();

    assert_eq!(sound.cut, None);
}

#[test]
fn a_pad_keeps_its_cut_when_saved_and_loaded() {
    use instantpad_domain::board::Sound;

    let saved = r##"{
        "id": "a", "name": "Bruh 2", "path": "/s/bruh.mp3", "color": "#ff0000", "volume": 1.0,
        "hotkey": null, "interfaceId": "i", "slot": 3, "cut": {"startMs": 1200, "endMs": 2500}
    }"##;

    let sound: Sound = serde_json::from_str(saved).unwrap();
    let again: Sound = serde_json::from_str(&serde_json::to_string(&sound).unwrap()).unwrap();

    assert_eq!(sound.cut, Some(cut(1200, 2500)));
    assert_eq!(again, sound);
}
