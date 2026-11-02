mod support;

use instantpad_application::events::{
    AiStatus, AppEvent, Calibration, ClonedVoiceView, DesignerView, VoiceIcon, VoiceView,
};
use instantpad_application::message::{Message, TextKey};
use instantpad_application::ports::VoiceAnalyzer;
use instantpad_domain::cloning::{
    DEFAULT_BLEND, DEFAULT_TIMBRE, MIN_CALIBRATION_SECONDS, VoiceLibrary, VoiceRecipe, Voiceprint,
};
use instantpad_domain::voice::{
    PitchShape, VoiceEffect, VoiceParam, VoicePreset, VoiceSettings, VoiceTarget,
};
use instantpad_test_support::fakes::FakeAnalyzer;
use support::{VoiceWorld, cloned_voice, own_voice, poll_once, speech};

fn error(key: TextKey) -> AppEvent {
    AppEvent::error(Message::text(key))
}

fn failed(key: TextKey, detail: &str) -> AppEvent {
    AppEvent::error(Message::with(key, [Message::plain(detail)]))
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
        Some(VoiceTarget::Own("a".to_owned()))
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
    assert!(world.saw(&failed(TextKey::AiDownloadFailed, "offline")));
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
    let world = VoiceWorld::installed();

    world.voice.start_calibration();

    assert!(*world.recorder.is_recording.lock().unwrap());
    assert_eq!(world.voice.view().ai.calibration, Calibration::Recording);
}

#[tokio::test]
async fn a_calibration_that_is_too_short_is_refused() {
    let world = VoiceWorld::installed();
    world.analyzer.voiceprints.lock().unwrap().push(own_voice());
    world.voice.start_calibration();
    *world.recorder.next.lock().unwrap() = speech(MIN_CALIBRATION_SECONDS - 1);

    world.voice.finish_calibration().await;

    let seconds = Message::plain(MIN_CALIBRATION_SECONDS.to_string());
    let reason = Message::with(TextKey::AiCalibrationTooShort, [seconds]);
    assert!(world.saw(&AppEvent::error(reason)));
    assert_eq!(world.library.lock().own, None);
    assert_eq!(world.repository.library.lock().unwrap().own, None);
    assert_eq!(world.analyzer.voiceprints.lock().unwrap().len(), 1);
    let view = world.voice.view();
    assert_eq!(view.ai.calibration, Calibration::Idle);
    assert_eq!(world.library.lock().own, None);
}

#[tokio::test]
async fn a_long_enough_calibration_saves_the_own_voice() {
    let world = VoiceWorld::installed();
    world.analyzer.voiceprints.lock().unwrap().push(own_voice());
    world.voice.start_calibration();
    *world.recorder.next.lock().unwrap() = speech(MIN_CALIBRATION_SECONDS);

    world.voice.finish_calibration().await;

    assert_eq!(
        world.repository.library.lock().unwrap().own,
        Some(own_voice())
    );
    assert!(world.saw(&shown(TextKey::AiCalibrationSaved)));
    assert!(!*world.recorder.is_recording.lock().unwrap());
    let view = world.voice.view();
    assert!(world.library.lock().own.is_some());
    assert_eq!(view.ai.calibration, Calibration::Idle);
}

#[tokio::test]
async fn a_calibration_the_ai_cannot_hear_shows_why() {
    let world = VoiceWorld::installed();
    world.voice.start_calibration();
    *world.recorder.next.lock().unwrap() = speech(10);

    world.voice.finish_calibration().await;

    assert!(world.saw(&failed(TextKey::AiLoadFailed, "no voiceprint prepared")));
    assert_eq!(world.library.lock().own, None);
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
fn calibration_needs_the_models() {
    let world = VoiceWorld::new();

    world.voice.start_calibration();

    assert!(world.saw(&error(TextKey::AiNotInstalled)));
    assert_eq!(world.voice.view().ai.calibration, Calibration::Idle);
    assert!(!*world.recorder.is_recording.lock().unwrap());
}

#[tokio::test]
async fn finishing_a_calibration_that_never_started_does_nothing() {
    let world = VoiceWorld::installed();
    *world.recorder.next.lock().unwrap() = speech(MIN_CALIBRATION_SECONDS);

    world.voice.finish_calibration().await;

    assert!(world.events.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_second_stop_during_the_analysis_does_nothing() {
    let world = VoiceWorld::installed();
    world.analyzer.voiceprints.lock().unwrap().push(own_voice());
    let voice = world.voice.clone();
    let recorder = world.recorder.clone();
    *world.analyzer.meanwhile.lock().unwrap() = Some(Box::new(move || {
        assert_eq!(voice.view().ai.calibration, Calibration::Analyzing);
        assert!(poll_once(voice.finish_calibration()).is_some());
        voice.start_calibration();
        assert!(!*recorder.is_recording.lock().unwrap());
        assert_eq!(voice.view().ai.calibration, Calibration::Analyzing);
    }));
    world.voice.start_calibration();
    *world.recorder.next.lock().unwrap() = speech(MIN_CALIBRATION_SECONDS);

    world.voice.finish_calibration().await;

    let saved = shown(TextKey::AiCalibrationSaved);
    let count = |event: &AppEvent| {
        world
            .events
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|e| *e == event)
            .count()
    };
    assert_eq!(count(&saved), 1);
    assert_eq!(
        world.repository.library.lock().unwrap().own,
        Some(own_voice())
    );
    assert_eq!(world.voice.view().ai.calibration, Calibration::Idle);
}

#[tokio::test]
async fn a_calibration_that_cannot_be_saved_is_not_used() {
    let world = VoiceWorld::installed();
    world.analyzer.voiceprints.lock().unwrap().push(own_voice());
    *world.repository.fails.lock().unwrap() = true;
    world.voice.start_calibration();
    *world.recorder.next.lock().unwrap() = speech(MIN_CALIBRATION_SECONDS);

    world.voice.finish_calibration().await;

    assert!(world.saw(&error(TextKey::SaveFailed)));
    assert!(!world.saw(&shown(TextKey::AiCalibrationSaved)));
    assert_eq!(world.library.lock().own, None);
    assert_eq!(world.library.lock().own, None);
}

#[test]
fn a_clone_that_could_not_be_deleted_stays_in_the_app() {
    let world = VoiceWorld::ready();
    world.keep_clone("a", "Ana");
    world.voice.select_clone("a");
    *world.repository.fails.lock().unwrap() = true;

    world.voice.delete_clone("a");

    assert!(world.saw(&error(TextKey::SaveFailed)));
    assert!(!world.saw(&shown(TextKey::CloneDeleted)));
    assert_eq!(world.library.lock().ids(), ["a"]);
    assert_eq!(
        active(&world.voice.view()),
        (VoiceEffect::Cloned, Some("a"))
    );
}

#[test]
fn an_ai_effect_that_cannot_run_yet_waits_for_the_setup() {
    let world = VoiceWorld::new();
    assert_eq!(world.voice.view().ai.waiting, None);

    world.voice.select(VoiceEffect::FeminineAi);

    let view = world.voice.view();
    assert_eq!(
        view.ai.waiting,
        Some(Message::text(TextKey::EffectFeminineAi))
    );
    assert_eq!(view.active, VoiceEffect::Natural);
}

#[test]
fn a_clone_that_cannot_run_yet_waits_under_its_name() {
    let world = VoiceWorld::new();
    world.keep_clone("a", "Ana");

    world.voice.select_clone("a");

    assert_eq!(world.voice.view().ai.waiting, Some(Message::plain("Ana")));
}

#[tokio::test]
async fn a_waiting_effect_comes_on_by_itself_after_the_install_and_the_calibration() {
    let world = VoiceWorld::new();
    world.voice.select(VoiceEffect::FeminineAi);

    world.voice.install_ai().await;

    let view = world.voice.view();
    assert_eq!(view.active, VoiceEffect::Natural);
    assert_eq!(
        view.ai.waiting,
        Some(Message::text(TextKey::EffectFeminineAi))
    );

    world.analyzer.voiceprints.lock().unwrap().push(own_voice());
    world.voice.start_calibration();
    *world.recorder.next.lock().unwrap() = speech(MIN_CALIBRATION_SECONDS);
    world.voice.finish_calibration().await;

    let view = world.voice.view();
    assert_eq!(view.active, VoiceEffect::FeminineAi);
    assert_eq!(view.ai.waiting, None);
    assert!(world.changer.conversion.lock().unwrap().is_some());
}

#[tokio::test]
async fn a_waiting_effect_comes_on_right_after_the_install_when_the_voice_is_known() {
    let world = VoiceWorld::resuming(VoiceSettings::default(), calibrated(), false);
    world.voice.select(VoiceEffect::FeminineAi);

    world.voice.install_ai().await;

    let view = world.voice.view();
    assert_eq!(view.active, VoiceEffect::FeminineAi);
    assert_eq!(view.ai.waiting, None);
    assert!(world.changer.conversion.lock().unwrap().is_some());
}

#[tokio::test]
async fn a_failed_install_keeps_the_effect_waiting() {
    let world = VoiceWorld::resuming(VoiceSettings::default(), calibrated(), false);
    world.voice.select(VoiceEffect::FeminineAi);
    *world.models.is_offline.lock().unwrap() = true;

    world.voice.install_ai().await;

    let view = world.voice.view();
    assert_eq!(view.active, VoiceEffect::Natural);
    assert_eq!(
        view.ai.waiting,
        Some(Message::text(TextKey::EffectFeminineAi))
    );
}

#[tokio::test]
async fn choosing_another_voice_cancels_the_waiting_one() {
    let world = VoiceWorld::resuming(VoiceSettings::default(), calibrated(), false);
    world.voice.select(VoiceEffect::FeminineAi);

    world.voice.select(VoiceEffect::Thin);
    assert_eq!(world.voice.view().ai.waiting, None);
    world.voice.install_ai().await;

    assert_eq!(world.voice.view().active, VoiceEffect::Thin);
}

fn recipe(base: &str) -> VoiceRecipe {
    VoiceRecipe {
        base_id: base.to_owned(),
        timbre: DEFAULT_TIMBRE,
        blend_id: None,
        blend: 0.0,
    }
}

fn assert_close(heard: &Voiceprint, expected: [f32; 2]) {
    assert_eq!(heard.values().len(), 2);
    for (heard, expected) in heard.values().iter().zip(expected) {
        assert!((heard - expected).abs() < 1e-5, "{heard} is not {expected}");
    }
}

fn target_of(world: &VoiceWorld) -> Voiceprint {
    world
        .changer
        .conversion
        .lock()
        .unwrap()
        .clone()
        .unwrap()
        .target
}

/// Two cloned voices that sound different, and the models and the own voice ready.
fn designer() -> VoiceWorld {
    let world = VoiceWorld::ready();
    world.keep_clone("a", "Ana");
    world.keep_clone("b", "Bia");
    world.library.lock().clones[1].voiceprint = FakeAnalyzer::voice([0.0, 1.0]);
    world
}

fn designs(world: &VoiceWorld) -> Vec<String> {
    world
        .library
        .lock()
        .designs
        .iter()
        .map(|design| design.name.clone())
        .collect()
}

#[test]
fn a_preview_sends_the_designed_voice_with_the_pitch_of_the_own_voice() {
    let world = designer();
    world.voice.open_designer();
    world.voice.set_param(VoiceParam::ClonePitch, 1.0);
    let deep = VoiceRecipe {
        timbre: 0.0,
        ..recipe("a")
    };

    world.voice.preview_design(deep);

    let chain = world.changer.chain.lock().unwrap().unwrap();
    assert_eq!(chain, world.settings.lock().voice.own_voice_chain());
    assert_eq!(chain.pitch, PitchShape::Fixed { semitones: 8.0 });
    let conversion = world.changer.conversion.lock().unwrap().clone().unwrap();
    assert_eq!(conversion.source, own_voice());
    // The voice of "a" moved 0.6 of the way to the masculine preset.
    assert_close(&conversion.target, [0.24, -0.28]);
    assert_eq!(world.voice.view().active, VoiceEffect::Natural);
    assert!(designs(&world).is_empty());
}

#[test]
fn a_preview_follows_the_pitch_knob() {
    let world = designer();
    world.voice.open_designer();
    world.voice.preview_design(recipe("a"));

    world.voice.set_param(VoiceParam::ClonePitch, 0.0);

    let pitch = world.changer.chain.lock().unwrap().unwrap().pitch;
    assert_eq!(pitch, PitchShape::Fixed { semitones: -8.0 });
    assert!(world.changer.conversion.lock().unwrap().is_some());
}

#[test]
fn a_preview_needs_the_models() {
    let world = VoiceWorld::new();
    world.keep_clone("a", "Ana");

    world.voice.open_designer();
    world.voice.preview_design(recipe("a"));

    assert!(world.saw(&error(TextKey::AiNotInstalled)));
    assert_eq!(*world.changer.chain.lock().unwrap(), None);
    assert_eq!(*world.changer.conversion.lock().unwrap(), None);
    assert_eq!(world.voice.view().ai.waiting, None);
}

#[test]
fn a_preview_needs_the_own_voice() {
    let world = VoiceWorld::installed();
    world.keep_clone("a", "Ana");

    world.voice.open_designer();
    world.voice.preview_design(recipe("a"));

    assert!(world.saw(&error(TextKey::AiNotCalibrated)));
    assert_eq!(*world.changer.conversion.lock().unwrap(), None);
    assert_eq!(world.voice.view().ai.waiting, None);
}

#[test]
fn a_preview_of_a_voice_that_is_gone_is_refused_even_without_the_models() {
    let world = VoiceWorld::new();
    world.keep_clone("a", "Ana");

    world.voice.open_designer();
    world.voice.preview_design(recipe("gone"));

    assert!(world.saw(&error(TextKey::CloneDeleted)));
    assert!(!world.saw(&error(TextKey::AiNotInstalled)));
}

#[test]
fn ending_the_preview_goes_back_to_the_effect_in_use() {
    let world = designer();
    world.voice.open_designer();
    world.voice.select(VoiceEffect::Ogre);
    let ogre = world.settings.lock().voice.chain();
    world.voice.preview_design(recipe("a"));
    assert_ne!(*world.changer.chain.lock().unwrap(), ogre);

    world.voice.end_preview();

    assert_eq!(*world.changer.chain.lock().unwrap(), ogre);
    assert_eq!(*world.changer.conversion.lock().unwrap(), None);
}

#[test]
fn ending_the_preview_goes_back_to_the_ai_effect_in_use() {
    let world = designer();
    world.voice.open_designer();
    world.voice.select(VoiceEffect::FeminineAi);
    world.voice.preview_design(recipe("a"));

    world.voice.end_preview();

    assert_close(&target_of(&world), [0.0, 1.0]);
}

#[test]
fn a_preview_whose_voice_is_deleted_ends_by_itself() {
    let world = designer();
    world.voice.open_designer();
    world.voice.select(VoiceEffect::Ogre);
    world.voice.preview_design(recipe("a"));

    world.voice.delete_clone("a");

    let ogre = world.settings.lock().voice.chain();
    assert_eq!(*world.changer.chain.lock().unwrap(), ogre);
    assert_eq!(*world.changer.conversion.lock().unwrap(), None);
}

#[test]
fn saving_a_design_keeps_it_and_turns_it_on() {
    let world = designer();
    let recipe = VoiceRecipe {
        blend_id: Some("b".to_owned()),
        blend: 0.5,
        ..recipe("a")
    };
    world.voice.open_designer();
    world.voice.preview_design(recipe.clone());

    world.voice.save_design("  Mix ", recipe);

    assert_eq!(designs(&world), ["Mix"]);
    let kept = world.repository.library.lock().unwrap().clone();
    assert_eq!(kept.designs.len(), 1);
    let id = kept.designs[0].id.clone();
    let view = world.voice.view();
    assert_eq!(active(&view), (VoiceEffect::Cloned, Some(id.as_str())));
    assert_close(&target_of(&world), [0.3, 0.9]);
    assert!(world.saw(&shown(TextKey::CloneSaved)));
    let design = view.effects.last().unwrap();
    assert_eq!(design.name, Message::plain("Mix"));
    assert_eq!(design.icon, VoiceIcon::Blend);
    assert_eq!(design.clone_id.as_deref(), Some(id.as_str()));
    assert_eq!(design.effect, VoiceEffect::Cloned);
}

#[test]
fn saving_a_design_ends_the_preview() {
    let world = designer();
    world.voice.open_designer();
    world.voice.preview_design(recipe("a"));
    world.voice.save_design("Mix", recipe("a"));

    world.voice.select(VoiceEffect::Natural);

    assert_eq!(*world.changer.chain.lock().unwrap(), None);
    assert_eq!(*world.changer.conversion.lock().unwrap(), None);
}

#[test]
fn a_design_that_could_not_be_saved_is_not_kept() {
    let world = designer();
    *world.repository.fails.lock().unwrap() = true;

    world.voice.open_designer();
    world.voice.save_design("Mix", recipe("a"));

    assert!(world.saw(&error(TextKey::SaveFailed)));
    assert!(!world.saw(&shown(TextKey::CloneSaved)));
    assert!(designs(&world).is_empty());
    assert!(world.repository.library.lock().unwrap().designs.is_empty());
    assert_eq!(world.voice.view().active, VoiceEffect::Natural);
}

#[test]
fn a_design_without_a_name_is_refused() {
    let world = designer();

    world.voice.open_designer();
    world.voice.save_design("   ", recipe("a"));

    assert!(world.saw(&error(TextKey::CloneMissingName)));
    assert!(designs(&world).is_empty());
}

#[test]
fn a_design_from_a_voice_that_is_gone_is_refused() {
    let world = designer();

    world.voice.open_designer();
    world.voice.save_design("Mix", recipe("gone"));

    assert!(world.saw(&error(TextKey::CloneDeleted)));
    assert!(designs(&world).is_empty());
    assert!(!world.saw(&shown(TextKey::CloneSaved)));
}

#[tokio::test]
async fn a_design_saved_before_the_own_voice_is_known_waits_to_come_on() {
    let world = VoiceWorld::installed();
    world.keep_clone("a", "Ana");

    world.voice.open_designer();
    world.voice.save_design("Mix", recipe("a"));

    assert_eq!(designs(&world), ["Mix"]);
    let view = world.voice.view();
    assert_eq!(view.active, VoiceEffect::Natural);
    assert_eq!(view.ai.waiting, Some(Message::plain("Mix")));

    world.analyzer.voiceprints.lock().unwrap().push(own_voice());
    world.voice.start_calibration();
    *world.recorder.next.lock().unwrap() = speech(MIN_CALIBRATION_SECONDS);
    world.voice.finish_calibration().await;

    let view = world.voice.view();
    assert_eq!(view.active, VoiceEffect::Cloned);
    assert_eq!(view.ai.waiting, None);
}

#[test]
fn deleting_a_clone_deletes_the_designs_made_from_it_and_turns_off_the_active_one() {
    let world = designer();
    let recipe = VoiceRecipe {
        blend_id: Some("b".to_owned()),
        blend: 0.5,
        ..recipe("a")
    };
    world.voice.open_designer();
    world.voice.save_design("Mix", recipe);
    assert_eq!(world.voice.view().active, VoiceEffect::Cloned);

    world.voice.delete_clone("b");

    let view = world.voice.view();
    assert_eq!(active(&view), (VoiceEffect::Natural, None));
    assert!(designs(&world).is_empty());
    assert!(world.repository.library.lock().unwrap().designs.is_empty());
    assert_eq!(world.library.lock().ids(), ["a"]);
    assert!(
        view.effects
            .iter()
            .all(|effect| effect.icon != VoiceIcon::Blend)
    );
    assert_eq!(*world.changer.chain.lock().unwrap(), None);
    assert_eq!(*world.changer.conversion.lock().unwrap(), None);
    assert!(world.saw(&shown(TextKey::CloneDeleted)));
}

#[test]
fn deleting_a_clone_keeps_the_designs_that_do_not_use_it() {
    let world = designer();
    world.voice.open_designer();
    world.voice.save_design("Mix", recipe("a"));

    world.voice.delete_clone("b");

    assert_eq!(designs(&world), ["Mix"]);
    assert_eq!(world.voice.view().active, VoiceEffect::Cloned);
}

#[test]
fn deleting_a_design_keeps_the_clones_it_is_made_of() {
    let world = designer();
    let design = world.keep_design("Mix", recipe("a"));
    world.voice.select_clone(&design);

    world.voice.delete_clone(&design);

    assert!(designs(&world).is_empty());
    assert_eq!(world.library.lock().ids(), ["a", "b"]);
    assert_eq!(world.voice.view().active, VoiceEffect::Natural);
}

#[test]
fn deleting_the_voice_a_design_waits_with_ends_the_wait() {
    let world = VoiceWorld::installed();
    world.keep_clone("a", "Ana");
    world.voice.open_designer();
    world.voice.save_design("Mix", recipe("a"));
    assert_eq!(world.voice.view().ai.waiting, Some(Message::plain("Mix")));

    world.voice.delete_clone("a");

    assert_eq!(world.voice.view().ai.waiting, None);
}

#[test]
fn a_voice_that_does_not_exist_never_waits_for_the_setup() {
    let world = VoiceWorld::new();

    world.voice.select_clone("nobody");

    assert!(world.saw(&error(TextKey::CloneDeleted)));
    assert!(!world.saw(&error(TextKey::AiNotInstalled)));
    assert_eq!(world.voice.view().ai.waiting, None);
}

#[test]
fn the_view_lists_the_cloned_voices_apart_and_the_designs_after_them() {
    let world = designer();
    let design = world.keep_design("Mix", recipe("a"));

    let view = world.voice.view();

    assert_eq!(
        view.cloned_voices,
        [
            ClonedVoiceView {
                id: "a".to_owned(),
                name: "Ana".to_owned()
            },
            ClonedVoiceView {
                id: "b".to_owned(),
                name: "Bia".to_owned()
            },
        ]
    );
    let own: Vec<_> = view.effects[VoiceEffect::BUILT_IN.len()..]
        .iter()
        .map(|effect| (effect.icon, effect.clone_id.clone().unwrap()))
        .collect();
    assert_eq!(
        own,
        [
            (VoiceIcon::Person, "a".to_owned()),
            (VoiceIcon::Person, "b".to_owned()),
            (VoiceIcon::Blend, design),
        ]
    );
}

#[test]
fn the_next_voice_walks_the_designs_after_the_clones() {
    let world = designer();
    let design = world.keep_design("Mix", recipe("a"));
    world.voice.select(VoiceEffect::FeminineAi);

    let mut walked = Vec::new();
    for _ in 0..4 {
        world.voice.select_next();
        walked.push(world.voice.view().active_clone_id);
    }

    assert_eq!(
        walked,
        [
            Some("a".to_owned()),
            Some("b".to_owned()),
            Some(design),
            None
        ]
    );
    assert!(world.saw(&AppEvent::show(Message::plain("Mix"))));
}

fn designer_view(world: &VoiceWorld) -> Option<DesignerView> {
    world.voice.view().designer
}

#[test]
fn the_designer_opens_only_when_there_is_a_cloned_voice() {
    let world = VoiceWorld::ready();

    world.voice.open_designer();
    assert_eq!(designer_view(&world), None);

    world.keep_clone("a", "Ana");
    world.voice.open_designer();
    assert_eq!(
        designer_view(&world),
        Some(DesignerView {
            is_previewing: false,
            default_timbre: f64::from(DEFAULT_TIMBRE),
            default_blend: f64::from(DEFAULT_BLEND),
        })
    );
}

#[test]
fn closing_the_designer_ends_the_preview_and_hides_it() {
    let world = designer();
    world.voice.open_designer();
    world.voice.preview_design(recipe("a"));
    assert!(designer_view(&world).unwrap().is_previewing);

    world.voice.close_designer();

    assert_eq!(designer_view(&world), None);
    assert_eq!(*world.changer.chain.lock().unwrap(), None);
    assert_eq!(*world.changer.conversion.lock().unwrap(), None);
}

#[test]
fn a_preview_needs_the_designer_to_be_open() {
    let world = designer();

    world.voice.preview_design(recipe("a"));

    assert_eq!(*world.changer.conversion.lock().unwrap(), None);
    assert!(!world.voice.view().is_hearing);
    assert_eq!(designer_view(&world), None);
}

#[test]
fn the_first_preview_turns_hearing_on_and_ending_it_restores_it() {
    let world = designer();
    world.voice.open_designer();

    world.voice.preview_design(recipe("a"));
    assert!(world.voice.view().is_hearing);
    assert!(*world.changer.is_hearing.lock().unwrap());
    world.voice.preview_design(recipe("b"));
    world.voice.end_preview();

    assert!(!world.voice.view().is_hearing);
    assert!(!*world.changer.is_hearing.lock().unwrap());
    assert!(!designer_view(&world).unwrap().is_previewing);
}

#[test]
fn hearing_that_was_on_stays_on_after_the_preview() {
    let world = designer();
    world.voice.set_hearing(true);
    world.voice.open_designer();
    world.voice.preview_design(recipe("a"));

    world.voice.end_preview();

    assert!(world.voice.view().is_hearing);
}

#[test]
fn the_choice_of_hearing_made_during_a_preview_is_what_comes_back() {
    let world = designer();
    world.voice.open_designer();
    world.voice.preview_design(recipe("a"));

    world.voice.set_hearing(true);
    world.voice.end_preview();

    assert!(world.voice.view().is_hearing);
    assert!(*world.changer.is_hearing.lock().unwrap());
}

#[test]
fn a_refused_preview_changes_nothing() {
    let world = VoiceWorld::installed();
    world.keep_clone("a", "Ana");
    world.voice.open_designer();

    world.voice.preview_design(recipe("a"));

    assert!(world.saw(&error(TextKey::AiNotCalibrated)));
    assert!(!world.voice.view().is_hearing);
    assert!(!*world.changer.is_hearing.lock().unwrap());
    assert!(!designer_view(&world).unwrap().is_previewing);
}

#[test]
fn closing_the_designer_restores_the_hearing_too() {
    let world = designer();
    world.voice.open_designer();
    world.voice.preview_design(recipe("a"));

    world.voice.close_designer();

    assert!(!world.voice.view().is_hearing);
}

#[test]
fn saving_needs_the_designer_to_be_open() {
    let world = designer();

    world.voice.save_design("Mix", recipe("a"));

    assert!(designs(&world).is_empty());
    assert!(world.events.0.lock().unwrap().is_empty());
}

#[test]
fn saving_closes_the_designer_and_restores_the_hearing() {
    let world = designer();
    world.voice.open_designer();
    world.voice.preview_design(recipe("a"));

    world.voice.save_design("Mix", recipe("a"));

    assert_eq!(designer_view(&world), None);
    assert!(!world.voice.view().is_hearing);
    assert_eq!(world.voice.view().active, VoiceEffect::Cloned);
}

#[test]
fn the_designer_stays_open_when_saving_fails_or_is_refused() {
    let world = designer();
    world.voice.open_designer();
    world.voice.preview_design(recipe("a"));
    *world.repository.fails.lock().unwrap() = true;

    world.voice.save_design("Mix", recipe("a"));
    world.voice.save_design("   ", recipe("a"));

    assert!(designer_view(&world).unwrap().is_previewing);
    assert!(world.voice.view().is_hearing);
    assert!(designs(&world).is_empty());
}

#[test]
fn a_design_whose_base_was_deleted_meanwhile_is_refused() {
    let world = designer();
    world.voice.open_designer();
    world.voice.preview_design(recipe("a"));
    world.voice.delete_clone("a");

    world.voice.save_design("Mix", recipe("a"));

    assert!(world.saw(&error(TextKey::CloneDeleted)));
    assert!(designs(&world).is_empty());
    assert!(world.repository.library.lock().unwrap().designs.is_empty());
    assert!(designer_view(&world).is_some());
}

#[test]
fn a_design_that_blends_a_voice_with_itself_keeps_no_blend() {
    let world = designer();
    world.voice.open_designer();
    let twice = VoiceRecipe {
        blend_id: Some("a".to_owned()),
        blend: 0.5,
        ..recipe("a")
    };

    world.voice.save_design("Mix", twice);

    assert_eq!(world.library.lock().designs[0].recipe.blend_id, None);
}

#[test]
fn a_failed_delete_keeps_what_was_added_before_it() {
    let world = designer();
    world.keep_clone("c", "Cia");
    *world.repository.fails.lock().unwrap() = true;

    world.voice.delete_clone("b");

    assert!(world.saw(&error(TextKey::SaveFailed)));
    assert_eq!(world.library.lock().ids(), ["a", "b", "c"]);
}

#[test]
fn deleting_a_voice_that_is_not_there_saves_nothing() {
    let world = designer();
    *world.repository.fails.lock().unwrap() = true;

    world.voice.delete_clone("nobody");

    assert!(world.events.0.lock().unwrap().is_empty());
}

#[test]
fn deleting_the_clone_a_voice_waits_for_ends_the_wait() {
    let world = VoiceWorld::installed();
    world.keep_clone("a", "Ana");
    world.voice.select_clone("a");
    assert_eq!(world.voice.view().ai.waiting, Some(Message::plain("Ana")));

    world.voice.delete_clone("a");

    assert_eq!(world.voice.view().ai.waiting, None);
}

#[test]
fn deleting_the_clone_named_in_the_settings_keeps_a_waiting_effect_that_is_not_a_clone() {
    let mut voice = VoiceSettings::default();
    voice.select_clone("a");
    voice.select(VoiceEffect::Thin);
    let library = VoiceLibrary {
        clones: vec![cloned_voice("a", "Ana")],
        ..VoiceLibrary::default()
    };
    let world = VoiceWorld::resuming(voice, library, true);
    world.voice.select(VoiceEffect::FeminineAi);
    assert_eq!(
        world.voice.view().ai.waiting,
        Some(Message::text(TextKey::EffectFeminineAi))
    );

    world.voice.delete_clone("a");

    assert_eq!(
        world.voice.view().ai.waiting,
        Some(Message::text(TextKey::EffectFeminineAi))
    );
}

#[tokio::test]
async fn a_waiting_effect_keeps_the_knobs_turned_while_it_waited() {
    let world = VoiceWorld::resuming(VoiceSettings::default(), calibrated(), false);
    world.voice.select(VoiceEffect::FeminineAi);
    world.voice.set_param(VoiceParam::FeminineAiPitch, 1.0);

    world.voice.install_ai().await;

    assert_eq!(world.voice.view().active, VoiceEffect::FeminineAi);
    assert_eq!(pitch_of(&world), 8.0);
}
