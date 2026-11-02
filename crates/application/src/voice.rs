use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use instantpad_domain::cloning::{
    DesignRefusal, MIN_CALIBRATION_SECONDS, VoiceLibrary, VoiceRecipe, Voiceprint,
};
use instantpad_domain::palette;
use instantpad_domain::voice::{Conversion, VoiceEffect, VoiceParam, VoiceSettings, VoiceTarget};

use crate::events::{
    AiStatus, AiView, AppEvent, Calibration, ClonedVoiceView, EffectView, ParamView, VoiceIcon,
    VoiceView,
};
use crate::message::{Failure, Message, TextKey};
use crate::ports::{EventSink, VoiceAnalyzer, VoiceChanger, VoiceModels};
use crate::recorder::{Recording, SharedRecorder};
use crate::settings::Settings;
use crate::voice_library::VoiceLibraryStore;

/// The rate of everything the recorder hands over.
pub(crate) const SPEECH_RATE: usize = 48_000;

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

/// What the user asked for that waits for the AI to be set up.
enum Waiting {
    Voice(VoiceSettings),
    Cloning,
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
    waiting: Mutex<Option<Waiting>>,
    /// A voice being designed, which the user hears instead of the effect until it is saved.
    preview: Mutex<Option<VoiceRecipe>>,
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
            waiting: Mutex::new(None),
            preview: Mutex::new(None),
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
            Waiting::Voice(voice) => voice_name(voice, &library),
            Waiting::Cloning => Message::text(TextKey::CloneVoice),
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
                is_calibrated: library.own.is_some(),
                calibration: *self.calibration(),
                min_calibration_seconds: MIN_CALIBRATION_SECONDS,
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
        self.is_hearing.store(enabled, Ordering::Relaxed);
        self.changer.set_hearing_voice(enabled);
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

    /// Starts listening to the user, so the AI learns their own voice.
    pub fn start_calibration(&self) {
        if !self.models.is_installed() {
            self.events
                .emit(AppEvent::error(Unready::NoModels.message()));
            return;
        }
        let mut calibration = self.calibration();
        if *calibration != Calibration::Idle || !self.recorder.start(Recording::Calibration) {
            return;
        }
        *calibration = Calibration::Recording;
        drop(calibration);
        self.publish();
    }

    pub async fn finish_calibration(&self) {
        {
            let mut calibration = self.calibration();
            if *calibration != Calibration::Recording {
                return;
            }
            *calibration = Calibration::Analyzing;
        }
        let speech = self.recorder.stop(Recording::Calibration);
        self.publish();

        let result = self.calibrate(speech).await;
        *self.calibration() = Calibration::Idle;
        match result {
            Ok(()) => {
                let saved = Message::text(TextKey::AiCalibrationSaved);
                self.events.emit(AppEvent::show(saved));
                self.resume();
            }
            Err(Failure(reason)) => self.events.emit(AppEvent::error(reason)),
        }
        self.publish();
    }

    /// Deletes a voice for good. A cloned one takes its consent record and the voices designed
    /// from it along.
    pub fn delete_clone(&self, id: &str) {
        let before = self.library.lock().clone();
        let removed = self.library.lock().remove(id);
        if removed.is_empty() {
            return;
        }
        if let Err(Failure(reason)) = self.library.save() {
            // Still on disk, so it must stay in the app too.
            *self.library.lock() = before;
            self.events.emit(AppEvent::error(reason));
            return;
        }
        let mut waiting = self.waiting();
        for id in &removed {
            self.settings.lock().voice.forget_clone(id);
            if matches!(&*waiting, Some(Waiting::Voice(voice)) if voice.clone_id.as_ref() == Some(id))
            {
                *waiting = None;
            }
        }
        drop(waiting);
        self.events
            .emit(AppEvent::show(Message::text(TextKey::CloneDeleted)));
        self.apply();
    }

    /// Lets the user hear a voice while designing it, instead of the effect in use.
    pub fn preview_design(&self, recipe: VoiceRecipe) {
        if let Err(unready) = self.design_conversion(&recipe) {
            self.events.emit(AppEvent::error(unready.message()));
            return;
        }
        *self.preview() = Some(recipe);
        self.send_to_engine();
    }

    /// Back to the effect in use.
    pub fn end_preview(&self) {
        *self.preview() = None;
        self.send_to_engine();
    }

    /// Keeps a designed voice and turns it on.
    pub fn save_design(&self, name: &str, recipe: VoiceRecipe) {
        let design = self.library.lock().design_voice(name, recipe);
        let design = match design {
            Ok(design) => design,
            Err(refusal) => {
                let key = match refusal {
                    DesignRefusal::MissingName => TextKey::CloneMissingName,
                    DesignRefusal::UnknownVoice => TextKey::CloneDeleted,
                };
                self.events.emit(AppEvent::error(Message::text(key)));
                return;
            }
        };
        let id = design.id.clone();
        self.library.lock().designs.push(design);
        if let Err(Failure(reason)) = self.library.save() {
            // Kept only in memory, a retry would add the voice twice.
            self.library.lock().remove(&id);
            self.events.emit(AppEvent::error(reason));
            return;
        }
        *self.preview() = None;
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

    async fn calibrate(&self, speech: Vec<f32>) -> Result<(), Failure> {
        if speech.len() < MIN_CALIBRATION_SECONDS as usize * SPEECH_RATE {
            let seconds = Message::plain(MIN_CALIBRATION_SECONDS.to_string());
            let reason = Message::with(TextKey::AiCalibrationTooShort, [seconds]);
            return Err(Failure(reason));
        }
        let own = self.analyzer.voiceprint(speech).await?;
        let before = self.library.lock().own.replace(own);
        self.library.save().inspect_err(|_| {
            // Not on disk, so the app must not use it either.
            self.library.lock().own = before;
        })
    }

    /// Takes what waited for the AI, once it can run.
    fn resume(&self) {
        let waiting = self.waiting().take();
        match waiting {
            Some(Waiting::Voice(voice)) => {
                if self.conversion(voice.target()) == Err(Unready::NotCalibrated) {
                    *self.waiting() = Some(Waiting::Voice(voice));
                } else {
                    self.switch_to(voice);
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
                *self.waiting() = Some(Waiting::Voice(candidate));
            }
            self.events.emit(AppEvent::error(unready.message()));
            self.publish();
            return false;
        }
        *self.waiting() = None;
        self.settings.lock().voice = candidate;
        self.apply();
        true
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
        let preview = self
            .preview()
            .as_ref()
            .and_then(|recipe| self.design_conversion(recipe).ok());
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

    fn set_download(&self, percent: Option<u8>) {
        *self.download.lock().expect("download lock poisoned") = percent;
        self.publish();
    }

    fn announce(&self) {
        let settings = self.settings.lock().voice.clone();
        let name = voice_name(&settings, &self.library.lock());
        self.events.emit(AppEvent::show(name));
    }

    fn calibration(&self) -> MutexGuard<'_, Calibration> {
        self.calibration.lock().expect("calibration lock poisoned")
    }

    fn waiting(&self) -> MutexGuard<'_, Option<Waiting>> {
        self.waiting.lock().expect("waiting lock poisoned")
    }

    fn preview(&self) -> MutexGuard<'_, Option<VoiceRecipe>> {
        self.preview.lock().expect("preview lock poisoned")
    }

    fn publish(&self) {
        self.events.emit(AppEvent::Voice(self.view()));
    }
}

/// The name of the effect `voice` has on, as the effects list shows it.
fn voice_name(voice: &VoiceSettings, library: &VoiceLibrary) -> Message {
    let own = voice
        .clone_id
        .as_deref()
        .and_then(|id| library.name(id))
        .filter(|_| voice.effect == VoiceEffect::Cloned);
    match own {
        Some(name) => Message::plain(name),
        None => EFFECTS
            .iter()
            .find(|info| info.effect == voice.effect)
            .map_or(Message::text(TextKey::EffectNatural), |info| {
                Message::text(info.name)
            }),
    }
}
