//! Converts speech that never ends in overlapping stretches, so each sounds like it was
//! heard whole.

use instantpad_application::message::Failure;

use crate::openvoice::HOP;
use crate::runtime::load_failed;

/// The stretch that is converted and handed back at a time.
const CHUNK: usize = 3072;
/// What the model hears after the chunk, so the end of it knows what comes next.
const LOOKAHEAD: usize = 768;
/// Where one chunk fades into the next, which the model converts twice.
const CROSSFADE: usize = 256;
/// What the model hears before the chunk, so its voice does not change at every join.
const CONTEXT: usize = 4352;

// The model reads whole hops, and this keeps every window one without padding it.
const _: () = assert!(
    CHUNK.is_multiple_of(HOP)
        && LOOKAHEAD.is_multiple_of(HOP)
        && CROSSFADE.is_multiple_of(HOP)
        && CONTEXT.is_multiple_of(HOP)
);

#[derive(Default)]
pub(crate) struct Stream {
    /// The context, which is already converted, followed by the speech still waiting.
    buffer: Vec<f32>,
    context: usize,
    /// The start of the next chunk as the last window converted it.
    tail: Option<Vec<f32>>,
}

impl Stream {
    /// Takes the next speech and returns the converted speech that is ready, a chunk at a
    /// time. `convert` turns a window of speech into one as long.
    pub(crate) fn convert(
        &mut self,
        speech: &[f32],
        convert: &mut impl FnMut(&[f32]) -> Result<Vec<f32>, Failure>,
    ) -> Result<Vec<f32>, Failure> {
        self.buffer.extend_from_slice(speech);

        let mut ready = Vec::new();
        while self.buffer.len() - self.context >= CHUNK + LOOKAHEAD + CROSSFADE {
            let window = &self.buffer[..self.context + CHUNK + LOOKAHEAD + CROSSFADE];
            let converted = convert(window)?;
            let stretch = converted
                .get(self.context..self.context + CHUNK + CROSSFADE)
                .ok_or_else(|| load_failed("the model returned too little speech"))?;

            let (chunk, tail) = stretch.split_at(CHUNK);
            match self.tail.replace(tail.to_vec()) {
                Some(previous) => {
                    for (position, (&before, &after)) in previous.iter().zip(chunk).enumerate() {
                        let weight = position as f32 / (CROSSFADE - 1) as f32;
                        ready.push(before * (1.0 - weight) + after * weight);
                    }
                    ready.extend_from_slice(&chunk[CROSSFADE..]);
                }
                None => ready.extend_from_slice(chunk),
            }

            self.context += CHUNK;
            let spent = self.context.saturating_sub(CONTEXT);
            self.buffer.drain(..spent);
            self.context -= spent;
        }
        Ok(ready)
    }
}

#[cfg(test)]
mod tests {
    use instantpad_application::message::TextKey;

    use super::*;

    fn ramp(length: usize) -> Vec<f32> {
        (0..length).map(|n| n as f32).collect()
    }

    fn identity(window: &[f32]) -> Result<Vec<f32>, Failure> {
        Ok(window.to_vec())
    }

    #[test]
    fn nothing_is_ready_until_a_chunk_has_its_lookahead() {
        let mut stream = Stream::default();

        let early = stream.convert(&ramp(CHUNK + LOOKAHEAD + CROSSFADE - 1), &mut identity);
        let ready = stream.convert(&[0.0], &mut identity);

        assert!(early.unwrap().is_empty());
        assert_eq!(ready.unwrap().len(), CHUNK);
    }

    #[test]
    fn speech_that_needs_no_conversion_comes_out_as_it_went_in() {
        let mut stream = Stream::default();
        let speech = ramp(5 * CHUNK + 1000);

        let ready = stream.convert(&speech, &mut identity).unwrap();

        // The last chunk waits for the lookahead the speech has not reached.
        let chunks = (speech.len() - LOOKAHEAD - CROSSFADE) / CHUNK;
        assert_eq!(ready.len(), chunks * CHUNK);
        for (heard, said) in ready.iter().zip(&speech) {
            assert!((heard - said).abs() < 0.01, "{heard} {said}");
        }
    }

    #[test]
    fn the_pieces_the_speech_arrives_in_do_not_change_what_comes_out() {
        let speech = ramp(4 * CHUNK + 2000);
        let whole = Stream::default().convert(&speech, &mut identity).unwrap();

        let mut stream = Stream::default();
        let mut pieces = Vec::new();
        for piece in speech.chunks(480) {
            pieces.extend(stream.convert(piece, &mut identity).unwrap());
        }

        assert_eq!(pieces, whole);
    }

    #[test]
    fn the_model_hears_the_speech_before_the_chunk_but_never_more_than_the_context() {
        let mut stream = Stream::default();
        let mut windows = Vec::new();

        stream
            .convert(&ramp(6 * CHUNK + LOOKAHEAD + CROSSFADE), &mut |window| {
                windows.push((window[0], window.len()));
                Ok(window.to_vec())
            })
            .unwrap();

        let ahead = CHUNK + LOOKAHEAD + CROSSFADE;
        assert_eq!(
            windows,
            [
                (0.0, ahead),
                (0.0, CHUNK + ahead),
                ((2 * CHUNK - CONTEXT) as f32, CONTEXT + ahead),
                ((3 * CHUNK - CONTEXT) as f32, CONTEXT + ahead),
                ((4 * CHUNK - CONTEXT) as f32, CONTEXT + ahead),
                ((5 * CHUNK - CONTEXT) as f32, CONTEXT + ahead),
            ]
        );
        assert!(windows.iter().all(|(_, length)| length % HOP == 0));
    }

    #[test]
    fn a_chunk_fades_in_from_the_end_of_the_chunk_before() {
        let mut stream = Stream::default();
        let mut windows = 0.0;
        let mut each_window_one_higher = |window: &[f32]| {
            windows += 1.0;
            Ok(window.iter().map(|sample| sample + windows).collect())
        };

        let ready = stream
            .convert(
                &vec![0.0; 3 * CHUNK + LOOKAHEAD + CROSSFADE],
                &mut each_window_one_higher,
            )
            .unwrap();

        assert_eq!(ready[0], 1.0);
        assert_eq!(ready[CHUNK - 1], 1.0);
        // The first window converted the start of the second chunk as 1, the second as 2.
        assert_eq!(ready[CHUNK], 1.0);
        assert!((ready[CHUNK + CROSSFADE / 2] - 1.5).abs() < 0.01);
        assert_eq!(ready[CHUNK + CROSSFADE - 1], 2.0);
        assert_eq!(ready[2 * CHUNK - 1], 2.0);
    }

    #[test]
    fn a_model_that_returns_too_little_is_a_failure() {
        let mut stream = Stream::default();

        let failure = stream
            .convert(&ramp(2 * CHUNK), &mut |_| Ok(vec![0.0; 10]))
            .unwrap_err();

        assert_eq!(failure, load_failed("the model returned too little speech"));
    }

    #[test]
    fn a_failing_model_stops_the_conversion() {
        let mut stream = Stream::default();
        let broken = Failure::text(TextKey::AiTooSlow);

        let failure = stream
            .convert(&ramp(2 * CHUNK), &mut |_| Err(broken.clone()))
            .unwrap_err();

        assert_eq!(failure, broken);
    }
}
