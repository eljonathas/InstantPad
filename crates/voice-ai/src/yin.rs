//! The pitch of the voice, frame by frame, the way YIN finds it: the delay at which the speech
//! looks most like itself is one period of the voice. Far cheaper than a neural pitch model,
//! which would take most of the time a trained voice has to answer in.

/// The rate the pitch is read at.
pub(crate) const RATE: usize = 16_000;
/// A frame every 10 ms, the rate the trained voices read their pitch at.
pub(crate) const HOP: usize = 160;
/// The stretch each frame looks at, long enough for two periods of the deepest voice.
const WINDOW: usize = 1024;
const LOWEST_HZ: f32 = 50.0;
const HIGHEST_HZ: f32 = 1100.0;
/// A delay counts as a period when the speech differs from itself this little.
const THRESHOLD: f32 = 0.1;
/// Above this, even the best delay is not a period: breath, noise or a consonant.
const UNVOICED_ABOVE: f32 = 0.5;
/// Quieter than this, a frame is silence, whatever it looks like.
const SILENCE: f32 = 0.003;

/// The longest and shortest periods looked for, in samples.
const LONGEST: usize = (RATE as f32 / LOWEST_HZ) as usize;
const SHORTEST: usize = (RATE as f32 / HIGHEST_HZ) as usize;
/// What each delay is compared over, so every delay compares the same number of samples.
const COMPARED: usize = WINDOW - LONGEST;

/// The pitch in hertz of every [`HOP`] of `speech` at [`RATE`], 0 where there is none. Frame `n`
/// is centered on sample `n * HOP`, with silence before the start and after the end, so there are
/// `1 + speech.len() / HOP` of them.
pub(crate) fn pitch(speech: &[f32]) -> Vec<f32> {
    let half = WINDOW / 2;
    let mut padded = vec![0.0; speech.len() + WINDOW];
    padded[half..half + speech.len()].copy_from_slice(speech);

    let frames = 1 + speech.len() / HOP;
    let mut difference = vec![0.0; LONGEST + 1];
    (0..frames)
        .map(|index| {
            let frame = &padded[index * HOP..index * HOP + WINDOW];
            frame_pitch(frame, &mut difference)
        })
        .collect()
}

fn frame_pitch(frame: &[f32], difference: &mut [f32]) -> f32 {
    let loudness = (frame.iter().map(|sample| sample * sample).sum::<f32>() / WINDOW as f32).sqrt();
    if loudness < SILENCE {
        return 0.0;
    }

    // How much the speech differs from itself `delay` samples later.
    for (delay, value) in difference.iter_mut().enumerate().skip(1) {
        *value = frame[..COMPARED]
            .iter()
            .zip(&frame[delay..delay + COMPARED])
            .map(|(a, b)| (a - b) * (a - b))
            .sum();
    }
    // Divided by the mean of the shorter delays, so a delay of almost nothing is not taken for
    // the best one.
    let mut normalized = vec![1.0; LONGEST + 1];
    let mut running = 0.0;
    for delay in 1..=LONGEST {
        running += difference[delay];
        normalized[delay] = if running > 0.0 {
            difference[delay] * delay as f32 / running
        } else {
            1.0
        };
    }

    let candidates = SHORTEST.max(2)..LONGEST;
    // The first dip under the threshold, followed to its bottom, or else the deepest one.
    let period = candidates
        .clone()
        .find(|&delay| normalized[delay] < THRESHOLD)
        .map(|mut delay| {
            while delay + 1 < LONGEST && normalized[delay + 1] < normalized[delay] {
                delay += 1;
            }
            delay
        })
        .or_else(|| candidates.min_by(|&a, &b| normalized[a].total_cmp(&normalized[b])));
    let Some(period) = period else {
        return 0.0;
    };
    if normalized[period] > UNVOICED_ABOVE {
        return 0.0;
    }

    // Between two samples: the bottom of the parabola through the dip and its neighbors.
    let (before, at, after) = (
        normalized[period - 1],
        normalized[period],
        normalized[period + 1],
    );
    let curve = before - 2.0 * at + after;
    let offset = if curve > 0.0 {
        (0.5 * (before - after) / curve).clamp(-0.5, 0.5)
    } else {
        0.0
    };
    RATE as f32 / (period as f32 + offset)
}

#[cfg(test)]
mod tests {
    use std::f32::consts::TAU;

    use super::*;

    /// A voice-like sound: a tone with its harmonics, which a plain sine is not.
    fn voice(hertz: f32, seconds: f32) -> Vec<f32> {
        (0..(RATE as f32 * seconds) as usize)
            .map(|n| {
                let phase = TAU * hertz * n as f32 / RATE as f32;
                0.3 * phase.sin() + 0.15 * (2.0 * phase).sin() + 0.08 * (3.0 * phase).sin()
            })
            .collect()
    }

    fn middle(pitches: &[f32]) -> &[f32] {
        &pitches[8..pitches.len() - 8]
    }

    #[test]
    fn a_voice_is_heard_at_its_pitch_from_deep_to_high() {
        for hertz in [85.0, 110.0, 196.0, 261.6, 440.0, 880.0] {
            let pitches = pitch(&voice(hertz, 0.5));

            for &found in middle(&pitches) {
                assert!(
                    (found / hertz - 1.0).abs() < 0.01,
                    "{hertz} Hz was heard as {found} Hz"
                );
            }
        }
    }

    #[test]
    fn there_is_a_frame_every_hop_centered_on_it() {
        assert_eq!(pitch(&vec![0.0; 5600]).len(), 36);
        assert_eq!(pitch(&[]).len(), 1);
    }

    #[test]
    fn silence_and_noise_have_no_pitch() {
        assert!(pitch(&vec![0.0; 4800]).iter().all(|&found| found == 0.0));

        // A sequence that never repeats itself, as noise does not.
        let mut state = 12_345_u32;
        let noise: Vec<f32> = (0..4800)
            .map(|_| {
                state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                (state >> 16) as f32 / 32_768.0 - 1.0
            })
            .collect();
        let voiced = pitch(&noise).iter().filter(|&&found| found > 0.0).count();
        assert!(voiced < 4, "{voiced} frames of noise had a pitch");
    }

    #[test]
    fn a_whisper_of_a_tone_is_silence() {
        let quiet: Vec<f32> = voice(200.0, 0.3)
            .iter()
            .map(|sample| sample * 0.001)
            .collect();

        assert!(pitch(&quiet).iter().all(|&found| found == 0.0));
    }
}
