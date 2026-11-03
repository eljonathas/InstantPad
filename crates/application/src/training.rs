use std::sync::{Arc, Mutex, MutexGuard};

use instantpad_domain::training::{
    GraphicsCard, MAX_TAKE_SECONDS, MAX_TRAINING_SECONDS, MIN_TRAINING_SECONDS, TakeRefusal,
    TrainingDevice, TrainingJob, TrainingPlan, missing_megabytes, needed_megabytes,
};

use crate::events::{AppEvent, TrainStep, TrainingJobView, TrainingPhase, TrainingSummary};
use crate::message::{Failure, Message, TextKey};
use crate::ports::{
    EventSink, Outcome, TrainingProgress, TrainingRepository, TrainingRun, TrainingStage,
    VoiceModels, VoiceTrainer,
};
use crate::recorder::{Recording, SharedRecorder};
use crate::voice::{SPEECH_RATE, VoiceSession};
use crate::voice_library::VoiceLibraryStore;

/// Where the voice would train, found when the dialog opens on a voice with no training yet.
#[derive(Clone)]
struct Prepared {
    card: Option<GraphicsCard>,
    plan: TrainingPlan,
}

/// What the training is busy with right now. Nothing of it outlives the app.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Activity {
    Idle,
    Checking,
    Installing { percent: u8 },
    Recording,
    /// A take is being written or a file read.
    Saving,
    Training {
        stage: TrainingStage,
        /// How long an epoch took, once one ran.
        seconds_per_epoch: Option<f32>,
    },
}

struct State {
    /// The cloned voice whose dialog is open.
    open: Option<String>,
    prepared: Option<Prepared>,
    job: Option<TrainingJob>,
    activity: Activity,
    problem: Option<Message>,
}

fn refusal_text(refusal: TakeRefusal) -> Message {
    match refusal {
        TakeRefusal::TooShort => Message::text(TextKey::TakeTooShort),
        TakeRefusal::Full => Message::text(TextKey::TrainingFull),
    }
}

/// Trains a model of a cloned voice inside the app: the user agrees to the download, gives 10 to
/// 40 minutes of speech, and the model trains, stops and resumes, then joins the voice. See
/// ADR 0007.
pub struct TrainingSession {
    trainer: Arc<dyn VoiceTrainer>,
    repository: Arc<dyn TrainingRepository>,
    recorder: Arc<SharedRecorder>,
    models: Arc<dyn VoiceModels>,
    library: Arc<VoiceLibraryStore>,
    voice: Arc<VoiceSession>,
    events: Arc<dyn EventSink>,
    state: Mutex<State>,
}

impl TrainingSession {
    pub fn new(
        trainer: Arc<dyn VoiceTrainer>,
        repository: Arc<dyn TrainingRepository>,
        recorder: Arc<SharedRecorder>,
        models: Arc<dyn VoiceModels>,
        library: Arc<VoiceLibraryStore>,
        voice: Arc<VoiceSession>,
        events: Arc<dyn EventSink>,
    ) -> Self {
        let mut job = repository.load();
        // The voice may have been deleted since, and nothing is kept of a voice that is gone.
        if let Some(gone) = job.take_if(|job| library.lock().name(&job.clone_id).is_none()) {
            trainer.discard(&gone.id);
            let _ = repository.save(None);
        }
        let session = Self {
            trainer,
            repository,
            recorder,
            models,
            library,
            voice,
            events,
            state: Mutex::new(State {
                open: None,
                prepared: None,
                job,
                activity: Activity::Idle,
                problem: None,
            }),
        };
        session.voice.show_training(session.summary(&session.state()));
        session
    }

    pub fn step(&self) -> TrainStep {
        self.step_of(&self.state())
    }

    /// Opens the training of a cloned voice: the one in progress, or a new one, which first
    /// looks for a graphics card.
    pub async fn open(&self, clone_id: &str) {
        if !self.trainer.is_supported() {
            self.events
                .emit(AppEvent::error(Message::text(TextKey::TrainingUnsupported)));
            return;
        }
        // The model is checked by loading it, which needs the runtime of the AI models.
        if !self.models.is_installed() {
            self.events
                .emit(AppEvent::error(Message::text(TextKey::AiNotInstalled)));
            return;
        }
        if self.library.lock().name(clone_id).is_none() {
            return;
        }
        {
            let mut state = self.state();
            if state
                .job
                .as_ref()
                .is_some_and(|job| job.clone_id != clone_id)
            {
                drop(state);
                self.events.emit(AppEvent::error(Message::text(
                    TextKey::TrainingAlreadyRunning,
                )));
                return;
            }
            state.open = Some(clone_id.to_owned());
            state.problem = None;
            let has_tools = state
                .job
                .as_ref()
                .is_some_and(|job| self.trainer.tools(job.plan.device).is_installed);
            let is_busy = state.activity != Activity::Idle;
            if has_tools || is_busy {
                self.publish(&state);
                return;
            }
            state.activity = Activity::Checking;
            self.publish(&state);
        }

        let card = self.trainer.graphics_card().await;

        let mut state = self.state();
        if state.activity != Activity::Checking {
            return;
        }
        state.activity = Activity::Idle;
        let device = TrainingDevice::choose(card.as_ref(), self.trainer.system());
        // A training that stopped before its tools were in keeps the plan it was agreed with.
        let plan = match &state.job {
            Some(job) => job.plan,
            None => TrainingPlan::new(device, card.as_ref()),
        };
        state.prepared = Some(Prepared { card, plan });
        self.publish(&state);
    }

    /// The user agrees to the download and the training: the tools install when they are
    /// missing, then the dialog asks for the speech.
    pub async fn accept(&self) {
        let device = {
            let mut state = self.state();
            let Some(clone_id) = state.open.clone() else {
                return;
            };
            let TrainStep::Prepare {
                missing_megabytes: None,
                ..
            } = self.step_of(&state)
            else {
                return;
            };
            let Some(prepared) = state.prepared.clone() else {
                return;
            };
            if state.job.is_none() {
                let job = TrainingJob::new(&clone_id, prepared.plan);
                if let Err(Failure(reason)) = self.repository.save(Some(&job)) {
                    state.problem = Some(reason);
                    self.publish(&state);
                    return;
                }
                state.job = Some(job);
            }
            state.problem = None;
            if self.trainer.tools(prepared.plan.device).is_installed {
                self.publish(&state);
                return;
            }
            state.activity = Activity::Installing { percent: 0 };
            self.publish(&state);
            prepared.plan.device
        };

        let progress = |percent: u8| {
            let mut state = self.state();
            if let Activity::Installing { percent: shown } = &mut state.activity
                && *shown != percent
            {
                *shown = percent;
                self.publish(&state);
            }
        };
        let installed = self.trainer.install(device, &progress).await;

        let mut state = self.state();
        state.activity = Activity::Idle;
        match installed {
            Ok(Outcome::Done(())) => {}
            Ok(Outcome::Stopped) => {}
            Err(Failure(reason)) => {
                state.problem = Some(Message::with(TextKey::TrainingInstallFailed, [reason]));
            }
        }
        self.publish(&state);
    }

    /// Records a take at the microphone, which [`TrainingSession::finish_take`] keeps.
    pub fn start_take(&self) {
        let mut state = self.state();
        if !self.takes_are_open(&state) || state.activity != Activity::Idle {
            return;
        }
        if !self.recorder.start(Recording::Training) {
            drop(state);
            self.events
                .emit(AppEvent::error(Message::text(TextKey::MicrophoneBusy)));
            return;
        }
        // The AI stops listening to the user, since the take needs the microphone.
        self.voice.hold_microphone(true);
        state.activity = Activity::Recording;
        state.problem = None;
        self.publish(&state);
    }

    pub async fn finish_take(&self) {
        {
            let mut state = self.state();
            if state.activity != Activity::Recording {
                return;
            }
            state.activity = Activity::Saving;
            self.publish(&state);
        }
        let speech = self.recorder.stop(Recording::Training);
        self.voice.hold_microphone(false);
        self.keep(speech).await;
    }

    /// Takes the speech of an audio file the user picked.
    pub async fn import_take(&self, path: &str) {
        {
            let mut state = self.state();
            if !self.takes_are_open(&state) || state.activity != Activity::Idle {
                return;
            }
            state.activity = Activity::Saving;
            state.problem = None;
            self.publish(&state);
        }
        match self.recorder.decode_file(path).await {
            Ok(speech) => self.keep(speech).await,
            Err(Failure(reason)) => self.finish_saving(Some(reason)),
        }
    }

    pub fn remove_take(&self, id: &str) {
        let mut state = self.state();
        if state.activity != Activity::Idle {
            return;
        }
        let Some(job) = state.job.as_mut() else {
            return;
        };
        let mut changed = job.clone();
        let Some(take) = changed.remove_take(id) else {
            return;
        };
        if let Err(Failure(reason)) = self.repository.save(Some(&changed)) {
            state.problem = Some(reason);
        } else {
            self.trainer.remove_take(&changed.id, &take.id);
            state.job = Some(changed);
        }
        self.publish(&state);
    }

    /// Starts the training, or resumes it from its last save.
    pub async fn train(&self) {
        let (run, clone_id) = {
            let mut state = self.state();
            if state.activity != Activity::Idle {
                return;
            }
            let Some(job) = state.job.clone() else {
                return;
            };
            if !job.has_enough() || !self.trainer.tools(job.plan.device).is_installed {
                return;
            }
            let Some(title) = self.library.lock().name(&job.clone_id).map(str::to_owned) else {
                return;
            };
            if !job.is_started {
                let started = TrainingJob {
                    is_started: true,
                    ..job.clone()
                };
                if let Err(Failure(reason)) = self.repository.save(Some(&started)) {
                    state.problem = Some(reason);
                    self.publish(&state);
                    return;
                }
                state.job = Some(started);
            }
            state.activity = Activity::Training {
                stage: TrainingStage::Cutting,
                seconds_per_epoch: None,
            };
            state.problem = None;
            self.publish(&state);
            let run = TrainingRun {
                job: job.id,
                plan: job.plan,
                title,
            };
            (run, job.clone_id)
        };

        let progress = |progress: TrainingProgress| self.on_progress(progress);
        let trained = self.trainer.train(&run, &progress).await;

        let folder = match trained {
            Ok(Outcome::Done(folder)) => folder,
            Ok(Outcome::Stopped) => {
                let mut state = self.state();
                state.activity = Activity::Idle;
                self.publish(&state);
                drop(state);
                self.events
                    .emit(AppEvent::show(Message::text(TextKey::TrainingPaused)));
                return;
            }
            Err(Failure(reason)) => {
                let mut state = self.state();
                state.activity = Activity::Idle;
                state.problem = Some(Message::with(TextKey::TrainingFailed, [reason]));
                self.publish(&state);
                return;
            }
        };

        let attached = self.voice.attach_trained_model(&clone_id, folder).await;
        let mut state = self.state();
        state.activity = Activity::Idle;
        match attached {
            Ok(()) => {
                self.end(&mut state);
                drop(state);
                self.events.emit(AppEvent::show(Message::with(
                    TextKey::TrainingDone,
                    [Message::plain(run.title)],
                )));
            }
            Err(Failure(reason)) => {
                state.problem = Some(reason);
                self.publish(&state);
            }
        }
    }

    /// Stops the download or the training, which can go on later.
    pub fn stop(&self) {
        let state = self.state();
        if matches!(
            state.activity,
            Activity::Installing { .. } | Activity::Training { .. }
        ) {
            self.trainer.stop();
        }
    }

    /// Gives the training up, deleting its takes and everything made of them.
    pub fn discard(&self) {
        let mut state = self.state();
        if !matches!(state.activity, Activity::Idle | Activity::Checking) {
            return;
        }
        if state.job.is_some() {
            self.end(&mut state);
        }
    }

    /// Closes the dialog. A training goes on, shown on the voice screen.
    pub fn close(&self) {
        let mut state = self.state();
        if state.activity == Activity::Recording {
            self.recorder.stop(Recording::Training);
            self.voice.hold_microphone(false);
            state.activity = Activity::Idle;
        }
        if state.activity == Activity::Checking {
            state.activity = Activity::Idle;
        }
        state.open = None;
        state.prepared = None;
        state.problem = None;
        self.publish(&state);
    }

    /// Deletes the training tools, which the next training downloads again.
    pub fn remove_tools(&self) {
        let state = self.state();
        let is_busy = matches!(
            state.activity,
            Activity::Installing { .. } | Activity::Training { .. }
        );
        if is_busy || self.trainer.installed_megabytes() == 0 {
            return;
        }
        self.trainer.remove_tools();
        self.publish(&state);
        drop(state);
        self.events
            .emit(AppEvent::show(Message::text(TextKey::TrainingToolsRemoved)));
    }

    fn on_progress(&self, progress: TrainingProgress) {
        let mut state = self.state();
        let Activity::Training {
            stage,
            seconds_per_epoch,
        } = &mut state.activity
        else {
            return;
        };
        match progress {
            TrainingProgress::Stage(new) => *stage = new,
            TrainingProgress::Epoch { epoch, seconds } => {
                *stage = TrainingStage::Learning;
                // The first report after a resume is where it went on from, which took no time.
                if seconds > 0.0 {
                    *seconds_per_epoch = Some(seconds_per_epoch.map_or(seconds, |average| {
                        // Recent epochs count more: the first ones also load the models.
                        average * 0.8 + seconds * 0.2
                    }));
                }
                if let Some(job) = state.job.as_mut() {
                    job.epoch = epoch;
                    // Only for the progress shown after a restart, so a failure is no matter.
                    let _ = self.repository.save(Some(job));
                }
            }
        }
        self.publish(&state);
    }

    async fn keep(&self, speech: Vec<f32>) {
        let seconds = u32::try_from(speech.len() / SPEECH_RATE).unwrap_or(u32::MAX);
        let candidate = {
            let state = self.state();
            let Some(job) = state.job.clone() else {
                return;
            };
            match job.room_for(seconds) {
                Ok(kept) => {
                    let mut candidate = job;
                    let take = candidate.add_take(kept);
                    Ok((candidate, take, kept))
                }
                Err(refusal) => Err(refusal_text(refusal)),
            }
        };
        let (candidate, take, kept) = match candidate {
            Ok(found) => found,
            Err(reason) => return self.finish_saving(Some(reason)),
        };

        let mut speech = speech;
        speech.truncate(kept as usize * SPEECH_RATE);
        let written = self.trainer.keep_take(&candidate.id, &take.id, speech).await;

        let mut state = self.state();
        state.activity = Activity::Idle;
        // Discarded while the take was written.
        if state.job.as_ref().map(|job| &job.id) != Some(&candidate.id) {
            self.trainer.remove_take(&candidate.id, &take.id);
            self.publish(&state);
            return;
        }
        let saved = written.and_then(|()| self.repository.save(Some(&candidate)));
        match saved {
            Ok(()) => {
                state.job = Some(candidate);
                state.problem = (kept < seconds).then(|| {
                    Message::with(
                        TextKey::TrainingFull,
                        [Message::plain(kept.to_string())],
                    )
                });
            }
            Err(Failure(reason)) => {
                self.trainer.remove_take(&candidate.id, &take.id);
                state.problem = Some(reason);
            }
        }
        self.publish(&state);
    }

    fn finish_saving(&self, problem: Option<Message>) {
        let mut state = self.state();
        state.activity = Activity::Idle;
        state.problem = problem;
        self.publish(&state);
    }

    fn takes_are_open(&self, state: &State) -> bool {
        matches!(self.step_of(state), TrainStep::Takes { .. })
    }

    /// Forgets the training and deletes its files, and closes the dialog.
    fn end(&self, state: &mut MutexGuard<'_, State>) {
        if let Some(job) = state.job.take() {
            self.trainer.discard(&job.id);
        }
        // Nothing is left to resume, whether this is saved or not.
        let _ = self.repository.save(None);
        state.open = None;
        state.prepared = None;
        state.problem = None;
        self.publish(state);
    }

    fn step_of(&self, state: &State) -> TrainStep {
        let Some(clone_id) = &state.open else {
            return TrainStep::Closed;
        };
        let voice = self
            .library
            .lock()
            .name(clone_id)
            .unwrap_or_default()
            .to_owned();
        let problem = state.problem.clone();
        if state.activity == Activity::Checking {
            return TrainStep::Checking;
        }
        if let Activity::Installing { percent } = state.activity {
            return TrainStep::Installing { voice, percent };
        }
        // A training whose tools are gone, removed or never installed, asks for them first.
        let job = state
            .job
            .as_ref()
            .filter(|job| self.trainer.tools(job.plan.device).is_installed);
        match job {
            None => self.prepare_step(state, voice, problem),
            Some(job) if !job.is_started => TrainStep::Takes {
                voice,
                takes: job.takes.clone(),
                seconds: job.seconds(),
                min_seconds: MIN_TRAINING_SECONDS,
                max_seconds: MAX_TRAINING_SECONDS,
                max_take_seconds: MAX_TAKE_SECONDS,
                is_recording: state.activity == Activity::Recording,
                is_busy: state.activity == Activity::Saving,
                estimated_seconds: job.plan.estimated_seconds(job.seconds()),
                problem,
            },
            Some(job) => {
                let (stage, seconds_per_epoch, is_running) = match state.activity {
                    Activity::Training {
                        stage,
                        seconds_per_epoch,
                    } => (stage, seconds_per_epoch, true),
                    _ => (TrainingStage::Learning, None, false),
                };
                TrainStep::Training {
                    voice,
                    device: job.plan.device,
                    stage,
                    epoch: job.epoch,
                    epochs: job.plan.epochs,
                    remaining_seconds: seconds_per_epoch
                        .map(|seconds| job.plan.remaining_seconds(job.epoch, seconds)),
                    is_running,
                    problem,
                }
            }
        }
    }

    fn prepare_step(&self, state: &State, voice: String, problem: Option<Message>) -> TrainStep {
        let Some(prepared) = &state.prepared else {
            // Opened on a training whose tools went missing: it looks for the card again.
            return TrainStep::Checking;
        };
        let tools = self.trainer.tools(prepared.plan.device);
        let needed = needed_megabytes(
            tools.download_megabytes,
            tools.installed_megabytes,
            tools.is_installed,
        );
        TrainStep::Prepare {
            voice,
            device: prepared.plan.device,
            card: prepared.card.as_ref().map(|card| card.name.clone()),
            download_megabytes: tools.download_megabytes,
            needed_megabytes: needed,
            missing_megabytes: missing_megabytes(needed, self.trainer.free_megabytes()),
            estimated_seconds: prepared.plan.estimated_seconds(MAX_TRAINING_SECONDS),
            problem,
        }
    }

    fn summary(&self, state: &State) -> TrainingSummary {
        let job = state.job.as_ref().map(|job| {
            let phase = match state.activity {
                Activity::Installing { percent } => TrainingPhase::Installing { percent },
                Activity::Training { .. } => TrainingPhase::Running {
                    epoch: job.epoch,
                    epochs: job.plan.epochs,
                },
                _ if job.is_started => TrainingPhase::Paused {
                    epoch: job.epoch,
                    epochs: job.plan.epochs,
                },
                _ => TrainingPhase::Takes {
                    seconds: job.seconds(),
                    min_seconds: MIN_TRAINING_SECONDS,
                },
            };
            TrainingJobView {
                clone_id: job.clone_id.clone(),
                phase,
            }
        });
        TrainingSummary {
            is_supported: self.trainer.is_supported(),
            job,
            tools_megabytes: self.trainer.installed_megabytes(),
        }
    }

    fn publish(&self, state: &State) {
        self.events.emit(AppEvent::Training(self.step_of(state)));
        self.voice.show_training(self.summary(state));
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().expect("training lock poisoned")
    }
}
