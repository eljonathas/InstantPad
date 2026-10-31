//! Routing:
//!   mic + sounds -> game output (a virtual cable, what the game hears)
//!   sounds       -> monitor output (what you hear)

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use instantpad_application::message::{Failure, TextKey};
use instantpad_application::ports::{
    AudioDevice, AudioMix, AudioRouter, EventSink, PlaybackTag, SoundPlayer,
};
use tokio::sync::OnceCell;

use crate::clip::SoundClip;
use crate::controls::{AtomicF32, Controls};
use crate::devices;
use crate::mic::{self, MicSlot};
use crate::mixer::{self, MixerHandle};
use crate::role::Role;

type Clips = HashMap<String, Arc<OnceCell<Arc<SoundClip>>>>;

/// One of the two outputs and the mixer that feeds it.
struct Output {
    role: Role<cpal::Stream>,
    controls: Arc<Controls>,
    /// The microphone to mix in, for the output that goes to the game.
    mic: Option<MicSlot>,
    /// `None` while no device is open. A mixer nobody reads would never finish its voices.
    voices: Mutex<Option<MixerHandle>>,
}

impl Output {
    fn new(name: &str, mic: Option<MicSlot>) -> Self {
        Self {
            role: Role::spawn(name),
            controls: Arc::new(Controls::new()),
            mic,
            voices: Mutex::new(None),
        }
    }

    fn with_voices<R>(&self, work: impl FnOnce(&mut MixerHandle) -> R) -> Option<R> {
        self.voices.lock().unwrap().as_mut().map(work)
    }

    async fn open(
        &self,
        device_id: Option<&str>,
        events: &Arc<dyn EventSink>,
    ) -> Result<(), Failure> {
        // The old device is closed first, and its voices would never finish on the new one.
        *self.voices.lock().unwrap() = None;

        let Some(id) = device_id.map(str::to_owned) else {
            return self.role.run(close).await;
        };

        let (mixer, handle) = mixer::pair(self.controls.clone(), self.mic.clone());
        let events = events.clone();
        self.role
            .run(move |stream| {
                *stream = None;
                *stream = Some(devices::open_output(&id, mixer, events)?);
                Ok(())
            })
            .await?;

        *self.voices.lock().unwrap() = Some(handle);
        Ok(())
    }
}

fn close(stream: &mut Option<cpal::Stream>) -> Result<(), Failure> {
    *stream = None;
    Ok(())
}

struct Microphone {
    role: Role<cpal::Stream>,
    reader: MicSlot,
    level: Arc<AtomicF32>,
}

/// Plays sounds and routes the microphone, with nothing in the audio callbacks that locks,
/// allocates or waits.
pub struct AudioEngine {
    events: Arc<dyn EventSink>,
    clips: Mutex<Clips>,
    microphone: Microphone,
    game: Output,
    monitor: Output,
}

impl AudioEngine {
    pub fn new(events: Arc<dyn EventSink>) -> Self {
        let reader = mic::empty_slot();
        Self {
            events,
            clips: Mutex::default(),
            microphone: Microphone {
                role: Role::spawn("microphone"),
                reader: reader.clone(),
                level: Arc::new(AtomicF32::new(0.0)),
            },
            game: Output::new("game output", Some(reader)),
            monitor: Output::new("monitor output", None),
        }
    }

    async fn load(&self, path: &str) -> Result<Arc<SoundClip>, Failure> {
        let entry = self
            .clips
            .lock()
            .unwrap()
            .entry(path.to_owned())
            .or_default()
            .clone();

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
        Ok(clip.clone())
    }

    fn play_on(&self, output: &Output, clip: &Arc<SoundClip>, volume: f32, tag: PlaybackTag) {
        output.with_voices(|voices| voices.add(clip.clone(), volume, tag));
    }
}

#[async_trait]
impl SoundPlayer for AudioEngine {
    async fn preload(&self, path: &str) -> Result<(), Failure> {
        self.load(path).await.map(drop)
    }

    async fn play(&self, path: &str, volume: f32, tag: PlaybackTag) -> Result<(), Failure> {
        let clip = self.load(path).await?;
        self.stop(tag);
        self.play_on(&self.game, &clip, volume, tag);
        self.play_on(&self.monitor, &clip, volume, tag);
        Ok(())
    }

    async fn preview(&self, path: &str, volume: f32, tag: PlaybackTag) -> Result<(), Failure> {
        let clip = self.load(path).await?;
        self.stop(tag);
        self.play_on(&self.monitor, &clip, volume, tag);
        Ok(())
    }

    fn forget(&self, path: &str) {
        self.clips.lock().unwrap().remove(path);
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
        *microphone.reader.lock().unwrap() = None;
        microphone.level.store(0.0);

        let Some(id) = device_id.map(str::to_owned) else {
            return microphone.role.run(close).await;
        };

        let (ring, reader) = mic::ring();
        let level = microphone.level.clone();
        let events = self.events.clone();
        microphone
            .role
            .run(move |stream| {
                *stream = None;
                *stream = Some(devices::open_capture(&id, ring, level, events)?);
                Ok(())
            })
            .await?;

        *microphone.reader.lock().unwrap() = Some(reader);
        Ok(())
    }

    async fn set_game_output(&self, device_id: Option<&str>) -> Result<(), Failure> {
        self.game.open(device_id, &self.events).await
    }

    async fn set_monitor_output(&self, device_id: Option<&str>) -> Result<(), Failure> {
        self.monitor.open(device_id, &self.events).await
    }
}

impl AudioMix for AudioEngine {
    fn set_mic_volume(&self, volume: f32) {
        self.game.controls.mic_volume.store(volume);
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
    use instantpad_test_support::contract;
    use instantpad_test_support::fakes::RecordingEvents;

    use super::*;
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

    fn engine() -> AudioEngine {
        AudioEngine::new(Arc::new(RecordingEvents::default()))
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
    async fn playing_reaches_the_game_and_the_monitor() {
        let engine = engine();
        let (mut game, mut monitor) = engine.attach_mixers();
        let file = wav(48_000, &[8192, 8192], 4800);

        engine
            .play(file.path(), 1.0, PlaybackTag::of("a"))
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
            .preview(file.path(), 1.0, PlaybackTag::of("a"))
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

        engine.play(file.path(), 1.0, tag).await.unwrap();
        engine.play(file.path(), 1.0, tag).await.unwrap();

        // Two overlapping copies would be twice as loud.
        assert!((heard(&mut game) - 8192.0 / 32768.0).abs() < 1e-3);
    }

    #[tokio::test]
    async fn a_mixer_with_no_device_takes_no_voices() {
        let engine = engine();
        let file = wav(48_000, &[8192, 8192], 4800);
        let tag = PlaybackTag::of("a");

        engine.play(file.path(), 1.0, tag).await.unwrap();

        assert!(!engine.is_playing(tag));
    }

    #[tokio::test]
    async fn closing_an_output_drops_its_voices_and_takes_no_more() {
        let engine = engine();
        engine.attach_mixers();
        let file = wav(48_000, &[8192, 8192], 4800);
        let tag = PlaybackTag::of("a");
        engine.play(file.path(), 1.0, tag).await.unwrap();

        engine.set_game_output(None).await.unwrap();
        engine.set_monitor_output(None).await.unwrap();

        assert!(!engine.is_playing(tag));
        engine.play(file.path(), 1.0, tag).await.unwrap();
        assert!(!engine.is_playing(tag));
    }

    #[tokio::test]
    async fn an_output_that_cannot_be_opened_is_closed_and_unavailable() {
        let engine = engine();
        engine.attach_mixers();
        let file = wav(48_000, &[8192, 8192], 4800);
        let tag = PlaybackTag::of("a");
        engine.play(file.path(), 1.0, tag).await.unwrap();

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
            .play(file.path(), 1.0, PlaybackTag::of("a"))
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
}
