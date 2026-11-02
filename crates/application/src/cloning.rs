use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use instantpad_domain::cloning::{
    Challenge, CloneRefusal, CloneRequest, MAX_PHRASE_SECONDS, MAX_SAMPLE_SECONDS,
    MIN_SAMPLE_SECONDS, Voiceprint,
};

use crate::events::{AppEvent, CloneStep};
use crate::message::{Failure, Message, TextKey};
use crate::ports::{EventSink, VoiceAnalyzer, VoiceModels, VoiceRecorder};
use crate::voice::{SPEECH_RATE, VoiceSession};
use crate::voice_library::VoiceLibraryStore;

/// What the owner of the voice agrees to, one clause at a time.
const CLAUSES: [TextKey; 5] = [
    TextKey::CloneTermsOwner,
    TextKey::CloneTermsPresent,
    TextKey::CloneTermsNoMisuse,
    TextKey::CloneTermsBiometric,
    TextKey::CloneTermsRevoke,
];

/// What was learned about the voice so far. The recordings themselves are never kept.
#[derive(Default)]
struct Draft {
    owner_name: String,
    sample: Option<(Voiceprint, String)>,
    challenge: Challenge,
    /// The voiceprint and the words of the phrase, once they passed the checks.
    phrase: Option<(Voiceprint, String)>,
}

struct State {
    step: CloneStep,
    draft: Draft,
}

fn sample_step(is_recording: bool, problem: Option<Message>) -> CloneStep {
    CloneStep::Sample {
        is_recording,
        min_seconds: MIN_SAMPLE_SECONDS,
        max_seconds: MAX_SAMPLE_SECONDS,
        problem,
    }
}

fn phrase_step(challenge: &Challenge, is_recording: bool, problem: Option<Message>) -> CloneStep {
    CloneStep::Phrase {
        digits: challenge.digits().to_owned(),
        is_recording,
        max_seconds: MAX_PHRASE_SECONDS,
        problem,
    }
}

fn refusal_text(refusal: CloneRefusal) -> Message {
    Message::text(match refusal {
        CloneRefusal::NotTheSameSpeaker => TextKey::CloneNotTheSameSpeaker,
        CloneRefusal::WrongPhrase => TextKey::CloneWrongPhrase,
        CloneRefusal::MissingName => TextKey::CloneMissingName,
    })
}

fn seconds(speech: &[f32]) -> usize {
    speech.len() / SPEECH_RATE
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// Clones a voice in steps: the terms, a sample, the phrase that proves the owner is present,
/// and a name. Only the voiceprint and the consent record are kept.
pub struct CloningSession {
    recorder: Arc<dyn VoiceRecorder>,
    analyzer: Arc<dyn VoiceAnalyzer>,
    models: Arc<dyn VoiceModels>,
    library: Arc<VoiceLibraryStore>,
    voice: Arc<VoiceSession>,
    events: Arc<dyn EventSink>,
    state: Mutex<State>,
}

impl CloningSession {
    pub fn new(
        recorder: Arc<dyn VoiceRecorder>,
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
            }),
        }
    }

    pub fn step(&self) -> CloneStep {
        self.state().step.clone()
    }

    /// Cloning listens with the AI, so the models must be there first.
    pub fn open(&self) {
        if !self.models.is_installed() {
            let reason = Message::text(TextKey::AiNotInstalled);
            self.events.emit(AppEvent::error(reason));
            return;
        }
        self.set(
            CloneStep::Terms {
                clauses: CLAUSES.to_vec(),
            },
            Draft::default(),
        );
    }

    pub fn accept_terms(&self, owner_name: &str) {
        let mut state = self.state();
        if !matches!(state.step, CloneStep::Terms { .. }) {
            return;
        }
        let owner_name = owner_name.trim();
        if owner_name.is_empty() {
            let reason = Message::text(TextKey::CloneMissingName);
            self.events.emit(AppEvent::error(reason));
            return;
        }
        state.draft.owner_name = owner_name.to_owned();
        self.show(&mut state, sample_step(false, None));
    }

    pub fn start_sample(&self) {
        let mut state = self.state();
        if !matches!(state.step, CloneStep::Sample { .. }) {
            return;
        }
        self.recorder.start_recording();
        self.show(&mut state, sample_step(true, None));
    }

    pub async fn finish_sample(&self) {
        if !matches!(
            self.step(),
            CloneStep::Sample {
                is_recording: true,
                ..
            }
        ) {
            return;
        }
        let speech = self.recorder.stop_recording();
        self.take_sample(speech).await;
    }

    pub async fn import_sample(&self, path: &str) {
        if !matches!(
            self.step(),
            CloneStep::Sample {
                is_recording: false,
                ..
            }
        ) {
            return;
        }
        match self.recorder.decode_file(path).await {
            Ok(speech) => self.take_sample(speech).await,
            Err(Failure(reason)) => self.show_now(sample_step(false, Some(reason))),
        }
    }

    pub fn start_phrase(&self) {
        let mut state = self.state();
        if !matches!(state.step, CloneStep::Phrase { .. }) {
            return;
        }
        self.recorder.start_recording();
        let step = phrase_step(&state.draft.challenge, true, None);
        self.show(&mut state, step);
    }

    pub async fn finish_phrase(&self) {
        if !matches!(
            self.step(),
            CloneStep::Phrase {
                is_recording: true,
                ..
            }
        ) {
            return;
        }
        let speech = self.recorder.stop_recording();
        if seconds(&speech) > MAX_PHRASE_SECONDS as usize {
            self.retry_phrase(Message::text(TextKey::PhraseTooLong));
            return;
        }

        self.show_now(CloneStep::Analyzing);
        let heard = tokio::try_join!(
            self.analyzer.voiceprint(speech.clone()),
            self.analyzer.transcribe(speech),
        );

        let mut state = self.state();
        // Closed while the AI was listening.
        if state.step != CloneStep::Analyzing {
            return;
        }
        let (phrase, transcript) = match heard {
            Ok(heard) => heard,
            Err(Failure(reason)) => {
                drop(state);
                self.retry_phrase(reason);
                return;
            }
        };
        let Some((sample, sample_sha256)) = &state.draft.sample else {
            return;
        };
        let request = CloneRequest {
            name: "",
            owner_name: &state.draft.owner_name,
            sample,
            sample_sha256,
            phrase: &phrase,
            challenge: &state.draft.challenge,
            transcript: &transcript,
            now: now(),
        };
        match request.verify() {
            Ok(()) => {
                state.draft.phrase = Some((phrase, transcript));
                self.show(&mut state, CloneStep::Name);
            }
            Err(refusal) => {
                drop(state);
                self.retry_phrase(refusal_text(refusal));
            }
        }
    }

    /// Keeps the voice, which then shows among the effects.
    pub fn save(&self, name: &str) {
        let mut state = self.state();
        if state.step != CloneStep::Name {
            return;
        }
        let draft = &state.draft;
        let (Some((sample, sample_sha256)), Some((phrase, transcript))) =
            (&draft.sample, &draft.phrase)
        else {
            return;
        };
        let request = CloneRequest {
            name,
            owner_name: &draft.owner_name,
            sample,
            sample_sha256,
            phrase,
            challenge: &draft.challenge,
            transcript,
            now: now(),
        };
        let voice = match request.clone_voice() {
            Ok(voice) => voice,
            Err(refusal) => {
                self.events.emit(AppEvent::error(refusal_text(refusal)));
                return;
            }
        };

        let id = voice.id.clone();
        self.library.lock().clones.push(voice);
        if let Err(Failure(reason)) = self.library.save() {
            // Kept only in memory, a retry would add the voice twice.
            self.library.lock().remove(&id);
            self.events.emit(AppEvent::error(reason));
            return;
        }
        self.show(&mut state, CloneStep::Closed);
        state.draft = Draft::default();
        drop(state);
        self.events
            .emit(AppEvent::show(Message::text(TextKey::CloneSaved)));
        self.voice.refresh();
    }

    /// Gives up, forgetting everything gathered so far.
    pub fn close(&self) {
        let mut state = self.state();
        let is_recording = matches!(
            state.step,
            CloneStep::Sample {
                is_recording: true,
                ..
            } | CloneStep::Phrase {
                is_recording: true,
                ..
            }
        );
        if is_recording {
            self.recorder.stop_recording();
        }
        state.draft = Draft::default();
        self.show(&mut state, CloneStep::Closed);
    }

    async fn take_sample(&self, speech: Vec<f32>) {
        let length = seconds(&speech);
        let problem = if length < MIN_SAMPLE_SECONDS as usize {
            Some(TextKey::SampleTooShort)
        } else if length > MAX_SAMPLE_SECONDS as usize {
            Some(TextKey::SampleTooLong)
        } else {
            None
        };
        if let Some(problem) = problem {
            self.show_now(sample_step(false, Some(Message::text(problem))));
            return;
        }

        {
            let mut state = self.state();
            // Closed while the file was being read.
            if state.step == CloneStep::Closed {
                return;
            }
            self.show(&mut state, CloneStep::Analyzing);
        }
        let sample_sha256 = self.analyzer.fingerprint(&speech);
        let voiceprint = self.analyzer.voiceprint(speech).await;

        let mut state = self.state();
        if state.step != CloneStep::Analyzing {
            return;
        }
        match voiceprint {
            Ok(voiceprint) => {
                state.draft.sample = Some((voiceprint, sample_sha256));
                state.draft.challenge = Challenge::new();
                let step = phrase_step(&state.draft.challenge, false, None);
                self.show(&mut state, step);
            }
            Err(Failure(reason)) => self.show(&mut state, sample_step(false, Some(reason))),
        }
    }

    /// Asks for new digits, so a recording of the old ones cannot be played back.
    fn retry_phrase(&self, problem: Message) {
        let mut state = self.state();
        if matches!(state.step, CloneStep::Closed) {
            return;
        }
        state.draft.challenge = Challenge::new();
        let step = phrase_step(&state.draft.challenge, false, Some(problem));
        self.show(&mut state, step);
    }

    fn set(&self, step: CloneStep, draft: Draft) {
        let mut state = self.state();
        state.draft = draft;
        self.show(&mut state, step);
    }

    /// Moves on, unless the user closed the dialog while a recording was being read.
    fn show_now(&self, step: CloneStep) {
        let mut state = self.state();
        if state.step != CloneStep::Closed {
            self.show(&mut state, step);
        }
    }

    fn show(&self, state: &mut MutexGuard<'_, State>, step: CloneStep) {
        state.step = step.clone();
        self.events.emit(AppEvent::Cloning(step));
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().expect("cloning lock poisoned")
    }
}
