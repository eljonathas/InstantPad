mod support;

use std::sync::Arc;

use instantpad_application::cloning::CloningSession;
use instantpad_application::events::{AppEvent, Calibration, CloneStep};
use instantpad_application::message::{Message, TextKey};
use instantpad_domain::cloning::{MAX_SAMPLE_SECONDS, MIN_SAMPLE_SECONDS};
use instantpad_domain::voice::VoiceEffect;
use instantpad_test_support::fakes::FakeAnalyzer;
use support::{VoiceWorld, poll_once, speech};

struct Cloning {
    world: VoiceWorld,
    session: Arc<CloningSession>,
}

impl Cloning {
    /// Models installed, the way cloning needs them.
    fn new() -> Self {
        Self::in_world(VoiceWorld::ready())
    }

    /// Models installed, but the AI does not know the voice of the user yet.
    fn learning() -> Self {
        Self::in_world(VoiceWorld::installed())
    }

    fn in_world(world: VoiceWorld) -> Self {
        let session = Arc::new(CloningSession::new(
            world.microphone.clone(),
            world.analyzer.clone(),
            world.models.clone(),
            world.library.clone(),
            world.voice.clone(),
            world.events.clone(),
        ));
        Self { world, session }
    }

    fn step(&self) -> CloneStep {
        self.session.step()
    }

    fn hears(&self, values: [f32; 2]) {
        let voiceprint = FakeAnalyzer::voice(values);
        self.world
            .analyzer
            .voiceprints
            .lock()
            .unwrap()
            .push(voiceprint);
    }

    fn records(&self, seconds: u32) {
        *self.world.recorder.next.lock().unwrap() = speech(seconds);
    }

    fn is_recording(&self) -> bool {
        *self.world.recorder.is_recording.lock().unwrap()
    }

    fn unused_voiceprints(&self) -> usize {
        self.world.analyzer.voiceprints.lock().unwrap().len()
    }

    async fn gives_sample(&self, seconds: u32, voice: [f32; 2]) {
        self.session.start_sample();
        self.records(seconds);
        self.hears(voice);
        self.session.finish_sample().await;
    }

    /// Opens the dialog and gives a good sample, which leaves the name to ask for.
    async fn arrives_at_the_name(&self) {
        self.session.open();
        self.gives_sample(30, [1.0, 0.0]).await;
    }

    fn problem(&self) -> Option<Message> {
        match self.step() {
            CloneStep::Sample { problem, .. } => problem,
            other => panic!("expected the sample, got {other:?}"),
        }
    }

    fn last_step_told(&self) -> Option<CloneStep> {
        self.world
            .events
            .0
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find_map(|event| match event {
                AppEvent::Cloning(step) => Some(step.clone()),
                _ => None,
            })
    }
}

fn error(key: TextKey) -> AppEvent {
    AppEvent::error(Message::text(key))
}

/// A recording that broke a limit, with the seconds the limit allows.
fn limit(key: TextKey, seconds: u32) -> Message {
    Message::with(key, [Message::plain(seconds.to_string())])
}

fn cannot_hear() -> Message {
    Message::with(
        TextKey::AiLoadFailed,
        [Message::plain("no voiceprint prepared")],
    )
}

#[test]
fn cloning_starts_closed() {
    assert_eq!(Cloning::new().step(), CloneStep::Closed);
}

#[test]
fn opening_without_the_models_is_refused_and_waits_for_the_install() {
    let cloning = Cloning::new();
    *cloning.world.models.is_installed.lock().unwrap() = false;

    cloning.session.open();

    assert_eq!(cloning.step(), CloneStep::Closed);
    assert!(cloning.world.saw(&error(TextKey::AiNotInstalled)));
    assert_eq!(
        cloning.world.voice.view().ai.waiting,
        Some(Message::text(TextKey::CloneVoice))
    );
}

#[tokio::test]
async fn the_install_ends_the_wait_but_the_user_opens_the_dialog_again() {
    let cloning = Cloning::new();
    *cloning.world.models.is_installed.lock().unwrap() = false;
    cloning.session.open();

    cloning.world.voice.install_ai().await;

    assert_eq!(cloning.world.voice.view().ai.waiting, None);
    assert_eq!(cloning.step(), CloneStep::Closed);
    cloning.session.open();
    assert!(matches!(cloning.step(), CloneStep::Sample { .. }));
}

#[test]
fn opening_asks_for_the_sample_with_nothing_in_between() {
    let cloning = Cloning::new();

    cloning.session.open();

    assert_eq!(
        cloning.step(),
        CloneStep::Sample {
            is_recording: false,
            min_seconds: MIN_SAMPLE_SECONDS,
            max_seconds: MAX_SAMPLE_SECONDS,
            problem: None,
        }
    );
    assert_eq!(cloning.last_step_told(), Some(cloning.step()));
}

#[test]
fn nothing_can_be_recorded_while_the_dialog_is_closed() {
    let cloning = Cloning::new();

    cloning.session.start_sample();

    assert!(!cloning.is_recording());
    assert_eq!(cloning.step(), CloneStep::Closed);
}

#[test]
fn recording_the_sample_shows_it_is_recording() {
    let cloning = Cloning::new();
    cloning.session.open();

    cloning.session.start_sample();

    assert!(cloning.is_recording());
    assert!(matches!(
        cloning.step(),
        CloneStep::Sample {
            is_recording: true,
            ..
        }
    ));
}

#[tokio::test]
async fn a_sample_that_is_too_short_is_refused_and_nothing_is_analyzed() {
    let cloning = Cloning::new();
    cloning.session.open();

    cloning
        .gives_sample(MIN_SAMPLE_SECONDS - 1, [1.0, 0.0])
        .await;

    assert_eq!(
        cloning.problem(),
        Some(limit(TextKey::SampleTooShort, MIN_SAMPLE_SECONDS))
    );
    assert!(matches!(
        cloning.step(),
        CloneStep::Sample {
            is_recording: false,
            ..
        }
    ));
    assert_eq!(cloning.unused_voiceprints(), 1);
}

#[tokio::test]
async fn a_sample_that_is_too_long_is_refused() {
    let cloning = Cloning::new();
    cloning.session.open();

    cloning
        .gives_sample(MAX_SAMPLE_SECONDS + 1, [1.0, 0.0])
        .await;

    assert_eq!(
        cloning.problem(),
        Some(limit(TextKey::SampleTooLong, MAX_SAMPLE_SECONDS))
    );
    assert_eq!(cloning.unused_voiceprints(), 1);
}

#[tokio::test]
async fn a_sample_the_ai_cannot_hear_shows_why() {
    let cloning = Cloning::new();
    cloning.session.open();
    cloning.session.start_sample();
    cloning.records(30);

    cloning.session.finish_sample().await;

    assert_eq!(cloning.problem(), Some(cannot_hear()));
}

#[tokio::test]
async fn a_good_sample_moves_straight_to_the_name() {
    let cloning = Cloning::new();
    cloning.session.open();

    cloning.gives_sample(MIN_SAMPLE_SECONDS, [1.0, 0.0]).await;

    assert_eq!(cloning.step(), CloneStep::Name);
    assert_eq!(cloning.last_step_told(), Some(CloneStep::Name));
}

#[tokio::test]
async fn a_file_of_the_voice_is_as_good_as_a_recording() {
    let cloning = Cloning::new();
    cloning.session.open();
    *cloning.world.recorder.file.lock().unwrap() = Some(speech(40));
    cloning.hears([1.0, 0.0]);

    cloning.session.import_sample("voice.wav").await;

    assert_eq!(cloning.step(), CloneStep::Name);
}

#[tokio::test]
async fn a_file_that_cannot_be_read_shows_why() {
    let cloning = Cloning::new();
    cloning.session.open();

    cloning.session.import_sample("broken.wav").await;

    assert_eq!(
        cloning.problem(),
        Some(Message::text(TextKey::SoundOpenFailed))
    );
}

#[tokio::test]
async fn saving_keeps_the_voice_and_closes() {
    let cloning = Cloning::new();
    cloning.session.open();
    cloning.gives_sample(30, [1.0, 0.0]).await;

    cloning.session.save("  My voice ");

    assert_eq!(cloning.step(), CloneStep::Closed);
    let kept = cloning.world.repository.library.lock().unwrap().clone();
    assert_eq!(kept.clones.len(), 1);
    let voice = &kept.clones[0];
    assert_eq!(voice.name, "My voice");
    assert_eq!(voice.voiceprint, FakeAnalyzer::voice([1.0, 0.0]));
    assert!(!voice.id.is_empty());
    assert!(
        cloning
            .world
            .saw(&AppEvent::show(Message::text(TextKey::CloneSaved)))
    );
}

#[tokio::test]
async fn a_saved_voice_shows_among_the_effects_and_can_speak() {
    let cloning = Cloning::new();
    cloning.arrives_at_the_name().await;

    cloning.session.save("My voice");

    let view = cloning.world.voice.view();
    let clone = view.effects.last().unwrap();
    assert_eq!(clone.effect, VoiceEffect::Cloned);
    assert_eq!(clone.name, Message::plain("My voice"));
    let id = clone.clone_id.clone().unwrap();
    cloning.world.voice.select_clone(&id);
    assert_eq!(cloning.world.voice.view().active_clone_id, Some(id));
}

#[tokio::test]
async fn a_voice_without_a_name_is_not_saved() {
    let cloning = Cloning::new();
    cloning.arrives_at_the_name().await;

    cloning.session.save("   ");

    assert_eq!(cloning.step(), CloneStep::Name);
    assert!(cloning.world.saw(&error(TextKey::CloneMissingName)));
    assert!(
        cloning
            .world
            .repository
            .library
            .lock()
            .unwrap()
            .clones
            .is_empty()
    );
}

#[tokio::test]
async fn closing_at_the_sample_forgets_everything() {
    let cloning = Cloning::new();
    cloning.session.open();

    cloning.session.close();

    assert_eq!(cloning.step(), CloneStep::Closed);
    assert_eq!(cloning.last_step_told(), Some(CloneStep::Closed));
}

#[tokio::test]
async fn closing_while_recording_stops_the_recording() {
    let cloning = Cloning::new();
    cloning.session.open();
    cloning.session.start_sample();
    assert!(cloning.is_recording());

    cloning.session.close();

    assert!(!cloning.is_recording());
    assert_eq!(cloning.step(), CloneStep::Closed);
}

#[tokio::test]
async fn closing_at_the_name_leaves_nothing_to_save() {
    let cloning = Cloning::new();
    cloning.arrives_at_the_name().await;

    cloning.session.close();
    cloning.session.save("My voice");

    assert_eq!(cloning.step(), CloneStep::Closed);
    assert!(
        cloning
            .world
            .repository
            .library
            .lock()
            .unwrap()
            .clones
            .is_empty()
    );
}

#[tokio::test]
async fn opening_again_starts_over_from_the_sample() {
    let cloning = Cloning::new();
    cloning.arrives_at_the_name().await;
    cloning.session.close();

    cloning.session.open();
    cloning.session.save("My voice");

    assert!(matches!(cloning.step(), CloneStep::Sample { .. }));
    assert!(
        cloning
            .world
            .repository
            .library
            .lock()
            .unwrap()
            .clones
            .is_empty()
    );
}

#[tokio::test]
async fn finishing_a_recording_after_closing_does_nothing() {
    let cloning = Cloning::new();
    cloning.session.open();
    cloning.session.start_sample();
    cloning.records(30);
    cloning.hears([1.0, 0.0]);
    cloning.session.close();

    cloning.session.finish_sample().await;

    assert_eq!(cloning.step(), CloneStep::Closed);
    assert_eq!(cloning.unused_voiceprints(), 1);
}

#[tokio::test]
async fn closing_while_the_ai_listens_to_the_sample_drops_the_result() {
    let cloning = Cloning::new();
    cloning.session.open();
    let session = cloning.session.clone();
    *cloning.world.analyzer.meanwhile.lock().unwrap() = Some(Box::new(move || session.close()));

    cloning.gives_sample(30, [1.0, 0.0]).await;

    assert_eq!(cloning.step(), CloneStep::Closed);
    assert_eq!(cloning.last_step_told(), Some(CloneStep::Closed));
}

#[tokio::test]
async fn a_voice_that_could_not_be_saved_is_not_kept_in_memory_and_can_be_saved_again() {
    let cloning = Cloning::new();
    cloning.arrives_at_the_name().await;
    *cloning.world.repository.fails.lock().unwrap() = true;

    cloning.session.save("My voice");

    assert!(cloning.world.saw(&error(TextKey::SaveFailed)));
    assert_eq!(cloning.step(), CloneStep::Name);
    assert!(cloning.world.library.lock().clones.is_empty());

    *cloning.world.repository.fails.lock().unwrap() = false;
    cloning.session.save("My voice");

    assert_eq!(cloning.step(), CloneStep::Closed);
    assert_eq!(cloning.world.library.lock().clones.len(), 1);
    assert_eq!(
        cloning
            .world
            .repository
            .library
            .lock()
            .unwrap()
            .clones
            .len(),
        1
    );
}

#[tokio::test]
async fn closing_while_a_file_is_read_analyzes_nothing() {
    let cloning = Cloning::new();
    cloning.session.open();
    *cloning.world.recorder.file.lock().unwrap() = Some(speech(40));
    cloning.hears([1.0, 0.0]);
    let session = cloning.session.clone();
    *cloning.world.recorder.meanwhile.lock().unwrap() = Some(Box::new(move || session.close()));

    cloning.session.import_sample("voice.wav").await;

    assert_eq!(cloning.step(), CloneStep::Closed);
    assert_eq!(cloning.last_step_told(), Some(CloneStep::Closed));
    assert_eq!(cloning.unused_voiceprints(), 1);
}

#[tokio::test]
async fn a_second_stop_of_the_sample_does_nothing() {
    let cloning = Cloning::new();
    cloning.session.open();
    let session = cloning.session.clone();
    *cloning.world.analyzer.meanwhile.lock().unwrap() = Some(Box::new(move || {
        assert_eq!(session.step(), CloneStep::Analyzing);
        assert!(poll_once(session.finish_sample()).is_some());
        assert_eq!(session.step(), CloneStep::Analyzing);
    }));

    cloning.gives_sample(30, [1.0, 0.0]).await;

    assert_eq!(cloning.step(), CloneStep::Name);
}

#[tokio::test]
async fn importing_a_file_twice_at_once_reads_only_the_first() {
    let cloning = Cloning::new();
    cloning.session.open();
    *cloning.world.recorder.file.lock().unwrap() = Some(speech(40));
    cloning.hears([1.0, 0.0]);
    let session = cloning.session.clone();
    *cloning.world.recorder.meanwhile.lock().unwrap() = Some(Box::new(move || {
        assert_eq!(session.step(), CloneStep::Analyzing);
        assert!(poll_once(session.import_sample("again.wav")).is_some());
    }));

    cloning.session.import_sample("voice.wav").await;

    assert_eq!(cloning.step(), CloneStep::Name);
    assert_eq!(cloning.unused_voiceprints(), 0);
}

#[tokio::test]
async fn closing_and_opening_again_while_a_file_is_read_drops_the_old_result() {
    let cloning = Cloning::new();
    cloning.session.open();
    *cloning.world.recorder.file.lock().unwrap() = Some(speech(40));
    cloning.hears([1.0, 0.0]);
    let session = cloning.session.clone();
    *cloning.world.recorder.meanwhile.lock().unwrap() = Some(Box::new(move || {
        session.close();
        session.open();
    }));

    cloning.session.import_sample("voice.wav").await;

    assert!(matches!(
        cloning.step(),
        CloneStep::Sample {
            is_recording: false,
            ..
        }
    ));
    assert_eq!(cloning.last_step_told(), Some(cloning.step()));
    assert_eq!(cloning.unused_voiceprints(), 1);
}

#[tokio::test]
async fn closing_and_opening_again_while_a_file_fails_to_read_stays_on_the_sample() {
    let cloning = Cloning::new();
    cloning.session.open();
    let session = cloning.session.clone();
    *cloning.world.recorder.meanwhile.lock().unwrap() = Some(Box::new(move || {
        session.close();
        session.open();
    }));

    cloning.session.import_sample("broken.wav").await;

    assert!(matches!(
        cloning.step(),
        CloneStep::Sample {
            is_recording: false,
            ..
        }
    ));
}

#[tokio::test]
async fn closing_and_opening_again_while_the_sample_is_analyzed_stays_on_the_sample() {
    let cloning = Cloning::new();
    cloning.session.open();
    let session = cloning.session.clone();
    *cloning.world.analyzer.meanwhile.lock().unwrap() = Some(Box::new(move || {
        session.close();
        session.open();
    }));

    cloning.gives_sample(30, [1.0, 0.0]).await;

    assert!(matches!(
        cloning.step(),
        CloneStep::Sample {
            is_recording: false,
            ..
        }
    ));
}

#[tokio::test]
async fn a_voice_that_could_not_be_saved_leaves_the_other_voices_alone() {
    let cloning = Cloning::new();
    cloning.world.keep_clone("earlier", "Earlier");
    cloning.arrives_at_the_name().await;
    *cloning.world.repository.fails.lock().unwrap() = true;

    cloning.session.save("My voice");

    assert_eq!(cloning.world.library.lock().ids(), ["earlier"]);
}

#[test]
fn the_ai_stops_listening_to_the_user_while_the_dialog_is_open() {
    let cloning = Cloning::learning();
    cloning.world.voice.select(VoiceEffect::FeminineAi);
    assert_eq!(
        cloning.world.voice.view().ai.calibration,
        Calibration::Listening
    );

    cloning.session.open();

    assert!(!cloning.is_recording());
    assert_eq!(cloning.world.voice.view().ai.calibration, Calibration::Idle);
    // The dialog can record at once, because the microphone is free.
    cloning.session.start_sample();
    assert!(cloning.is_recording());
    assert!(matches!(
        cloning.step(),
        CloneStep::Sample {
            is_recording: true,
            ..
        }
    ));
    assert!(!cloning.world.saw(&error(TextKey::MicrophoneBusy)));
}

#[tokio::test]
async fn the_ai_does_not_start_listening_again_while_the_dialog_is_open() {
    let cloning = Cloning::learning();
    cloning.world.voice.select(VoiceEffect::FeminineAi);
    cloning.session.open();

    cloning.world.voice.learn().await;

    assert!(!cloning.is_recording());
    assert_eq!(cloning.world.voice.view().ai.calibration, Calibration::Idle);
}

#[tokio::test]
async fn the_ai_goes_back_to_listening_when_the_dialog_closes() {
    let cloning = Cloning::learning();
    cloning.world.voice.select(VoiceEffect::FeminineAi);
    cloning.session.open();
    cloning.session.close();

    cloning.world.voice.learn().await;

    assert!(cloning.is_recording());
    assert_eq!(
        cloning.world.voice.view().ai.calibration,
        Calibration::Listening
    );
}

#[tokio::test]
async fn saving_a_voice_gives_the_microphone_back_to_the_ai() {
    let cloning = Cloning::learning();
    cloning.world.voice.select(VoiceEffect::FeminineAi);
    cloning.arrives_at_the_name().await;

    cloning.session.save("My voice");
    cloning.world.voice.learn().await;

    assert_eq!(cloning.step(), CloneStep::Closed);
    assert_eq!(
        cloning.world.voice.view().ai.calibration,
        Calibration::Listening
    );
}

#[tokio::test]
async fn the_ai_waits_for_a_recording_of_the_dialog_that_is_still_going_on() {
    let cloning = Cloning::learning();
    cloning.session.open();
    cloning.session.start_sample();
    cloning.world.voice.select(VoiceEffect::FeminineAi);

    cloning.world.voice.learn().await;

    // The sample is still being recorded, and nothing of it is lost to the AI.
    assert!(matches!(
        cloning.step(),
        CloneStep::Sample {
            is_recording: true,
            ..
        }
    ));
    assert_eq!(cloning.world.voice.view().ai.calibration, Calibration::Idle);
}
