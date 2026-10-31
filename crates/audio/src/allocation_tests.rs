//! Proves the audio callbacks neither allocate nor free, by counting what the allocator is
//! asked for while they run.

// A global allocator is unsafe to write, and this is the only file that needs one.
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::Arc;

use instantpad_application::ports::PlaybackTag;
use rtrb::RingBuffer;

use crate::capture::CaptureProcessor;
use crate::clip::SoundClip;
use crate::controls::{AtomicF32, Controls};
use crate::format::{CHANNELS, SAMPLE_RATE, samples_for};
use crate::mic;
use crate::mixer::{self, Mixer, MixerHandle};
use crate::playback::{Conversion, Playback};

thread_local! {
    static COUNTING: Cell<bool> = const { Cell::new(false) };
    static REQUESTS: Cell<usize> = const { Cell::new(0) };
}

struct CountingAllocator;

fn count() {
    if COUNTING.try_with(Cell::get).unwrap_or(false) {
        let _ = REQUESTS.try_with(|requests| requests.set(requests.get() + 1));
    }
}

// SAFETY: every call is forwarded unchanged to the system allocator.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        count();
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        count();
        unsafe { System.realloc(pointer, layout, size) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

/// How many times `work` asked the allocator to allocate, reallocate or free.
fn requests_during(work: impl FnOnce()) -> usize {
    REQUESTS.set(0);
    COUNTING.set(true);
    work();
    COUNTING.set(false);
    REQUESTS.get()
}

fn clip(level: f32, frames: usize) -> Arc<SoundClip> {
    Arc::new(SoundClip::new(vec![level; frames * CHANNELS]))
}

#[test]
fn the_counter_sees_an_allocation() {
    assert!(requests_during(|| drop(Vec::<u8>::with_capacity(64))) > 0);
}

/// A game mixer with a microphone that has more than a block of backlog.
fn game_mixer() -> (Mixer, MixerHandle, Arc<Controls>) {
    let (mut writer, reader) = mic::ring();
    assert!(
        writer
            .push_partial_slice(&vec![0.3; samples_for(500)])
            .1
            .is_empty()
    );
    let controls = Arc::new(Controls::new());
    let slot = mic::empty_slot();
    *slot.lock().unwrap() = Some(reader);
    let (mixer, handle) = mixer::pair(controls.clone(), Some(slot));
    (mixer, handle, controls)
}

#[test]
fn mixing_allocates_and_frees_nothing() {
    let (mut mixer, mut handle, controls) = game_mixer();
    controls.sound_volume.store(0.7);
    let (kept, stopped) = (PlaybackTag::of("kept"), PlaybackTag::of("stopped"));
    handle.add(clip(0.6, 10_000), 0.9, kept);
    handle.add(clip(0.6, 10_000), 0.9, stopped);
    handle.add(clip(0.5, 100), 1.0, kept);
    handle.remove(stopped);
    let mut block = vec![0.0; samples_for(30)];

    let requests = requests_during(|| {
        // The first block takes the commands, and the rest play the voices out and drain the mic.
        for _ in 0..8 {
            mixer.render(&mut block);
        }
    });

    assert_eq!(requests, 0);
}

#[test]
fn stopping_everything_and_starting_more_than_fit_allocates_and_frees_nothing() {
    let (mut mixer, mut handle, _) = game_mixer();
    for index in 0..100 {
        handle.add(clip(0.1, 1000), 1.0, PlaybackTag(index));
    }
    let mut block = vec![0.0; samples_for(10)];
    mixer.render(&mut block);
    handle.clear();
    handle.add(clip(0.1, 1000), 1.0, PlaybackTag(0));

    let requests = requests_during(|| {
        mixer.render(&mut block);
        mixer.render(&mut block);
    });

    assert_eq!(requests, 0);
}

#[test]
fn capturing_allocates_and_frees_nothing() {
    let (ring, _reader) = RingBuffer::new(SAMPLE_RATE as usize * CHANNELS);
    let level = Arc::new(AtomicF32::new(0.0));
    let mut processor = CaptureProcessor::new(3, 44_100, ring, level).unwrap();
    let callback = vec![0.2_f32; 441 * 3];

    let requests = requests_during(|| {
        for _ in 0..50 {
            processor.push(&callback);
        }
    });

    assert_eq!(requests, 0);
}

#[test]
fn capturing_integers_at_the_engine_rate_allocates_and_frees_nothing() {
    let (ring, _reader) = RingBuffer::new(SAMPLE_RATE as usize * CHANNELS);
    let level = Arc::new(AtomicF32::new(0.0));
    let mut processor = CaptureProcessor::new(2, SAMPLE_RATE, ring, level).unwrap();
    let callback = vec![1000_i16; 480 * 2];

    let requests = requests_during(|| {
        for _ in 0..50 {
            processor.push(&callback);
        }
    });

    assert_eq!(requests, 0);
}

#[test]
fn playing_on_a_device_that_needs_conversion_allocates_and_frees_nothing() {
    let (mixer, mut handle, _) = game_mixer();
    handle.add(clip(0.4, 20_000), 1.0, PlaybackTag::of("a"));
    let mut playback = Playback::new(mixer, Some(Conversion::new(6, 44_100).unwrap()));
    let mut block = vec![0.0; 6 * 441];

    let requests = requests_during(|| {
        for _ in 0..20 {
            playback.fill(&mut block);
        }
    });

    assert_eq!(requests, 0);
}
