use instantpad_domain::cloning::{
    CloneRefusal, ClonedVoice, DEFAULT_BLEND, DEFAULT_TIMBRE, DesignRefusal, VoiceLibrary,
    VoiceRecipe, Voiceprint,
};
use instantpad_domain::voice::VoicePreset;

fn voice(values: &[f32]) -> Voiceprint {
    Voiceprint::new(values.to_vec())
}

fn cloned(id: &str) -> ClonedVoice {
    ClonedVoice {
        id: id.to_owned(),
        name: format!("voice {id}"),
        voiceprint: voice(&[1.0, 0.0]),
    }
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
}

#[test]
fn a_cloned_voice_keeps_the_voiceprint_of_the_sample_and_a_trimmed_name() {
    let sample = voice(&[1.0, 0.0]);

    let cloned = ClonedVoice::clone_from("  My voice ", sample.clone()).unwrap();

    assert_eq!(cloned.name, "My voice");
    assert_eq!(cloned.voiceprint, sample);
}

#[test]
fn a_voice_without_a_name_is_refused() {
    for name in ["", "  ", "\t"] {
        let refusal = ClonedVoice::clone_from(name, voice(&[1.0, 0.0]));

        assert_eq!(refusal, Err(CloneRefusal::MissingName));
    }
}

#[test]
fn every_cloned_voice_gets_an_id_of_its_own() {
    let sample = voice(&[1.0, 0.0]);

    let first = ClonedVoice::clone_from("A", sample.clone()).unwrap();
    let second = ClonedVoice::clone_from("A", sample).unwrap();

    assert!(!first.id.is_empty());
    assert_ne!(first.id, second.id);
}

/// Two cloned voices that point in unrelated directions.
fn library() -> VoiceLibrary {
    VoiceLibrary {
        clones: vec![cloned("a"), cloned_toward("b", [0.0, 1.0])],
        ..VoiceLibrary::default()
    }
}

fn cloned_toward(id: &str, values: [f32; 2]) -> ClonedVoice {
    ClonedVoice {
        voiceprint: voice(&values),
        ..cloned(id)
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
fn removing_a_voice_says_which_voices_went() {
    let mut library = VoiceLibrary {
        own: Some(voice(&[1.0])),
        clones: vec![cloned("a"), cloned("b")],
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

#[test]
fn a_voice_saved_with_a_consent_record_by_an_earlier_version_still_loads() {
    let saved = r#"{
        "id": "a",
        "name": "Ana",
        "voiceprint": [0.25, 0.75],
        "consent": {
            "ownerName": "Ana Souza",
            "termsVersion": 1,
            "acceptedAt": 1700000000,
            "similarity": 0.875,
            "sampleSha256": "abc123"
        }
    }"#;

    let voice: ClonedVoice = serde_json::from_str(saved).unwrap();

    assert_eq!(voice.id, "a");
    assert_eq!(voice.name, "Ana");
    assert_eq!(voice.voiceprint, Voiceprint::new(vec![0.25, 0.75]));
}
