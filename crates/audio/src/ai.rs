//! Sends the voice through the AI converter. The converter takes far longer than a callback to
//! answer, so a thread of its own runs it, and rings carry the voice there and back.

use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::thread;
use std::time::Duration;

use instantpad_application::events::AppEvent;
use instantpad_application::message::{Failure, Message, TextKey};
use instantpad_application::ports::{EventSink, SpeechConverter};
use instantpad_application::sync::lock;
use instantpad_domain::voice::Conversion;
use rtrb::{Consumer, Producer, RingBuffer};

use crate::format::{CHANNELS, downmix, frames_for};
use crate::worker::Worker;

const CAPACITY: usize = frames_for(2000);
/// The converter answers late and in pieces of about 140 ms. The output starts once this much
/// has come, which is less than a piece, so it starts with the first one. Asking for more than
/// a piece makes it wait for a second one, which is a whole piece of delay, at the start and
/// again after every time the converter runs late.
pub(crate) const PRIME: usize = frames_for(100);
/// Silence before the first voice of a conversion, so that a piece that comes a little late
/// finds the output still playing. The delay stays about this much longer for the whole
/// conversion, and every late piece costs a gap, and then that much more delay, without it. On a
/// 4-core 2.1 GHz processor, 60 ms still ran dry every few seconds and 100 ms rarely did.
pub(crate) const CUSHION: usize = frames_for(100);
/// Waiting longer than this means the converter is slower than the voice.
const MAX_BACKLOG: usize = frames_for(1000);
const POLL: Duration = Duration::from_millis(10);
/// Frames the callback handles at a time, so that no buffer depends on the size of a callback.
const PIECE: usize = 480;

/// What the callback, the thread and the control side agree on.
pub(crate) struct AiShared {
    /// Whether the voice goes through the converter (the lowest bit), and the count of
    /// conversions started (the rest). What the converter answers to an earlier one is dropped.
    /// They share a word so that giving up the conversion that failed cannot switch off one that
    /// started in the meantime.
    state: AtomicU64,
    /// Whether the conversion that runs is a trained voice, which decides what the user is told
    /// when it falls behind.
    is_trained: AtomicBool,
}

impl AiShared {
    const ROUTING: u64 = 1;

    /// Whether the voice goes through the converter, and the conversion it is.
    fn snapshot(&self) -> (bool, u64) {
        let state = self.state.load(Ordering::Acquire);
        (state & Self::ROUTING != 0, state >> 1)
    }

    fn is_routing(&self) -> bool {
        self.snapshot().0
    }

    fn generation(&self) -> u64 {
        self.snapshot().1
    }

    /// Counts a new conversion, so that nothing the old one made is delivered or fed to the
    /// converter, which [`AiShared::resume`] then lets through.
    fn begin(&self, is_trained: bool) {
        self.is_trained.store(is_trained, Ordering::Relaxed);
        self.state.fetch_add(1 << 1, Ordering::AcqRel);
    }

    fn resume(&self) {
        self.state.fetch_or(Self::ROUTING, Ordering::AcqRel);
    }

    fn stop(&self) {
        self.state.fetch_and(!Self::ROUTING, Ordering::AcqRel);
    }

    /// Switches off `generation`, unless a newer conversion has started.
    fn stop_if_current(&self, generation: u64) -> bool {
        let running = (generation << 1) | Self::ROUTING;
        self.state
            .compare_exchange(
                running,
                generation << 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    /// What the user is told when the converter cannot keep up. A trained voice that cannot has
    /// the instant engine to fall back on.
    fn too_slow(&self) -> TextKey {
        if self.is_trained.load(Ordering::Relaxed) {
            TextKey::AiTrainedTooSlow
        } else {
            TextKey::AiTooSlow
        }
    }
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
    /// What is left of the silence the conversion starts with.
    cushion: usize,
    stereo: [f32; PIECE * CHANNELS],
}

impl AiStage {
    /// Hands `emit` the voice as it is, or as the AI turned it. Runs in the capture callback.
    pub(crate) fn process(&mut self, stereo: &[f32], mut emit: impl FnMut(&[f32])) {
        let (is_routing, generation) = self.shared.snapshot();
        if !is_routing {
            self.was_routing = false;
            emit(stereo);
            return;
        }
        if !self.was_routing || generation != self.generation {
            // What the last conversion left must not play before the new voice.
            self.was_routing = true;
            self.generation = generation;
            self.is_primed = false;
            self.cushion = CUSHION;
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
                let sample = if !self.is_primed {
                    0.0
                } else if self.cushion > 0 {
                    self.cushion -= 1;
                    0.0
                } else {
                    self.from_ai.pop().unwrap_or_else(|_| {
                        // Ran dry: wait for the next piece instead of crackling sample by sample.
                        self.is_primed = false;
                        0.0
                    })
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
                state: AtomicU64::new(0),
                is_trained: AtomicBool::new(false),
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
            cushion: 0,
            stereo: [0.0; PIECE * CHANNELS],
        };
        (stage, WorkerEnds { heard, converted })
    }

    pub(crate) fn attach(&self, ends: Option<WorkerEnds>) {
        *lock(&self.ends) = ends;
    }

    /// Sends the voice through the converter for `conversion`, or back to the DSP chain alone.
    /// The same conversion again changes nothing, because a knob that moves asks for it on every
    /// step, and starting over would drop the voice that is being converted.
    pub(crate) fn route(&self, conversion: Option<&Conversion>) {
        let mut active = lock(&self.active);
        let Some(conversion) = conversion else {
            *active = None;
            self.shared.stop();
            return;
        };
        if let Some(running) = active.as_ref().filter(|_| self.shared.is_routing())
            && running.same_voice(conversion)
        {
            // Only the pitch of a trained voice moved, which it follows as it speaks.
            if let Some(semitones) = conversion.semitones()
                && running.semitones() != Some(semitones)
            {
                self.converter.retune(semitones);
                *active = Some(conversion.clone());
            }
            return;
        }

        // The new generation comes first, so that nothing the old speakers made is delivered
        // or fed to the converter once it has started over.
        self.shared
            .begin(matches!(conversion, Conversion::Trained { .. }));
        self.converter.start(conversion);
        *active = Some(conversion.clone());
        self.shared.resume();
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
        let mut generation = self.shared.generation();
        // The converter may load its model while it converts the first piece of a conversion,
        // which takes longer than the backlog allows and is not falling behind.
        let mut is_warm = false;
        loop {
            let Some(ends) = self.ends.upgrade() else {
                return;
            };
            let (is_routing, current) = self.shared.snapshot();
            if !is_routing {
                drop(ends);
                thread::park();
                continue;
            }

            if generation != current {
                generation = current;
                is_warm = false;
                forget(&ends);
            } else {
                match hear(&ends, is_warm, &mut speech) {
                    Heard::Nothing => {}
                    Heard::TooMuch => {
                        self.give_up(generation, Message::text(self.shared.too_slow()))
                    }
                    Heard::Speech(count) => match self.convert(&speech[..count]) {
                        Ok(converted) => {
                            deliver(&ends, &self.shared, generation, &converted);
                            if !is_warm {
                                is_warm = true;
                                forget(&ends);
                            }
                        }
                        Err(Failure(reason)) => self.give_up(generation, reason),
                    },
                }
            }
            drop(ends);
            thread::sleep(POLL);
        }
    }

    /// Runs the converter, which is foreign code. A panic in it becomes a failure, because a dead
    /// thread would leave the voice routed to a converter that never answers, and so muted.
    fn convert(&self, speech: &[f32]) -> Result<Vec<f32>, Failure> {
        panic::catch_unwind(AssertUnwindSafe(|| self.converter.convert(speech))).unwrap_or_else(
            |_| {
                Err(Failure::because(
                    TextKey::AiLoadFailed,
                    "the voice converter stopped unexpectedly",
                ))
            },
        )
    }

    /// Goes back to the voice without the AI, once, and says why. A conversion that started
    /// since is not told off for the failure of the one before it.
    fn give_up(&self, generation: u64, reason: Message) {
        if self.shared.stop_if_current(generation) {
            self.events.emit(AppEvent::error(reason));
        }
    }
}

/// Drops the voice that is waiting, which belongs to an earlier conversion or came in while the
/// converter was getting ready.
fn forget(ends: &Slot) {
    if let Some(ends) = lock(ends).as_mut()
        && let Ok(chunk) = ends.heard.read_chunk(ends.heard.slots())
    {
        chunk.commit_all();
    }
}

/// Takes what the callback sent. Only a converter that is ready can be told it falls behind.
fn hear(ends: &Slot, is_warm: bool, speech: &mut [f32]) -> Heard {
    let mut ends = lock(ends);
    let Some(ends) = ends.as_mut() else {
        return Heard::Nothing;
    };

    if is_warm && ends.heard.slots() > MAX_BACKLOG {
        return Heard::TooMuch;
    }
    match ends.heard.pop_partial_slice(speech).0.len() {
        0 => Heard::Nothing,
        count => Heard::Speech(count),
    }
}

fn deliver(ends: &Slot, shared: &AiShared, generation: u64, converted: &[f32]) {
    if shared.generation() != generation {
        return;
    }
    if let Some(ends) = lock(ends).as_mut() {
        // When the output does not keep up, the newest voice is dropped.
        let _ = ends.converted.push_partial_slice(converted);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::mpsc;
    use std::time::Instant;

    use instantpad_domain::cloning::{TrainedModel, Voiceprint};
    use instantpad_test_support::fakes::{FakeConverter, RecordingEvents};

    use super::*;

    pub(crate) fn conversion() -> Conversion {
        Conversion::Timbre {
            source: Voiceprint::new(vec![0.1]),
            target: Voiceprint::new(vec![0.2]),
        }
    }

    fn other_speakers() -> Conversion {
        Conversion::Timbre {
            source: Voiceprint::new(vec![0.1]),
            target: Voiceprint::new(vec![0.9]),
        }
    }

    pub(crate) fn trained(semitones: f32) -> Conversion {
        Conversion::Trained {
            model: TrainedModel {
                id: "model".to_owned(),
                name: "Ana".to_owned(),
                credit: String::new(),
                pitch_hz: 210.0,
            },
            semitones,
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

        /// Hands over the first block whole. The thread forgets what came in while it warmed
        /// up, which would be the rest of a block it caught halfway.
        fn process_first(&mut self, stereo: &[f32]) -> Vec<f32> {
            let _held = lock(&self.bridge.ends);
            let mut output = Vec::new();
            self.stage
                .process(stereo, |voice| output.extend_from_slice(voice));
            output
        }

        /// Plays the part of the converter, which answers whenever it likes.
        fn feed(&self, converted: &[f32]) {
            let mut ends = lock(&self.bridge.ends);
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
        fn start(&self, _: &Conversion) {}

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
        fn start(&self, _: &Conversion) {}

        fn convert(&self, _: &[f32]) -> Result<Vec<f32>, Failure> {
            self.asked.send(()).unwrap();
            let _ = self.open.lock().unwrap().recv();
            Ok(Vec::new())
        }
    }

    /// Fails like a model that cannot run.
    struct Broken;

    impl SpeechConverter for Broken {
        fn start(&self, _: &Conversion) {}

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
    fn the_converted_voice_comes_out_after_priming_and_the_cushion_as_the_voice_delayed() {
        let mut rig = Rig::new(Arc::new(FakeConverter::default()));
        rig.bridge.route(Some(&conversion()));
        let first_voice = (PRIME + CUSHION) / PIECE;
        let blocks = first_voice + 8;

        let mut heard = Vec::new();
        for block in 0..blocks {
            let voice = ramp(block * PIECE, PIECE);
            heard.extend(if block == 0 {
                rig.process_first(&voice)
            } else {
                rig.process(&voice)
            });
            // Nothing is played until the first block after priming and the cushion.
            rig.settle(
                (block + 1) * PIECE,
                block.saturating_sub(first_voice - 1) * PIECE,
            );
        }

        let (silence, voice) = heard.split_at((PRIME + CUSHION) * CHANNELS);
        assert!(silence.iter().all(|&sample| sample == 0.0));
        assert_eq!(voice, &ramp(0, 8 * PIECE)[..]);
    }

    #[test]
    fn nothing_is_played_while_the_output_primes() {
        let mut rig = Rig::new(Arc::new(FakeConverter::default()));
        rig.bridge.route(Some(&conversion()));

        let first = rig.process_first(&ramp(0, PIECE));
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

        let cushion: Vec<f32> = (0..CUSHION / PIECE)
            .flat_map(|_| rig.process(&silence))
            .collect();
        let played: Vec<f32> = (0..PRIME / PIECE + 1)
            .flat_map(|_| rig.process(&silence))
            .collect();
        let dry = rig.process(&silence);
        rig.feed(&mono(0, PIECE));
        let waiting = rig.process(&silence);
        rig.feed(&mono(0, PRIME));
        let resumed = rig.process(&silence);

        assert_eq!(cushion, vec![0.0; CUSHION * CHANNELS]);
        assert_eq!(played, ramp(0, PRIME + PIECE));
        assert_eq!(dry, silence);
        assert_eq!(waiting, silence);
        // The cushion is only for the start: a late piece is played as soon as it is here.
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
    fn a_converter_that_fails_is_bypassed_and_says_why_once() {
        let mut rig = Rig::new(Arc::new(Broken));
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

    /// A converter that stops on every piece until it is released, with a callback that has
    /// gone through the first piece, which is the one that may load a model.
    fn warmed_up_gate() -> (Rig, mpsc::Sender<()>) {
        warmed_up_gate_for(&conversion())
    }

    fn warmed_up_gate_for(conversion: &Conversion) -> (Rig, mpsc::Sender<()>) {
        let (asked, was_asked) = mpsc::channel();
        let (release, open) = mpsc::channel();
        let mut rig = Rig::new(Arc::new(Gate {
            asked,
            open: Mutex::new(open),
        }));
        rig.bridge.route(Some(conversion));
        rig.process(&ramp(0, PIECE));
        was_asked.recv_timeout(Duration::from_secs(10)).unwrap();

        // More than a second of voice comes in while the first piece is converted.
        for block in 0..=MAX_BACKLOG / PIECE + 1 {
            rig.process(&ramp(block * PIECE, PIECE));
        }
        release.send(()).unwrap();

        // The converter is asked again with voice that came after it was ready.
        let started = Instant::now();
        while was_asked.recv_timeout(Duration::from_millis(5)).is_err() {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "never asked again"
            );
            rig.process(&ramp(0, PIECE));
        }
        (rig, release)
    }

    #[test]
    fn a_converter_that_takes_long_on_its_first_piece_is_not_falling_behind() {
        let (rig, _release) = warmed_up_gate();

        assert!(rig.bridge.shared.is_routing());
        assert_eq!(rig.errors(), []);
    }

    #[test]
    fn a_converter_that_stays_slow_is_bypassed_and_the_user_is_told() {
        let (mut rig, release) = warmed_up_gate();

        // Stuck on the second piece, while more than a second of voice comes in.
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
        let mut rig = Rig::new(Arc::new(Broken));
        rig.bridge.route(Some(&conversion()));
        rig.process(&ramp(0, PIECE));
        eventually("the failure was not reported", || !rig.errors().is_empty());

        rig.bridge.route(Some(&conversion()));

        assert!(rig.bridge.shared.is_routing());
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
        let generation = rig.bridge.shared.generation();

        rig.bridge.route(Some(&conversion()));

        assert_eq!(converter.started.lock().unwrap().len(), 1);
        assert_eq!(rig.bridge.shared.generation(), generation);
        // The cushion of the first start plays out, and then what was converted.
        for _ in 0..CUSHION / PIECE {
            rig.process(&ramp(0, PIECE));
        }
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
        let other = Conversion::Timbre {
            source: Voiceprint::new(vec![0.1]),
            target: Voiceprint::new(vec![0.9]),
        };

        rig.bridge.route(Some(&other));

        assert_eq!(converter.started.lock().unwrap().len(), 2);
        assert_eq!(rig.process(&ramp(0, PIECE)), vec![0.0; PIECE * CHANNELS]);
    }

    #[test]
    fn moving_the_pitch_of_a_trained_voice_retunes_it_without_starting_over() {
        let converter = Arc::new(FakeConverter::default());
        let rig = Rig::new(converter.clone());
        rig.bridge.route(Some(&trained(0.0)));
        let generation = rig.bridge.shared.generation();

        rig.bridge.route(Some(&trained(3.0)));
        rig.bridge.route(Some(&trained(3.0)));
        rig.bridge.route(Some(&trained(-2.0)));

        assert_eq!(converter.started.lock().unwrap().len(), 1);
        assert_eq!(*converter.retuned.lock().unwrap(), [3.0, -2.0]);
        assert_eq!(rig.bridge.shared.generation(), generation);
    }

    #[test]
    fn another_trained_voice_or_the_instant_engine_starts_over() {
        let converter = Arc::new(FakeConverter::default());
        let rig = Rig::new(converter.clone());
        rig.bridge.route(Some(&trained(0.0)));
        let Conversion::Trained { mut model, .. } = trained(0.0) else {
            unreachable!()
        };
        model.id = "other".to_owned();

        rig.bridge.route(Some(&Conversion::Trained {
            model,
            semitones: 0.0,
        }));
        rig.bridge.route(Some(&conversion()));

        assert_eq!(converter.started.lock().unwrap().len(), 3);
        assert!(converter.retuned.lock().unwrap().is_empty());
    }

    #[test]
    fn a_trained_voice_that_falls_behind_suggests_the_instant_engine() {
        let (mut rig, release) = warmed_up_gate_for(&trained(0.0));

        for block in 0..=MAX_BACKLOG / PIECE + 1 {
            rig.process(&ramp(block * PIECE, PIECE));
        }
        release.send(()).unwrap();

        eventually("the bypass did not happen", || !rig.errors().is_empty());
        assert_eq!(
            rig.errors(),
            [AppEvent::error(Message::text(TextKey::AiTrainedTooSlow))]
        );
    }

    /// Fails like a converter that panics, and counts how many pieces it was asked to convert.
    struct Panicking(AtomicU64);

    impl SpeechConverter for Panicking {
        fn start(&self, _: &Conversion) {}

        fn convert(&self, _: &[f32]) -> Result<Vec<f32>, Failure> {
            self.0.fetch_add(1, Ordering::Relaxed);
            panic!("the converter fails");
        }
    }

    #[test]
    fn a_converter_that_panics_goes_back_to_the_plain_voice_and_says_so() {
        let mut rig = Rig::new(Arc::new(Panicking(AtomicU64::new(0))));
        rig.bridge.route(Some(&conversion()));
        rig.process(&ramp(0, PIECE));

        eventually("the failure was not reported", || !rig.errors().is_empty());

        let voice = ramp(0, 700);
        assert_eq!(rig.process(&voice), voice);
        assert_eq!(rig.errors().len(), 1);
    }

    #[test]
    fn a_conversion_started_after_a_panic_tries_the_converter_again() {
        let converter = Arc::new(Panicking(AtomicU64::new(0)));
        let mut rig = Rig::new(converter.clone());
        rig.bridge.route(Some(&conversion()));
        rig.process(&ramp(0, PIECE));
        eventually("the failure was not reported", || !rig.errors().is_empty());

        rig.bridge.route(Some(&conversion()));

        // The thread drops what came before it noticed the new conversion, so the voice goes on.
        eventually("the converter was not asked again", || {
            rig.process(&ramp(0, PIECE));
            converter.0.load(Ordering::Relaxed) >= 2
        });
    }

    #[test]
    fn switching_the_kind_of_conversion_changes_what_a_slow_converter_is_told() {
        let (mut rig, release) = warmed_up_gate();
        // The same bridge goes on to a trained voice, whose thread is the one that is running.
        rig.bridge.route(Some(&trained(0.0)));
        assert_eq!(rig.bridge.shared.too_slow(), TextKey::AiTrainedTooSlow);
        rig.bridge.route(Some(&conversion()));
        assert_eq!(rig.bridge.shared.too_slow(), TextKey::AiTooSlow);
        rig.bridge.route(Some(&trained(0.0)));
        drop(release);

        eventually("the thread did not warm up", || {
            rig.process(&ramp(0, PIECE));
            rig.bridge.shared.is_routing()
        });
        assert_eq!(rig.bridge.shared.too_slow(), TextKey::AiTrainedTooSlow);
    }

    #[test]
    fn giving_up_an_old_conversion_leaves_the_new_one_running() {
        let rig = Rig::new(Arc::new(FakeConverter::default()));
        rig.bridge.route(Some(&conversion()));
        let old = rig.bridge.shared.generation();
        rig.bridge.route(Some(&other_speakers()));

        assert!(!rig.bridge.shared.stop_if_current(old));
        assert!(rig.bridge.shared.is_routing());
    }
}
