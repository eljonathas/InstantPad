use instantpad_domain::cloning::{
    Challenge, CloneRefusal, CloneRequest, ClonedVoice, Consent, DEFAULT_BLEND, DEFAULT_TIMBRE,
    DesignRefusal, TERMS_VERSION, VoiceLibrary, VoiceRecipe, Voiceprint,
};
use instantpad_domain::voice::VoicePreset;

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
    assert!(cloned.has_current_consent());
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
fn a_voice_cloned_under_older_terms_has_no_current_consent() {
    assert!(cloned("a", TERMS_VERSION).has_current_consent());
    assert!(!cloned("a", TERMS_VERSION - 1).has_current_consent());
}

/// Two cloned voices that point in unrelated directions.
fn library() -> VoiceLibrary {
    VoiceLibrary {
        clones: vec![cloned("a", TERMS_VERSION), cloned_toward("b", [0.0, 1.0])],
        ..VoiceLibrary::default()
    }
}

fn cloned_toward(id: &str, values: [f32; 2]) -> ClonedVoice {
    ClonedVoice {
        voiceprint: voice(&values),
        ..cloned(id, TERMS_VERSION)
    }
}

/// Presets far from the cloned voices and from each other.
fn preset(preset: VoicePreset) -> Voiceprint {
    match preset {
        VoicePreset::Feminine => voice(&[10.0, 0.0]),
        VoicePreset::Masculine => voice(&[-10.0, 0.0]),
    }
}

fn recipe(base: &str) -> VoiceRecipe {
    VoiceRecipe {
        base_id: base.to_owned(),
        timbre: DEFAULT_TIMBRE,
        blend_id: None,
        blend: 0.0,
    }
}

fn blended(base: &str, with: &str, blend: f32) -> VoiceRecipe {
    VoiceRecipe {
        blend_id: Some(with.to_owned()),
        blend,
        ..recipe(base)
    }
}

fn assert_close(heard: &Voiceprint, expected: &[f32]) {
    assert_eq!(heard.values().len(), expected.len());
    for (heard, expected) in heard.values().iter().zip(expected) {
        assert!((heard - expected).abs() < 1e-5, "{heard} is not {expected}");
    }
}

/// Designs a voice and adds it to the library, returning its id.
fn design(library: &mut VoiceLibrary, name: &str, recipe: VoiceRecipe) -> String {
    let designed = library.design_voice(name, recipe).unwrap();
    let id = designed.id.clone();
    library.designs.push(designed);
    id
}

#[test]
fn moving_toward_a_voice_goes_that_part_of_the_way() {
    let (from, to) = (voice(&[1.0, 0.0]), voice(&[0.0, 1.0]));

    assert_close(&from.toward(&to, 0.0), &[1.0, 0.0]);
    assert_close(&from.toward(&to, 0.25), &[0.75, 0.25]);
    assert_close(&from.toward(&to, 1.0), &[0.0, 1.0]);
}

#[test]
fn moving_toward_a_voice_never_goes_past_either_end() {
    let (from, to) = (voice(&[1.0, 0.0]), voice(&[0.0, 1.0]));

    assert_close(&from.toward(&to, 3.0), &[0.0, 1.0]);
    assert_close(&from.toward(&to, -3.0), &[1.0, 0.0]);
}

#[test]
fn a_designed_voice_without_changes_sounds_like_its_base() {
    let library = library();

    let heard = library.design_voiceprint(&recipe("a"), preset).unwrap();

    assert_close(&heard, &[1.0, 0.0]);
}

#[test]
fn a_blend_moves_the_voice_toward_the_other_one() {
    let library = library();

    let heard = library
        .design_voiceprint(&blended("a", "b", 0.25), preset)
        .unwrap();

    assert_close(&heard, &[0.75, 0.25]);
}

#[test]
fn an_amount_of_blend_without_a_voice_to_blend_changes_nothing() {
    let library = library();
    let lonely = VoiceRecipe {
        blend: 0.9,
        ..recipe("a")
    };

    let heard = library.design_voiceprint(&lonely, preset).unwrap();

    assert_close(&heard, &[1.0, 0.0]);
}

#[test]
fn a_higher_timbre_moves_the_voice_toward_the_feminine_preset() {
    let library = library();
    let high = VoiceRecipe {
        timbre: 0.75,
        ..recipe("a")
    };

    let heard = library.design_voiceprint(&high, preset).unwrap();

    // Half of the range is 0.3 of the way to the preset.
    assert_close(&heard, &[0.7 + 3.0, 0.0]);
}

#[test]
fn a_deeper_timbre_moves_the_voice_toward_the_masculine_preset() {
    let library = library();
    let deep = VoiceRecipe {
        timbre: 0.25,
        ..recipe("a")
    };

    let heard = library.design_voiceprint(&deep, preset).unwrap();

    assert_close(&heard, &[0.7 - 3.0, 0.0]);
}

#[test]
fn the_timbre_comes_after_the_blend() {
    let library = library();
    let both = VoiceRecipe {
        timbre: 1.0,
        ..blended("a", "b", 0.5)
    };

    let heard = library.design_voiceprint(&both, preset).unwrap();

    // The middle of the two voices, 0.6 of the way to the feminine preset.
    assert_close(&heard, &[0.5 + (10.0 - 0.5) * 0.6, 0.5 - 0.5 * 0.6]);
}

#[test]
fn a_recipe_with_a_voice_that_is_gone_sounds_like_nobody() {
    let library = library();

    assert_eq!(library.design_voiceprint(&recipe("gone"), preset), None);
    assert_eq!(
        library.design_voiceprint(&blended("a", "gone", 0.5), preset),
        None
    );
}

#[test]
fn the_library_tells_who_a_cloned_or_a_designed_voice_sounds_like() {
    let mut library = library();
    let id = design(&mut library, "Mix", blended("a", "b", 0.5));

    let cloned = library.voiceprint("b", preset).unwrap();
    let designed = library.voiceprint(&id, preset).unwrap();

    assert_close(&cloned, &[0.0, 1.0]);
    assert_close(&designed, &[0.5, 0.5]);
    assert_eq!(library.voiceprint("gone", preset), None);
}

#[test]
fn a_voice_is_found_by_name_whether_it_is_cloned_or_designed() {
    let mut library = library();
    let id = design(&mut library, "Mix", recipe("a"));

    assert_eq!(library.name("a"), Some("voice a"));
    assert_eq!(library.name(&id), Some("Mix"));
    assert_eq!(library.name("gone"), None);
}

#[test]
fn the_cloned_voices_are_listed_before_the_designed_ones() {
    let mut library = library();
    let first = design(&mut library, "One", recipe("a"));
    let second = design(&mut library, "Two", recipe("b"));

    assert_eq!(library.ids(), ["a", "b", &first, &second]);
}

#[test]
fn a_designed_voice_is_named_and_gets_an_id_of_its_own() {
    let library = library();

    let first = library.design_voice("  Mix ", recipe("a")).unwrap();
    let second = library.design_voice("Mix", recipe("a")).unwrap();

    assert_eq!(first.name, "Mix");
    assert_eq!(first.recipe, recipe("a"));
    assert!(!first.id.is_empty());
    assert_ne!(first.id, second.id);
}

#[test]
fn the_default_timbre_leaves_the_voice_as_it_is() {
    assert_eq!(DEFAULT_TIMBRE, 0.5);
    assert!((0.0..=1.0).contains(&DEFAULT_BLEND));
    let library = library();

    let heard = library.design_voiceprint(&recipe("a"), preset).unwrap();

    assert_close(&heard, &[1.0, 0.0]);
}

#[test]
fn the_ends_of_the_timbre_reach_the_same_distance_from_the_voice() {
    let library = library();
    let (deep, high) = (
        VoiceRecipe {
            timbre: 0.0,
            ..recipe("a")
        },
        VoiceRecipe {
            timbre: 1.0,
            ..recipe("a")
        },
    );

    let deep = library.design_voiceprint(&deep, preset).unwrap();
    let high = library.design_voiceprint(&high, preset).unwrap();

    assert_close(&deep, &[0.4 - 6.0, 0.0]);
    assert_close(&high, &[0.4 + 6.0, 0.0]);
}

#[test]
fn blending_a_voice_with_itself_blends_nothing() {
    let library = library();

    let designed = library.design_voice("Mix", blended("a", "a", 0.5)).unwrap();

    assert_eq!(designed.recipe.blend_id, None);
    assert_eq!(designed.recipe.base_id, "a");
}

#[test]
fn a_designed_voice_without_a_name_is_refused() {
    assert_eq!(
        library().design_voice("   ", recipe("a")),
        Err(DesignRefusal::MissingName)
    );
}

#[test]
fn a_designed_voice_from_a_voice_that_is_not_there_is_refused() {
    let library = library();

    assert_eq!(
        library.design_voice("Mix", recipe("gone")),
        Err(DesignRefusal::UnknownVoice)
    );
    assert_eq!(
        library.design_voice("Mix", blended("a", "gone", 0.5)),
        Err(DesignRefusal::UnknownVoice)
    );
}

#[test]
fn the_name_is_checked_before_the_voices() {
    assert_eq!(
        library().design_voice("", recipe("gone")),
        Err(DesignRefusal::MissingName)
    );
}

#[test]
fn a_designed_voice_keeps_its_settings_inside_their_range() {
    let wild = VoiceRecipe {
        timbre: 5.0,
        blend: -2.0,
        ..blended("a", "b", 0.0)
    };
    let library = library();

    let low = library.design_voice("Mix", wild.clone()).unwrap().recipe;
    let high = library
        .design_voice(
            "Mix",
            VoiceRecipe {
                timbre: -5.0,
                blend: 2.0,
                ..wild
            },
        )
        .unwrap()
        .recipe;

    assert_eq!((low.timbre, low.blend), (1.0, 0.0));
    assert_eq!((high.timbre, high.blend), (0.0, 1.0));
}

#[test]
fn voices_cloned_under_older_terms_are_dropped() {
    let mut library = VoiceLibrary {
        own: Some(voice(&[1.0])),
        clones: vec![
            cloned("a", TERMS_VERSION),
            cloned("old", TERMS_VERSION - 1),
            cloned("b", TERMS_VERSION),
        ],
        ..VoiceLibrary::default()
    };

    assert!(library.drop_outdated());

    assert_eq!(library.ids(), ["a", "b"]);
    assert!(library.own.is_some());
    assert!(!library.drop_outdated());
}

#[test]
fn dropping_the_outdated_voices_drops_the_designs_made_from_them() {
    let mut library = VoiceLibrary {
        clones: vec![
            cloned("a", TERMS_VERSION),
            cloned("old", TERMS_VERSION - 1),
            cloned("b", TERMS_VERSION),
        ],
        ..VoiceLibrary::default()
    };
    let from_old = design(&mut library, "From old", recipe("old"));
    let blends_old = design(&mut library, "Blends old", blended("a", "old", 0.5));
    let kept = design(&mut library, "Kept", blended("a", "b", 0.5));

    assert!(library.drop_outdated());

    assert_eq!(library.ids(), ["a", "b", kept.as_str()]);
    assert_eq!(library.name(&from_old), None);
    assert_eq!(library.name(&blends_old), None);
}

#[test]
fn removing_a_voice_says_which_voices_went() {
    let mut library = VoiceLibrary {
        own: Some(voice(&[1.0])),
        clones: vec![cloned("a", TERMS_VERSION), cloned("b", TERMS_VERSION)],
        ..VoiceLibrary::default()
    };

    assert_eq!(library.remove("a"), ["a"]);
    assert!(library.remove("a").is_empty());

    assert_eq!(library.ids(), ["b"]);
    assert!(library.own.is_some());
}

#[test]
fn removing_a_cloned_voice_removes_the_designs_that_start_from_it() {
    let mut library = library();
    let from_a = design(&mut library, "From a", recipe("a"));
    let from_b = design(&mut library, "From b", recipe("b"));

    let removed = library.remove("a");

    assert_eq!(removed, ["a", from_a.as_str()]);
    assert_eq!(library.ids(), ["b", from_b.as_str()]);
}

#[test]
fn removing_a_cloned_voice_removes_the_designs_that_blend_it_in() {
    let mut library = library();
    let mixes_b = design(&mut library, "Mixes b", blended("a", "b", 0.5));
    let plain_a = design(&mut library, "Plain a", recipe("a"));

    let removed = library.remove("b");

    assert_eq!(removed, ["b", mixes_b.as_str()]);
    assert_eq!(library.ids(), ["a", plain_a.as_str()]);
}

#[test]
fn removing_a_designed_voice_keeps_the_cloned_voices_it_is_made_of() {
    let mut library = library();
    let mix = design(&mut library, "Mix", blended("a", "b", 0.5));

    let removed = library.remove(&mix);

    assert_eq!(removed, [mix]);
    assert_eq!(library.ids(), ["a", "b"]);
}
