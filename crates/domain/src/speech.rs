//! Telling the parts of a recording where someone talks from the rest of it.

/// The pieces the recording is cut into, which are short enough to follow the words.
const FRAME_MILLISECONDS: usize = 20;
/// A talker is louder than the room by at least this many times.
const LOUDER_THAN_THE_ROOM: f32 = 3.0;
/// Below this the microphone hears nothing, whatever the room.
const SILENCE: f32 = 0.004;
/// The room is never taken louder than this share of the loudest moment. Someone who talks the
/// whole time, with no pause, makes the quietest tenth of the recording their own voice, which
/// would then never count; a voice is always far louder than its quietest syllables. A steady
/// noise under the voice, however loud, stays below it and is left out.
const ROOM_BELOW_LOUDEST: f32 = 0.1;
/// Speech is louder at its loudest than at its quietest by at least this much. A recording
/// that is not is silence, a steady noise or a hum, whatever its level.
const SPEECH_SWING: f32 = 2.0;
/// A click or a cough is shorter than a word, so a stretch has to last this many frames.
const MIN_FRAMES: usize = 5;
/// A quiet moment this short is the breath between two syllables, not the end of the talk.
const BRIDGED_FRAMES: usize = 8;

/// The parts of `speech` where someone talks, one after the other. `rate` is the samples in a
/// second.
///
/// A frame counts as talk when it is clearly louder than the room. The room is what the
/// quietest tenth of the recording sounds like, so a fan or a steady hum is not taken for a
/// voice, and neither are pauses or a click of the keyboard. Someone who never pauses makes the
/// quietest tenth their own voice, so the room is never taken louder than a tenth of the
/// loudest moment, and a recording that stays at one level throughout, loud or quiet, is no
/// voice at all.
pub fn voiced(speech: &[f32], rate: usize) -> Vec<f32> {
    // A rate too low for a frame still makes one of a single sample, which never panics.
    let frame = (rate * FRAME_MILLISECONDS / 1000).max(1);
    let frames: Vec<&[f32]> = speech.chunks_exact(frame).collect();
    if frames.is_empty() {
        return Vec::new();
    }
    let levels: Vec<f32> = frames.iter().map(|frame| level(frame)).collect();

    let mut sorted = levels.clone();
    sorted.sort_by(f32::total_cmp);
    let quiet = sorted[sorted.len() / 10];
    // The loudest frame, not a percentile: someone who said a few words in a long silence
    // still said them, and a click that stands alone is dropped further down for being short.
    let loud = sorted[sorted.len() - 1];
    if loud < SILENCE || loud < quiet * SPEECH_SWING {
        return Vec::new();
    }
    let room = quiet.min(loud * ROOM_BELOW_LOUDEST);
    let threshold = (room * LOUDER_THAN_THE_ROOM).max(SILENCE);

    let mut talking = Vec::new();
    let mut stretch = 0..0;
    let mut pause = 0;
    for (index, &level) in levels.iter().enumerate() {
        if level >= threshold {
            if stretch.is_empty() {
                stretch.start = index;
            }
            stretch.end = index + 1;
            pause = 0;
        } else if !stretch.is_empty() {
            pause += 1;
            if pause > BRIDGED_FRAMES {
                keep(&frames, &mut stretch, &mut talking);
            }
        }
    }
    keep(&frames, &mut stretch, &mut talking);
    talking
}

/// How loud a frame is: the root of the mean of its squares.
fn level(frame: &[f32]) -> f32 {
    (frame.iter().map(|sample| sample * sample).sum::<f32>() / frame.len() as f32).sqrt()
}

/// Adds the stretch to `talking` when it lasted long enough, and starts the next one.
fn keep(frames: &[&[f32]], stretch: &mut std::ops::Range<usize>, talking: &mut Vec<f32>) {
    if stretch.len() >= MIN_FRAMES {
        for frame in &frames[stretch.clone()] {
            talking.extend_from_slice(frame);
        }
    }
    *stretch = 0..0;
}
