use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use instantpad_domain::cloning::MIN_CALIBRATION_SECONDS;
use instantpad_domain::palette;
use instantpad_domain::voice::{Conversion, VoiceEffect, VoiceParam, VoiceSettings, VoiceTarget};

use crate::events::{AiStatus, AiView, AppEvent, EffectView, ParamView, VoiceIcon, VoiceView};
use crate::message::{Failure, Message, TextKey};
use crate::ports::{EventSink, VoiceAnalyzer, VoiceChanger, VoiceModels, VoiceRecorder};
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
    pub recorder: Arc<dyn VoiceRecorder>,
}

/// The effect on the voice, which keeps working on both screens of the app.
pub struct VoiceSession {
    changer: Arc<dyn VoiceChanger>,
    models: Arc<dyn VoiceModels>,
    analyzer: Arc<dyn VoiceAnalyzer>,
    recorder: Arc<dyn VoiceRecorder>,
    library: Arc<VoiceLibraryStore>,
    settings: Arc<Settings>,
    events: Arc<dyn EventSink>,
    /// Not saved: a speaker near the mic would howl as soon as the app opens.
    is_hearing: AtomicBool,
    /// How far the AI models have downloaded, while they do.
    download: Mutex<Option<u8>>,
    is_calibrating: AtomicBool,
}

impl VoiceSession {
    pub fn new(
        ports: VoicePorts,
        library: Arc<VoiceLibraryStore>,
        settings: Arc<Settings>,
        events: Arc<dyn EventSink>,
    ) -> Self {
        let session = Self {
            changer: ports.changer,
            models: ports.models,
            analyzer: ports.analyzer,
            recorder: ports.recorder,
            library,
            settings,
            events,
            is_hearing: AtomicBool::new(false),
            download: Mutex::new(None),
            is_calibrating: AtomicBool::new(false),
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
        let cloned = library
            .clones
            .iter()
            .filter(|clone| clone.is_usable())
            .map(|clone| EffectView {
                effect: VoiceEffect::Cloned,
                clone_id: Some(clone.id.clone()),
                name: Message::plain(clone.name.clone()),
                icon: VoiceIcon::Person,
                color: CLONE_COLOR.to_owned(),
                needs_ai: true,
                params: param_views(CLONE_PARAMS, voice),
            });

        let status = match *self.download.lock().expect("download lock poisoned") {
            Some(percent) => AiStatus::Downloading { percent },
            None if self.models.is_installed() => AiStatus::Ready,
            None => AiStatus::Missing,
        };

        VoiceView {
            active: voice.effect,
            active_clone_id: voice
                .clone_id
                .clone()
                .filter(|_| voice.effect == VoiceEffect::Cloned),
            effects: built_in.chain(cloned).collect(),
            is_hearing: self.is_hearing.load(Ordering::Relaxed),
            ai: AiView {
                status,
                download_megabytes: self.models.download_megabytes(),
                is_calibrated: library.own.is_some(),
                is_calibrating: self.is_calibrating.load(Ordering::Relaxed),
                min_calibration_seconds: MIN_CALIBRATION_SECONDS,
            },
        }
    }

    /// An AI effect stays off until the models and the user's own voice are ready.
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
        let clones = library.usable_ids();
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
        let is_busy = self
            .download
            .lock()
            .expect("download lock poisoned")
            .is_some();
        if is_busy || self.models.is_installed() {
            return;
        }
        self.set_download(Some(0));

        let progress = |percent: u8| self.set_download(Some(percent));
        let result = self.models.install(&progress).await;

        self.set_download(None);
        match result {
            Ok(()) => self
                .events
                .emit(AppEvent::show(Message::text(TextKey::AiInstalled))),
            Err(Failure(reason)) => self.events.emit(AppEvent::error(reason)),
        }
    }

    /// Starts listening to the user, so the AI learns their own voice.
    pub fn start_calibration(&self) {
        self.recorder.start_recording();
        self.is_calibrating.store(true, Ordering::Relaxed);
        self.publish();
    }

    pub async fn finish_calibration(&self) {
        let speech = self.recorder.stop_recording();
        let result = self.calibrate(speech).await;
        self.is_calibrating.store(false, Ordering::Relaxed);
        match result {
            Ok(()) => {
                self.events
                    .emit(AppEvent::show(Message::text(TextKey::AiCalibrationSaved)));
                self.apply();
            }
            Err(Failure(reason)) => {
                self.events.emit(AppEvent::error(reason));
                self.publish();
            }
        }
    }

    /// Deletes a cloned voice for good, with its consent record.
    pub fn delete_clone(&self, id: &str) {
        let removed = self.library.lock().remove(id);
        if !removed {
            return;
        }
        if let Err(Failure(reason)) = self.library.save() {
            self.events.emit(AppEvent::error(reason));
        }
        self.settings.lock().voice.forget_clone(id);
        self.events
            .emit(AppEvent::show(Message::text(TextKey::CloneDeleted)));
        self.apply();
    }

    /// Tells the interface about a voice cloned elsewhere.
    pub fn refresh(&self) {
        self.publish();
    }

    async fn calibrate(&self, speech: Vec<f32>) -> Result<(), Failure> {
        if speech.len() < MIN_CALIBRATION_SECONDS as usize * SPEECH_RATE {
            return Err(Failure::text(TextKey::AiCalibrationTooShort));
        }
        let own = self.analyzer.voiceprint(speech).await?;
        self.library.lock().own = Some(own);
        self.library.save()
    }

    /// Takes `candidate` when the engine can run it. Returns whether it did.
    fn switch_to(&self, candidate: VoiceSettings) -> bool {
        if let Err(Failure(reason)) = self.conversion(candidate.target()) {
            self.events.emit(AppEvent::error(reason));
            return false;
        }
        self.settings.lock().voice = candidate;
        self.apply();
        true
    }

    /// The speakers for the AI, or why it cannot speak yet.
    fn conversion(&self, target: Option<VoiceTarget>) -> Result<Option<Conversion>, Failure> {
        let Some(target) = target else {
            return Ok(None);
        };
        if !self.models.is_installed() {
            return Err(Failure::text(TextKey::AiNotInstalled));
        }
        let library = self.library.lock();
        let source = library
            .own
            .clone()
            .ok_or(Failure::text(TextKey::AiNotCalibrated))?;
        let target = match target {
            VoiceTarget::Preset(preset) => self.analyzer.preset(preset),
            VoiceTarget::Clone(id) => library
                .find(&id)
                .filter(|clone| clone.is_usable())
                .map(|clone| clone.voiceprint.clone())
                .ok_or(Failure::text(TextKey::CloneDeleted))?,
        };
        Ok(Some(Conversion { source, target }))
    }

    fn send_to_engine(&self) {
        let (chain, target) = {
            let settings = self.settings.lock();
            (settings.voice.chain(), settings.voice.target())
        };
        // Checked when the effect was picked, and only a delete removes what it needs, which
        // also moves to another effect.
        let conversion = self.conversion(target).unwrap_or_default();
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
        let view = self.view();
        let active = view
            .effects
            .into_iter()
            .find(|effect| effect.effect == view.active && effect.clone_id == view.active_clone_id);
        if let Some(effect) = active {
            self.events.emit(AppEvent::show(effect.name));
        }
    }

    fn publish(&self) {
        self.events.emit(AppEvent::Voice(self.view()));
    }
}
