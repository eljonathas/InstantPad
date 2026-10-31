//! Hands samples from the capture thread to the render thread. The two devices run on
//! different clocks, so the reader drops the backlog when latency builds up.

use std::sync::{Arc, Mutex};

use rtrb::{Consumer, Producer, RingBuffer};

use crate::format::samples_for;

const PREBUFFER: usize = samples_for(30);
const MAX_BACKLOG: usize = samples_for(120);
const CAPACITY: usize = samples_for(1000);

/// Where the game mixer finds the microphone. It is empty while no microphone is open, and it
/// outlives the game output so a reopened output keeps hearing the microphone. The lock is
/// only ever contended while the microphone is being swapped, and the callback uses `try_lock`.
pub(crate) type MicSlot = Arc<Mutex<Option<MicReader>>>;

/// An empty slot. The std mutex of some platforms allocates the first time it is locked, so it
/// is locked here, where allocating is fine, and not for the first time in a callback.
pub(crate) fn empty_slot() -> MicSlot {
    let slot = MicSlot::default();
    drop(slot.lock());
    slot
}

pub(crate) fn ring() -> (Producer<f32>, MicReader) {
    let (writer, samples) = RingBuffer::new(CAPACITY);
    (
        writer,
        MicReader {
            samples,
            primed: false,
        },
    )
}

pub(crate) struct MicReader {
    samples: Consumer<f32>,
    primed: bool,
}

impl MicReader {
    /// Fills `destination` with as much as is ready and returns how many samples that was.
    pub(crate) fn read(&mut self, destination: &mut [f32]) -> usize {
        let backlog = self.samples.slots();
        if backlog > MAX_BACKLOG {
            // The backlog and the prebuffer are both whole frames, or left and right would swap.
            self.samples
                .read_chunk(backlog - PREBUFFER)
                .expect("the backlog is available")
                .commit_all();
        }

        if !self.primed {
            if self.samples.slots() < PREBUFFER {
                return 0;
            }
            self.primed = true;
        }

        let (read, _) = self.samples.pop_partial_slice(destination);
        let read = read.len();

        // Ran dry: wait for the prebuffer again instead of crackling sample by sample.
        if read < destination.len() {
            self.primed = false;
        }
        read
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(writer: &mut Producer<f32>, samples: &[f32]) {
        assert!(writer.push_partial_slice(samples).1.is_empty());
    }

    #[test]
    fn the_mic_buffer_drops_backlog_instead_of_building_delay() {
        let (mut writer, mut reader) = ring();
        write(&mut writer, &vec![0.5; samples_for(1000)]);

        let mut block = vec![0.0; samples_for(1000)];

        assert!(reader.read(&mut block) <= samples_for(120));
    }

    #[test]
    fn dropping_backlog_never_swaps_left_and_right() {
        let (mut writer, mut reader) = ring();
        let frames = [1.0, -1.0].repeat(samples_for(500) / 2);
        write(&mut writer, &frames);

        let mut block = [0.0; 4];
        reader.read(&mut block);

        assert_eq!(block, [1.0, -1.0, 1.0, -1.0]);
    }

    #[test]
    fn the_mic_buffer_waits_for_its_prebuffer() {
        let (mut writer, mut reader) = ring();
        write(&mut writer, &[0.0; 10]);

        assert_eq!(reader.read(&mut [0.0; 10]), 0);
    }

    #[test]
    fn the_mic_buffer_waits_for_its_prebuffer_again_after_running_dry() {
        let (mut writer, mut reader) = ring();
        write(&mut writer, &vec![0.5; PREBUFFER]);
        assert_eq!(reader.read(&mut [0.0; 10]), 10);

        let mut rest = vec![0.0; PREBUFFER];
        assert_eq!(reader.read(&mut rest), PREBUFFER - 10);

        write(&mut writer, &[0.5; 10]);
        assert_eq!(reader.read(&mut [0.0; 10]), 0);
    }
}
