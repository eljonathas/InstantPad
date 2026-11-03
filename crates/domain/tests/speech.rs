use instantpad_domain::speech::voiced;

const RATE: usize = 48_000;
const FRAME: usize = RATE / 50;

/// Something that varies like a voice does, `frames` frames long, at a given loudness.
fn talk(frames: usize, loudness: f32) -> Vec<f32> {
    (0..frames * FRAME)
        .map(|n| loudness * (n as f32 * 0.07).sin() * (1.0 + (n as f32 * 0.0005).sin()) / 2.0)
        .collect()
}

/// Someone who talks without a single pause: the syllables swing between loud and quiet, as
/// they do, but never fall silent.
fn nonstop(frames: usize, loudness: f32) -> Vec<f32> {
    (0..frames * FRAME)
        .map(|n| {
            let syllable = 0.15 + 0.85 * (0.5 + 0.5 * (n as f32 * 0.0025).sin());
            loudness * (n as f32 * 0.07).sin() * syllable
        })
        .collect()
}

/// A steady hum, which never changes.
fn hum(frames: usize, loudness: f32) -> Vec<f32> {
    (0..frames * FRAME)
        .map(|n| loudness * (n as f32 * 0.3).sin())
        .collect()
}

fn quiet(frames: usize) -> Vec<f32> {
    vec![0.0; frames * FRAME]
}

#[test]
fn a_recording_of_silence_has_no_voice() {
    assert!(voiced(&quiet(200), RATE).is_empty());
}

#[test]
fn a_recording_shorter_than_a_frame_has_no_voice() {
    assert!(voiced(&[0.5; 100], RATE).is_empty());
    assert!(voiced(&[], RATE).is_empty());
}

#[test]
fn speech_between_silences_is_kept_and_the_silences_are_not() {
    let speech = [quiet(50), talk(100, 0.2), quiet(50)].concat();

    let heard = voiced(&speech, RATE);

    assert_eq!(heard.len(), 100 * FRAME);
    assert!(heard.iter().any(|sample| sample.abs() > 0.05));
}

#[test]
fn pauses_between_words_are_left_out() {
    let speech = [
        quiet(30),
        talk(40, 0.2),
        quiet(30),
        talk(60, 0.2),
        quiet(30),
    ]
    .concat();

    assert_eq!(voiced(&speech, RATE).len(), 100 * FRAME);
}

#[test]
fn a_steady_noise_is_not_a_voice() {
    assert!(voiced(&hum(300, 0.05), RATE).is_empty());
}

#[test]
fn a_voice_over_a_steady_noise_is_kept_without_the_noise() {
    let noise = hum(30, 0.01);
    let speech = [noise.clone(), talk(100, 0.3), noise.clone(), noise].concat();

    let heard = voiced(&speech, RATE);

    assert!(heard.len() >= 95 * FRAME, "{}", heard.len());
    assert!(heard.len() <= 105 * FRAME, "{}", heard.len());
}

#[test]
fn a_click_shorter_than_a_word_is_not_a_voice() {
    let speech = [quiet(50), talk(2, 0.5), quiet(50)].concat();

    assert!(voiced(&speech, RATE).is_empty());
}

#[test]
fn a_quiet_microphone_is_still_heard_when_it_is_louder_than_its_room() {
    let speech = [quiet(50), talk(100, 0.02), quiet(50)].concat();

    // The first frame of a word may still be too faint to count.
    let heard = voiced(&speech, RATE).len();
    assert!((95 * FRAME..=100 * FRAME).contains(&heard), "{heard}");
}

#[test]
fn a_voice_too_faint_to_tell_from_silence_is_not_kept() {
    let speech = [quiet(50), talk(100, 0.002), quiet(50)].concat();

    assert!(voiced(&speech, RATE).is_empty());
}

#[test]
fn someone_who_never_stops_talking_is_kept_whole() {
    let speech = talk(200, 0.2);

    assert!(voiced(&speech, RATE).len() >= 190 * FRAME);
}

#[test]
fn a_rate_too_low_for_a_frame_does_not_panic() {
    let speech = vec![0.5; 100];

    for rate in [0, 1, 49] {
        let _ = voiced(&speech, rate);
    }
}

#[test]
fn someone_who_talks_from_the_first_frame_with_no_pause_is_heard() {
    let speech = nonstop(500, 0.1);

    let heard = voiced(&speech, RATE).len();

    assert!(heard >= 250 * FRAME, "{heard}");
}

#[test]
fn a_short_silence_before_nonstop_talk_is_not_needed_to_hear_it() {
    let speech = [quiet(50), nonstop(500, 0.1)].concat();

    let heard = voiced(&speech, RATE).len();

    assert!(heard >= 250 * FRAME, "{heard}");
    assert!(heard <= 500 * FRAME, "{heard}");
}

#[test]
fn a_quiet_talker_who_never_pauses_is_still_heard() {
    let speech = nonstop(500, 0.012);

    let heard = voiced(&speech, RATE).len();

    assert!(heard >= 150 * FRAME, "{heard}");
}

#[test]
fn a_loud_steady_hum_is_not_a_voice_either() {
    assert!(voiced(&hum(300, 0.2), RATE).is_empty());
}

#[test]
fn a_voice_over_a_loud_steady_noise_is_kept_and_the_noise_is_not() {
    let noise = hum(50, 0.02);
    let speech = [noise.clone(), nonstop(200, 0.15), noise].concat();

    let heard = voiced(&speech, RATE).len();

    assert!(heard >= 100 * FRAME, "{heard}");
    assert!(heard <= 200 * FRAME, "{heard}");
}

#[test]
fn a_few_words_in_a_long_silence_still_count() {
    let speech = [talk(100, 0.2), quiet(1200)].concat();

    let heard = voiced(&speech, RATE).len();

    assert!(heard >= 90 * FRAME, "{heard}");
    assert!(heard <= 100 * FRAME, "{heard}");
}
