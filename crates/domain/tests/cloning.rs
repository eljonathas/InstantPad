use instantpad_domain::cloning::{
    Challenge, CloneRefusal, CloneRequest, ClonedVoice, Consent, TERMS_VERSION, VoiceLibrary,
    Voiceprint,
};

fn voice(values: &[f32]) -> Voiceprint {
    Voiceprint::new(values.to_vec())
}

fn cloned(id: &str, terms_version: u32) -> ClonedVoice {
    ClonedVoice {
        id: id.to_owned(),
        name: format!("voice {id}"),
        voiceprint: voice(&[1.0, 0.0]),
        consent: Consent {
            owner_name: "Ana".to_owned(),
            terms_version,
            accepted_at: 0,
            similarity: 1.0,
            sample_sha256: "abc".to_owned(),
        },
    }
}

const WORDS: [[&str; 9]; 2] = [
    [
        "um", "dois", "três", "quatro", "cinco", "seis", "sete", "oito", "nove",
    ],
    [
        "one", "two", "three", "four", "five", "six", "seven", "eight", "nine",
    ],
];

fn digit_of(character: char) -> usize {
    character.to_digit(10).expect("a digit") as usize
}

/// A digit that is not `digit`.
fn other_than(digit: char) -> char {
    if digit == '1' { '2' } else { '1' }
}

fn spelled(challenge: &Challenge, language: usize, separator: &str) -> String {
    challenge
        .digits()
        .chars()
        .map(|digit| WORDS[language][digit_of(digit) - 1])
        .collect::<Vec<_>>()
        .join(separator)
}

fn spaced(challenge: &Challenge) -> String {
    challenge
        .digits()
        .chars()
        .map(String::from)
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn the_same_voice_is_fully_similar() {
    let print = voice(&[0.3, -0.4, 0.5]);

    assert!((print.similarity(&print) - 1.0).abs() < 1e-6);
}

#[test]
fn the_size_of_a_voiceprint_does_not_change_the_similarity() {
    let print = voice(&[1.0, 2.0, 3.0]);

    assert!((print.similarity(&voice(&[10.0, 20.0, 30.0])) - 1.0).abs() < 1e-6);
}

#[test]
fn unrelated_voices_have_no_similarity() {
    assert_eq!(voice(&[1.0, 0.0]).similarity(&voice(&[0.0, 1.0])), 0.0);
}

#[test]
fn opposite_voices_are_the_least_similar() {
    assert!((voice(&[1.0, 0.0]).similarity(&voice(&[-1.0, 0.0])) + 1.0).abs() < 1e-6);
}

#[test]
fn a_voiceprint_of_zeros_is_similar_to_nothing() {
    let silent = voice(&[0.0, 0.0]);

    assert_eq!(silent.similarity(&voice(&[1.0, 0.0])), 0.0);
    assert_eq!(silent.similarity(&silent), 0.0);
    assert!(!silent.is_same_speaker(&silent));
}

#[test]
fn two_recordings_of_one_speaker_pass_and_two_speakers_do_not() {
    let speaker = voice(&[1.0, 0.0]);

    let just_above = voice(&[0.81, (1.0f32 - 0.81 * 0.81).sqrt()]);
    let just_below = voice(&[0.79, (1.0f32 - 0.79 * 0.79).sqrt()]);

    assert!(speaker.is_same_speaker(&just_above));
    assert!(!speaker.is_same_speaker(&just_below));
}

#[test]
fn a_challenge_is_six_digits_from_one_to_nine() {
    for _ in 0..500 {
        let challenge = Challenge::new();

        assert_eq!(challenge.digits().len(), 6);
        assert!(
            challenge
                .digits()
                .chars()
                .all(|digit| ('1'..='9').contains(&digit))
        );
    }
}

#[test]
fn a_challenge_never_repeats_a_digit_in_a_row() {
    for _ in 0..500 {
        let challenge = Challenge::new();

        let digits: Vec<char> = challenge.digits().chars().collect();
        assert!(
            digits.windows(2).all(|pair| pair[0] != pair[1]),
            "{digits:?}"
        );
    }
}

#[test]
fn challenges_differ_from_one_draw_to_the_next() {
    let draws: std::collections::HashSet<_> = (0..50)
        .map(|_| Challenge::new().digits().to_owned())
        .collect();

    assert!(draws.len() > 40);
}

#[test]
fn the_digits_read_aloud_answer_the_challenge() {
    for _ in 0..50 {
        let challenge = Challenge::new();

        assert!(challenge.is_answered_by(challenge.digits()));
        assert!(challenge.is_answered_by(&spaced(&challenge)));
    }
}

#[test]
fn digits_spoken_as_words_in_portuguese_or_english_answer_the_challenge() {
    for _ in 0..50 {
        let challenge = Challenge::new();

        assert!(challenge.is_answered_by(&spelled(&challenge, 0, " ")));
        assert!(challenge.is_answered_by(&spelled(&challenge, 1, ", ")));
        assert!(challenge.is_answered_by(&spelled(&challenge, 1, " ").to_uppercase()));
    }
}

#[test]
fn punctuation_between_the_digits_does_not_matter() {
    let challenge = Challenge::new();
    let digits: Vec<char> = challenge.digits().chars().collect();

    let written = format!(
        "{}, {}-{}. {}; {}... {}!",
        digits[0], digits[1], digits[2], digits[3], digits[4], digits[5]
    );

    assert!(challenge.is_answered_by(&written));
}

#[test]
fn one_wrong_digit_is_forgiven() {
    let challenge = Challenge::new();
    let mut digits: Vec<char> = challenge.digits().chars().collect();
    digits[2] = other_than(digits[2]);

    assert!(challenge.is_answered_by(&digits.iter().collect::<String>()));
}

#[test]
fn one_missing_digit_is_forgiven() {
    let challenge = Challenge::new();

    assert!(challenge.is_answered_by(&challenge.digits()[1..]));
    assert!(challenge.is_answered_by(&challenge.digits()[..5]));
}

#[test]
fn two_wrong_digits_are_refused() {
    let challenge = Challenge::new();
    let mut digits: Vec<char> = challenge.digits().chars().collect();
    digits[0] = other_than(digits[0]);
    digits[4] = other_than(digits[4]);

    assert!(!challenge.is_answered_by(&digits.iter().collect::<String>()));
}

#[test]
fn two_missing_digits_are_refused() {
    let challenge = Challenge::new();

    assert!(!challenge.is_answered_by(&challenge.digits()[2..]));
}

#[test]
fn words_that_are_not_digits_answer_nothing() {
    let challenge = Challenge::new();

    assert!(!challenge.is_answered_by(""));
    assert!(!challenge.is_answered_by("hello there, this is my voice"));
}

fn request<'a>(
    sample: &'a Voiceprint,
    phrase: &'a Voiceprint,
    challenge: &'a Challenge,
    transcript: &'a str,
) -> CloneRequest<'a> {
    CloneRequest {
        name: "  My voice ",
        owner_name: " Ana ",
        sample,
        sample_sha256: "abc123",
        phrase,
        challenge,
        transcript,
        now: 1_700_000_000,
    }
}

#[test]
fn a_request_with_the_right_phrase_and_speaker_is_accepted() {
    let (sample, challenge) = (voice(&[1.0, 0.0]), Challenge::new());
    let transcript = spaced(&challenge);

    assert_eq!(
        request(&sample, &sample, &challenge, &transcript).verify(),
        Ok(())
    );
}

#[test]
fn a_wrong_phrase_is_refused() {
    let (sample, challenge) = (voice(&[1.0, 0.0]), Challenge::new());

    let refusal = request(&sample, &sample, &challenge, "").verify();

    assert_eq!(refusal, Err(CloneRefusal::WrongPhrase));
}

#[test]
fn another_speaker_is_refused() {
    let (sample, challenge) = (voice(&[1.0, 0.0]), Challenge::new());
    let phrase = voice(&[0.0, 1.0]);
    let transcript = spaced(&challenge);

    let refusal = request(&sample, &phrase, &challenge, &transcript).verify();

    assert_eq!(refusal, Err(CloneRefusal::NotTheSameSpeaker));
}

#[test]
fn the_phrase_is_checked_before_the_speaker() {
    let (sample, challenge) = (voice(&[1.0, 0.0]), Challenge::new());
    let phrase = voice(&[0.0, 1.0]);

    let refusal = request(&sample, &phrase, &challenge, "").verify();

    assert_eq!(refusal, Err(CloneRefusal::WrongPhrase));
}

#[test]
fn the_speaker_is_checked_before_the_names() {
    let (sample, challenge) = (voice(&[1.0, 0.0]), Challenge::new());
    let phrase = voice(&[0.0, 1.0]);
    let transcript = spaced(&challenge);
    let nameless = CloneRequest {
        name: "",
        owner_name: "",
        ..request(&sample, &phrase, &challenge, &transcript)
    };

    assert_eq!(nameless.clone_voice(), Err(CloneRefusal::NotTheSameSpeaker));
}

#[test]
fn a_voice_without_a_name_or_an_owner_is_refused() {
    let (sample, challenge) = (voice(&[1.0, 0.0]), Challenge::new());
    let transcript = spaced(&challenge);
    let base = request(&sample, &sample, &challenge, &transcript);

    let no_name = CloneRequest { name: "  ", ..base };
    assert_eq!(no_name.clone_voice(), Err(CloneRefusal::MissingName));

    let no_owner = CloneRequest {
        owner_name: "\t",
        ..request(&sample, &sample, &challenge, &transcript)
    };
    assert_eq!(no_owner.clone_voice(), Err(CloneRefusal::MissingName));
}

#[test]
fn a_cloned_voice_keeps_the_voiceprint_of_the_sample_and_trimmed_names() {
    let sample = voice(&[1.0, 0.0]);
    let challenge = Challenge::new();
    let transcript = spaced(&challenge);

    let cloned = request(&sample, &sample, &challenge, &transcript)
        .clone_voice()
        .unwrap();

    assert_eq!(cloned.name, "My voice");
    assert_eq!(cloned.voiceprint, sample);
    assert_eq!(cloned.consent.owner_name, "Ana");
}

#[test]
fn a_cloned_voice_records_the_consent() {
    let sample = voice(&[1.0, 0.0]);
    let phrase = voice(&[0.9, 0.1]);
    let challenge = Challenge::new();
    let transcript = spaced(&challenge);

    let cloned = request(&sample, &phrase, &challenge, &transcript)
        .clone_voice()
        .unwrap();

    assert_eq!(cloned.consent.terms_version, TERMS_VERSION);
    assert_eq!(cloned.consent.accepted_at, 1_700_000_000);
    assert_eq!(cloned.consent.sample_sha256, "abc123");
    assert_eq!(cloned.consent.similarity, sample.similarity(&phrase));
    assert!(cloned.is_usable());
}

#[test]
fn every_cloned_voice_gets_an_id_of_its_own() {
    let (sample, challenge) = (voice(&[1.0, 0.0]), Challenge::new());
    let transcript = spaced(&challenge);
    let request = request(&sample, &sample, &challenge, &transcript);

    let (first, second) = (
        request.clone_voice().unwrap(),
        request.clone_voice().unwrap(),
    );

    assert!(!first.id.is_empty());
    assert_ne!(first.id, second.id);
}

#[test]
fn a_voice_cloned_under_older_terms_cannot_be_used() {
    assert!(cloned("a", TERMS_VERSION).is_usable());
    assert!(!cloned("a", TERMS_VERSION - 1).is_usable());
}

#[test]
fn the_library_finds_a_voice_by_its_id() {
    let library = VoiceLibrary {
        own: None,
        clones: vec![cloned("a", TERMS_VERSION), cloned("b", TERMS_VERSION)],
    };

    assert_eq!(
        library.find("b").map(|voice| voice.name.as_str()),
        Some("voice b")
    );
    assert_eq!(library.find("c"), None);
}

#[test]
fn the_voices_that_can_speak_are_listed_in_the_order_they_were_cloned() {
    let library = VoiceLibrary {
        own: None,
        clones: vec![
            cloned("a", TERMS_VERSION),
            cloned("old", TERMS_VERSION - 1),
            cloned("b", TERMS_VERSION),
        ],
    };

    assert_eq!(library.usable_ids(), ["a", "b"]);
}

#[test]
fn removing_a_voice_says_whether_it_was_there() {
    let mut library = VoiceLibrary {
        own: Some(voice(&[1.0])),
        clones: vec![cloned("a", TERMS_VERSION), cloned("b", TERMS_VERSION)],
    };

    assert!(library.remove("a"));
    assert!(!library.remove("a"));

    assert_eq!(library.usable_ids(), ["b"]);
    assert!(library.own.is_some());
}
