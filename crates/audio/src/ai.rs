//! Sends the voice through the AI converter. The converter takes far longer than a callback to
//! answer, so a thread of its own runs it, and rings carry the voice there and back.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::thread;
use std::time::Duration;

use instantpad_application::events::AppEvent;
use instantpad_application::message::{Failure, Message, TextKey};
use instantpad_application::ports::{EventSink, SpeechConverter};
use instantpad_domain::voice::Conversion;
use rtrb::{Consumer, Producer, RingBuffer};

use crate::format::{CHANNELS, downmix, frames_for};
use crate::worker::Worker;

const CAPACITY: usize = frames_for(2000);
/// The converter answers late and in pieces, so the output waits until this much has come,
/// which is about one piece, and it can then bridge the gaps between pieces.
pub(crate) const PRIME: usize = frames_for(160);
/// Waiting longer than this means the converter is slower than the voice.
const MAX_BACKLOG: usize = frames_for(1000);
const POLL: Duration = Duration::from_millis(10);
/// Frames the callback handles at a time, so that no buffer depends on the size of a callback.
const PIECE: usize = 480;

/// What the callback, the thread and the control side agree on.
pub(crate) struct AiShared {
    /// Whether the voice goes through the converter.
    is_routing: AtomicBool,
    /// Counts the conversions started. What the converter answers to an earlier one is dropped.
    generation: AtomicU64,
}

/// The ends of the rings that the thread keeps.
pub(crate) struct WorkerEnds {
    heard: Consumer<f32>,
    converted: Producer<f32>,
}

/// Where the thread finds the rings of the open microphone. It is empty while no microphone
/// is open. The thread locks it for a moment at a time and never while it converts.
type Slot = Mutex<Option<WorkerEnds>>;

/// The callback's half: it hands the voice over and plays what comes back.
pub(crate) struct AiStage {
    shared: Arc<AiShared>,
    to_ai: Producer<f32>,
    from_ai: Consumer<f32>,
    was_routing: bool,
    /// The conversion that the output is playing.
    generation: u64,
    /// Whether the output has been waiting for enough converted voice and has begun.
    is_primed: bool,
    stereo: [f32; PIECE * CHANNELS],
}

impl AiStage {
    /// Hands `emit` the voice as it is, or as the AI turned it. Runs in the capture callback.
    pub(crate) fn process(&mut self, stereo: &[f32], mut emit: impl FnMut(&[f32])) {
        if !self.shared.is_routing.load(Ordering::Relaxed) {
            self.was_routing = false;
            emit(stereo);
            return;
        }
        let generation = self.shared.generation.load(Ordering::Relaxed);
        if !self.was_routing || generation != self.generation {
            // What the last conversion left must not play before the new voice.
            self.was_routing = true;
            self.generation = generation;
            self.is_primed = false;
            let waiting = self.from_ai.slots();
            if let Ok(chunk) = self.from_ai.read_chunk(waiting) {
                chunk.commit_all();
            }
        }

        for piece in stereo.chunks(PIECE * CHANNELS) {
            // A full ring means the converter is not taking the voice, which the thread notices.
            for sample in downmix(piece) {
                let _ = self.to_ai.push(sample);
            }
            if !self.is_primed && self.from_ai.slots() >= PRIME {
                self.is_primed = true;
            }

            let frames = piece.len() / CHANNELS;
            for frame in &mut self.stereo.as_chunks_mut::<CHANNELS>().0[..frames] {
                let sample = if self.is_primed {
                    self.from_ai.pop().unwrap_or_else(|_| {
                        // Ran dry: wait for the next piece instead of crackling sample by sample.
                        self.is_primed = false;
                        0.0
                    })
                } else {
                    0.0
                };
                *frame = [sample; CHANNELS];
            }
            emit(&self.stereo[..frames * CHANNELS]);
        }
    }
}

/// The control side: it starts conversions and owns the thread that runs them.
pub(crate) struct AiBridge {
    converter: Arc<dyn SpeechConverter>,
    events: Arc<dyn EventSink>,
    shared: Arc<AiShared>,
    /// What the thread is converting for, to leave it alone when nothing changed.
    active: Mutex<Option<Conversion>>,
    ends: Arc<Slot>,
    // Last, so that the thread finds `ends` gone when it is woken to end.
    worker: Worker,
}

impl AiBridge {
    pub(crate) fn new(converter: Arc<dyn SpeechConverter>, events: Arc<dyn EventSink>) -> Self {
        Self {
            converter,
            events,
            shared: Arc::new(AiShared {
                is_routing: AtomicBool::new(false),
                generation: AtomicU64::new(0),
            }),
            active: Mutex::new(None),
            ends: Arc::default(),
            worker: Worker::new(),
        }
    }

    /// New rings for a microphone: the callback's stage, and the thread's ends to hand to
    /// [`AiBridge::attach`] once the microphone is open.
    pub(crate) fn rings(&self) -> (AiStage, WorkerEnds) {
        let (to_ai, heard) = RingBuffer::new(CAPACITY);
        let (converted, from_ai) = RingBuffer::new(CAPACITY);
        let stage = AiStage {
            shared: self.shared.clone(),
            to_ai,
            from_ai,
            was_routing: false,
            generation: 0,
            is_primed: false,
            stereo: [0.0; PIECE * CHANNELS],
        };
        (stage, WorkerEnds { heard, converted })
    }

    pub(crate) fn attach(&self, ends: Option<WorkerEnds>) {
        *self.ends.lock().unwrap() = ends;
    }

    /// Sends the voice through the converter for `conversion`, or back to the DSP chain alone.
    /// The same conversion again changes nothing, because a knob that moves asks for it on every
    /// step, and starting over would drop the voice that is being converted.
    pub(crate) fn route(&self, conversion: Option<&Conversion>) {
        let mut active = self.active.lock().unwrap();
        let Some(conversion) = conversion else {
            *active = None;
            self.shared.is_routing.store(false, Ordering::Relaxed);
            return;
        };
        if active.as_ref() == Some(conversion) && self.shared.is_routing.load(Ordering::Relaxed) {
            return;
        }

        // The new generation comes first, so that nothing the old speakers made is delivered
        // or fed to the converter once it has started over.
        self.shared.generation.fetch_add(1, Ordering::Relaxed);
        if let Err(Failure(reason)) = self.converter.start(conversion) {
            *active = None;
            self.shared.is_routing.store(false, Ordering::Relaxed);
            self.events.emit(AppEvent::error(reason));
            return;
        }

        *active = Some(conversion.clone());
        self.shared.is_routing.store(true, Ordering::Relaxed);
        let converter = Converting {
            ends: Arc::downgrade(&self.ends),
            shared: self.shared.clone(),
            converter: self.converter.clone(),
            events: self.events.clone(),
        };
        self.worker
            .wake("voice conversion", move || converter.serve());
    }
}

/// What the thread of the conversion works with.
struct Converting {
    ends: Weak<Slot>,
    shared: Arc<AiShared>,
    converter: Arc<dyn SpeechConverter>,
    events: Arc<dyn EventSink>,
}

/// What the thread finds waiting.
enum Heard {
    Nothing,
    Speech(usize),
    TooMuch,
}

impl Converting {
    fn serve(self) {
        let mut speech = vec![0.0; MAX_BACKLOG];
        let mut generation = self.shared.generation.load(Ordering::Relaxed);
        loop {
            let Some(ends) = self.ends.upgrade() else {
                return;
            };
            if !self.shared.is_routing.load(Ordering::Relaxed) {
                drop(ends);
                thread::park();
                continue;
            }

            let current = self.shared.generation.load(Ordering::Relaxed);
            let is_new = generation != current;
            generation = current;
            match hear(&ends, is_new, &mut speech) {
                Heard::Nothing => {}
                Heard::TooMuch => self.give_up(generation, Message::text(TextKey::AiTooSlow)),
                Heard::Speech(count) => match self.converter.convert(&speech[..count]) {
                    Ok(converted) => deliver(&ends, &self.shared, generation, &converted),
                    Err(Failure(reason)) => self.give_up(generation, reason),
                },
            }
            drop(ends);
            thread::sleep(POLL);
        }
    }

    /// Goes back to the voice without the AI, once, and says why. A conversion that started
    /// since is not told off for the failure of the one before it.
    fn give_up(&self, generation: u64, reason: Message) {
        if self.shared.generation.load(Ordering::Relaxed) == generation {
            self.shared.is_routing.store(false, Ordering::Relaxed);
            self.events.emit(AppEvent::error(reason));
        }
    }
}

/// Takes what the callback sent. The voice that waited for an earlier conversion is dropped.
fn hear(ends: &Slot, is_new: bool, speech: &mut [f32]) -> Heard {
    let mut ends = ends.lock().unwrap();
    let Some(ends) = ends.as_mut() else {
        return Heard::Nothing;
    };

    let waiting = ends.heard.slots();
    if is_new {
        if let Ok(chunk) = ends.heard.read_chunk(waiting) {
            chunk.commit_all();
        }
        return Heard::Nothing;
    }
    if waiting > MAX_BACKLOG {
        return Heard::TooMuch;
    }
    match ends.heard.pop_partial_slice(speech).0.len() {
        0 => Heard::Nothing,
        count => Heard::Speech(count),
    }
}

fn deliver(ends: &Slot, shared: &AiShared, generation: u64, converted: &[f32]) {
    if shared.generation.load(Ordering::Relaxed) != generation {
        return;
    }
    if let Some(ends) = ends.lock().unwrap().as_mut() {
        // When the output does not keep up, the newest voice is dropped.
        let _ = ends.converted.push_partial_slice(converted);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::mpsc;
    use std::time::Instant;

    use instantpad_domain::cloning::Voiceprint;
    use instantpad_test_support::fakes::{FakeConverter, RecordingEvents};

    use super::*;

    pub(crate) fn conversion() -> Conversion {
        Conversion {
            source: Voiceprint::new(vec![0.1]),
            target: Voiceprint::new(vec![0.2]),
        }
    }

    /// Waits for something that another thread does, for as long as a slow machine needs.
    pub(crate) fn eventually(what: &str, mut is_done: impl FnMut() -> bool) {
        let started = Instant::now();
        while !is_done() {
            assert!(started.elapsed() < Duration::from_secs(10), "{what}");
            thread::sleep(Duration::from_millis(1));
        }
    }

    struct Rig {
        bridge: AiBridge,
        stage: AiStage,
        events: Arc<RecordingEvents>,
    }

    impl Rig {
        fn new(converter: Arc<dyn SpeechConverter>) -> Self {
            let events = Arc::new(RecordingEvents::default());
            let bridge = AiBridge::new(converter, events.clone());
            let (stage, ends) = bridge.rings();
            bridge.attach(Some(ends));
            Self {
                bridge,
                stage,
                events,
            }
        }

        fn process(&mut self, stereo: &[f32]) -> Vec<f32> {
            let mut output = Vec::new();
            self.stage
                .process(stereo, |voice| output.extend_from_slice(voice));
            output
        }

        /// Plays the part of the converter, which answers whenever it likes.
        fn feed(&self, converted: &[f32]) {
            let mut ends = self.bridge.ends.lock().unwrap();
            let _ = ends
                .as_mut()
                .unwrap()
                .converted
                .push_partial_slice(converted);
        }

        fn errors(&self) -> Vec<AppEvent> {
            self.events.0.lock().unwrap().clone()
        }

        /// Waits until the thread has converted everything the callback handed over.
        fn settle(&self, sent: usize, played: usize) {
            eventually("the converted voice did not come back", || {
                self.stage.from_ai.slots() == sent - played
            });
        }
    }

    /// A voice whose every sample is different, left and right the same.
    pub(crate) fn ramp(from: usize, frames: usize) -> Vec<f32> {
        (from..from + frames)
            .flat_map(|index| [(index + 1) as f32 * 1e-5; CHANNELS])
            .collect()
    }

    fn mono(from: usize, samples: usize) -> Vec<f32> {
        (from..from + samples)
            .map(|index| (index + 1) as f32 * 1e-5)
            .collect()
    }

    /// Takes the voice and never gives any back.
    struct Mute;

    impl SpeechConverter for Mute {
        fn start(&self, _: &Conversion) -> Result<(), Failure> {
            Ok(())
        }

        fn convert(&self, _: &[f32]) -> Result<Vec<f32>, Failure> {
            Ok(Vec::new())
        }
    }

    /// Never answers until it is told to, and says when it was asked.
    struct Gate {
        asked: mpsc::Sender<()>,
        open: Mutex<mpsc::Receiver<()>>,
    }

    impl SpeechConverter for Gate {
        fn start(&self, _: &Conversion) -> Result<(), Failure> {
            Ok(())
        }

        fn convert(&self, _: &[f32]) -> Result<Vec<f32>, Failure> {
            self.asked.send(()).unwrap();
            let _ = self.open.lock().unwrap().recv();
            Ok(Vec::new())
        }
    }

    /// Fails like a model that cannot run.
    pub(crate) struct Broken {
        pub(crate) fails_to_start: bool,
    }

    impl SpeechConverter for Broken {
        fn start(&self, _: &Conversion) -> Result<(), Failure> {
            if self.fails_to_start {
                return Err(Failure::text(TextKey::AiLoadFailed));
            }
            Ok(())
        }

        fn convert(&self, _: &[f32]) -> Result<Vec<f32>, Failure> {
            Err(Failure::because(TextKey::AiLoadFailed, "out of memory"))
        }
    }

    #[test]
    fn the_voice_goes_through_untouched_until_a_conversion_starts() {
        let mut rig = Rig::new(Arc::new(FakeConverter::default()));
        let voice = ramp(0, 480);

        assert_eq!(rig.process(&voice), voice);
    }

    #[test]
    fn the_converted_voice_comes_out_after_priming_as_the_voice_delayed() {
        let mut rig = Rig::new(Arc::new(FakeConverter::default()));
        rig.bridge.route(Some(&conversion()));
        let blocks = PRIME / PIECE + 8;

        let mut heard = Vec::new();
        for block in 0..blocks {
            heard.extend(rig.process(&ramp(block * PIECE, PIECE)));
            // Nothing is played until the first block after priming.
            rig.settle(
                (block + 1) * PIECE,
                block.saturating_sub(PRIME / PIECE - 1) * PIECE,
            );
        }

        let (silence, voice) = heard.split_at(PRIME * CHANNELS);
        assert!(silence.iter().all(|&sample| sample == 0.0));
        assert_eq!(voice, &ramp(0, 8 * PIECE)[..]);
    }

    #[test]
    fn nothing_is_played_while_the_output_primes() {
        let mut rig = Rig::new(Arc::new(FakeConverter::default()));
        rig.bridge.route(Some(&conversion()));

        let first = rig.process(&ramp(0, PIECE));
        rig.settle(PIECE, 0);
        let second = rig.process(&ramp(PIECE, PIECE));

        assert_eq!(first, vec![0.0; PIECE * CHANNELS]);
        assert_eq!(second, vec![0.0; PIECE * CHANNELS]);
    }

    #[test]
    fn an_output_that_runs_dry_waits_for_the_converter_again() {
        let mut rig = Rig::new(Arc::new(Mute));
        rig.bridge.route(Some(&conversion()));
        rig.process(&[]);
        rig.feed(&mono(0, PRIME + PIECE));
        let silence = vec![0.0; PIECE * CHANNELS];

        let played: Vec<f32> = (0..PRIME / PIECE + 1)
            .flat_map(|_| rig.process(&silence))
            .collect();
        let dry = rig.process(&silence);
        rig.feed(&mono(0, PIECE));
        let waiting = rig.process(&silence);
        rig.feed(&mono(0, PRIME));
        let resumed = rig.process(&silence);

        assert_eq!(played, ramp(0, PRIME + PIECE));
        assert_eq!(dry, silence);
        assert_eq!(waiting, silence);
        assert_eq!(resumed, ramp(0, PIECE));
    }

    #[test]
    fn turning_the_conversion_off_goes_straight_back_to_the_voice() {
        let mut rig = Rig::new(Arc::new(FakeConverter::default()));
        rig.bridge.route(Some(&conversion()));
        rig.process(&ramp(0, PIECE));

        rig.bridge.route(None);

        let voice = ramp(PIECE, 700);
        assert_eq!(rig.process(&voice), voice);
    }

    #[test]
    fn a_conversion_that_cannot_start_keeps_the_voice_and_says_why() {
        let mut rig = Rig::new(Arc::new(Broken {
            fails_to_start: true,
        }));

        rig.bridge.route(Some(&conversion()));

        let voice = ramp(0, 480);
        assert_eq!(rig.process(&voice), voice);
        assert_eq!(
            rig.errors(),
            [AppEvent::error(Message::text(TextKey::AiLoadFailed))]
        );
    }

    #[test]
    fn a_converter_that_fails_is_bypassed_and_says_why_once() {
        let mut rig = Rig::new(Arc::new(Broken {
            fails_to_start: false,
        }));
        rig.bridge.route(Some(&conversion()));
        rig.process(&ramp(0, PIECE));

        eventually("the failure was not reported", || !rig.errors().is_empty());
        rig.process(&ramp(PIECE, PIECE));
        thread::sleep(POLL * 5);

        let voice = ramp(2 * PIECE, 700);
        assert_eq!(rig.process(&voice), voice);
        assert_eq!(
            rig.errors(),
            [AppEvent::error(Message::with(
                TextKey::AiLoadFailed,
                [Message::plain("out of memory")]
            ))]
        );
    }

    #[test]
    fn a_converter_that_falls_behind_is_bypassed_and_the_user_is_told() {
        let (asked, was_asked) = mpsc::channel();
        let (release, open) = mpsc::channel();
        let mut rig = Rig::new(Arc::new(Gate {
            asked,
            open: Mutex::new(open),
        }));
        rig.bridge.route(Some(&conversion()));
        rig.process(&ramp(0, PIECE));
        was_asked.recv_timeout(Duration::from_secs(10)).unwrap();

        // The converter is stuck on its first piece while more than a second of voice comes in.
        for block in 0..=MAX_BACKLOG / PIECE + 1 {
            rig.process(&ramp(block * PIECE, PIECE));
        }
        release.send(()).unwrap();

        eventually("the bypass did not happen", || !rig.errors().is_empty());
        let voice = ramp(0, 700);
        assert_eq!(rig.process(&voice), voice);
        assert_eq!(
            rig.errors(),
            [AppEvent::error(Message::text(TextKey::AiTooSlow))]
        );
    }

    #[test]
    fn starting_a_conversion_again_after_a_bypass_tries_the_converter_again() {
        let mut rig = Rig::new(Arc::new(Broken {
            fails_to_start: false,
        }));
        rig.bridge.route(Some(&conversion()));
        rig.process(&ramp(0, PIECE));
        eventually("the failure was not reported", || !rig.errors().is_empty());

        rig.bridge.route(Some(&conversion()));

        assert!(rig.bridge.shared.is_routing.load(Ordering::Relaxed));
        let voice = ramp(0, 480);
        assert_ne!(rig.process(&voice), voice);
    }

    #[test]
    fn a_new_conversion_does_not_play_what_the_last_one_left() {
        let mut rig = Rig::new(Arc::new(Mute));
        rig.bridge.route(Some(&conversion()));
        rig.process(&ramp(0, PIECE));
        rig.feed(&mono(0, PRIME));
        rig.bridge.route(None);
        rig.process(&ramp(0, PIECE));

        rig.bridge.route(Some(&conversion()));

        assert_eq!(rig.process(&ramp(0, PIECE)), vec![0.0; PIECE * CHANNELS]);
    }

    #[test]
    fn a_voice_in_pieces_of_any_size_is_handled() {
        let mut rig = Rig::new(Arc::new(FakeConverter::default()));
        rig.bridge.route(Some(&conversion()));

        let heard = rig.process(&ramp(0, 3 * PIECE + 17));

        assert_eq!(heard.len(), (3 * PIECE + 17) * CHANNELS);
    }

    #[test]
    fn the_thread_ends_when_the_bridge_is_gone() {
        let converter = Arc::new(FakeConverter::default());
        let held = Arc::downgrade(&converter);
        let rig = Rig::new(converter);
        rig.bridge.route(Some(&conversion()));

        drop(rig);

        // Only the thread still holds the converter.
        eventually("the thread kept running", || held.strong_count() == 0);
    }

    #[test]
    fn the_same_conversion_again_does_not_start_over() {
        let converter = Arc::new(FakeConverter::default());
        let mut rig = Rig::new(converter.clone());
        rig.bridge.route(Some(&conversion()));
        rig.process(&[]);
        rig.feed(&mono(0, PRIME));
        let generation = rig.bridge.shared.generation.load(Ordering::Relaxed);

        rig.bridge.route(Some(&conversion()));

        assert_eq!(converter.started.lock().unwrap().len(), 1);
        assert_eq!(
            rig.bridge.shared.generation.load(Ordering::Relaxed),
            generation
        );
        // What was converted still plays.
        assert_ne!(rig.process(&ramp(0, PIECE)), vec![0.0; PIECE * CHANNELS]);
    }

    #[test]
    fn a_conversion_that_was_turned_off_starts_again() {
        let converter = Arc::new(FakeConverter::default());
        let rig = Rig::new(converter.clone());
        rig.bridge.route(Some(&conversion()));
        rig.bridge.route(None);

        rig.bridge.route(Some(&conversion()));

        assert_eq!(converter.started.lock().unwrap().len(), 2);
    }

    #[test]
    fn switching_straight_to_other_speakers_does_not_play_what_the_old_ones_left() {
        let converter = Arc::new(FakeConverter::default());
        let mut rig = Rig::new(converter.clone());
        rig.bridge.route(Some(&conversion()));
        rig.process(&[]);
        rig.feed(&mono(0, PRIME));
        let other = Conversion {
            source: conversion().source,
            target: Voiceprint::new(vec![0.9]),
        };

        rig.bridge.route(Some(&other));

        assert_eq!(converter.started.lock().unwrap().len(), 2);
        assert_eq!(rig.process(&ramp(0, PIECE)), vec![0.0; PIECE * CHANNELS]);
    }
}
