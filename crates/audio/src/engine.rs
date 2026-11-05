//! Routing:
//!   mic + sounds -> game output (a virtual cable, what the game hears)
//!   sounds       -> monitor output (what you hear)

use std::collections::HashMap;
use std::ops::Range;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;
use instantpad_application::message::{Failure, TextKey};
use instantpad_application::ports::{
    AudioDevice, AudioMix, AudioRouter, DeviceRole, EventSink, NowPlaying, PlaybackTag,
    SoundPlayer, SpeechConverter, StreamStop, VoiceChanger, VoiceRecorder, Waveform,
};
use instantpad_application::sync::lock;
use instantpad_domain::cuts::Cut;
use instantpad_domain::voice::{Conversion, VoiceChain};
use tokio::sync::OnceCell;

use crate::ai::AiBridge;
use crate::clip::{self, SoundClip};
use crate::controls::{AtomicF32, Controls};
use crate::devices::{self, StreamHealth};
use crate::format::{CHANNELS, SAMPLE_RATE};
use crate::mic::{self, MicSlot};
use crate::mixer::{self, MixerHandle};
use crate::recording::Recorder;
use crate::role::Role;
use crate::voice::VoiceControls;

/// How much memory the decoded sounds may take together, about 22 minutes of audio. A board of
/// whole songs keeps the ones played last and decodes the others again when they are needed.
const CLIP_BUDGET_BYTES: usize = 1 << 30;

struct CachedClip {
    cell: Arc<OnceCell<Arc<SoundClip>>>,
    /// When the sound was last asked for, to find the one nobody wants.
    used: u64,
}

/// The decoded sounds, within a memory budget.
struct Clips {
    entries: HashMap<String, CachedClip>,
    clock: u64,
    budget: usize,
}

impl Clips {
    fn new(budget: usize) -> Self {
        Self {
            entries: HashMap::new(),
            clock: 0,
            budget,
        }
    }

    fn entry(&mut self, path: &str) -> Arc<OnceCell<Arc<SoundClip>>> {
        self.clock += 1;
        let item = self
            .entries
            .entry(path.to_owned())
            .or_insert_with(|| CachedClip {
                cell: Arc::default(),
                used: 0,
            });
        item.used = self.clock;
        item.cell.clone()
    }

    /// Drops the sounds used longest ago until the rest fits the budget. A sound that is
    /// playing, or that someone is about to play, is held by more than the cache and stays.
    /// It is dropped here, off the audio thread, because the mixer never holds the last
    /// reference of a sound it has finished.
    fn trim(&mut self) {
        let size = |item: &CachedClip| item.cell.get().map_or(0, |clip| clip.bytes());
        let mut total: usize = self.entries.values().map(size).sum();
        while total > self.budget {
            let oldest = self
                .entries
                .iter()
                .filter(|(_, item)| item.cell.get().is_some_and(|c| Arc::strong_count(c) == 1))
                .min_by_key(|(_, item)| item.used)
                .map(|(path, _)| path.clone());
            let Some(path) = oldest else { return };
            if let Some(item) = self.entries.remove(&path) {
                total -= size(&item);
            }
        }
    }
}

/// A sound that started, for the display. The newest comes last.
struct Started {
    tag: PlaybackTag,
    at: Instant,
    seconds: f32,
    /// The display bars of the part that plays: the whole clip, or its cut.
    peaks: Vec<f32>,
}

/// One of the two outputs and the mixer that feeds it.
struct Output {
    role: Role<cpal::Stream>,
    controls: Arc<Controls>,
    /// The microphone to mix in, for the output that goes to the game.
    mic: Option<MicSlot>,
    /// `None` while no device is open. A mixer nobody reads would never finish its voices.
    voices: Mutex<Option<MixerHandle>>,
    health: HealthSlot,
}

impl Output {
    fn new(name: &str, mic: Option<MicSlot>) -> Self {
        Self {
            role: Role::spawn(name),
            controls: Arc::new(Controls::new()),
            mic,
            voices: Mutex::new(None),
            health: HealthSlot::default(),
        }
    }

    fn with_voices<R>(&self, work: impl FnOnce(&mut MixerHandle) -> R) -> Option<R> {
        lock(&self.voices).as_mut().map(work)
    }

    async fn open(&self, device_id: Option<&str>) -> Result<(), Failure> {
        // The old device is closed first, and its voices would never finish on the new one.
        *lock(&self.voices) = None;
        let health = self.health.renew();

        let Some(id) = device_id.map(str::to_owned) else {
            return self.role.run(close).await;
        };

        let (mixer, handle) = mixer::pair(self.controls.clone(), self.mic.clone());
        self.role
            .run(move |stream| {
                *stream = None;
                *stream = Some(devices::open_output(&id, mixer, health)?);
                Ok(())
            })
            .await?;

        *lock(&self.voices) = Some(handle);
        Ok(())
    }
}

fn close(stream: &mut Option<cpal::Stream>) -> Result<(), Failure> {
    *stream = None;
    Ok(())
}

/// The health of the stream a role has open now.
#[derive(Default)]
struct HealthSlot(Mutex<Arc<StreamHealth>>);

impl HealthSlot {
    /// Gives the stream about to open a health of its own, so the stream it replaces, which may
    /// have stopped, no longer speaks for the role.
    fn renew(&self) -> Arc<StreamHealth> {
        let health = Arc::new(StreamHealth::default());
        *lock(&self.0) = health.clone();
        health
    }

    fn take_stop(&self) -> Option<StreamStop> {
        lock(&self.0).take_stop()
    }
}

struct Microphone {
    role: Role<cpal::Stream>,
    reader: MicSlot,
    /// The same voice for the headphones, which the monitor mixer only mixes on request.
    hearing: MicSlot,
    level: Arc<AtomicF32>,
    voice: Arc<VoiceControls>,
    health: HealthSlot,
}

/// Plays sounds and routes the microphone, with nothing in the audio callbacks that allocates or
/// waits. The one lock a callback takes is the microphone slot's `try_lock`, which gives up
/// instead of waiting.
pub struct AudioEngine {
    ai: AiBridge,
    recorder: Recorder,
    clips: Mutex<Clips>,
    started: Mutex<Vec<Started>>,
    microphone: Microphone,
    game: Output,
    monitor: Output,
}

impl AudioEngine {
    pub fn new(events: Arc<dyn EventSink>, converter: Arc<dyn SpeechConverter>) -> Self {
        let reader = mic::empty_slot();
        let hearing = mic::empty_slot();
        let monitor = Output::new("monitor output", Some(hearing.clone()));
        monitor.controls.mic_enabled.store(false, Ordering::Relaxed);
        Self {
            ai: AiBridge::new(converter, events),
            recorder: Recorder::new(),
            clips: Mutex::new(Clips::new(CLIP_BUDGET_BYTES)),
            started: Mutex::default(),
            microphone: Microphone {
                role: Role::spawn("microphone"),
                reader: reader.clone(),
                hearing,
                level: Arc::new(AtomicF32::new(0.0)),
                voice: Arc::new(VoiceControls::new()),
                health: HealthSlot::default(),
            },
            game: Output::new("game output", Some(reader)),
            monitor,
        }
    }

    async fn load(&self, path: &str) -> Result<Arc<SoundClip>, Failure> {
        let entry = lock(&self.clips).entry(path);

        // A failed load leaves the entry empty, so it is retried next time.
        let clip = entry
            .get_or_try_init(|| async {
                let path = path.to_owned();
                let loaded = tokio::task::spawn_blocking(move || SoundClip::load(&path)).await;
                loaded
                    .map_err(|error| Failure::because(TextKey::SoundOpenFailed, error))?
                    .map(Arc::new)
            })
            .await?;
        let clip = clip.clone();
        // The clip in hand keeps itself from being dropped, so a sound just loaded stays.
        lock(&self.clips).trim();
        Ok(clip)
    }

    fn remember(&self, clip: &SoundClip, range: Range<usize>, tag: PlaybackTag) {
        let seconds = range.len() as f32 / (SAMPLE_RATE as usize * CHANNELS) as f32;
        let peaks = if range == clip.range(None) {
            clip.peaks().to_vec()
        } else {
            clip.peaks_of(range)
        };
        let mut started = lock(&self.started);
        started.retain(|item| item.tag != tag);
        started.push(Started {
            tag,
            at: Instant::now(),
            seconds,
            peaks,
        });
    }

    fn play_on(
        &self,
        output: &Output,
        clip: &Arc<SoundClip>,
        range: Range<usize>,
        volume: f32,
        tag: PlaybackTag,
    ) {
        output.with_voices(|voices| voices.add(clip.clone(), range, volume, tag));
    }
}

#[async_trait]
impl SoundPlayer for AudioEngine {
    async fn preload(&self, path: &str) -> Result<(), Failure> {
        self.load(path).await.map(drop)
    }

    async fn play(
        &self,
        path: &str,
        volume: f32,
        tag: PlaybackTag,
        cut: Option<Cut>,
    ) -> Result<(), Failure> {
        let clip = self.load(path).await?;
        let range = clip.range(cut);
        self.stop(tag);
        self.play_on(&self.game, &clip, range.clone(), volume, tag);
        self.play_on(&self.monitor, &clip, range.clone(), volume, tag);
        self.remember(&clip, range, tag);
        Ok(())
    }

    async fn preview(
        &self,
        path: &str,
        volume: f32,
        tag: PlaybackTag,
        cut: Option<Cut>,
    ) -> Result<(), Failure> {
        let clip = self.load(path).await?;
        let range = clip.range(cut);
        self.stop(tag);
        self.play_on(&self.monitor, &clip, range.clone(), volume, tag);
        self.remember(&clip, range, tag);
        Ok(())
    }

    async fn waveform(
        &self,
        path: &str,
        start_ms: u32,
        end_ms: u32,
        bars: u32,
    ) -> Result<Waveform, Failure> {
        let clip = self.load(path).await?;
        Ok(Waveform {
            duration_ms: clip.duration_ms(),
            peaks: clip.waveform(start_ms, end_ms, bars as usize),
        })
    }

    async fn levels(&self, path: &str, frame_ms: u32) -> Result<Vec<f32>, Failure> {
        Ok(self.load(path).await?.levels(frame_ms))
    }

    fn forget(&self, path: &str) {
        lock(&self.clips).entries.remove(path);
    }

    fn stop(&self, tag: PlaybackTag) {
        for output in [&self.game, &self.monitor] {
            output.with_voices(|voices| voices.remove(tag));
        }
    }

    fn stop_all(&self) {
        for output in [&self.game, &self.monitor] {
            output.with_voices(MixerHandle::clear);
        }
    }

    fn is_playing(&self, tag: PlaybackTag) -> bool {
        [&self.game, &self.monitor]
            .into_iter()
            .any(|output| output.with_voices(|voices| voices.is_playing(tag)) == Some(true))
    }

    fn now_playing(&self) -> Option<NowPlaying> {
        let mut started = lock(&self.started);
        started.retain(|item| self.is_playing(item.tag));
        let newest = started.last()?;

        Some(NowPlaying {
            position: newest.at.elapsed().as_secs_f32().min(newest.seconds),
            duration: newest.seconds,
            peaks: newest.peaks.clone(),
        })
    }
}

#[async_trait]
impl AudioRouter for AudioEngine {
    fn list_microphones(&self) -> Vec<AudioDevice> {
        devices::microphones()
    }

    fn list_outputs(&self) -> Vec<AudioDevice> {
        devices::outputs()
    }

    async fn set_microphone(&self, device_id: Option<&str>) -> Result<(), Failure> {
        let microphone = &self.microphone;
        *lock(&microphone.reader) = None;
        *lock(&microphone.hearing) = None;
        self.recorder.attach(None);
        self.ai.attach(None);
        microphone.level.store(0.0);
        let health = microphone.health.renew();

        let Some(id) = device_id.map(str::to_owned) else {
            return microphone.role.run(close).await;
        };

        let (ring, reader) = mic::ring();
        let (hearing_ring, hearing) = mic::ring();
        let level = microphone.level.clone();
        let voice = microphone.voice.clone();
        let (tap, recording) = self.recorder.tap();
        let (ai, converting) = self.ai.rings();
        microphone
            .role
            .run(move |stream| {
                *stream = None;
                let rings = [ring, hearing_ring];
                *stream = Some(devices::open_capture(
                    &id, rings, level, voice, tap, ai, health,
                )?);
                Ok(())
            })
            .await?;
        self.recorder.attach(Some(recording));
        self.ai.attach(Some(converting));

        *lock(&microphone.reader) = Some(reader);
        *lock(&microphone.hearing) = Some(hearing);
        Ok(())
    }

    async fn set_game_output(&self, device_id: Option<&str>) -> Result<(), Failure> {
        self.game.open(device_id).await
    }

    async fn set_monitor_output(&self, device_id: Option<&str>) -> Result<(), Failure> {
        self.monitor.open(device_id).await
    }

    fn take_stopped(&self) -> Vec<(DeviceRole, StreamStop)> {
        [
            (DeviceRole::Microphone, &self.microphone.health),
            (DeviceRole::GameOutput, &self.game.health),
            (DeviceRole::MonitorOutput, &self.monitor.health),
        ]
        .into_iter()
        .filter_map(|(role, health)| Some((role, health.take_stop()?)))
        .collect()
    }
}

impl VoiceChanger for AudioEngine {
    fn set_voice(&self, chain: Option<VoiceChain>, conversion: Option<Conversion>) {
        self.microphone.voice.set(chain);
        self.ai.route(conversion.as_ref());
    }

    fn set_hearing_voice(&self, enabled: bool) {
        self.monitor
            .controls
            .mic_enabled
            .store(enabled, Ordering::Relaxed);
    }
}

#[async_trait]
impl VoiceRecorder for AudioEngine {
    fn start_recording(&self) {
        self.recorder.start();
    }

    fn stop_recording(&self) -> Vec<f32> {
        self.recorder.stop()
    }

    fn recorded(&self) -> Vec<f32> {
        self.recorder.peek()
    }

    async fn decode_file(&self, path: &str) -> Result<Vec<f32>, Failure> {
        let path = path.to_owned();
        tokio::task::spawn_blocking(move || clip::load_speech(&path))
            .await
            .map_err(|error| Failure::because(TextKey::SoundOpenFailed, error))?
    }
}

impl AudioMix for AudioEngine {
    fn set_mic_volume(&self, volume: f32) {
        self.game.controls.mic_volume.store(volume);
        self.monitor.controls.mic_volume.store(volume);
    }

    fn set_game_volume(&self, volume: f32) {
        self.game.controls.sound_volume.store(volume);
    }

    fn set_monitor_volume(&self, volume: f32) {
        self.monitor.controls.sound_volume.store(volume);
    }

    fn set_mic_enabled(&self, enabled: bool) {
        self.game
            .controls
            .mic_enabled
            .store(enabled, Ordering::Relaxed);
    }

    fn mic_level(&self) -> f32 {
        self.microphone.level.load()
    }

    fn game_output_level(&self) -> f32 {
        self.game.controls.peak.load()
    }
}

#[cfg(test)]
mod tests {
    use instantpad_application::message::Message;
    use instantpad_test_support::contract;
    use instantpad_test_support::fakes::{FakeConverter, RecordingEvents};

    use super::*;
    use crate::ai::tests::conversion;
    use crate::clip::tests::wav;
    use crate::mixer::Mixer;

    impl AudioEngine {
        /// Opens both mixers without a device, and hands out the halves a stream would read.
        fn attach_mixers(&self) -> (Mixer, Mixer) {
            let attach = |output: &Output| {
                let (mixer, handle) = mixer::pair(output.controls.clone(), output.mic.clone());
                *output.voices.lock().unwrap() = Some(handle);
                mixer
            };
            (attach(&self.game), attach(&self.monitor))
        }
    }

    /// A cache entry that holds a clip of `samples` samples.
    fn cached(clips: &mut Clips, path: &str, samples: usize) -> Arc<SoundClip> {
        let clip = Arc::new(SoundClip::new(vec![0.0; samples]));
        assert!(clips.entry(path).set(clip.clone()).is_ok());
        clip
    }

    #[test]
    fn the_sound_used_longest_ago_goes_first_when_the_budget_is_full() {
        let mut clips = Clips::new(2000);
        drop(cached(&mut clips, "old", 200));
        drop(cached(&mut clips, "middle", 200));
        drop(cached(&mut clips, "new", 200));
        clips.entry("old");

        clips.trim();

        let kept: Vec<_> = ["old", "middle", "new"]
            .into_iter()
            .filter(|path| clips.entries.contains_key(*path))
            .collect();
        assert_eq!(kept, ["old", "new"]);
    }

    #[test]
    fn a_sound_that_is_playing_stays_even_over_the_budget() {
        let mut clips = Clips::new(1000);
        let playing = cached(&mut clips, "playing", 200);
        drop(cached(&mut clips, "idle", 200));

        clips.trim();

        assert!(clips.entries.contains_key("playing"));
        assert!(!clips.entries.contains_key("idle"));
        assert_eq!(Arc::strong_count(&playing), 2);
    }

    #[test]
    fn sounds_within_the_budget_are_all_kept() {
        let mut clips = Clips::new(1_000_000);
        drop(cached(&mut clips, "a", 200));
        drop(cached(&mut clips, "b", 200));

        clips.trim();

        assert_eq!(clips.entries.len(), 2);
    }

    fn engine() -> AudioEngine {
        AudioEngine::new(
            Arc::new(RecordingEvents::default()),
            Arc::new(FakeConverter::default()),
        )
    }

    fn heard(mixer: &mut Mixer) -> f32 {
        let mut block = [0.0; 100];
        mixer.render(&mut block);
        block[0]
    }

    #[tokio::test]
    async fn the_engine_passes_the_sound_player_contract() {
        let engine = engine();
        engine.attach_mixers();
        let file = wav(48_000, &[8192, 8192], 4800);

        contract::sound_player(&engine, file.path()).await;
    }

    #[tokio::test]
    async fn the_display_follows_a_sound_until_it_stops() {
        let engine = engine();
        engine.attach_mixers();
        let file = wav(48_000, &[8192, 8192], 4800);
        let tag = PlaybackTag::of("a");
        assert_eq!(engine.now_playing(), None);

        engine.play(file.path(), 1.0, tag, None).await.unwrap();

        let playing = engine.now_playing().unwrap();
        assert_eq!(playing.duration, 0.1);
        assert!(!playing.peaks.is_empty());

        engine.stop(tag);
        assert_eq!(engine.now_playing(), None);
    }

    #[tokio::test]
    async fn the_display_shows_the_newest_sound_and_goes_back_when_it_ends() {
        let engine = engine();
        engine.attach_mixers();
        let short = wav(48_000, &[8192, 8192], 4800);
        let long = wav(48_000, &[8192, 8192], 9600);
        let (first, second) = (PlaybackTag::of("a"), PlaybackTag::of("b"));
        engine.play(long.path(), 1.0, first, None).await.unwrap();
        engine.play(short.path(), 1.0, second, None).await.unwrap();

        assert_eq!(engine.now_playing().unwrap().duration, 0.1);

        engine.stop(second);

        assert_eq!(engine.now_playing().unwrap().duration, 0.2);
    }

    #[tokio::test]
    async fn playing_reaches_the_game_and_the_monitor() {
        let engine = engine();
        let (mut game, mut monitor) = engine.attach_mixers();
        let file = wav(48_000, &[8192, 8192], 4800);

        engine
            .play(file.path(), 1.0, PlaybackTag::of("a"), None)
            .await
            .unwrap();

        assert!(heard(&mut game) > 0.0);
        assert!(heard(&mut monitor) > 0.0);
    }

    #[tokio::test]
    async fn previewing_reaches_only_the_monitor() {
        let engine = engine();
        let (mut game, mut monitor) = engine.attach_mixers();
        let file = wav(48_000, &[8192, 8192], 4800);

        engine
            .preview(file.path(), 1.0, PlaybackTag::of("a"), None)
            .await
            .unwrap();

        assert_eq!(heard(&mut game), 0.0);
        assert!(heard(&mut monitor) > 0.0);
    }

    #[tokio::test]
    async fn pressing_again_restarts_the_sound() {
        let engine = engine();
        let (mut game, _) = engine.attach_mixers();
        let file = wav(48_000, &[8192, 8192], 4800);
        let tag = PlaybackTag::of("a");

        engine.play(file.path(), 1.0, tag, None).await.unwrap();
        engine.play(file.path(), 1.0, tag, None).await.unwrap();

        // Two overlapping copies would be twice as loud.
        assert!((heard(&mut game) - 8192.0 / 32768.0).abs() < 1e-3);
    }

    #[tokio::test]
    async fn a_mixer_with_no_device_takes_no_voices() {
        let engine = engine();
        let file = wav(48_000, &[8192, 8192], 4800);
        let tag = PlaybackTag::of("a");

        engine.play(file.path(), 1.0, tag, None).await.unwrap();

        assert!(!engine.is_playing(tag));
    }

    #[tokio::test]
    async fn closing_an_output_drops_its_voices_and_takes_no_more() {
        let engine = engine();
        engine.attach_mixers();
        let file = wav(48_000, &[8192, 8192], 4800);
        let tag = PlaybackTag::of("a");
        engine.play(file.path(), 1.0, tag, None).await.unwrap();

        engine.set_game_output(None).await.unwrap();
        engine.set_monitor_output(None).await.unwrap();

        assert!(!engine.is_playing(tag));
        engine.play(file.path(), 1.0, tag, None).await.unwrap();
        assert!(!engine.is_playing(tag));
    }

    #[tokio::test]
    async fn an_output_that_cannot_be_opened_is_closed_and_unavailable() {
        let engine = engine();
        engine.attach_mixers();
        let file = wav(48_000, &[8192, 8192], 4800);
        let tag = PlaybackTag::of("a");
        engine.play(file.path(), 1.0, tag, None).await.unwrap();

        let failure = engine.set_game_output(Some("nowhere:nothing")).await;

        assert!(matches!(
            failure.unwrap_err().0,
            instantpad_application::message::Message::Text {
                key: TextKey::DeviceUnavailable,
                ..
            }
        ));
        // The monitor still has the sound, and the game no longer does.
        assert!(
            engine
                .monitor
                .with_voices(|voices| voices.is_playing(tag))
                .unwrap()
        );
        assert!(
            engine
                .game
                .with_voices(|voices| voices.is_playing(tag))
                .is_none()
        );
    }

    #[tokio::test]
    async fn a_microphone_that_cannot_be_opened_is_unavailable() {
        let failure = engine().set_microphone(Some("nowhere:nothing")).await;

        assert!(matches!(
            failure.unwrap_err().0,
            instantpad_application::message::Message::Text {
                key: TextKey::DeviceUnavailable,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn the_engine_passes_the_audio_router_contract() {
        contract::audio_router(&engine()).await;
    }

    /// What the error callback of the stream `slot` has open does when the device goes away.
    fn unplug(slot: &HealthSlot) {
        let health = slot.0.lock().unwrap().clone();
        devices::watch(health)(cpal::Error::new(cpal::ErrorKind::DeviceNotAvailable));
    }

    #[test]
    fn a_stream_that_stops_by_itself_is_told_once_with_its_role() {
        let engine = engine();

        unplug(&engine.microphone.health);
        unplug(&engine.monitor.health);

        assert_eq!(
            engine.take_stopped(),
            [
                (DeviceRole::Microphone, StreamStop::Lost),
                (DeviceRole::MonitorOutput, StreamStop::Lost)
            ]
        );
        assert!(engine.take_stopped().is_empty());
    }

    #[tokio::test]
    async fn a_stream_that_was_replaced_no_longer_speaks_for_its_role() {
        let engine = engine();
        let old = engine.game.health.0.lock().unwrap().clone();

        engine.set_game_output(None).await.unwrap();
        devices::watch(old)(cpal::Error::new(cpal::ErrorKind::DeviceNotAvailable));

        assert!(engine.take_stopped().is_empty());
    }

    #[tokio::test]
    async fn a_failed_load_is_tried_again_next_time() {
        let engine = engine();
        let file = wav(48_000, &[8192, 8192], 4800);
        let path = format!("{}.later", file.path());

        assert!(engine.preload(&path).await.is_err());
        std::fs::copy(file.path(), &path).unwrap();
        let loaded = engine.preload(&path).await;
        std::fs::remove_file(&path).unwrap();

        assert_eq!(loaded, Ok(()));
    }

    #[tokio::test]
    async fn a_loaded_sound_is_cached_until_it_is_forgotten() {
        let engine = engine();
        let copy = wav(48_000, &[8192, 8192], 4800);
        let path = format!("{}.cached", copy.path());
        std::fs::copy(copy.path(), &path).unwrap();
        engine.preload(&path).await.unwrap();
        std::fs::remove_file(&path).unwrap();

        assert_eq!(engine.preload(&path).await, Ok(()));

        engine.forget(&path);
        assert!(engine.preload(&path).await.is_err());
    }

    #[tokio::test]
    async fn the_volumes_reach_the_mixers() {
        let engine = engine();
        let (mut game, mut monitor) = engine.attach_mixers();
        let file = wav(48_000, &[16384, 16384], 4800);
        engine
            .play(file.path(), 1.0, PlaybackTag::of("a"), None)
            .await
            .unwrap();

        engine.set_game_volume(0.5);
        engine.set_monitor_volume(0.25);

        assert!((heard(&mut game) - 0.25).abs() < 1e-3);
        assert!((heard(&mut monitor) - 0.125).abs() < 1e-3);
        assert!(engine.game_output_level() > 0.0);
    }

    #[test]
    fn the_meters_start_silent() {
        let engine = engine();

        assert_eq!(engine.mic_level(), 0.0);
        assert_eq!(engine.game_output_level(), 0.0);
    }

    #[test]
    fn a_conversion_starts_the_converter_with_its_speakers() {
        let converter = Arc::new(FakeConverter::default());
        let engine = AudioEngine::new(Arc::new(RecordingEvents::default()), converter.clone());

        engine.set_voice(None, Some(conversion()));

        assert_eq!(*converter.started.lock().unwrap(), [conversion()]);
    }

    #[test]
    fn the_natural_voice_does_not_start_the_converter() {
        let converter = Arc::new(FakeConverter::default());
        let engine = AudioEngine::new(Arc::new(RecordingEvents::default()), converter.clone());

        engine.set_voice(None, None);

        assert!(converter.started.lock().unwrap().is_empty());
    }

    #[test]
    fn recording_with_no_microphone_open_gives_nothing() {
        let engine = engine();

        engine.start_recording();

        assert!(engine.stop_recording().is_empty());
    }

    #[tokio::test]
    async fn a_file_is_decoded_to_mono_at_the_engine_rate() {
        let file = wav(24_000, &[8192, 0], 12_000);

        let speech = engine().decode_file(file.path()).await.unwrap();

        assert!(speech.len().abs_diff(24_000) <= 1024, "{}", speech.len());
    }

    #[tokio::test]
    async fn a_file_that_cannot_be_decoded_is_refused() {
        let failure = engine().decode_file("/no/such/voice.wav").await;

        assert!(matches!(
            failure.unwrap_err().0,
            Message::Text {
                key: TextKey::SoundOpenFailed,
                ..
            }
        ));
    }
}
