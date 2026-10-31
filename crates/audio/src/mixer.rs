//! Sums the voices and an optional live input, applying gain and a soft limiter.
//!
//! A mixer comes in two halves. [`Mixer`] lives in the audio callback and never allocates,
//! frees or blocks. [`MixerHandle`] stays with the control side and talks to it through
//! lock-free queues.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use instantpad_application::ports::PlaybackTag;
use rtrb::{Consumer, Producer, RingBuffer};

use crate::clip::SoundClip;
use crate::controls::Controls;
use crate::mic::MicSlot;

/// Sounds playing at once on one mixer. Starting more drops the newest.
const MAX_VOICES: usize = 64;
const COMMAND_CAPACITY: usize = 256;
/// Every voice is either still queued, playing, or on its way back, so this never fills up.
const RETURN_CAPACITY: usize = COMMAND_CAPACITY + MAX_VOICES;
const LIMITER_KNEE: f32 = 0.8;
/// How much of the microphone is mixed in one go, so no buffer has to be allocated for it.
const MIC_BLOCK: usize = 1024;

/// One playback of a clip on one mixer.
struct Voice {
    id: u64,
    tag: PlaybackTag,
    clip: Arc<SoundClip>,
    volume: f32,
    position: usize,
}

impl Voice {
    fn finished(&self) -> bool {
        self.position >= self.clip.samples().len()
    }

    fn mix_into(&mut self, block: &mut [f32]) {
        let rest = &self.clip.samples()[self.position..];
        let mixed = block.len().min(rest.len());
        for (out, sample) in block.iter_mut().zip(rest) {
            *out += sample * self.volume;
        }
        self.position += mixed;
    }
}

enum Command {
    Add(Voice),
    Remove(PlaybackTag),
    Clear,
}

pub(crate) struct Mixer {
    controls: Arc<Controls>,
    mic: Option<MicSlot>,
    commands: Consumer<Command>,
    /// Finished voices go back to the control side, because dropping the last reference to a
    /// clip frees it, and the callback must not free.
    returned: Producer<Voice>,
    voices: Vec<Voice>,
}

pub(crate) struct MixerHandle {
    commands: Producer<Command>,
    returned: Consumer<Voice>,
    playing: Vec<(u64, PlaybackTag)>,
    next_id: u64,
}

/// `mic` is the live input to mix in, or `None` for a mixer of sounds only.
pub(crate) fn pair(controls: Arc<Controls>, mic: Option<MicSlot>) -> (Mixer, MixerHandle) {
    let (commands_in, commands) = RingBuffer::new(COMMAND_CAPACITY);
    let (returned, returned_out) = RingBuffer::new(RETURN_CAPACITY);
    (
        Mixer {
            controls,
            mic,
            commands,
            returned,
            voices: Vec::with_capacity(MAX_VOICES),
        },
        MixerHandle {
            commands: commands_in,
            returned: returned_out,
            playing: Vec::new(),
            next_id: 0,
        },
    )
}

impl MixerHandle {
    pub(crate) fn add(&mut self, clip: Arc<SoundClip>, volume: f32, tag: PlaybackTag) {
        let id = self.next_id;
        self.next_id += 1;
        let voice = Voice {
            id,
            tag,
            clip,
            volume,
            position: 0,
        };
        // A full queue means the callback is not running, and this voice would never finish.
        if self.commands.push(Command::Add(voice)).is_ok() {
            self.playing.push((id, tag));
        }
    }

    pub(crate) fn remove(&mut self, tag: PlaybackTag) {
        self.playing.retain(|&(_, playing)| playing != tag);
        let _ = self.commands.push(Command::Remove(tag));
    }

    pub(crate) fn clear(&mut self) {
        self.playing.clear();
        let _ = self.commands.push(Command::Clear);
    }

    pub(crate) fn is_playing(&mut self, tag: PlaybackTag) -> bool {
        while let Ok(finished) = self.returned.pop() {
            self.playing.retain(|&(id, _)| id != finished.id);
        }
        self.playing.iter().any(|&(_, playing)| playing == tag)
    }
}

impl Mixer {
    /// Fills the whole block: a live output must never run out.
    pub(crate) fn render(&mut self, block: &mut [f32]) {
        self.apply_commands();

        block.fill(0.0);
        self.mix_voices(block);
        self.apply_sound_volume(block);
        self.mix_mic(block);
        self.controls.peak.store(limit(block));
    }

    fn apply_commands(&mut self) {
        while let Ok(command) = self.commands.pop() {
            match command {
                Command::Add(voice) if self.voices.len() < MAX_VOICES => self.voices.push(voice),
                Command::Add(voice) => self.give_back(voice),
                Command::Remove(tag) => self.retire(|voice| voice.tag == tag),
                Command::Clear => self.retire(|_| true),
            }
        }
    }

    fn mix_voices(&mut self, block: &mut [f32]) {
        for voice in &mut self.voices {
            voice.mix_into(block);
        }
        self.retire(Voice::finished);
    }

    fn apply_sound_volume(&self, block: &mut [f32]) {
        let volume = self.controls.sound_volume.load();
        if volume != 1.0 {
            block.iter_mut().for_each(|sample| *sample *= volume);
        }
    }

    fn mix_mic(&self, block: &mut [f32]) {
        let Some(mic) = &self.mic else { return };
        // It only fails while the microphone is being swapped, and one block without it is fine.
        let Ok(mut mic) = mic.try_lock() else { return };
        let Some(reader) = mic.as_mut() else { return };

        // Always drained, so the mic stays current while it is muted.
        let enabled = self.controls.mic_enabled.load(Ordering::Relaxed);
        let volume = self.controls.mic_volume.load();
        let mut heard = [0.0; MIC_BLOCK];
        for part in block.chunks_mut(MIC_BLOCK) {
            let read = reader.read(&mut heard[..part.len()]);
            if enabled {
                for (out, sample) in part.iter_mut().zip(&heard[..read]) {
                    *out += sample * volume;
                }
            }
        }
    }

    fn retire(&mut self, mut leaves: impl FnMut(&Voice) -> bool) {
        let mut index = 0;
        while index < self.voices.len() {
            if leaves(&self.voices[index]) {
                let voice = self.voices.swap_remove(index);
                self.give_back(voice);
            } else {
                index += 1;
            }
        }
    }

    fn give_back(&mut self, voice: Voice) {
        let _ = self.returned.push(voice);
    }
}

/// Soft limits the block and returns its peak.
fn limit(block: &mut [f32]) -> f32 {
    let mut peak = 0.0_f32;
    for sample in block {
        *sample = soft_clip(*sample);
        peak = peak.max(sample.abs());
    }
    peak
}

/// Transparent below the knee, then eases towards 1 so overlapping sounds never hard clip.
fn soft_clip(sample: f32) -> f32 {
    let magnitude = sample.abs();
    if magnitude <= LIMITER_KNEE {
        return sample;
    }

    let over = (magnitude - LIMITER_KNEE) / (1.0 - LIMITER_KNEE);
    let shaped = LIMITER_KNEE + (1.0 - LIMITER_KNEE) * (over / (1.0 + over));
    shaped.copysign(sample)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::format::samples_for;
    use crate::mic;

    fn clip(level: f32, samples: usize) -> Arc<SoundClip> {
        Arc::new(SoundClip::new(vec![level; samples]))
    }

    fn mixer() -> (Mixer, MixerHandle, Arc<Controls>) {
        let controls = Arc::new(Controls::new());
        let (mixer, handle) = pair(controls.clone(), None);
        (mixer, handle, controls)
    }

    #[test]
    fn a_live_output_always_fills_the_block() {
        let (mut mixer, ..) = mixer();
        let mut block = [1.0; 960];

        mixer.render(&mut block);

        assert!(block.iter().all(|&sample| sample == 0.0));
    }

    #[test]
    fn finished_voices_leave_the_mixer() {
        let (mut mixer, mut handle, _) = mixer();
        let tag = PlaybackTag::of("a");
        handle.add(clip(0.1, 100), 1.0, tag);
        assert!(handle.is_playing(tag));

        mixer.render(&mut [0.0; 200]);

        assert!(!handle.is_playing(tag));
    }

    #[test]
    fn overlapping_sounds_never_clip() {
        let (mut mixer, mut handle, controls) = mixer();
        let tag = PlaybackTag::of("a");
        handle.add(clip(0.9, 4800), 1.0, tag);
        handle.add(clip(0.9, 4800), 1.0, tag);

        let mut block = [0.0; 4800];
        mixer.render(&mut block);

        assert!(block.iter().all(|sample| sample.abs() <= 1.0));
        assert!(controls.peak.load() > 0.8);
    }

    #[test]
    fn the_limiter_leaves_quiet_samples_alone_and_keeps_the_sign() {
        assert_eq!(soft_clip(0.8), 0.8);
        assert_eq!(soft_clip(-0.3), -0.3);
        assert!((soft_clip(1.0) - 0.9).abs() < 1e-6);
        assert!((soft_clip(-1.0) + 0.9).abs() < 1e-6);
    }

    #[test]
    fn sound_volume_scales_the_voices() {
        let (mut mixer, mut handle, controls) = mixer();
        controls.sound_volume.store(0.5);
        handle.add(clip(0.4, 4800), 1.0, PlaybackTag::of("a"));

        let mut block = [0.0; 100];
        mixer.render(&mut block);

        assert!((block[0] - 0.2).abs() < 1e-3);
    }

    #[test]
    fn a_voice_volume_scales_only_that_voice() {
        let (mut mixer, mut handle, _) = mixer();
        handle.add(clip(0.4, 4800), 0.5, PlaybackTag::of("a"));

        let mut block = [0.0; 100];
        mixer.render(&mut block);

        assert!((block[0] - 0.2).abs() < 1e-3);
    }

    #[test]
    fn stopping_by_tag_leaves_other_sounds_playing() {
        let (mut mixer, mut handle, _) = mixer();
        let (stopped, kept) = (PlaybackTag::of("stopped"), PlaybackTag::of("kept"));
        handle.add(clip(0.1, 4800), 1.0, stopped);
        handle.add(clip(0.2, 4800), 1.0, kept);

        handle.remove(stopped);
        let mut block = [0.0; 100];
        mixer.render(&mut block);

        assert!(!handle.is_playing(stopped));
        assert!(handle.is_playing(kept));
        assert!((block[0] - 0.2).abs() < 1e-3);
    }

    #[test]
    fn clearing_stops_every_sound() {
        let (mut mixer, mut handle, _) = mixer();
        let tag = PlaybackTag::of("a");
        handle.add(clip(0.1, 4800), 1.0, tag);

        handle.clear();
        let mut block = [1.0; 100];
        mixer.render(&mut block);

        assert!(!handle.is_playing(tag));
        assert!(block.iter().all(|&sample| sample == 0.0));
    }

    #[test]
    fn voices_beyond_the_capacity_are_dropped_and_still_come_back() {
        let (mut mixer, mut handle, _) = mixer();
        let tag = PlaybackTag::of("a");
        for _ in 0..=MAX_VOICES {
            handle.add(clip(0.001, 100), 1.0, tag);
        }

        mixer.render(&mut [0.0; 50]);
        mixer.render(&mut [0.0; 200]);

        assert!(!handle.is_playing(tag));
    }

    fn mixer_with_mic(level: f32) -> (Mixer, Arc<Controls>) {
        let (mut writer, reader) = mic::ring();
        assert!(writer.push_partial_slice(&vec![level; 9600]).1.is_empty());
        let controls = Arc::new(Controls::new());
        let slot = Arc::new(Mutex::new(Some(reader)));
        (pair(controls.clone(), Some(slot)).0, controls)
    }

    #[test]
    fn a_muted_mic_stays_current_and_silent() {
        let (mut mixer, controls) = mixer_with_mic(0.5);
        controls.mic_enabled.store(false, Ordering::Relaxed);

        let mut block = [0.0; 1920];
        mixer.render(&mut block);
        assert!(block.iter().all(|&sample| sample == 0.0));

        controls.mic_enabled.store(true, Ordering::Relaxed);
        mixer.render(&mut block);
        assert!((block[0] - 0.5).abs() < 1e-3);
    }

    #[test]
    fn the_mic_volume_scales_the_mic() {
        let (mut mixer, controls) = mixer_with_mic(0.5);
        controls.mic_volume.store(0.5);

        let mut block = [0.0; 100];
        mixer.render(&mut block);

        assert!((block[0] - 0.25).abs() < 1e-3);
    }

    #[test]
    fn the_mic_is_mixed_across_blocks_longer_than_its_scratch_space() {
        let (mut mixer, _) = mixer_with_mic(0.5);

        let mut block = [0.0; MIC_BLOCK * 2 + 10];
        mixer.render(&mut block);

        assert!(block.iter().all(|&sample| (sample - 0.5).abs() < 1e-3));
    }

    #[test]
    fn a_mixer_without_a_microphone_is_silent_without_one() {
        let controls = Arc::new(Controls::new());
        let (mut mixer, _) = pair(controls, Some(Arc::new(Mutex::new(None))));
        let mut block = [1.0; samples_for(10)];

        mixer.render(&mut block);

        assert!(block.iter().all(|&sample| sample == 0.0));
    }
}
