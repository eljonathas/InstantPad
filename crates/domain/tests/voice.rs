use instantpad_domain::cloning::{TrainedModel, Voiceprint};
use instantpad_domain::voice::{
    Conversion, PitchShape, VoiceChain, VoiceColor, VoiceEffect, VoiceEngine, VoiceParam,
    VoicePreset, VoiceSettings, VoiceTarget,
};

fn with(effect: VoiceEffect) -> VoiceSettings {
    let mut settings = VoiceSettings::default();
    settings.select(effect);
    settings
}

fn chain_of(settings: &VoiceSettings) -> VoiceChain {
    settings.chain().expect("an effect has a chain")
}

fn semitones_of(chain: VoiceChain) -> f32 {
    match chain.pitch {
        PitchShape::Fixed { semitones } => semitones,
        PitchShape::Cycle { .. } => panic!("expected a fixed pitch"),
    }
}

fn fixed_semitones(settings: &VoiceSettings) -> f32 {
    semitones_of(chain_of(settings))
}

/// What `select_next` lands on, one step at a time.
fn walk(settings: &mut VoiceSettings, clones: &[&str], steps: usize) -> Vec<(VoiceEffect, String)> {
    (0..steps)
        .map(|_| {
            settings.select_next(clones);
            (
                settings.effect,
                settings.clone_id.clone().unwrap_or_default(),
            )
        })
        .collect()
}

#[test]
fn the_natural_voice_is_left_untouched() {
    assert_eq!(VoiceSettings::default().chain(), None);
}

#[test]
fn a_thin_voice_goes_up_and_a_deep_one_goes_down() {
    assert!(fixed_semitones(&with(VoiceEffect::Thin)) > 0.0);
    assert!(fixed_semitones(&with(VoiceEffect::Deep)) < 0.0);
}

#[test]
fn more_intensity_moves_the_pitch_further() {
    let mut settings = with(VoiceEffect::Deep);
    settings.set(VoiceParam::DeepIntensity, 0.0);
    let soft = fixed_semitones(&settings);

    settings.set(VoiceParam::DeepIntensity, 1.0);

    assert!(fixed_semitones(&settings) < soft);
}

#[test]
fn the_pitch_cycle_follows_its_three_settings() {
    let mut settings = with(VoiceEffect::PitchCycle);
    settings.set(VoiceParam::CycleTime, 1.0);
    settings.set(VoiceParam::CycleHigh, 0.5);
    settings.set(VoiceParam::CycleLow, 1.0);

    assert_eq!(
        chain_of(&settings).pitch,
        PitchShape::Cycle {
            low: -12.0,
            high: 6.0,
            seconds: 6.0
        }
    );
}

#[test]
fn a_value_past_the_knob_is_limited() {
    let mut settings = VoiceSettings::default();

    settings.set(VoiceParam::ThinIntensity, 7.0);

    assert_eq!(settings.get(VoiceParam::ThinIntensity), 1.0);
}

#[test]
fn the_plain_effects_add_no_character() {
    for effect in [
        VoiceEffect::Thin,
        VoiceEffect::Deep,
        VoiceEffect::PitchCycle,
    ] {
        let color = chain_of(&with(effect)).color;

        assert_eq!(color, Default::default(), "{effect:?}");
    }
}

#[test]
fn a_feminine_voice_goes_up_with_brighter_highs() {
    let mut settings = with(VoiceEffect::Feminine);
    settings.set(VoiceParam::FeminineIntensity, 0.0);
    settings.set(VoiceParam::FeminineFormant, 1.0);

    let chain = chain_of(&settings);

    assert_eq!(semitones_of(chain), 3.0);
    assert_eq!(chain.formant, 4.0);
    assert!(chain.color.brightness > 0.0);
    assert_eq!(chain.color.whisper, 0.0);
    assert_eq!(chain.color.growl, 0.0);
}

#[test]
fn more_feminine_intensity_raises_the_pitch() {
    let mut settings = with(VoiceEffect::Feminine);
    settings.set(VoiceParam::FeminineIntensity, 0.0);
    let low = fixed_semitones(&settings);

    settings.set(VoiceParam::FeminineIntensity, 1.0);

    assert_eq!(low, 3.0);
    assert_eq!(fixed_semitones(&settings), 7.0);
}

#[test]
fn the_ghost_whispers_and_echoes_as_much_as_asked() {
    let mut settings = with(VoiceEffect::Ghost);
    settings.set(VoiceParam::GhostWhisper, 0.25);
    settings.set(VoiceParam::GhostEcho, 0.75);

    let color = chain_of(&settings).color;

    assert_eq!(color.whisper, 0.25);
    assert_eq!(color.echo, 0.75);
    assert!(color.tremolo > 0.0);
    assert_eq!(color.growl, 0.0);
}

#[test]
fn the_ghost_pitch_goes_both_ways_and_its_formant_stays() {
    let mut settings = with(VoiceEffect::Ghost);

    settings.set(VoiceParam::GhostPitch, 0.0);
    assert_eq!(fixed_semitones(&settings), -4.0);

    settings.set(VoiceParam::GhostPitch, 0.5);
    assert_eq!(fixed_semitones(&settings), 0.0);

    settings.set(VoiceParam::GhostPitch, 1.0);
    assert_eq!(fixed_semitones(&settings), 4.0);
    assert_eq!(chain_of(&settings).formant, 0.0);
}

#[test]
fn an_ogre_goes_down_and_growls_as_much_as_asked() {
    let mut settings = with(VoiceEffect::Ogre);
    settings.set(VoiceParam::OgreGrowl, 0.8);

    let chain = chain_of(&settings);

    assert!(semitones_of(chain) < 0.0);
    assert!(chain.formant < 0.0);
    assert_eq!(chain.color.growl, 0.8);
    assert_eq!(chain.color.whisper, 0.0);
    assert_eq!(chain.color.brightness, 0.0);
}

#[test]
fn more_ogre_depth_moves_the_pitch_further_down() {
    let mut settings = with(VoiceEffect::Ogre);
    settings.set(VoiceParam::OgreDepth, 0.0);
    let soft = fixed_semitones(&settings);

    settings.set(VoiceParam::OgreDepth, 1.0);

    assert_eq!(soft, -4.0);
    assert_eq!(fixed_semitones(&settings), -10.0);
}

#[test]
fn the_ai_effects_give_a_pitch_for_the_engine_to_apply_first() {
    let mut settings = with(VoiceEffect::FeminineAi);
    settings.set(VoiceParam::FeminineAiPitch, 1.0);
    assert_eq!(fixed_semitones(&settings), 8.0);

    settings.select_clone("a");
    settings.set(VoiceParam::ClonePitch, 0.0);
    assert_eq!(fixed_semitones(&settings), -8.0);
}

#[test]
fn only_the_ai_effects_have_a_target() {
    for effect in VoiceEffect::BUILT_IN {
        let target = with(effect).target();

        if effect == VoiceEffect::FeminineAi {
            assert_eq!(target, Some(VoiceTarget::Preset(VoicePreset::Feminine)));
        } else {
            assert_eq!(target, None, "{effect:?}");
        }
    }
}

#[test]
fn a_cloned_voice_targets_its_clone() {
    let mut settings = VoiceSettings::default();

    settings.select_clone("a");

    assert_eq!(settings.target(), Some(VoiceTarget::Own("a".to_owned())));
}

#[test]
fn only_the_ai_effects_need_the_ai() {
    let needing: Vec<_> = VoiceEffect::BUILT_IN
        .into_iter()
        .chain([VoiceEffect::Cloned])
        .filter(|effect| effect.needs_ai())
        .collect();

    assert_eq!(needing, [VoiceEffect::FeminineAi, VoiceEffect::Cloned]);
}

#[test]
fn a_cloned_effect_cannot_be_selected_without_a_clone() {
    let mut settings = with(VoiceEffect::Deep);

    settings.select(VoiceEffect::Cloned);

    assert_eq!(settings.effect, VoiceEffect::Deep);
    assert_eq!(settings.last_effect, VoiceEffect::Deep);
    assert_eq!(settings.target(), None);
}

#[test]
fn selecting_a_clone_turns_it_on_and_remembers_it() {
    let mut settings = VoiceSettings::default();

    settings.select_clone("a");

    assert_eq!(settings.effect, VoiceEffect::Cloned);
    assert_eq!(settings.last_effect, VoiceEffect::Cloned);
    assert_eq!(settings.clone_id.as_deref(), Some("a"));
}

#[test]
fn the_clone_is_kept_while_another_effect_is_on() {
    let mut settings = VoiceSettings::default();
    settings.select_clone("a");

    settings.select(VoiceEffect::Thin);

    assert_eq!(settings.clone_id.as_deref(), Some("a"));
    settings.select(VoiceEffect::Cloned);
    assert_eq!(settings.target(), Some(VoiceTarget::Own("a".to_owned())));
}

#[test]
fn switching_off_and_on_returns_to_the_same_clone() {
    let mut settings = VoiceSettings::default();
    settings.select_clone("a");

    settings.toggle();
    assert_eq!(settings.effect, VoiceEffect::Natural);

    settings.toggle();
    assert_eq!(settings.effect, VoiceEffect::Cloned);
    assert_eq!(settings.clone_id.as_deref(), Some("a"));
}

#[test]
fn forgetting_the_clone_in_use_goes_back_to_the_natural_voice() {
    let mut settings = VoiceSettings::default();
    settings.select_clone("a");

    settings.forget_clone("a");

    assert_eq!(settings.effect, VoiceEffect::Natural);
    assert_eq!(settings.clone_id, None);
    assert_ne!(settings.last_effect, VoiceEffect::Cloned);
    settings.toggle();
    assert_ne!(settings.effect, VoiceEffect::Cloned);
}

#[test]
fn forgetting_a_clone_not_in_use_keeps_the_effect() {
    let mut settings = VoiceSettings::default();
    settings.select_clone("a");
    settings.select(VoiceEffect::Ogre);

    settings.forget_clone("a");

    assert_eq!(settings.effect, VoiceEffect::Ogre);
    assert_eq!(settings.clone_id, None);
}

#[test]
fn forgetting_another_clone_changes_nothing() {
    let mut settings = VoiceSettings::default();
    settings.select_clone("a");
    let before = settings.clone();

    settings.forget_clone("b");

    assert_eq!(settings, before);
}

#[test]
fn switching_off_and_on_returns_to_the_last_effect() {
    let mut settings = with(VoiceEffect::Deep);

    settings.toggle();
    assert_eq!(settings.effect, VoiceEffect::Natural);

    settings.toggle();
    assert_eq!(settings.effect, VoiceEffect::Deep);
}

#[test]
fn the_next_effect_wraps_around_and_skips_the_natural_voice() {
    let mut settings = VoiceSettings::default();

    settings.select_next(&[]);
    assert_eq!(settings.effect, VoiceEffect::Thin);

    settings.select(VoiceEffect::FeminineAi);
    settings.select_next(&[]);
    assert_eq!(settings.effect, VoiceEffect::Thin);
}

#[test]
fn the_next_voice_walks_the_built_in_effects_and_then_the_clones() {
    let mut settings = VoiceSettings::default();
    let clones = ["a", "b"];

    let walked = walk(&mut settings, &clones, 10);

    let expected: Vec<_> = [
        VoiceEffect::Thin,
        VoiceEffect::Deep,
        VoiceEffect::PitchCycle,
        VoiceEffect::Feminine,
        VoiceEffect::Ghost,
        VoiceEffect::Ogre,
        VoiceEffect::FeminineAi,
    ]
    .into_iter()
    .map(|effect| (effect, String::new()))
    .chain([
        (VoiceEffect::Cloned, "a".to_owned()),
        (VoiceEffect::Cloned, "b".to_owned()),
        (VoiceEffect::Thin, "b".to_owned()),
    ])
    .collect();
    assert_eq!(walked, expected);
}

#[test]
fn the_next_voice_never_lands_on_the_natural_voice() {
    let mut settings = VoiceSettings::default();

    let walked = walk(&mut settings, &["a"], 20);

    assert!(
        walked
            .iter()
            .all(|(effect, _)| *effect != VoiceEffect::Natural)
    );
}

#[test]
fn the_next_voice_after_a_clone_that_is_gone_starts_over() {
    let mut settings = VoiceSettings::default();
    settings.select_clone("gone");

    settings.select_next(&["a"]);

    assert_eq!(settings.effect, VoiceEffect::Thin);
}

#[test]
fn the_next_voice_moves_between_two_clones() {
    let mut settings = VoiceSettings::default();
    settings.select_clone("a");

    settings.select_next(&["a", "b"]);

    assert_eq!(settings.effect, VoiceEffect::Cloned);
    assert_eq!(settings.clone_id.as_deref(), Some("b"));
}

#[test]
fn values_read_in_the_units_of_the_effect() {
    let mut settings = VoiceSettings::default();
    settings.set(VoiceParam::CycleTime, 1.0);
    settings.set(VoiceParam::CycleHigh, 0.5);
    settings.set(VoiceParam::CycleLow, 1.0);

    assert_eq!(settings.describe(VoiceParam::CycleTime), "6.0 s");
    assert_eq!(settings.describe(VoiceParam::CycleHigh), "+6.0 st");
    assert_eq!(settings.describe(VoiceParam::CycleLow), "-12.0 st");
}

#[test]
fn the_knobs_of_the_new_effects_read_in_their_own_units() {
    let settings = VoiceSettings::default();
    let read = |param| settings.describe(param);

    assert_eq!(read(VoiceParam::FeminineIntensity), "+5.0 st");
    assert_eq!(read(VoiceParam::FeminineFormant), "+2.5 st");
    assert_eq!(read(VoiceParam::GhostWhisper), "60%");
    assert_eq!(read(VoiceParam::GhostEcho), "60%");
    assert_eq!(read(VoiceParam::GhostPitch), "+0.0 st");
    assert_eq!(read(VoiceParam::OgreDepth), "-7.0 st");
    assert_eq!(read(VoiceParam::OgreGrowl), "50%");
    assert_eq!(read(VoiceParam::OgreFormant), "-4.5 st");
    assert_eq!(read(VoiceParam::FeminineAiPitch), "+4.0 st");
    assert_eq!(read(VoiceParam::ClonePitch), "+0.0 st");
}

#[test]
fn the_voice_of_the_user_goes_through_a_pitch_alone_whatever_the_effect() {
    let mut settings = with(VoiceEffect::Ogre);
    settings.set(VoiceParam::ClonePitch, 1.0);

    let chain = settings.own_voice_chain();

    assert_eq!(chain.pitch, PitchShape::Fixed { semitones: 8.0 });
    assert_eq!(chain.formant, 0.0);
    assert_eq!(chain.color, Default::default());
    settings.select_clone("a");
    assert_eq!(settings.chain(), Some(chain));
}

fn model(id: &str) -> TrainedModel {
    TrainedModel {
        id: id.to_owned(),
        name: "feminine".to_owned(),
        credit: String::new(),
        pitch_hz: 210.0,
    }
}

#[test]
fn a_voice_with_a_trained_model_speaks_through_it_until_the_user_picks_otherwise() {
    let mut voice = VoiceSettings::default();
    let feminine = VoiceTarget::Preset(VoicePreset::Feminine);

    assert_eq!(voice.engine(&feminine, false), VoiceEngine::Instant);
    assert_eq!(voice.engine(&feminine, true), VoiceEngine::Trained);

    voice.set_engine(&feminine, VoiceEngine::Instant);

    assert_eq!(voice.engine(&feminine, true), VoiceEngine::Instant);
    let ana = VoiceTarget::Own("a".to_owned());
    assert_eq!(voice.engine(&ana, true), VoiceEngine::Trained);
}

#[test]
fn a_voice_without_a_model_always_uses_the_instant_engine() {
    let mut voice = VoiceSettings::default();
    let ana = VoiceTarget::Own("a".to_owned());
    voice.set_engine(&ana, VoiceEngine::Trained);

    assert_eq!(voice.engine(&ana, false), VoiceEngine::Instant);
}

#[test]
fn forgetting_a_cloned_voice_forgets_its_engine() {
    let mut voice = VoiceSettings::default();
    let ana = VoiceTarget::Own("a".to_owned());
    voice.set_engine(&ana, VoiceEngine::Instant);

    voice.forget_clone("a");

    assert_eq!(voice.engine(&ana, true), VoiceEngine::Trained);
}

#[test]
fn the_pitch_of_a_trained_voice_goes_an_octave_either_way_from_its_own() {
    let mut voice = VoiceSettings::default();
    assert_eq!(voice.trained_semitones(), 0.0);
    assert_eq!(voice.describe(VoiceParam::TrainedPitch), "+0.0 st");

    voice.set(VoiceParam::TrainedPitch, 0.0);
    assert_eq!(voice.trained_semitones(), -12.0);
    voice.set(VoiceParam::TrainedPitch, 1.0);
    assert_eq!(voice.trained_semitones(), 12.0);
}

#[test]
fn a_trained_voice_differs_from_itself_only_in_pitch() {
    let low = Conversion::Trained {
        model: model("m"),
        semitones: -2.0,
    };
    let high = Conversion::Trained {
        model: model("m"),
        semitones: 5.0,
    };
    let other = Conversion::Trained {
        model: model("n"),
        semitones: 5.0,
    };

    assert!(low.same_voice(&high));
    assert!(!high.same_voice(&other));
    assert_eq!(high.semitones(), Some(5.0));
    let timbre = Conversion::Timbre {
        source: Voiceprint::new(vec![1.0]),
        target: Voiceprint::new(vec![0.5]),
    };
    assert!(timbre.same_voice(&timbre.clone()));
    assert!(!timbre.same_voice(&low));
    assert_eq!(timbre.semitones(), None);
}

#[test]
fn the_keys_of_the_voices_tell_them_apart() {
    assert_eq!(VoiceTarget::Preset(VoicePreset::Feminine).key(), "feminine");
    assert_eq!(VoiceTarget::Own("abc".to_owned()).key(), "abc");
}

#[test]
fn a_trained_model_leaves_the_voice_as_it_is_for_the_model_to_change() {
    let chain = VoiceChain::untouched();

    assert_eq!(chain.pitch, PitchShape::Fixed { semitones: 0.0 });
    assert_eq!(chain.formant, 0.0);
    assert_eq!(chain.color, VoiceColor::default());
}
