mod support;

use instantpad_application::events::{AiStatus, AppEvent, VoiceIcon, VoiceView};
use instantpad_application::message::{Message, TextKey};
use instantpad_application::ports::VoiceAnalyzer;
use instantpad_domain::cloning::{TERMS_VERSION, VoiceLibrary};
use instantpad_domain::voice::{
    PitchShape, VoiceEffect, VoiceParam, VoicePreset, VoiceSettings, VoiceTarget,
};
use instantpad_test_support::fakes::FakeAnalyzer;
use support::{VoiceWorld, cloned_voice, own_voice, speech};

fn error(key: TextKey) -> AppEvent {
    AppEvent::error(Message::text(key))
}

fn shown(key: TextKey) -> AppEvent {
    AppEvent::show(Message::text(key))
}

fn saved_with(effect: VoiceEffect) -> VoiceSettings {
    let mut voice = VoiceSettings::default();
    voice.select(effect);
    voice
}

fn calibrated() -> VoiceLibrary {
    VoiceLibrary {
        own: Some(own_voice()),
        ..VoiceLibrary::default()
    }
}

fn active(view: &VoiceView) -> (VoiceEffect, Option<&str>) {
    (view.active, view.active_clone_id.as_deref())
}

fn pitch_of(world: &VoiceWorld) -> f32 {
    match world.changer.chain.lock().unwrap().unwrap().pitch {
        PitchShape::Fixed { semitones } => semitones,
        PitchShape::Cycle { .. } => panic!("expected a fixed pitch"),
    }
}

#[test]
fn an_ai_effect_without_the_models_is_refused_and_nothing_changes() {
    let world = VoiceWorld::new();

    world.voice.select(VoiceEffect::FeminineAi);

    assert!(world.saw(&error(TextKey::AiNotInstalled)));
    assert_eq!(world.voice.view().active, VoiceEffect::Natural);
    assert_eq!(world.settings.lock().voice, VoiceSettings::default());
    assert_eq!(*world.changer.chain.lock().unwrap(), None);
    assert_eq!(*world.changer.conversion.lock().unwrap(), None);
}

#[test]
fn an_ai_effect_without_the_own_voice_known_asks_for_the_calibration() {
    let world = VoiceWorld::new();
    *world.models.is_installed.lock().unwrap() = true;

    world.voice.select(VoiceEffect::FeminineAi);

    assert!(world.saw(&error(TextKey::AiNotCalibrated)));
    assert_eq!(world.voice.view().active, VoiceEffect::Natural);
    assert_eq!(*world.changer.conversion.lock().unwrap(), None);
}

#[test]
fn an_ai_effect_hands_the_engine_its_chain_and_the_voice_of_the_preset() {
    let world = VoiceWorld::ready();
    world.voice.set_param(VoiceParam::FeminineAiPitch, 1.0);

    world.voice.select(VoiceEffect::FeminineAi);

    assert_eq!(world.voice.view().active, VoiceEffect::FeminineAi);
    assert_eq!(pitch_of(&world), 8.0);
    let conversion = world.changer.conversion.lock().unwrap().clone().unwrap();
    assert_eq!(conversion.source, own_voice());
    assert_eq!(
        conversion.target,
        FakeAnalyzer::default().preset(VoicePreset::Feminine)
    );
}

#[test]
fn the_effects_without_the_ai_leave_the_conversion_empty() {
    let world = VoiceWorld::ready();
    world.voice.select(VoiceEffect::FeminineAi);

    world.voice.select(VoiceEffect::Ogre);

    assert!(world.changer.chain.lock().unwrap().is_some());
    assert_eq!(*world.changer.conversion.lock().unwrap(), None);
}

#[test]
fn a_cloned_voice_hands_the_engine_its_voiceprint() {
    let world = VoiceWorld::ready();
    world.keep_clone("a", "Ana");

    world.voice.select_clone("a");

    assert_eq!(
        active(&world.voice.view()),
        (VoiceEffect::Cloned, Some("a"))
    );
    let conversion = world.changer.conversion.lock().unwrap().clone().unwrap();
    assert_eq!(conversion.target, cloned_voice("a", "Ana").voiceprint);
    assert_eq!(
        world.settings.lock().voice.target(),
        Some(VoiceTarget::Clone("a".to_owned()))
    );
}

#[test]
fn a_cloned_voice_that_does_not_exist_is_refused() {
    let world = VoiceWorld::ready();

    world.voice.select_clone("nobody");

    assert!(world.saw(&error(TextKey::CloneDeleted)));
    assert_eq!(world.voice.view().active, VoiceEffect::Natural);
    assert_eq!(world.settings.lock().voice.clone_id, None);
}

#[test]
fn a_cloned_voice_under_older_terms_is_refused() {
    let world = VoiceWorld::ready();
    world.keep_clone("old", "Old");
    world.library.lock().clones[0].consent.terms_version = TERMS_VERSION - 1;

    world.voice.select_clone("old");

    assert!(world.saw(&error(TextKey::CloneDeleted)));
    assert_eq!(world.voice.view().active, VoiceEffect::Natural);
}

#[tokio::test]
async fn installing_the_models_reports_the_progress_and_then_ready() {
    let world = VoiceWorld::new();
    assert_eq!(world.voice.view().ai.status, AiStatus::Missing);

    world.voice.install_ai().await;

    assert_eq!(
        world.ai_statuses(),
        [
            AiStatus::Downloading { percent: 0 },
            AiStatus::Downloading { percent: 50 },
            AiStatus::Downloading { percent: 100 },
            AiStatus::Ready,
        ]
    );
    assert!(world.saw(&shown(TextKey::AiInstalled)));
    assert_eq!(world.voice.view().ai.status, AiStatus::Ready);
}

#[tokio::test]
async fn a_failed_install_shows_the_error_and_goes_back_to_missing() {
    let world = VoiceWorld::new();
    *world.models.is_offline.lock().unwrap() = true;

    world.voice.install_ai().await;

    assert_eq!(
        world.ai_statuses(),
        [AiStatus::Downloading { percent: 0 }, AiStatus::Missing]
    );
    assert!(world.saw(&error(TextKey::AiDownloadFailed)));
    assert!(!world.saw(&shown(TextKey::AiInstalled)));
    assert_eq!(world.voice.view().ai.status, AiStatus::Missing);
}

#[tokio::test]
async fn installing_what_is_already_installed_does_nothing() {
    let world = VoiceWorld::ready();

    world.voice.install_ai().await;

    assert!(world.events.0.lock().unwrap().is_empty());
}

#[test]
fn the_screen_is_told_the_size_of_the_download() {
    let world = VoiceWorld::new();

    assert_eq!(world.voice.view().ai.download_megabytes, 260);
}

#[test]
fn calibration_listens_to_the_microphone_until_it_is_finished() {
    let world = VoiceWorld::new();

    world.voice.start_calibration();

    assert!(*world.recorder.is_recording.lock().unwrap());
    assert!(world.voice.view().ai.is_calibrating);
}

#[tokio::test]
async fn a_calibration_that_is_too_short_is_refused() {
    let world = VoiceWorld::new();
    world.analyzer.voiceprints.lock().unwrap().push(own_voice());
    world.voice.start_calibration();
    *world.recorder.next.lock().unwrap() = speech(7);

    world.voice.finish_calibration().await;

    assert!(world.saw(&error(TextKey::AiCalibrationTooShort)));
    assert_eq!(world.library.lock().own, None);
    assert_eq!(world.repository.library.lock().unwrap().own, None);
    assert_eq!(world.analyzer.voiceprints.lock().unwrap().len(), 1);
    let view = world.voice.view();
    assert!(!view.ai.is_calibrating);
    assert!(!view.ai.is_calibrated);
}

#[tokio::test]
async fn a_long_enough_calibration_saves_the_own_voice() {
    let world = VoiceWorld::new();
    world.analyzer.voiceprints.lock().unwrap().push(own_voice());
    world.voice.start_calibration();
    *world.recorder.next.lock().unwrap() = speech(8);

    world.voice.finish_calibration().await;

    assert_eq!(
        world.repository.library.lock().unwrap().own,
        Some(own_voice())
    );
    assert!(world.saw(&shown(TextKey::AiCalibrationSaved)));
    assert!(!*world.recorder.is_recording.lock().unwrap());
    let view = world.voice.view();
    assert!(view.ai.is_calibrated);
    assert!(!view.ai.is_calibrating);
}

#[tokio::test]
async fn a_calibration_the_ai_cannot_hear_shows_why() {
    let world = VoiceWorld::new();
    world.voice.start_calibration();
    *world.recorder.next.lock().unwrap() = speech(10);

    world.voice.finish_calibration().await;

    assert!(world.saw(&error(TextKey::AiLoadFailed)));
    assert!(!world.voice.view().ai.is_calibrated);
}

#[tokio::test]
async fn calibrating_again_replaces_the_own_voice() {
    let world = VoiceWorld::resuming(VoiceSettings::default(), calibrated(), true);
    let newer = FakeAnalyzer::voice([0.0, 2.0]);
    world
        .analyzer
        .voiceprints
        .lock()
        .unwrap()
        .push(newer.clone());
    world.voice.start_calibration();
    *world.recorder.next.lock().unwrap() = speech(9);

    world.voice.finish_calibration().await;

    assert_eq!(world.repository.library.lock().unwrap().own, Some(newer));
}

#[test]
fn a_saved_ai_effect_starts_as_natural_when_the_models_are_gone() {
    let world = VoiceWorld::resuming(saved_with(VoiceEffect::FeminineAi), calibrated(), false);

    assert_eq!(world.voice.view().active, VoiceEffect::Natural);
    assert_eq!(*world.changer.chain.lock().unwrap(), None);
    assert_eq!(*world.changer.conversion.lock().unwrap(), None);
}

#[test]
fn a_saved_ai_effect_starts_as_natural_when_the_own_voice_is_unknown() {
    let world = VoiceWorld::resuming(
        saved_with(VoiceEffect::FeminineAi),
        VoiceLibrary::default(),
        true,
    );

    assert_eq!(world.voice.view().active, VoiceEffect::Natural);
    assert_eq!(*world.changer.conversion.lock().unwrap(), None);
}

#[test]
fn a_saved_ai_effect_starts_running_when_everything_is_there() {
    let world = VoiceWorld::resuming(saved_with(VoiceEffect::FeminineAi), calibrated(), true);

    assert_eq!(world.voice.view().active, VoiceEffect::FeminineAi);
    assert!(world.changer.chain.lock().unwrap().is_some());
    assert!(world.changer.conversion.lock().unwrap().is_some());
}

#[test]
fn a_saved_clone_that_no_longer_exists_starts_as_natural() {
    let mut voice = VoiceSettings::default();
    voice.select_clone("gone");

    let world = VoiceWorld::resuming(voice, calibrated(), true);

    assert_eq!(world.voice.view().active, VoiceEffect::Natural);
    assert_eq!(*world.changer.chain.lock().unwrap(), None);
}

#[test]
fn a_saved_effect_without_the_ai_starts_whatever_the_models() {
    let world = VoiceWorld::resuming(
        saved_with(VoiceEffect::Ghost),
        VoiceLibrary::default(),
        false,
    );

    assert_eq!(world.voice.view().active, VoiceEffect::Ghost);
    assert!(world.changer.chain.lock().unwrap().is_some());
}

#[test]
fn deleting_the_active_clone_goes_back_to_the_natural_voice_and_forgets_it() {
    let world = VoiceWorld::ready();
    world.keep_clone("a", "Ana");
    world.keep_clone("b", "Bia");
    world.voice.select_clone("a");

    world.voice.delete_clone("a");

    let view = world.voice.view();
    assert_eq!(active(&view), (VoiceEffect::Natural, None));
    assert_eq!(*world.changer.chain.lock().unwrap(), None);
    assert_eq!(*world.changer.conversion.lock().unwrap(), None);
    assert!(
        view.effects
            .iter()
            .all(|effect| effect.clone_id.as_deref() != Some("a"))
    );
    assert!(
        view.effects
            .iter()
            .any(|effect| effect.clone_id.as_deref() == Some("b"))
    );
    let kept = world.repository.library.lock().unwrap().clone();
    assert_eq!(kept.clones.len(), 1);
    assert_eq!(kept.clones[0].id, "b");
    assert!(world.saw(&shown(TextKey::CloneDeleted)));
}

#[test]
fn deleting_a_clone_that_is_not_in_use_keeps_the_effect() {
    let world = VoiceWorld::ready();
    world.keep_clone("a", "Ana");
    world.voice.select(VoiceEffect::Ogre);

    world.voice.delete_clone("a");

    assert_eq!(world.voice.view().active, VoiceEffect::Ogre);
    assert!(world.repository.library.lock().unwrap().clones.is_empty());
}

#[test]
fn deleting_a_clone_that_does_not_exist_does_nothing() {
    let world = VoiceWorld::ready();
    world.keep_clone("a", "Ana");

    world.voice.delete_clone("b");

    assert!(world.events.0.lock().unwrap().is_empty());
    assert_eq!(world.library.lock().clones.len(), 1);
}

#[test]
fn the_next_voice_skips_the_ai_voices_that_cannot_run() {
    let world = VoiceWorld::resuming(
        saved_with(VoiceEffect::Ogre),
        VoiceLibrary::default(),
        false,
    );

    world.voice.select_next();

    assert_eq!(world.voice.view().active, VoiceEffect::Thin);
    assert!(world.saw(&shown(TextKey::EffectThin)));
    assert!(
        !world
            .events
            .0
            .lock()
            .unwrap()
            .contains(&error(TextKey::AiNotInstalled))
    );
}

#[test]
fn the_next_voice_skips_the_clones_when_the_own_voice_is_unknown() {
    let world = VoiceWorld::resuming(saved_with(VoiceEffect::Ogre), VoiceLibrary::default(), true);
    world.keep_clone("a", "Ana");

    world.voice.select_next();

    assert_eq!(world.voice.view().active, VoiceEffect::Thin);
}

#[test]
fn the_next_voice_walks_into_the_ai_voices_and_the_clones_when_they_can_run() {
    let world = VoiceWorld::ready();
    world.keep_clone("a", "Ana");
    world.keep_clone("b", "Bia");
    world.voice.select(VoiceEffect::Ogre);

    let mut walked = Vec::new();
    for _ in 0..4 {
        world.voice.select_next();
        let view = world.voice.view();
        walked.push((view.active, view.active_clone_id));
    }

    assert_eq!(
        walked,
        [
            (VoiceEffect::FeminineAi, None),
            (VoiceEffect::Cloned, Some("a".to_owned())),
            (VoiceEffect::Cloned, Some("b".to_owned())),
            (VoiceEffect::Thin, None),
        ]
    );
    assert!(world.saw(&AppEvent::show(Message::plain("Bia"))));
}

#[test]
fn the_next_voice_skips_a_clone_under_older_terms() {
    let world = VoiceWorld::ready();
    world.keep_clone("old", "Old");
    world.keep_clone("a", "Ana");
    world.library.lock().clones[0].consent.terms_version = TERMS_VERSION - 1;
    world.voice.select(VoiceEffect::FeminineAi);

    world.voice.select_next();

    assert_eq!(
        active(&world.voice.view()),
        (VoiceEffect::Cloned, Some("a"))
    );
}

#[test]
fn the_toggle_key_comes_back_to_the_clone_and_says_its_name() {
    let world = VoiceWorld::ready();
    world.keep_clone("a", "Ana");
    world.voice.select_clone("a");

    world.voice.toggle();
    assert_eq!(world.voice.view().active, VoiceEffect::Natural);

    world.voice.toggle();
    assert_eq!(
        active(&world.voice.view()),
        (VoiceEffect::Cloned, Some("a"))
    );
    assert!(world.saw(&AppEvent::show(Message::plain("Ana"))));
}

#[test]
fn the_toggle_key_stays_off_when_the_ai_voice_cannot_run() {
    let world = VoiceWorld::ready();
    world.voice.select(VoiceEffect::FeminineAi);
    world.voice.toggle();
    *world.models.is_installed.lock().unwrap() = false;

    world.voice.toggle();

    assert_eq!(world.voice.view().active, VoiceEffect::Natural);
    assert!(world.saw(&error(TextKey::AiNotInstalled)));
    assert!(!world.saw(&shown(TextKey::EffectFeminineAi)));
}

#[test]
fn clones_appear_among_the_effects_with_their_names() {
    let world = VoiceWorld::ready();
    world.keep_clone("a", "Ana");
    world.keep_clone("b", "Bia");

    let view = world.voice.view();

    assert_eq!(view.effects.len(), VoiceEffect::BUILT_IN.len() + 2);
    let clones: Vec<_> = view.effects[VoiceEffect::BUILT_IN.len()..].iter().collect();
    assert_eq!(clones[0].name, Message::plain("Ana"));
    assert_eq!(clones[1].name, Message::plain("Bia"));
    for (clone, id) in clones.iter().zip(["a", "b"]) {
        assert_eq!(clone.effect, VoiceEffect::Cloned);
        assert_eq!(clone.clone_id.as_deref(), Some(id));
        assert_eq!(clone.icon, VoiceIcon::Person);
        assert!(clone.needs_ai);
        assert_eq!(clone.params.len(), 1);
        assert_eq!(clone.params[0].param, VoiceParam::ClonePitch);
    }
}

#[test]
fn a_clone_under_older_terms_does_not_appear_among_the_effects() {
    let world = VoiceWorld::ready();
    world.keep_clone("old", "Old");
    world.library.lock().clones[0].consent.terms_version = TERMS_VERSION - 1;

    let view = world.voice.view();

    assert_eq!(view.effects.len(), VoiceEffect::BUILT_IN.len());
}

#[test]
fn the_built_in_effects_come_first_and_only_the_ai_ones_say_they_need_it() {
    let world = VoiceWorld::new();

    let view = world.voice.view();

    let listed: Vec<_> = view.effects.iter().map(|effect| effect.effect).collect();
    assert_eq!(listed, VoiceEffect::BUILT_IN);
    for effect in &view.effects {
        assert_eq!(effect.needs_ai, effect.effect == VoiceEffect::FeminineAi);
        assert_eq!(effect.clone_id, None);
    }
}

#[test]
fn the_knob_of_a_clone_changes_its_pitch() {
    let world = VoiceWorld::ready();
    world.keep_clone("a", "Ana");
    world.voice.select_clone("a");

    world.voice.set_param(VoiceParam::ClonePitch, 1.0);

    assert_eq!(pitch_of(&world), 8.0);
    let view = world.voice.view();
    let clone = view.effects.last().unwrap();
    assert_eq!(clone.params[0].text, "+8.0 st");
}

#[test]
fn the_screen_is_told_after_a_clone_is_cloned_elsewhere() {
    let world = VoiceWorld::ready();
    world.keep_clone("a", "Ana");

    world.voice.refresh();

    let events = world.events.0.lock().unwrap();
    let Some(AppEvent::Voice(view)) = events.last() else {
        panic!("expected a voice view");
    };
    assert_eq!(view.effects.len(), VoiceEffect::BUILT_IN.len() + 1);
}
