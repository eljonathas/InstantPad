mod support;

use std::sync::Arc;

use instantpad_application::cloning::CloningSession;
use instantpad_application::events::{AppEvent, CloneStep};
use instantpad_application::message::{Message, TextKey};
use instantpad_domain::cloning::{
    MAX_PHRASE_SECONDS, MAX_SAMPLE_SECONDS, MIN_SAMPLE_SECONDS, TERMS_VERSION,
};
use instantpad_domain::voice::VoiceEffect;
use instantpad_test_support::fakes::FakeAnalyzer;
use support::{VoiceWorld, speech};

struct Cloning {
    world: VoiceWorld,
    session: Arc<CloningSession>,
}

impl Cloning {
    /// Models installed, the way cloning needs them.
    fn new() -> Self {
        let world = VoiceWorld::ready();
        let session = Arc::new(CloningSession::new(
            world.recorder.clone(),
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

    fn say(&self, words: &str) {
        *self.world.analyzer.transcript.lock().unwrap() = words.to_owned();
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

    /// The digits asked for, read back the way the AI hears them.
    fn digits(&self) -> String {
        match self.step() {
            CloneStep::Phrase { digits, .. } => digits,
            other => panic!("expected the phrase, got {other:?}"),
        }
    }

    fn reads_the_digits(&self) {
        let spaced: Vec<_> = self.digits().chars().map(String::from).collect();
        self.say(&spaced.join(" "));
    }

    async fn speaks_phrase(&self, voice: [f32; 2]) {
        self.session.start_phrase();
        self.records(5);
        self.hears(voice);
        self.session.finish_phrase().await;
    }

    /// Goes through the terms and the sample, arriving at the phrase.
    async fn arrives_at_the_phrase(&self) {
        self.session.open();
        self.session.accept_terms("Ana");
        self.gives_sample(30, [1.0, 0.0]).await;
    }

    async fn arrives_at_the_name(&self) {
        self.arrives_at_the_phrase().await;
        self.reads_the_digits();
        self.speaks_phrase([1.0, 0.0]).await;
    }

    fn problem(&self) -> Option<Message> {
        match self.step() {
            CloneStep::Sample { problem, .. } | CloneStep::Phrase { problem, .. } => problem,
            other => panic!("expected a step with problems, got {other:?}"),
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

#[test]
fn cloning_starts_closed() {
    assert_eq!(Cloning::new().step(), CloneStep::Closed);
}

#[test]
fn opening_without_the_models_is_refused() {
    let cloning = Cloning::new();
    *cloning.world.models.is_installed.lock().unwrap() = false;

    cloning.session.open();

    assert_eq!(cloning.step(), CloneStep::Closed);
    assert!(cloning.world.saw(&error(TextKey::AiNotInstalled)));
}

#[test]
fn opening_shows_every_clause_of_the_terms() {
    let cloning = Cloning::new();

    cloning.session.open();

    let CloneStep::Terms { clauses } = cloning.step() else {
        panic!("expected the terms");
    };
    assert_eq!(clauses.len(), 5);
    assert_eq!(cloning.last_step_told(), Some(cloning.step()));
}

#[test]
fn the_terms_need_the_name_of_the_owner() {
    let cloning = Cloning::new();
    cloning.session.open();

    cloning.session.accept_terms("   ");

    assert!(matches!(cloning.step(), CloneStep::Terms { .. }));
    assert!(cloning.world.saw(&error(TextKey::CloneMissingName)));
}

#[test]
fn accepting_the_terms_moves_to_the_sample() {
    let cloning = Cloning::new();
    cloning.session.open();

    cloning.session.accept_terms("Ana");

    assert_eq!(
        cloning.step(),
        CloneStep::Sample {
            is_recording: false,
            min_seconds: MIN_SAMPLE_SECONDS,
            max_seconds: MAX_SAMPLE_SECONDS,
            problem: None,
        }
    );
}

#[test]
fn nothing_can_be_recorded_before_the_terms_are_accepted() {
    let cloning = Cloning::new();
    cloning.session.open();

    cloning.session.start_sample();
    cloning.session.start_phrase();

    assert!(!cloning.is_recording());
    assert!(matches!(cloning.step(), CloneStep::Terms { .. }));
}

#[test]
fn recording_the_sample_shows_it_is_recording() {
    let cloning = Cloning::new();
    cloning.session.open();
    cloning.session.accept_terms("Ana");

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
    cloning.session.accept_terms("Ana");

    cloning
        .gives_sample(MIN_SAMPLE_SECONDS - 1, [1.0, 0.0])
        .await;

    assert_eq!(
        cloning.problem(),
        Some(Message::text(TextKey::SampleTooShort))
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
    cloning.session.accept_terms("Ana");

    cloning
        .gives_sample(MAX_SAMPLE_SECONDS + 1, [1.0, 0.0])
        .await;

    assert_eq!(
        cloning.problem(),
        Some(Message::text(TextKey::SampleTooLong))
    );
    assert_eq!(cloning.unused_voiceprints(), 1);
}

#[tokio::test]
async fn a_sample_the_ai_cannot_hear_shows_why() {
    let cloning = Cloning::new();
    cloning.session.open();
    cloning.session.accept_terms("Ana");
    cloning.session.start_sample();
    cloning.records(30);

    cloning.session.finish_sample().await;

    assert_eq!(
        cloning.problem(),
        Some(Message::text(TextKey::AiLoadFailed))
    );
}

#[tokio::test]
async fn a_good_sample_moves_to_a_phrase_of_six_digits() {
    let cloning = Cloning::new();
    cloning.session.open();
    cloning.session.accept_terms("Ana");

    cloning.gives_sample(MIN_SAMPLE_SECONDS, [1.0, 0.0]).await;

    let CloneStep::Phrase {
        digits,
        is_recording,
        max_seconds,
        problem,
    } = cloning.step()
    else {
        panic!("expected the phrase");
    };
    assert_eq!(digits.len(), 6);
    assert!(digits.chars().all(|digit| digit.is_ascii_digit()));
    assert!(!is_recording);
    assert_eq!(max_seconds, MAX_PHRASE_SECONDS);
    assert_eq!(problem, None);
}

#[tokio::test]
async fn a_file_of_the_voice_is_as_good_as_a_recording() {
    let cloning = Cloning::new();
    cloning.session.open();
    cloning.session.accept_terms("Ana");
    *cloning.world.recorder.file.lock().unwrap() = Some(speech(40));
    cloning.hears([1.0, 0.0]);

    cloning.session.import_sample("voice.wav").await;

    assert!(matches!(cloning.step(), CloneStep::Phrase { .. }));
}

#[tokio::test]
async fn a_file_that_cannot_be_read_shows_why() {
    let cloning = Cloning::new();
    cloning.session.open();
    cloning.session.accept_terms("Ana");

    cloning.session.import_sample("broken.wav").await;

    assert_eq!(
        cloning.problem(),
        Some(Message::text(TextKey::SoundOpenFailed))
    );
}

#[tokio::test]
async fn a_wrong_transcript_asks_for_new_digits() {
    let cloning = Cloning::new();
    cloning.arrives_at_the_phrase().await;
    let mut asked = vec![cloning.digits()];

    for _ in 0..5 {
        cloning.say("hello there");
        cloning.speaks_phrase([1.0, 0.0]).await;

        assert_eq!(
            cloning.problem(),
            Some(Message::text(TextKey::CloneWrongPhrase))
        );
        assert!(matches!(
            cloning.step(),
            CloneStep::Phrase {
                is_recording: false,
                ..
            }
        ));
        asked.push(cloning.digits());
    }

    asked.sort();
    asked.dedup();
    assert!(asked.len() > 1, "the same digits were asked every time");
}

#[tokio::test]
async fn a_different_speaker_asks_for_new_digits() {
    let cloning = Cloning::new();
    cloning.arrives_at_the_phrase().await;
    cloning.reads_the_digits();

    cloning.speaks_phrase([0.0, 1.0]).await;

    assert_eq!(
        cloning.problem(),
        Some(Message::text(TextKey::CloneNotTheSameSpeaker))
    );
    assert!(matches!(cloning.step(), CloneStep::Phrase { .. }));
}

#[tokio::test]
async fn a_phrase_that_goes_on_too_long_asks_for_new_digits() {
    let cloning = Cloning::new();
    cloning.arrives_at_the_phrase().await;
    cloning.session.start_phrase();
    cloning.records(MAX_PHRASE_SECONDS + 1);
    cloning.hears([1.0, 0.0]);

    cloning.session.finish_phrase().await;

    assert_eq!(
        cloning.problem(),
        Some(Message::text(TextKey::PhraseTooLong))
    );
    assert_eq!(cloning.unused_voiceprints(), 1);
}

#[tokio::test]
async fn a_phrase_the_ai_cannot_hear_asks_for_new_digits() {
    let cloning = Cloning::new();
    cloning.arrives_at_the_phrase().await;
    cloning.session.start_phrase();
    cloning.records(5);

    cloning.session.finish_phrase().await;

    assert_eq!(
        cloning.problem(),
        Some(Message::text(TextKey::AiLoadFailed))
    );
    assert!(matches!(cloning.step(), CloneStep::Phrase { .. }));
}

#[tokio::test]
async fn the_right_phrase_from_the_same_speaker_moves_to_the_name() {
    let cloning = Cloning::new();
    cloning.arrives_at_the_phrase().await;
    cloning.reads_the_digits();

    cloning.speaks_phrase([1.0, 0.0]).await;

    assert_eq!(cloning.step(), CloneStep::Name);
    assert_eq!(cloning.last_step_told(), Some(CloneStep::Name));
}

#[tokio::test]
async fn saving_keeps_the_voice_with_its_consent_and_closes() {
    let cloning = Cloning::new();
    cloning.session.open();
    cloning.session.accept_terms("  Ana Souza ");
    cloning.gives_sample(30, [1.0, 0.0]).await;
    cloning.reads_the_digits();
    cloning.speaks_phrase([1.0, 0.0]).await;

    cloning.session.save("  My voice ");

    assert_eq!(cloning.step(), CloneStep::Closed);
    let kept = cloning.world.repository.library.lock().unwrap().clone();
    assert_eq!(kept.clones.len(), 1);
    let voice = &kept.clones[0];
    assert_eq!(voice.name, "My voice");
    assert_eq!(voice.voiceprint, FakeAnalyzer::voice([1.0, 0.0]));
    assert!(!voice.id.is_empty());
    assert_eq!(voice.consent.owner_name, "Ana Souza");
    assert_eq!(voice.consent.terms_version, TERMS_VERSION);
    assert_eq!(voice.consent.sample_sha256, "1440000 samples");
    assert!((voice.consent.similarity - 1.0).abs() < 1e-6);
    assert!(voice.consent.accepted_at > 0);
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
async fn nothing_is_saved_before_the_proof_of_the_owner() {
    let cloning = Cloning::new();
    cloning.arrives_at_the_phrase().await;

    cloning.session.save("My voice");

    assert!(matches!(cloning.step(), CloneStep::Phrase { .. }));
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
async fn closing_at_the_terms_forgets_everything() {
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
    cloning.session.accept_terms("Ana");
    cloning.session.start_sample();
    assert!(cloning.is_recording());

    cloning.session.close();

    assert!(!cloning.is_recording());
    assert_eq!(cloning.step(), CloneStep::Closed);
}

#[tokio::test]
async fn closing_while_recording_the_phrase_stops_the_recording() {
    let cloning = Cloning::new();
    cloning.arrives_at_the_phrase().await;
    cloning.session.start_phrase();
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
async fn opening_again_starts_over_from_the_terms() {
    let cloning = Cloning::new();
    cloning.arrives_at_the_name().await;
    cloning.session.close();

    cloning.session.open();
    cloning.session.save("My voice");

    assert!(matches!(cloning.step(), CloneStep::Terms { .. }));
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
    cloning.session.accept_terms("Ana");
    cloning.session.start_sample();
    cloning.records(30);
    cloning.hears([1.0, 0.0]);
    cloning.session.close();

    cloning.session.finish_sample().await;

    assert_eq!(cloning.step(), CloneStep::Closed);
    assert_eq!(cloning.unused_voiceprints(), 1);
}

#[tokio::test]
async fn finishing_a_phrase_after_closing_does_nothing() {
    let cloning = Cloning::new();
    cloning.arrives_at_the_phrase().await;
    cloning.session.start_phrase();
    cloning.session.close();
    cloning.records(5);
    cloning.hears([1.0, 0.0]);

    cloning.session.finish_phrase().await;

    assert_eq!(cloning.step(), CloneStep::Closed);
    assert_eq!(cloning.unused_voiceprints(), 1);
}

#[tokio::test]
async fn closing_while_the_ai_listens_to_the_sample_drops_the_result() {
    let cloning = Cloning::new();
    cloning.session.open();
    cloning.session.accept_terms("Ana");
    let session = cloning.session.clone();
    *cloning.world.analyzer.meanwhile.lock().unwrap() = Some(Box::new(move || session.close()));

    cloning.gives_sample(30, [1.0, 0.0]).await;

    assert_eq!(cloning.step(), CloneStep::Closed);
    assert_eq!(cloning.last_step_told(), Some(CloneStep::Closed));
}

#[tokio::test]
async fn closing_while_the_ai_listens_to_the_phrase_drops_the_result() {
    let cloning = Cloning::new();
    cloning.arrives_at_the_phrase().await;
    cloning.reads_the_digits();
    let session = cloning.session.clone();
    *cloning.world.analyzer.meanwhile.lock().unwrap() = Some(Box::new(move || session.close()));

    cloning.speaks_phrase([1.0, 0.0]).await;

    assert_eq!(cloning.step(), CloneStep::Closed);
    cloning.session.save("My voice");
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

#[test]
fn accepting_the_terms_outside_the_terms_shows_no_error() {
    let cloning = Cloning::new();

    cloning.session.accept_terms("");

    assert_eq!(cloning.step(), CloneStep::Closed);
    assert!(cloning.world.events.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn closing_while_a_file_is_read_analyzes_nothing() {
    let cloning = Cloning::new();
    cloning.session.open();
    cloning.session.accept_terms("Ana");
    *cloning.world.recorder.file.lock().unwrap() = Some(speech(40));
    cloning.hears([1.0, 0.0]);
    let session = cloning.session.clone();
    *cloning.world.recorder.meanwhile.lock().unwrap() = Some(Box::new(move || session.close()));

    cloning.session.import_sample("voice.wav").await;

    assert_eq!(cloning.step(), CloneStep::Closed);
    assert_eq!(cloning.last_step_told(), Some(CloneStep::Closed));
    assert_eq!(cloning.unused_voiceprints(), 1);
}
