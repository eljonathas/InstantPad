//! Training a model of a cloned voice inside the app, from the download to the model in the voice.

mod support;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, Weak};

use instantpad_application::events::{AppEvent, TrainStep, TrainingPhase};
use instantpad_application::message::{Failure, Message, TextKey};
use instantpad_application::ports::{TrainingProgress, TrainingStage};
use instantpad_application::training::TrainingSession;
use instantpad_domain::training::{
    GraphicsCard, MAX_TAKE_SECONDS, MIN_TRAINING_SECONDS, ProcessorReason, TrainingDevice,
    TrainingJob, TrainingPlan,
};
use instantpad_domain::voice::{VoiceEngine, VoiceTarget};
use instantpad_test_support::fakes::{FakeTrainer, InMemoryTraining};
use support::{VoiceWorld, poll_once, speech};

const ANA: &str = "ana";

struct TrainingWorld {
    voices: VoiceWorld,
    trainer: Arc<FakeTrainer>,
    repository: Arc<InMemoryTraining>,
    training: Arc<TrainingSession>,
}

impl TrainingWorld {
    /// A cloned voice, Ana, with the AI models installed and no training tools yet.
    fn new() -> Self {
        Self::with(InMemoryTraining::default(), true)
    }

    /// The app opening with what an earlier run saved, Ana's voice there or not.
    fn with(repository: InMemoryTraining, keeps_ana: bool) -> Self {
        let voices = VoiceWorld::ready();
        if keeps_ana {
            voices.keep_clone(ANA, "Ana");
        }
        let trainer = Arc::new(FakeTrainer::default());
        let repository = Arc::new(repository);
        let training = Arc::new(TrainingSession::new(
            trainer.clone(),
            repository.clone(),
            voices.microphone.clone(),
            voices.models.clone(),
            voices.library.clone(),
            voices.voice.clone(),
            voices.events.clone(),
        ));
        Self {
            voices,
            trainer,
            repository,
            training,
        }
    }

    fn with_card(self) -> Self {
        *self.trainer.card.lock().unwrap() = Some(card(8192));
        self
    }

    fn open(&self) {
        poll_once(self.training.open(ANA)).expect("the fakes answer at once");
    }

    fn accept(&self) {
        poll_once(self.training.accept()).expect("the fakes answer at once");
    }

    /// Opens the training, agrees to it and installs the tools.
    fn ready_for_takes(self) -> Self {
        self.open();
        self.accept();
        self
    }

    fn record(&self, seconds: u32) {
        self.training.start_take();
        *self.voices.recorder.next.lock().unwrap() = speech(seconds);
        poll_once(self.training.finish_take()).expect("the fakes answer at once");
    }

    /// Takes enough speech to train.
    fn with_enough_speech(self) -> Self {
        let mut left = MIN_TRAINING_SECONDS;
        while left > 0 {
            let take = left.min(MAX_TAKE_SECONDS);
            self.record(take);
            left -= take;
        }
        self
    }

    fn train(&self) {
        poll_once(self.training.train()).expect("the fakes answer at once");
    }

    fn step(&self) -> TrainStep {
        self.training.step()
    }

    fn job(&self) -> Option<TrainingJob> {
        self.repository.job.lock().unwrap().clone()
    }

    fn phase(&self) -> Option<TrainingPhase> {
        self.voices.voice.view().training.job.map(|job| job.phase)
    }

    /// The user does `action` while the trainer works, with the session it acts on.
    fn meanwhile(&self, action: impl Fn(&TrainingSession) + Send + Sync + 'static) {
        let session: Weak<TrainingSession> = Arc::downgrade(&self.training);
        *self.trainer.meanwhile.lock().unwrap() = Some(Box::new(move || {
            if let Some(session) = session.upgrade() {
                action(&session);
            }
        }));
    }

    fn saw(&self, event: &AppEvent) -> bool {
        self.voices.saw(event)
    }
}

fn card(memory_megabytes: u32) -> GraphicsCard {
    GraphicsCard {
        name: "NVIDIA GeForce RTX 3070 Ti".to_owned(),
        memory_megabytes,
        driver: "610.88".to_owned(),
    }
}

fn error(key: TextKey) -> AppEvent {
    AppEvent::error(Message::text(key))
}

#[test]
fn opening_shows_the_card_the_download_and_the_room_it_needs() {
    let world = TrainingWorld::new().with_card();

    world.open();

    let TrainStep::Prepare {
        voice,
        device,
        card,
        download_megabytes,
        missing_megabytes,
        ..
    } = world.step()
    else {
        panic!("expected the preparation, got {:?}", world.step());
    };
    assert_eq!(voice, "Ana");
    assert_eq!(device, TrainingDevice::Card);
    assert_eq!(card.as_deref(), Some("NVIDIA GeForce RTX 3070 Ti"));
    assert_eq!(download_megabytes, FakeTrainer::CARD_DOWNLOAD);
    assert_eq!(missing_megabytes, None);
}

#[test]
fn without_an_nvidia_card_the_preparation_says_it_trains_on_the_processor_for_more_than_a_day() {
    let world = TrainingWorld::new();

    world.open();

    let TrainStep::Prepare {
        device,
        download_megabytes,
        estimated_seconds,
        ..
    } = world.step()
    else {
        panic!("expected the preparation");
    };
    assert_eq!(
        device,
        TrainingDevice::Processor {
            reason: ProcessorReason::NoCard
        }
    );
    assert_eq!(download_megabytes, FakeTrainer::PROCESSOR_DOWNLOAD);
    assert!(estimated_seconds > 16 * 3_600);
}

#[test]
fn a_disk_short_of_room_is_told_and_nothing_downloads() {
    let world = TrainingWorld::new().with_card();
    *world.trainer.free_megabytes.lock().unwrap() = Some(2_000);
    world.open();

    let TrainStep::Prepare {
        missing_megabytes: Some(missing),
        needed_megabytes,
        ..
    } = world.step()
    else {
        panic!("expected the missing room");
    };
    assert_eq!(missing, needed_megabytes - 2_000);

    world.accept();

    assert_eq!(*world.trainer.tools.lock().unwrap(), None);
    assert_eq!(world.job(), None);
}

#[test]
fn agreeing_installs_the_tools_for_the_device_then_asks_for_speech() {
    let world = TrainingWorld::new().with_card().ready_for_takes();

    assert_eq!(*world.trainer.tools.lock().unwrap(), Some(TrainingDevice::Card));
    assert!(matches!(world.step(), TrainStep::Takes { seconds: 0, .. }));
    let job = world.job().expect("the training is saved");
    assert_eq!(job.clone_id, ANA);
    assert_eq!(job.plan.device, TrainingDevice::Card);
}

#[test]
fn the_install_progress_shows_in_the_dialog_and_on_the_voice_screen() {
    let world = TrainingWorld::new().with_card();
    world.open();

    world.accept();

    assert!(world.saw(&AppEvent::Training(TrainStep::Installing {
        voice: "Ana".to_owned(),
        percent: 40,
    })));
}

#[test]
fn a_failed_install_says_why_and_can_be_tried_again() {
    let world = TrainingWorld::new().with_card();
    *world.trainer.install_fails.lock().unwrap() =
        Some(Failure::because(TextKey::AiDownloadFailed, "offline"));
    world.open();

    world.accept();

    let TrainStep::Prepare {
        problem: Some(problem),
        ..
    } = world.step()
    else {
        panic!("expected the preparation with the failure");
    };
    let Message::Text { key, .. } = problem else {
        panic!("expected a translated message");
    };
    assert_eq!(key, TextKey::TrainingInstallFailed);

    world.accept();
    assert!(matches!(world.step(), TrainStep::Takes { .. }));
}

#[test]
fn stopping_the_download_goes_back_to_the_preparation() {
    let world = TrainingWorld::new().with_card();
    world.open();
    world.meanwhile(|training| training.stop());

    world.accept();

    assert!(matches!(world.step(), TrainStep::Prepare { .. }));
    assert_eq!(*world.trainer.tools.lock().unwrap(), None);
}

#[test]
fn with_the_tools_already_there_agreeing_goes_straight_to_the_speech() {
    let world = TrainingWorld::new().with_card();
    *world.trainer.tools.lock().unwrap() = Some(TrainingDevice::Card);
    world.open();

    let TrainStep::Prepare {
        download_megabytes, ..
    } = world.step()
    else {
        panic!("expected the preparation");
    };
    assert_eq!(download_megabytes, 0);
    world.accept();

    assert!(matches!(world.step(), TrainStep::Takes { .. }));
}

#[test]
fn takes_are_written_to_the_training_and_add_up() {
    let world = TrainingWorld::new().with_card().ready_for_takes();

    world.record(120);
    world.record(60);

    let TrainStep::Takes { takes, seconds, .. } = world.step() else {
        panic!("expected the takes");
    };
    assert_eq!(seconds, 180);
    assert_eq!(takes.len(), 2);
    let job = world.job().unwrap();
    let written = world.trainer.takes.lock().unwrap().clone();
    assert_eq!(written.len(), 2);
    assert_eq!(written[0], (job.id.clone(), takes[0].id.clone(), 120 * 48_000));
}

#[test]
fn recording_takes_the_microphone_from_the_ai_and_gives_it_back() {
    let world = TrainingWorld::new().with_card().ready_for_takes();

    world.training.start_take();
    assert!(world.voices.is_listening());
    assert!(matches!(
        world.step(),
        TrainStep::Takes {
            is_recording: true,
            ..
        }
    ));

    *world.voices.recorder.next.lock().unwrap() = speech(30);
    poll_once(world.training.finish_take()).unwrap();
    assert!(!world.voices.is_listening());
}

#[test]
fn a_take_longer_than_the_longest_is_cut() {
    let world = TrainingWorld::new().with_card().ready_for_takes();

    world.record(MAX_TAKE_SECONDS + 120);

    let written = world.trainer.takes.lock().unwrap().clone();
    assert_eq!(written[0].2, MAX_TAKE_SECONDS as usize * 48_000);
    assert!(matches!(
        world.step(),
        TrainStep::Takes {
            problem: Some(_),
            ..
        }
    ));
}

#[test]
fn a_take_of_a_few_seconds_is_refused() {
    let world = TrainingWorld::new().with_card().ready_for_takes();

    world.record(2);

    assert!(world.trainer.takes.lock().unwrap().is_empty());
    let TrainStep::Takes {
        problem: Some(problem),
        ..
    } = world.step()
    else {
        panic!("expected the takes with a problem");
    };
    assert_eq!(problem, Message::text(TextKey::TakeTooShort));
}

#[test]
fn a_file_the_user_picks_is_a_take_too() {
    let world = TrainingWorld::new().with_card().ready_for_takes();
    *world.voices.recorder.file.lock().unwrap() = Some(speech(300));

    poll_once(world.training.import_take("ana.mp3")).unwrap();

    let TrainStep::Takes { seconds, .. } = world.step() else {
        panic!("expected the takes");
    };
    assert_eq!(seconds, 300);
}

#[test]
fn a_take_removed_is_deleted_from_the_training() {
    let world = TrainingWorld::new().with_card().ready_for_takes();
    world.record(60);
    let take = world.job().unwrap().takes[0].clone();

    world.training.remove_take(&take.id);

    assert!(world.job().unwrap().takes.is_empty());
    let removed = world.trainer.removed_takes.lock().unwrap().clone();
    assert_eq!(removed, [(world.job().unwrap().id, take.id)]);
}

#[test]
fn the_training_does_not_start_without_enough_speech() {
    let world = TrainingWorld::new().with_card().ready_for_takes();
    world.record(MIN_TRAINING_SECONDS / 2);

    world.train();

    assert!(world.trainer.runs.lock().unwrap().is_empty());
}

#[test]
fn a_finished_training_puts_the_model_in_the_voice_and_deletes_the_recordings() {
    let world = TrainingWorld::new().with_card().ready_for_takes().with_enough_speech();
    let id = world.job().unwrap().id;

    world.train();

    let run = world.trainer.runs.lock().unwrap()[0].clone();
    assert_eq!(run.job, id);
    assert_eq!(run.title, "Ana");
    let added = world.voices.trained.added.lock().unwrap().clone();
    assert_eq!(added[0].0, PathBuf::from("Ana"));
    assert!(world.voices.library.lock().trained.contains_key(ANA));
    let target = VoiceTarget::Own(ANA.to_owned());
    assert_eq!(
        world.voices.settings.lock().voice.engine(&target, true),
        VoiceEngine::Trained
    );
    assert_eq!(*world.trainer.discarded.lock().unwrap(), [id]);
    assert_eq!(world.job(), None);
    assert_eq!(world.step(), TrainStep::Closed);
    assert!(world.saw(&AppEvent::show(Message::with(
        TextKey::TrainingDone,
        [Message::plain("Ana")]
    ))));
}

#[test]
fn epochs_move_the_progress_and_tell_what_is_left() {
    let world = TrainingWorld::new().with_card().ready_for_takes().with_enough_speech();
    *world.trainer.progress.lock().unwrap() = vec![
        TrainingProgress::Stage(TrainingStage::Pitch),
        TrainingProgress::Epoch {
            epoch: 1,
            seconds: 20.0,
        },
        TrainingProgress::Epoch {
            epoch: 2,
            seconds: 20.0,
        },
    ];
    // Seen as the training stands when it stops, with everything reported.
    world.meanwhile(|training| training.stop());

    world.train();

    let epochs = world.job().unwrap().plan.epochs;
    let running = world.voices.events.0.lock().unwrap().iter().any(|event| {
        *event
            == AppEvent::Training(TrainStep::Training {
                voice: "Ana".to_owned(),
                device: TrainingDevice::Card,
                stage: TrainingStage::Learning,
                epoch: 2,
                epochs,
                remaining_seconds: Some((epochs - 2) * 20),
                is_running: true,
                problem: None,
            })
    });
    assert!(running);
    assert_eq!(world.job().unwrap().epoch, 2);
}

#[test]
fn a_stopped_training_is_paused_and_resumes_from_where_it_was() {
    let world = TrainingWorld::new().with_card().ready_for_takes().with_enough_speech();
    *world.trainer.progress.lock().unwrap() = vec![TrainingProgress::Epoch {
        epoch: 35,
        seconds: 20.0,
    }];
    world.meanwhile(|training| training.stop());

    world.train();

    assert!(matches!(
        world.step(),
        TrainStep::Training {
            epoch: 35,
            is_running: false,
            ..
        }
    ));
    assert!(matches!(
        world.phase(),
        Some(TrainingPhase::Paused { epoch: 35, .. })
    ));
    assert!(world.saw(&AppEvent::show(Message::text(TextKey::TrainingPaused))));
    assert!(world.voices.trained.added.lock().unwrap().is_empty());

    *world.trainer.meanwhile.lock().unwrap() = None;
    world.train();

    assert_eq!(world.trainer.runs.lock().unwrap().len(), 2);
    assert!(world.voices.library.lock().trained.contains_key(ANA));
}

#[test]
fn a_training_that_fails_says_why_and_keeps_what_it_has() {
    let world = TrainingWorld::new().with_card().ready_for_takes().with_enough_speech();
    *world.trainer.ending.lock().unwrap() =
        Some(Err(Failure::because(TextKey::TrainingFailed, "out of memory")));

    world.train();

    assert!(matches!(
        world.step(),
        TrainStep::Training {
            is_running: false,
            problem: Some(_),
            ..
        }
    ));
    assert!(world.job().is_some());
    assert!(world.trainer.discarded.lock().unwrap().is_empty());
}

#[test]
fn closing_the_app_and_opening_it_again_shows_the_training_paused() {
    let first = TrainingWorld::new().with_card().ready_for_takes().with_enough_speech();
    *first.trainer.progress.lock().unwrap() = vec![TrainingProgress::Epoch {
        epoch: 50,
        seconds: 20.0,
    }];
    first.meanwhile(|training| training.stop());
    first.train();
    let saved = first.job().unwrap();

    let again = TrainingWorld::with(
        InMemoryTraining {
            job: Mutex::new(Some(saved)),
            ..InMemoryTraining::default()
        },
        true,
    );

    assert!(matches!(
        again.phase(),
        Some(TrainingPhase::Paused { epoch: 50, .. })
    ));
}

#[test]
fn a_training_of_a_voice_deleted_meanwhile_is_dropped_with_its_files() {
    let job = TrainingJob::new(ANA, TrainingPlan::new(TrainingDevice::Card, Some(&card(8192))));
    let id = job.id.clone();

    let world = TrainingWorld::with(
        InMemoryTraining {
            job: Mutex::new(Some(job)),
            ..InMemoryTraining::default()
        },
        false,
    );

    assert_eq!(*world.trainer.discarded.lock().unwrap(), [id]);
    assert_eq!(world.job(), None);
    assert_eq!(world.phase(), None);
}

#[test]
fn a_voice_deleted_while_it_trains_gets_no_model() {
    let world = TrainingWorld::new().with_card().ready_for_takes().with_enough_speech();
    let voice = world.voices.voice.clone();
    *world.trainer.meanwhile.lock().unwrap() = Some(Box::new(move || voice.delete_clone(ANA)));

    world.train();

    assert!(world.voices.library.lock().trained.is_empty());
    assert!(matches!(
        world.step(),
        TrainStep::Training {
            problem: Some(_),
            ..
        }
    ));
}

#[test]
fn discarding_deletes_the_training_and_its_takes() {
    let world = TrainingWorld::new().with_card().ready_for_takes();
    world.record(60);
    let id = world.job().unwrap().id;

    world.training.discard();

    assert_eq!(*world.trainer.discarded.lock().unwrap(), [id]);
    assert_eq!(world.job(), None);
    assert_eq!(world.step(), TrainStep::Closed);
}

#[test]
fn only_one_voice_trains_at_a_time() {
    let world = TrainingWorld::new().with_card().ready_for_takes();
    world.voices.keep_clone("bia", "Bia");
    world.training.close();

    poll_once(world.training.open("bia")).unwrap();

    assert_eq!(world.step(), TrainStep::Closed);
    assert!(world.saw(&error(TextKey::TrainingAlreadyRunning)));
}

#[test]
fn closing_the_dialog_keeps_the_training_on_the_voice_screen() {
    let world = TrainingWorld::new().with_card().ready_for_takes();
    world.record(90);

    world.training.close();

    assert_eq!(world.step(), TrainStep::Closed);
    assert_eq!(
        world.phase(),
        Some(TrainingPhase::Takes {
            seconds: 90,
            min_seconds: MIN_TRAINING_SECONDS
        })
    );
    world.open();
    assert!(matches!(world.step(), TrainStep::Takes { seconds: 90, .. }));
}

#[test]
fn a_system_that_cannot_train_says_so() {
    let world = TrainingWorld::new();
    *world.trainer.is_supported.lock().unwrap() = false;

    world.open();

    assert_eq!(world.step(), TrainStep::Closed);
    assert!(world.saw(&error(TextKey::TrainingUnsupported)));
    world.training.close();
    assert!(!world.voices.voice.view().training.is_supported);
}

#[test]
fn training_needs_the_ai_models_that_check_the_model() {
    let world = TrainingWorld::new();
    *world.voices.models.is_installed.lock().unwrap() = false;

    world.open();

    assert_eq!(world.step(), TrainStep::Closed);
    assert!(world.saw(&error(TextKey::AiNotInstalled)));
}

#[test]
fn removing_the_tools_frees_their_room() {
    let world = TrainingWorld::new().with_card().ready_for_takes();
    assert!(world.voices.voice.view().training.tools_megabytes > 0);

    world.training.remove_tools();

    assert!(*world.trainer.tools_removed.lock().unwrap());
    assert_eq!(world.voices.voice.view().training.tools_megabytes, 0);
}

#[test]
fn a_training_whose_tools_are_gone_asks_for_them_again_and_resumes() {
    let world = TrainingWorld::new().with_card().ready_for_takes().with_enough_speech();
    world.meanwhile(|training| training.stop());
    world.train();
    world.training.close();
    world.training.remove_tools();
    *world.trainer.meanwhile.lock().unwrap() = None;

    world.open();
    assert!(matches!(world.step(), TrainStep::Prepare { .. }));
    world.accept();

    assert!(matches!(
        world.step(),
        TrainStep::Training {
            is_running: false,
            ..
        }
    ));
    world.train();
    assert!(world.voices.library.lock().trained.contains_key(ANA));
}
