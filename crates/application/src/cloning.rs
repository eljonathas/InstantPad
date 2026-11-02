use std::sync::{Arc, Mutex, MutexGuard};

use instantpad_domain::cloning::{
    CloneRefusal, ClonedVoice, MAX_SAMPLE_SECONDS, MIN_SAMPLE_SECONDS, Voiceprint,
};

use crate::events::{AppEvent, CloneStep};
use crate::message::{Failure, Message, TextKey};
use crate::ports::{EventSink, VoiceAnalyzer, VoiceModels};
use crate::recorder::{Recording, SharedRecorder};
use crate::voice::{SPEECH_RATE, VoiceSession};
use crate::voice_library::VoiceLibraryStore;

/// What was learned about the voice so far. The recording itself is never kept.
#[derive(Default)]
struct Draft {
    sample: Option<Voiceprint>,
}

struct State {
    step: CloneStep,
    draft: Draft,
    /// Counts the times the dialog opened or closed, so what the AI hears for a dialog that is
    /// gone is dropped, even when another one opened meanwhile.
    generation: u64,
}

fn sample_step(is_recording: bool, problem: Option<Message>) -> CloneStep {
    CloneStep::Sample {
        is_recording,
        min_seconds: MIN_SAMPLE_SECONDS,
        max_seconds: MAX_SAMPLE_SECONDS,
        problem,
    }
}

fn refusal_text(refusal: CloneRefusal) -> Message {
    Message::text(match refusal {
        CloneRefusal::MissingName => TextKey::CloneMissingName,
    })
}

fn seconds(speech: &[f32]) -> usize {
    speech.len() / SPEECH_RATE
}

/// Clones a voice in steps: a sample and a name. Only the voiceprint is kept.
pub struct CloningSession {
    recorder: Arc<SharedRecorder>,
    analyzer: Arc<dyn VoiceAnalyzer>,
    models: Arc<dyn VoiceModels>,
    library: Arc<VoiceLibraryStore>,
    voice: Arc<VoiceSession>,
    events: Arc<dyn EventSink>,
    state: Mutex<State>,
}

impl CloningSession {
    pub fn new(
        recorder: Arc<SharedRecorder>,
        analyzer: Arc<dyn VoiceAnalyzer>,
        models: Arc<dyn VoiceModels>,
        library: Arc<VoiceLibraryStore>,
        voice: Arc<VoiceSession>,
        events: Arc<dyn EventSink>,
    ) -> Self {
        Self {
            recorder,
            analyzer,
            models,
            library,
            voice,
            events,
            state: Mutex::new(State {
                step: CloneStep::Closed,
                draft: Draft::default(),
                generation: 0,
            }),
        }
    }

    pub fn step(&self) -> CloneStep {
        self.state().step.clone()
    }

    /// Cloning listens with the AI, so the models must be there first.
    pub fn open(&self) {
        if !self.models.is_installed() {
            self.voice.wait_for_cloning();
            return;
        }
        let mut state = self.state();
        state.generation += 1;
        state.draft = Draft::default();
        self.show(&mut state, sample_step(false, None));
    }

    pub fn start_sample(&self) {
        let mut state = self.state();
        let is_ready = matches!(
            state.step,
            CloneStep::Sample {
                is_recording: false,
                ..
            }
        );
        if is_ready && self.start_recording() {
            self.show(&mut state, sample_step(true, None));
        }
    }

    pub async fn finish_sample(&self) {
        let is_recording = |step: &CloneStep| {
            matches!(
                step,
                CloneStep::Sample {
                    is_recording: true,
                    ..
                }
            )
        };
        let Some(generation) = self.listen(is_recording) else {
            return;
        };
        let speech = self.recorder.stop(Recording::Cloning);
        self.take_sample(generation, speech).await;
    }

    pub async fn import_sample(&self, path: &str) {
        let is_waiting = |step: &CloneStep| {
            matches!(
                step,
                CloneStep::Sample {
                    is_recording: false,
                    ..
                }
            )
        };
        let Some(generation) = self.listen(is_waiting) else {
            return;
        };
        match self.recorder.decode_file(path).await {
            Ok(speech) => self.take_sample(generation, speech).await,
            Err(Failure(reason)) => {
                self.show_if_current(generation, sample_step(false, Some(reason)));
            }
        }
    }

    /// Keeps the voice, which then shows among the effects.
    pub fn save(&self, name: &str) {
        let mut state = self.state();
        if state.step != CloneStep::Name {
            return;
        }
        let Some(sample) = state.draft.sample.clone() else {
            return;
        };
        let voice = match ClonedVoice::clone_from(name, sample) {
            Ok(voice) => voice,
            Err(refusal) => {
                self.events.emit(AppEvent::error(refusal_text(refusal)));
                return;
            }
        };

        if let Err(Failure(reason)) = self.library.change(|library| library.clones.push(voice)) {
            self.events.emit(AppEvent::error(reason));
            return;
        }
        self.end(&mut state);
        drop(state);
        self.events
            .emit(AppEvent::show(Message::text(TextKey::CloneSaved)));
        self.voice.refresh();
    }

    /// Gives up, forgetting everything gathered so far.
    pub fn close(&self) {
        let mut state = self.state();
        self.end(&mut state);
    }

    fn start_recording(&self) -> bool {
        let is_started = self.recorder.start(Recording::Cloning);
        if !is_started {
            let reason = Message::text(TextKey::MicrophoneBusy);
            self.events.emit(AppEvent::error(reason));
        }
        is_started
    }

    /// Moves to the analysis when `step` allows it, and returns the dialog it belongs to. In one
    /// go, so a second press of Stop finds the analysis already running.
    fn listen(&self, step: impl Fn(&CloneStep) -> bool) -> Option<u64> {
        let mut state = self.state();
        if !step(&state.step) {
            return None;
        }
        self.show(&mut state, CloneStep::Analyzing);
        Some(state.generation)
    }

    async fn take_sample(&self, generation: u64, speech: Vec<f32>) {
        let length = seconds(&speech);
        let problem = if length < MIN_SAMPLE_SECONDS as usize {
            Some(Message::limit(TextKey::SampleTooShort, MIN_SAMPLE_SECONDS))
        } else if length > MAX_SAMPLE_SECONDS as usize {
            Some(Message::limit(TextKey::SampleTooLong, MAX_SAMPLE_SECONDS))
        } else {
            None
        };
        if problem.is_some() {
            self.show_if_current(generation, sample_step(false, problem));
            return;
        }
        // Closed while the file was being read.
        if self.state().generation != generation {
            return;
        }

        let voiceprint = self.analyzer.voiceprint(speech).await;

        let mut state = self.state();
        if state.generation != generation {
            return;
        }
        match voiceprint {
            Ok(voiceprint) => {
                state.draft.sample = Some(voiceprint);
                self.show(&mut state, CloneStep::Name);
            }
            Err(Failure(reason)) => self.show(&mut state, sample_step(false, Some(reason))),
        }
    }

    fn show_if_current(&self, generation: u64, step: CloneStep) {
        let mut state = self.state();
        if state.generation == generation {
            self.show(&mut state, step);
        }
    }

    /// Closes the dialog, and drops whatever the AI is still listening to for it.
    fn end(&self, state: &mut MutexGuard<'_, State>) {
        self.recorder.stop(Recording::Cloning);
        state.generation += 1;
        state.draft = Draft::default();
        self.show(state, CloneStep::Closed);
    }

    fn show(&self, state: &mut MutexGuard<'_, State>, step: CloneStep) {
        state.step = step.clone();
        self.events.emit(AppEvent::Cloning(step));
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().expect("cloning lock poisoned")
    }
}
