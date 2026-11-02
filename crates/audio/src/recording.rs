//! Keeps the microphone as it was heard, before any effect, while the user records.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::thread;
use std::time::Duration;

use instantpad_domain::cloning::MAX_SAMPLE_SECONDS;
use rtrb::{Consumer, Producer, RingBuffer};

use crate::format::{downmix, frames_for};
use crate::worker::Worker;

/// Mono samples the callback can leave before the drain thread has to come, which is a second.
const CAPACITY: usize = frames_for(1000);
/// The longest a recording grows, which is a second more than anything the app takes. The
/// application reports a longer one as too long, so a forgotten recording does not fill the memory.
const MAX_RECORDED: usize = frames_for(1000 * (MAX_SAMPLE_SECONDS as usize + 1));
const DRAIN_EVERY: Duration = Duration::from_millis(20);

/// The callback's half: it feeds the ring only while the user records.
pub(crate) struct RecordTap {
    is_recording: Arc<AtomicBool>,
    ring: Producer<f32>,
}

impl RecordTap {
    /// Runs in the capture callback, with the voice as stereo at the engine rate.
    pub(crate) fn push(&mut self, stereo: &[f32]) {
        if !self.is_recording.load(Ordering::Relaxed) {
            return;
        }
        // A ring that is full means nobody drains it, and the newest audio is dropped.
        for sample in downmix(stereo) {
            if self.ring.push(sample).is_err() {
                break;
            }
        }
    }
}

/// What has been recorded, and the ring of the open microphone that feeds it.
struct Kept {
    ring: Option<Consumer<f32>>,
    speech: Vec<f32>,
}

impl Kept {
    /// Drops what was recorded and what is still on its way.
    fn forget(&mut self) {
        if let Some(ring) = &mut self.ring
            && let Ok(chunk) = ring.read_chunk(ring.slots())
        {
            chunk.commit_all();
        }
        self.speech.clear();
    }

    fn collect(&mut self) {
        let Some(ring) = &mut self.ring else {
            return;
        };
        let room = MAX_RECORDED.saturating_sub(self.speech.len());
        if let Ok(chunk) = ring.read_chunk(ring.slots().min(room)) {
            let (first, second) = chunk.as_slices();
            self.speech.extend_from_slice(first);
            self.speech.extend_from_slice(second);
            chunk.commit_all();
        }
    }
}

pub(crate) struct Recorder {
    is_recording: Arc<AtomicBool>,
    kept: Arc<Mutex<Kept>>,
    // Last, so that the thread finds `kept` gone when it is woken to end.
    drain: Worker,
}

impl Recorder {
    pub(crate) fn new() -> Self {
        Self {
            is_recording: Arc::new(AtomicBool::new(false)),
            kept: Arc::new(Mutex::new(Kept {
                ring: None,
                speech: Vec::new(),
            })),
            drain: Worker::new(),
        }
    }

    /// The callback's half and the ring end to hand to [`Recorder::attach`] once it works.
    pub(crate) fn tap(&self) -> (RecordTap, Consumer<f32>) {
        let (ring, drained) = RingBuffer::new(CAPACITY);
        let tap = RecordTap {
            is_recording: self.is_recording.clone(),
            ring,
        };
        (tap, drained)
    }

    /// Follows another microphone, or none, and keeps what the last one still had.
    pub(crate) fn attach(&self, ring: Option<Consumer<f32>>) {
        let mut kept = self.kept.lock().unwrap();
        kept.collect();
        kept.ring = ring;
    }

    /// Starts over, dropping anything recorded before.
    pub(crate) fn start(&self) {
        let mut kept = self.kept.lock().unwrap();
        kept.forget();
        self.is_recording.store(true, Ordering::Relaxed);
        drop(kept);

        let kept = Arc::downgrade(&self.kept);
        let is_recording = self.is_recording.clone();
        self.drain
            .wake("voice recording", move || drain(kept, is_recording));
    }

    pub(crate) fn stop(&self) -> Vec<f32> {
        self.is_recording.store(false, Ordering::Relaxed);
        let mut kept = self.kept.lock().unwrap();
        kept.collect();
        std::mem::take(&mut kept.speech)
    }
}

/// Moves the audio out of the ring while recording, which the callback must not do itself,
/// because growing the recording allocates.
fn drain(kept: Weak<Mutex<Kept>>, is_recording: Arc<AtomicBool>) {
    loop {
        let Some(kept) = kept.upgrade() else {
            return;
        };
        if is_recording.load(Ordering::Relaxed) {
            kept.lock().unwrap().collect();
            drop(kept);
            thread::sleep(DRAIN_EVERY);
        } else {
            drop(kept);
            thread::park();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::tests::eventually;
    use crate::format::CHANNELS;

    fn recorder() -> (Recorder, RecordTap) {
        let recorder = Recorder::new();
        let (tap, ring) = recorder.tap();
        recorder.attach(Some(ring));
        (recorder, tap)
    }

    #[test]
    fn nothing_is_kept_until_the_recording_starts() {
        let (recorder, mut tap) = recorder();

        tap.push(&[0.5; 960]);
        recorder.start();

        assert!(recorder.stop().is_empty());
    }

    #[test]
    fn the_recording_is_the_voice_as_mono() {
        let (recorder, mut tap) = recorder();
        recorder.start();

        tap.push(&[0.5, 0.25, -1.0, 1.0, 0.2, 0.2]);

        assert_eq!(recorder.stop(), [0.375, 0.0, 0.2]);
    }

    #[test]
    fn the_recording_stops_keeping_when_it_stops() {
        let (recorder, mut tap) = recorder();
        recorder.start();
        tap.push(&[0.5; 4]);
        recorder.stop();

        tap.push(&[0.5; 4]);

        assert!(recorder.stop().is_empty());
    }

    #[test]
    fn starting_again_drops_the_earlier_recording() {
        let (recorder, mut tap) = recorder();
        recorder.start();
        tap.push(&[0.5; 4]);

        recorder.start();
        tap.push(&[0.25; 2]);

        assert_eq!(recorder.stop(), [0.25]);
    }

    #[test]
    fn with_no_microphone_nothing_is_recorded() {
        let recorder = Recorder::new();

        recorder.start();

        assert!(recorder.stop().is_empty());
    }

    #[test]
    fn a_microphone_that_closes_keeps_what_it_recorded() {
        let (recorder, mut tap) = recorder();
        recorder.start();
        tap.push(&[0.5; 4]);

        recorder.attach(None);

        assert_eq!(recorder.stop(), [0.5, 0.5]);
    }

    #[test]
    fn a_recording_longer_than_the_ring_is_drained_while_it_goes_on() {
        let (recorder, mut tap) = recorder();
        let wait_for = |samples: usize| {
            eventually("the thread did not drain", || {
                recorder.kept.lock().unwrap().speech.len() >= samples
            });
        };
        recorder.start();

        // Together more than the ring holds, so the first part must be gone by the second.
        let three_quarters = CAPACITY * 3 / 4;
        tap.push(&vec![0.5; three_quarters * CHANNELS]);
        wait_for(three_quarters);
        tap.push(&vec![0.5; three_quarters * CHANNELS]);
        wait_for(three_quarters * 2);

        assert_eq!(recorder.stop().len(), three_quarters * 2);
    }

    #[test]
    fn a_recording_nobody_stops_stops_growing() {
        let (mut writer, reader) = RingBuffer::new(CAPACITY);
        let mut kept = Kept {
            ring: Some(reader),
            speech: vec![0.0; MAX_RECORDED - 10],
        };
        let _ = writer.push_partial_slice(&[0.5; 100]);

        kept.collect();

        assert_eq!(kept.speech.len(), MAX_RECORDED);
    }

    #[test]
    fn starting_again_drops_what_a_full_recording_left_on_its_way() {
        let (recorder, mut tap) = recorder();
        recorder.start();
        recorder.kept.lock().unwrap().speech = vec![0.0; MAX_RECORDED];
        tap.push(&[0.25; 4]);
        recorder.stop();

        recorder.start();
        tap.push(&[0.75; 2]);

        assert_eq!(recorder.stop(), [0.75]);
    }
}
