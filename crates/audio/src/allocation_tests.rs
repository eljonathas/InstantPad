//! Proves the audio callbacks neither allocate nor free, by counting what the allocator is
//! asked for while they run.

// A global allocator is unsafe to write, and this is the only file that needs one.
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::Arc;

use instantpad_application::ports::PlaybackTag;
use instantpad_domain::voice::{PitchShape, VoiceChain, VoiceColor};
use instantpad_test_support::fakes::{FakeConverter, RecordingEvents};
use rtrb::{Consumer, RingBuffer};

use crate::ai::AiBridge;
use crate::ai::tests::conversion;
use crate::capture::CaptureProcessor;
use crate::clip::SoundClip;
use crate::controls::{AtomicF32, Controls};
use crate::format::{CHANNELS, SAMPLE_RATE, samples_for};
use crate::mic;
use crate::mixer::{self, Mixer, MixerHandle};
use crate::playback::{Conversion, Playback};
use crate::recording::Recorder;
use crate::voice::VoiceControls;

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
/// The pitch library is C++, whose allocations never reach a Rust allocator. On macOS the system
/// allocator has a hook that sees them too.
#[cfg(target_os = "macos")]
mod system_allocator {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Mutex, Once};

    type Logger = unsafe extern "C" fn(u32, usize, usize, usize, usize, u32);
    const ALLOCATES_OR_FREES: u32 = 2 | 4;

    unsafe extern "C" {
        static mut malloc_logger: Option<Logger>;
        fn pthread_self() -> usize;
    }

    static WATCHED_THREAD: AtomicUsize = AtomicUsize::new(0);
    static SEEN: AtomicUsize = AtomicUsize::new(0);

    // A thread local would allocate on first use, and call this again.
    unsafe extern "C" fn log(kind: u32, _: usize, _: usize, _: usize, _: usize, _: u32) {
        let is_watched = WATCHED_THREAD.load(Ordering::Relaxed) == unsafe { pthread_self() };
        if is_watched && kind & ALLOCATES_OR_FREES != 0 {
            SEEN.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn requests_during(work: impl FnOnce()) -> usize {
        static INSTALL: Once = Once::new();
        static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

        let _turn = ONE_AT_A_TIME.lock();
        // SAFETY: set once, and `log` touches nothing that allocates.
        INSTALL.call_once(|| unsafe { malloc_logger = Some(log) });

        SEEN.store(0, Ordering::Relaxed);
        WATCHED_THREAD.store(unsafe { pthread_self() }, Ordering::Relaxed);
        work();
        WATCHED_THREAD.store(0, Ordering::Relaxed);
        SEEN.load(Ordering::Relaxed)
    }
}

#[cfg(not(target_os = "macos"))]
mod system_allocator {
    pub fn requests_during(work: impl FnOnce()) -> usize {
        work();
        0
    }
}

fn requests_during(work: impl FnOnce()) -> usize {
    REQUESTS.set(0);
    let system = system_allocator::requests_during(|| {
        COUNTING.set(true);
        work();
        COUNTING.set(false);
    });
    REQUESTS.get() + system
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

fn bridge() -> AiBridge {
    AiBridge::new(
        Arc::new(FakeConverter::default()),
        Arc::new(RecordingEvents::default()),
    )
}

/// A capture callback with the rings it fills, a recorder to start and an AI bridge to route.
fn wired(
    channels: usize,
    rate: u32,
    voice: Arc<VoiceControls>,
) -> (CaptureProcessor, Consumer<f32>, Recorder, AiBridge) {
    let (ring, game) = RingBuffer::new(SAMPLE_RATE as usize * CHANNELS);
    let (headphones, _) = RingBuffer::new(SAMPLE_RATE as usize * CHANNELS);
    let level = Arc::new(AtomicF32::new(0.0));
    let recorder = Recorder::new();
    let (tap, recording) = recorder.tap();
    recorder.attach(Some(recording));
    let bridge = bridge();
    let (stage, converting) = bridge.rings();
    bridge.attach(Some(converting));
    let processor =
        CaptureProcessor::new(channels, rate, [ring, headphones], level, voice, tap, stage)
            .unwrap();
    (processor, game, recorder, bridge)
}

fn capture(channels: usize, rate: u32, voice: Arc<VoiceControls>) -> CaptureProcessor {
    wired(channels, rate, voice).0
}

#[cfg(target_os = "macos")]
#[test]
fn the_counter_sees_what_the_pitch_library_allocates() {
    use crate::voice::VoiceProcessor;

    let requests = system_allocator::requests_during(|| {
        drop(VoiceProcessor::new(Arc::new(VoiceControls::new())));
    });

    assert!(requests > 0);
}

#[test]
fn changing_the_voice_allocates_and_frees_nothing() {
    let voice = Arc::new(VoiceControls::new());
    let mut processor = capture(2, 44_100, voice.clone());
    let callback: Vec<f32> = (0..441 * 2)
        .map(|index| (index as f32 * 0.03).sin() * 0.3)
        .collect();
    let cycle = VoiceChain {
        pitch: PitchShape::Cycle {
            low: -12.0,
            high: 12.0,
            seconds: 0.2,
        },
        formant: 3.0,
        color: VoiceColor::default(),
    };

    let requests = requests_during(|| {
        for round in 0..200 {
            // Includes switching the effect on and off, which resets the library.
            voice.set((round % 50 != 49).then_some(cycle));
            processor.push(&callback);
        }
    });

    assert_eq!(requests, 0);
}

#[test]
fn coloring_the_voice_allocates_and_frees_nothing() {
    let voice = Arc::new(VoiceControls::new());
    let mut processor = capture(2, 44_100, voice.clone());
    let callback: Vec<f32> = (0..441 * 2)
        .map(|index| (index as f32 * 0.03).sin() * 0.3)
        .collect();
    let chain = |amount: f32| VoiceChain {
        pitch: PitchShape::Fixed { semitones: 3.0 },
        formant: 0.0,
        color: VoiceColor {
            whisper: amount,
            echo: amount,
            tremolo: amount,
            growl: amount,
            brightness: amount,
        },
    };

    let requests = requests_during(|| {
        for round in 0..200 {
            // The amounts move all the time, which also makes the shelf work out its filter
            // again, and they drop to zero now and then, which puts the colors to sleep.
            let amount = if round % 40 < 5 {
                0.0
            } else {
                (round % 10) as f32 / 10.0 + 0.1
            };
            voice.set(Some(chain(amount)));
            processor.push(&callback);
        }
    });

    assert_eq!(requests, 0);
}

#[test]
fn recording_the_microphone_allocates_and_frees_nothing() {
    let (mut processor, _, recorder, _) = wired(2, 44_100, Arc::new(VoiceControls::new()));
    let callback = vec![0.2_f32; 441 * 2];
    recorder.start();

    let requests = requests_during(|| {
        for _ in 0..50 {
            processor.push(&callback);
        }
    });

    assert_eq!(requests, 0);
}

#[test]
fn sending_the_voice_through_the_ai_allocates_and_frees_nothing() {
    let (mut processor, mut game, _, bridge) = wired(2, 44_100, Arc::new(VoiceControls::new()));
    bridge.route(Some(&conversion()));
    let callback: Vec<f32> = (0..441 * 2)
        .map(|index| (index as f32 * 0.03).sin() * 0.3 + 0.4)
        .collect();
    let mut heard = vec![0.0; SAMPLE_RATE as usize * CHANNELS];

    // The converter runs on a thread of its own, so the callback goes on until the converted
    // voice plays, which takes it through priming, playing and, if the thread lags, running dry.
    let mut requests = 0;
    let mut played = 0;
    for round in 0..5000 {
        requests += requests_during(|| processor.push(&callback));
        std::thread::sleep(std::time::Duration::from_millis(1));
        let (popped, _) = game.pop_partial_slice(&mut heard);
        played += popped.iter().filter(|&&sample| sample != 0.0).count();
        if played > 20_000 && round > 100 {
            break;
        }
    }

    assert!(played > 20_000, "the voice was never converted");
    assert_eq!(requests, 0);
}

#[test]
fn capturing_allocates_and_frees_nothing() {
    let mut processor = capture(3, 44_100, Arc::new(VoiceControls::new()));
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
    let mut processor = capture(2, SAMPLE_RATE, Arc::new(VoiceControls::new()));
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
