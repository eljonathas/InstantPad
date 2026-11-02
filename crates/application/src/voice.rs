use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use instantpad_domain::cloning::{
    DEFAULT_BLEND, DEFAULT_TIMBRE, DesignRefusal, MIN_CALIBRATION_SECONDS, VoiceLibrary,
    VoiceRecipe, Voiceprint,
};
use instantpad_domain::palette;
use instantpad_domain::speech::voiced;
use instantpad_domain::voice::{Conversion, VoiceEffect, VoiceParam, VoiceSettings, VoiceTarget};

use crate::events::{
    AiStatus, AiView, AppEvent, Calibration, ClonedVoiceView, DesignerView, EffectView, ParamView,
    VoiceIcon, VoiceView,
};
use crate::message::{Failure, Message, TextKey};
use crate::ports::{EventSink, VoiceAnalyzer, VoiceChanger, VoiceModels};
use crate::recorder::{Recording, SharedRecorder};
use crate::settings::Settings;
use crate::voice_library::VoiceLibraryStore;

/// The rate of everything the recorder hands over.
pub(crate) const SPEECH_RATE: usize = 48_000;
/// Listening that has not heard enough talk by now starts over, so the recording stays small.
const LISTEN_LIMIT_SECONDS: usize = 40;

struct EffectInfo {
    effect: VoiceEffect,
    name: TextKey,
    icon: VoiceIcon,
    color: &'static str,
    params: &'static [(VoiceParam, TextKey)],
}

const EFFECTS: [EffectInfo; 8] = [
    EffectInfo {
        effect: VoiceEffect::Natural,
        name: TextKey::EffectNatural,
        icon: VoiceIcon::Mic,
        color: palette::PADS[4],
        params: &[],
    },
    EffectInfo {
        effect: VoiceEffect::Thin,
        name: TextKey::EffectThin,
        icon: VoiceIcon::Feather,
        color: palette::PADS[4],
        params: &[
            (VoiceParam::ThinIntensity, TextKey::ParamIntensity),
            (VoiceParam::ThinFormant, TextKey::ParamFormant),
        ],
    },
    EffectInfo {
        effect: VoiceEffect::Deep,
        name: TextKey::EffectDeep,
        icon: VoiceIcon::Mountain,
        color: palette::PADS[6],
        params: &[
            (VoiceParam::DeepIntensity, TextKey::ParamIntensity),
            (VoiceParam::DeepFormant, TextKey::ParamFormant),
        ],
    },
    EffectInfo {
        effect: VoiceEffect::PitchCycle,
        name: TextKey::EffectCycle,
        icon: VoiceIcon::Wave,
        color: palette::PADS[3],
        params: &[
            (VoiceParam::CycleTime, TextKey::ParamTime),
            (VoiceParam::CycleHigh, TextKey::ParamHigh),
            (VoiceParam::CycleLow, TextKey::ParamLow),
        ],
    },
    EffectInfo {
        effect: VoiceEffect::Feminine,
        name: TextKey::EffectFeminine,
        icon: VoiceIcon::Flower,
        color: palette::PADS[7],
        params: &[
            (VoiceParam::FeminineIntensity, TextKey::ParamIntensity),
            (VoiceParam::FeminineFormant, TextKey::ParamFormant),
        ],
    },
    EffectInfo {
        effect: VoiceEffect::Ghost,
        name: TextKey::EffectGhost,
        icon: VoiceIcon::Ghost,
        color: palette::PADS[5],
        params: &[
            (VoiceParam::GhostWhisper, TextKey::ParamWhisper),
            (VoiceParam::GhostEcho, TextKey::ParamEcho),
            (VoiceParam::GhostPitch, TextKey::ParamPitch),
        ],
    },
    EffectInfo {
        effect: VoiceEffect::Ogre,
        name: TextKey::EffectOgre,
        icon: VoiceIcon::Horns,
        color: palette::PADS[1],
        params: &[
            (VoiceParam::OgreDepth, TextKey::ParamIntensity),
            (VoiceParam::OgreGrowl, TextKey::ParamGrowl),
            (VoiceParam::OgreFormant, TextKey::ParamFormant),
        ],
    },
    EffectInfo {
        effect: VoiceEffect::FeminineAi,
        name: TextKey::EffectFeminineAi,
        icon: VoiceIcon::Sparkles,
        color: palette::PADS[7],
        params: &[(VoiceParam::FeminineAiPitch, TextKey::ParamPitch)],
    },
];

const CLONE_COLOR: &str = palette::PADS[2];
const CLONE_PARAMS: &[(VoiceParam, TextKey)] = &[(VoiceParam::ClonePitch, TextKey::ParamPitch)];

fn param_views(params: &[(VoiceParam, TextKey)], voice: &VoiceSettings) -> Vec<ParamView> {
    params
        .iter()
        .map(|&(param, label)| ParamView {
            param,
            label,
            value: voice.get(param),
            default_value: param.default_value(),
            text: voice.describe(param),
        })
        .collect()
}

fn effect_view(info: &EffectInfo, voice: &VoiceSettings) -> EffectView {
    EffectView {
        effect: info.effect,
        clone_id: None,
        name: Message::text(info.name),
        icon: info.icon,
        color: info.color.to_owned(),
        needs_ai: info.effect.needs_ai(),
        params: param_views(info.params, voice),
    }
}

/// What the voice session talks to.
pub struct VoicePorts {
    pub changer: Arc<dyn VoiceChanger>,
    pub models: Arc<dyn VoiceModels>,
    pub analyzer: Arc<dyn VoiceAnalyzer>,
}

/// Why the AI cannot speak with a voice yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Unready {
    NoModels,
    NotCalibrated,
    NoSuchVoice,
}

impl Unready {
    fn message(self) -> Message {
        Message::text(match self {
            Self::NoModels => TextKey::AiNotInstalled,
            Self::NotCalibrated => TextKey::AiNotCalibrated,
            Self::NoSuchVoice => TextKey::CloneDeleted,
        })
    }
}

/// A voice the user picked, by its effect and, for one of their own, its id.
#[derive(Clone)]
struct Choice {
    effect: VoiceEffect,
    clone_id: Option<String>,
}

impl Choice {
    fn of(voice: &VoiceSettings) -> Self {
        let clone_id = voice.clone_id.clone();
        Self {
            effect: voice.effect,
            clone_id: clone_id.filter(|_| voice.effect == VoiceEffect::Cloned),
        }
    }
}

/// What the user asked for that waits for the AI to be set up.
enum Waiting {
    Voice(Choice),
    Cloning,
}

/// The voice designer while it is open.
#[derive(Default)]
struct Designer {
    /// The voice being tried, which the user hears instead of the effect.
    preview: Option<VoiceRecipe>,
    /// What "hear my voice" was before the preview turned it on, while a preview runs.
    hearing_before: Option<bool>,
}

/// The effect on the voice, which keeps working on both screens of the app.
pub struct VoiceSession {
    changer: Arc<dyn VoiceChanger>,
    models: Arc<dyn VoiceModels>,
    analyzer: Arc<dyn VoiceAnalyzer>,
    recorder: Arc<SharedRecorder>,
    library: Arc<VoiceLibraryStore>,
    settings: Arc<Settings>,
    events: Arc<dyn EventSink>,
    /// Not saved: a speaker near the mic would howl as soon as the app opens.
    is_hearing: AtomicBool,
    /// How far the AI models have downloaded, while they do.
    download: Mutex<Option<u8>>,
    calibration: Mutex<Calibration>,
    /// How much of the talk the AI needs it has heard, from 0 to 100.
    heard_percent: AtomicU8,
    /// The talk heard before the recording started over, which only grows to what is needed.
    carried: Mutex<Vec<f32>>,
    /// Set while the cloning dialog is open, which needs the microphone for itself.
    is_microphone_held: AtomicBool,
    waiting: Mutex<Option<Waiting>>,
    designer: Mutex<Option<Designer>>,
}

impl VoiceSession {
    pub fn new(
        ports: VoicePorts,
        recorder: Arc<SharedRecorder>,
        library: Arc<VoiceLibraryStore>,
        settings: Arc<Settings>,
        events: Arc<dyn EventSink>,
    ) -> Self {
        let session = Self {
            changer: ports.changer,
            models: ports.models,
            analyzer: ports.analyzer,
            recorder,
            library,
            settings,
            events,
            is_hearing: AtomicBool::new(false),
            download: Mutex::new(None),
            calibration: Mutex::new(Calibration::Idle),
            heard_percent: AtomicU8::new(0),
            carried: Mutex::new(Vec::new()),
            is_microphone_held: AtomicBool::new(false),
            waiting: Mutex::new(None),
            designer: Mutex::new(None),
        };
        // The models or the cloned voice may be gone since the effect was saved.
        let target = session.settings.lock().voice.target();
        if session.conversion(target).is_err() {
            session.settings.lock().voice.select(VoiceEffect::Natural);
        }
        session.send_to_engine();
        session
    }

    pub fn view(&self) -> VoiceView {
        let settings = self.settings.lock();
        let voice = &settings.voice;
        let library = self.library.lock();

        let built_in = EFFECTS.iter().map(|info| effect_view(info, voice));
        let own_voice = |id: &str, name: &str, icon| EffectView {
            effect: VoiceEffect::Cloned,
            clone_id: Some(id.to_owned()),
            name: Message::plain(name),
            icon,
            color: CLONE_COLOR.to_owned(),
            needs_ai: VoiceEffect::Cloned.needs_ai(),
            params: param_views(CLONE_PARAMS, voice),
        };
        let cloned = library
            .clones
            .iter()
            .map(|clone| own_voice(&clone.id, &clone.name, VoiceIcon::Person));
        let designed = library
            .designs
            .iter()
            .map(|design| own_voice(&design.id, &design.name, VoiceIcon::Blend));

        let status = match *self.download.lock().expect("download lock poisoned") {
            Some(percent) => AiStatus::Downloading { percent },
            None if self.models.is_installed() => AiStatus::Ready,
            None => AiStatus::Missing,
        };
        let waiting = self.waiting().as_ref().map(|waiting| match waiting {
            Waiting::Voice(choice) => voice_name(choice, &library),
            Waiting::Cloning => Message::text(TextKey::CloneVoice),
        });
        let designer = self.designer().as_ref().map(|designer| DesignerView {
            is_previewing: designer.preview.is_some(),
            default_timbre: f64::from(DEFAULT_TIMBRE),
            default_blend: f64::from(DEFAULT_BLEND),
        });

        VoiceView {
            active: voice.effect,
            active_clone_id: voice
                .clone_id
                .clone()
                .filter(|_| voice.effect == VoiceEffect::Cloned),
            effects: built_in.chain(cloned).chain(designed).collect(),
            is_hearing: self.is_hearing.load(Ordering::Relaxed),
            ai: AiView {
                status,
                download_megabytes: self.models.download_megabytes(),
                calibration: *self.calibration(),
                heard_percent: self.heard_percent.load(Ordering::Relaxed),
                waiting,
            },
            cloned_voices: library
                .clones
                .iter()
                .map(|clone| ClonedVoiceView {
                    id: clone.id.clone(),
                    name: clone.name.clone(),
                })
                .collect(),
            designer,
        }
    }

    /// An AI effect waits until the models and the user's own voice are ready, then comes on.
    pub fn select(&self, effect: VoiceEffect) {
        let mut candidate = self.settings.lock().voice.clone();
        candidate.select(effect);
        self.switch_to(candidate);
    }

    pub fn select_clone(&self, id: &str) {
        let mut candidate = self.settings.lock().voice.clone();
        candidate.select_clone(id);
        self.switch_to(candidate);
    }

    pub fn set_param(&self, param: VoiceParam, value: f64) {
        self.settings.lock().voice.set(param, value);
        self.apply();
    }

    pub fn set_hearing(&self, enabled: bool) {
        // The user's own choice is what comes back when a preview ends.
        if let Some(Designer {
            hearing_before: Some(before),
            ..
        }) = &mut *self.designer()
        {
            *before = enabled;
        }
        self.hear(enabled);
        self.publish();
    }

    /// For the key that switches the effect on and off from inside a game.
    pub fn toggle(&self) {
        let mut candidate = self.settings.lock().voice.clone();
        candidate.toggle();
        if self.switch_to(candidate) {
            self.announce();
        }
    }

    /// For the key that moves to the next effect from inside a game. Voices the AI cannot
    /// speak with yet are skipped.
    pub fn select_next(&self) {
        let library = self.library.lock().clone();
        let clones = library.ids();
        let mut candidate = self.settings.lock().voice.clone();
        for _ in 0..VoiceEffect::BUILT_IN.len() + clones.len() {
            candidate.select_next(&clones);
            if self.conversion(candidate.target()).is_ok() {
                break;
            }
        }
        if self.switch_to(candidate) {
            self.announce();
        }
    }

    /// Downloads the AI models, reporting the progress on the voice screen.
    pub async fn install_ai(&self) {
        {
            // Checked and taken in one go, so a second press never starts a second download.
            let mut download = self.download.lock().expect("download lock poisoned");
            if download.is_some() || self.models.is_installed() {
                return;
            }
            *download = Some(0);
        }
        self.publish();

        let progress = |percent: u8| self.set_download(Some(percent));
        let result = self.models.install(&progress).await;

        *self.download.lock().expect("download lock poisoned") = None;
        match result {
            Ok(()) => {
                self.events
                    .emit(AppEvent::show(Message::text(TextKey::AiInstalled)));
                self.resume();
            }
            Err(Failure(reason)) => self.events.emit(AppEvent::error(reason)),
        }
        self.publish();
    }

    /// Called every half second while the app runs. The AI learns the voice of the user by
    /// itself: when an AI voice waits for it, it listens to the microphone, and once the user
    /// has talked long enough it learns the voice and the AI voice comes on.
    pub async fn learn(&self) {
        let state = *self.calibration();
        match state {
            Calibration::Idle => self.begin_listening(),
            Calibration::Listening => self.keep_listening().await,
            Calibration::Analyzing => {}
        }
    }

    /// Asks the AI to learn the voice of the user again, for when the first time was in a noisy
    /// room or on another microphone. The effect in use waits for it, then comes back on.
    pub fn relearn(&self) {
        let voice = self.settings.lock().voice.clone();
        if !voice.effect.needs_ai() || !self.models.is_installed() {
            return;
        }
        self.stop_listening();
        if let Err(Failure(reason)) = self.library.change(|library| library.own = None) {
            self.events.emit(AppEvent::error(reason));
            return;
        }
        *self.waiting() = Some(Waiting::Voice(Choice::of(&voice)));
        self.settings.lock().voice.select(VoiceEffect::Natural);
        self.events
            .emit(AppEvent::show(Message::text(TextKey::AiListening)));
        self.apply();
        self.begin_listening();
    }

    /// The cloning dialog takes the microphone while it is open, and gives it back when it closes.
    pub(crate) fn hold_microphone(&self, is_held: bool) {
        self.is_microphone_held.store(is_held, Ordering::Relaxed);
        if is_held {
            self.stop_listening();
        }
    }

    /// Whether an AI voice waits for the AI to know the user, and nothing else prevents it.
    fn wants_own_voice(&self) -> bool {
        self.models.is_installed()
            && !self.is_microphone_held.load(Ordering::Relaxed)
            && self.library.lock().own.is_none()
            && matches!(*self.waiting(), Some(Waiting::Voice(_)))
    }

    fn begin_listening(&self) {
        if !self.wants_own_voice() {
            return;
        }
        let mut calibration = self.calibration();
        // Checked again under the lock: the cloning dialog may have taken the microphone since,
        // and its `hold_microphone` waits for this lock to stop what it finds listening. The
        // microphone may also be busy, and the next call tries again.
        if *calibration != Calibration::Idle
            || self.is_microphone_held.load(Ordering::Relaxed)
            || !self.recorder.start(Recording::Calibration)
        {
            return;
        }
        *calibration = Calibration::Listening;
        self.carried.lock().expect("carried lock poisoned").clear();
        self.heard_percent.store(0, Ordering::Relaxed);
        drop(calibration);
        self.publish();
    }

    /// The recorder stops while the lock is held, so that no listening starts in between and has
    /// its recording stopped from under it.
    fn stop_listening(&self) {
        let mut calibration = self.calibration();
        if *calibration != Calibration::Listening {
            return;
        }
        self.recorder.stop(Recording::Calibration);
        *calibration = Calibration::Idle;
        self.carried.lock().expect("carried lock poisoned").clear();
        self.heard_percent.store(0, Ordering::Relaxed);
        drop(calibration);
        self.publish();
    }

    async fn keep_listening(&self) {
        if !self.wants_own_voice() {
            self.stop_listening();
            return;
        }
        let heard = self.recorder.peek(Recording::Calibration);
        let needed = MIN_CALIBRATION_SECONDS as usize * SPEECH_RATE;
        let talk = self.carried.lock().expect("carried lock poisoned").len()
            + voiced(&heard, SPEECH_RATE).len();
        if talk < needed {
            let percent = (talk * 100 / needed) as u8;
            let calibration = self.calibration();
            // Stopped or taken meanwhile, which is not for this call to undo.
            if *calibration != Calibration::Listening {
                return;
            }
            if heard.len() > LISTEN_LIMIT_SECONDS * SPEECH_RATE {
                // The recording starts over so it stays small, and keeps the talk heard so far.
                let speech = self.recorder.stop(Recording::Calibration);
                self.carried
                    .lock()
                    .expect("carried lock poisoned")
                    .extend(voiced(&speech, SPEECH_RATE));
                self.recorder.start(Recording::Calibration);
            }
            let changed = self.heard_percent.swap(percent, Ordering::Relaxed) != percent;
            drop(calibration);
            if changed {
                self.publish();
            }
            return;
        }

        let speech = {
            let mut calibration = self.calibration();
            if *calibration != Calibration::Listening {
                return;
            }
            *calibration = Calibration::Analyzing;
            self.recorder.stop(Recording::Calibration)
        };
        self.heard_percent.store(100, Ordering::Relaxed);
        self.publish();

        let mut talk = std::mem::take(&mut *self.carried.lock().expect("carried lock poisoned"));
        talk.extend(voiced(&speech, SPEECH_RATE));
        let result = self.learn_from(talk).await;
        *self.calibration() = Calibration::Idle;
        self.heard_percent.store(0, Ordering::Relaxed);
        match result {
            Ok(()) => {
                let saved = Message::text(TextKey::AiCalibrationSaved);
                self.events.emit(AppEvent::show(saved));
                self.resume();
            }
            Err(Failure(reason)) => {
                // Left waiting, the voice would listen again and fail the same way.
                *self.waiting() = None;
                self.events.emit(AppEvent::error(reason));
            }
        }
        self.publish();
    }

    /// Deletes a voice for good. A cloned one takes the voices designed from it along.
    pub fn delete_clone(&self, id: &str) {
        let removed = match self.library.change(|library| library.remove(id)) {
            Ok(removed) => removed,
            Err(Failure(reason)) => {
                self.events.emit(AppEvent::error(reason));
                return;
            }
        };
        if removed.is_empty() {
            return;
        }
        for id in &removed {
            self.settings.lock().voice.forget_clone(id);
        }
        let mut waiting = self.waiting();
        if let Some(Waiting::Voice(Choice {
            clone_id: Some(waiting_id),
            ..
        })) = &*waiting
            && removed.contains(waiting_id)
        {
            *waiting = None;
        }
        drop(waiting);
        self.events
            .emit(AppEvent::show(Message::text(TextKey::CloneDeleted)));
        self.apply();
    }

    /// Opens the voice designer, which starts from a cloned voice.
    pub fn open_designer(&self) {
        if self.library.lock().clones.is_empty() {
            return;
        }
        *self.designer() = Some(Designer::default());
        self.publish();
    }

    /// Lets the user hear a voice while designing it, on the headphones, instead of the effect
    /// in use.
    pub fn preview_design(&self, recipe: VoiceRecipe) {
        if self.designer().is_none() {
            return;
        }
        if let Err(unready) = self.design_conversion(&recipe) {
            self.events.emit(AppEvent::error(unready.message()));
            return;
        }
        let is_hearing = self.is_hearing.load(Ordering::Relaxed);
        {
            let mut designer = self.designer();
            let Some(designer) = designer.as_mut() else {
                return;
            };
            designer.preview = Some(recipe);
            designer.hearing_before.get_or_insert(is_hearing);
        }
        self.hear(true);
        self.apply();
    }

    /// Back to the effect in use, and to the headphones as the user had them.
    pub fn end_preview(&self) {
        let hearing_before = self.designer().as_mut().and_then(|designer| {
            designer.preview = None;
            designer.hearing_before.take()
        });
        if let Some(enabled) = hearing_before {
            self.hear(enabled);
        }
        self.apply();
    }

    pub fn close_designer(&self) {
        self.end_preview();
        *self.designer() = None;
        self.publish();
    }

    /// Keeps a designed voice and turns it on. The designer stays open when that fails.
    pub fn save_design(&self, name: &str, recipe: VoiceRecipe) {
        if self.designer().is_none() {
            return;
        }
        let saved = self.library.change(|library| {
            let design = library.design_voice(name, recipe)?;
            let id = design.id.clone();
            library.designs.push(design);
            Ok(id)
        });
        let id = match saved {
            Ok(Ok(id)) => id,
            Ok(Err(refusal)) => {
                let key = match refusal {
                    DesignRefusal::MissingName => TextKey::CloneMissingName,
                    DesignRefusal::UnknownVoice => TextKey::CloneDeleted,
                };
                self.events.emit(AppEvent::error(Message::text(key)));
                return;
            }
            Err(Failure(reason)) => {
                self.events.emit(AppEvent::error(reason));
                return;
            }
        };
        self.close_designer();
        self.events
            .emit(AppEvent::show(Message::text(TextKey::CloneSaved)));
        self.select_clone(&id);
    }

    /// Cloning listens with the AI, so the voice screen leads through its download first.
    pub(crate) fn wait_for_cloning(&self) {
        *self.waiting() = Some(Waiting::Cloning);
        self.events
            .emit(AppEvent::error(Unready::NoModels.message()));
        self.publish();
    }

    /// Tells the interface about a voice cloned elsewhere.
    pub(crate) fn refresh(&self) {
        self.publish();
    }

    async fn learn_from(&self, talk: Vec<f32>) -> Result<(), Failure> {
        let own = self.analyzer.voiceprint(talk).await?;
        self.library.change(|library| library.own = Some(own))
    }

    /// Takes what waited for the AI, once it can run.
    fn resume(&self) {
        let waiting = self.waiting().take();
        match waiting {
            Some(Waiting::Voice(choice)) => {
                let candidate = self.candidate(&choice);
                if self.conversion(candidate.target()) == Err(Unready::NotCalibrated) {
                    *self.waiting() = Some(Waiting::Voice(choice));
                    self.begin_listening();
                } else {
                    self.switch_to(candidate);
                }
            }
            // The user opens the dialog again, now that the models are here.
            Some(Waiting::Cloning) | None => {}
        }
    }

    /// Takes `candidate` when the engine can run it, or keeps it waiting for the AI setup.
    /// Returns whether it was taken.
    fn switch_to(&self, candidate: VoiceSettings) -> bool {
        if let Err(unready) = self.conversion(candidate.target()) {
            if unready != Unready::NoSuchVoice {
                *self.waiting() = Some(Waiting::Voice(Choice::of(&candidate)));
            }
            // Not knowing the user is no error: the AI starts learning their voice right now.
            let is_learning = unready == Unready::NotCalibrated;
            self.events.emit(if is_learning {
                AppEvent::show(Message::text(TextKey::AiListening))
            } else {
                AppEvent::error(unready.message())
            });
            self.publish();
            if is_learning {
                self.begin_listening();
            }
            return false;
        }
        *self.waiting() = None;
        self.stop_listening();
        self.settings.lock().voice = candidate;
        self.apply();
        true
    }

    /// The settings in use with `choice` picked, so the knobs turned meanwhile stay.
    fn candidate(&self, choice: &Choice) -> VoiceSettings {
        let mut candidate = self.settings.lock().voice.clone();
        match &choice.clone_id {
            Some(id) => candidate.select_clone(id),
            None => candidate.select(choice.effect),
        }
        candidate
    }

    /// The speakers for the AI, or why it cannot speak yet.
    fn conversion(&self, target: Option<VoiceTarget>) -> Result<Option<Conversion>, Unready> {
        let Some(target) = target else {
            return Ok(None);
        };
        // Checked first, so a voice that is gone never waits for the setup.
        let target = match target {
            VoiceTarget::Preset(preset) => self.analyzer.preset(preset),
            VoiceTarget::Own(id) => self
                .library
                .lock()
                .voiceprint(&id, |preset| self.analyzer.preset(preset))
                .ok_or(Unready::NoSuchVoice)?,
        };
        self.speakers(target).map(Some)
    }

    fn design_conversion(&self, recipe: &VoiceRecipe) -> Result<Conversion, Unready> {
        let target = self
            .library
            .lock()
            .design_voiceprint(recipe, |preset| self.analyzer.preset(preset))
            .ok_or(Unready::NoSuchVoice)?;
        self.speakers(target)
    }

    /// The user and who they should sound like, once the AI can tell them apart.
    fn speakers(&self, target: Voiceprint) -> Result<Conversion, Unready> {
        if !self.models.is_installed() {
            return Err(Unready::NoModels);
        }
        let source = self
            .library
            .lock()
            .own
            .clone()
            .ok_or(Unready::NotCalibrated)?;
        Ok(Conversion { source, target })
    }

    fn send_to_engine(&self) {
        let settings = self.settings.lock().voice.clone();
        // A preview whose voices are gone ends by itself, back to the effect.
        let recipe = self
            .designer()
            .as_ref()
            .and_then(|designer| designer.preview.clone());
        let preview = recipe.and_then(|recipe| self.design_conversion(&recipe).ok());
        let (chain, conversion) = match preview {
            Some(conversion) => (Some(settings.own_voice_chain()), Some(conversion)),
            // Checked when the effect was picked, and only a delete removes what it needs,
            // which also moves to another effect.
            None => (
                settings.chain(),
                self.conversion(settings.target()).unwrap_or_default(),
            ),
        };
        self.changer.set_voice(chain, conversion);
    }

    fn apply(&self) {
        self.send_to_engine();
        self.publish();
    }

    fn hear(&self, enabled: bool) {
        self.is_hearing.store(enabled, Ordering::Relaxed);
        self.changer.set_hearing_voice(enabled);
    }

    fn set_download(&self, percent: Option<u8>) {
        *self.download.lock().expect("download lock poisoned") = percent;
        self.publish();
    }

    fn announce(&self) {
        let choice = Choice::of(&self.settings.lock().voice);
        let name = voice_name(&choice, &self.library.lock());
        self.events.emit(AppEvent::show(name));
    }

    fn calibration(&self) -> MutexGuard<'_, Calibration> {
        self.calibration.lock().expect("calibration lock poisoned")
    }

    fn waiting(&self) -> MutexGuard<'_, Option<Waiting>> {
        self.waiting.lock().expect("waiting lock poisoned")
    }

    fn designer(&self) -> MutexGuard<'_, Option<Designer>> {
        self.designer.lock().expect("designer lock poisoned")
    }

    fn publish(&self) {
        self.events.emit(AppEvent::Voice(self.view()));
    }
}

/// The name of a voice, as the effects list shows it.
fn voice_name(choice: &Choice, library: &VoiceLibrary) -> Message {
    let own = choice.clone_id.as_deref().and_then(|id| library.name(id));
    match own {
        Some(name) => Message::plain(name),
        None => EFFECTS
            .iter()
            .find(|info| info.effect == choice.effect)
            .map_or(Message::text(TextKey::EffectNatural), |info| {
                Message::text(info.name)
            }),
    }
}
