//! Pieces of a sound, cut at cue points like the hot cues of a DJ player, each for a pad of
//! its own. The file is never changed: a pad keeps the piece it plays.

use serde::{Deserialize, Serialize};
use specta::Type;

/// Shorter than this is a click, not a sound worth a pad.
pub const MIN_CUT_MS: u32 = 50;
/// A sound starts after at least this much quiet, so the gaps inside a word do not split it.
const GAP_MS: u32 = 150;
/// Quiet is this many times quieter than the loudest moment of the recording, or more.
const QUIET_BELOW_LOUDEST: f32 = 0.05;
/// Below this the recording is silent, however quiet it is overall.
const SILENCE: f32 = 1e-4;
/// A detected cue sits this much before the sound, so its attack is not cut off.
const PRE_ROLL_MS: u32 = 10;

/// A piece of a sound that a pad plays instead of the whole file, in milliseconds from its
/// start. It always lasts at least [`MIN_CUT_MS`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Cut {
    pub start_ms: u32,
    pub end_ms: u32,
}

impl Cut {
    /// The piece from `start_ms` to `end_ms` of a sound `duration_ms` long, kept inside it.
    /// `None` when what is left is shorter than [`MIN_CUT_MS`].
    pub fn within(start_ms: u32, end_ms: u32, duration_ms: u32) -> Option<Self> {
        let end_ms = end_ms.min(duration_ms);
        let start_ms = start_ms.min(end_ms);
        (end_ms - start_ms >= MIN_CUT_MS).then_some(Self { start_ms, end_ms })
    }

    pub fn duration_ms(&self) -> u32 {
        self.end_ms - self.start_ms
    }

    /// Where the piece starts and ends, such as "0:01.2-0:02.5".
    pub fn label(&self) -> String {
        format!("{}-{}", clock(self.start_ms), clock(self.end_ms))
    }
}

/// Minutes, seconds and tenths, such as "1:02.5".
fn clock(ms: u32) -> String {
    let tenths = ms / 100;
    format!("{}:{:02}.{}", tenths / 600, tenths / 10 % 60, tenths % 10)
}

/// The pieces between cue points, the way the hot cues of a DJ player split a track: from each
/// cue to the next one, and from the last to the end of the sound. Cues outside the sound, the
/// same cue twice, and a cue too close to the one before it are left out, so every piece lasts
/// at least [`MIN_CUT_MS`].
pub fn pieces(cues: &[u32], duration_ms: u32) -> Vec<Cut> {
    let mut sorted: Vec<u32> = cues
        .iter()
        .copied()
        .filter(|&cue| cue < duration_ms)
        .collect();
    sorted.sort_unstable();

    let mut starts: Vec<u32> = Vec::with_capacity(sorted.len());
    for cue in sorted {
        if starts.last().is_none_or(|&last| cue - last >= MIN_CUT_MS) {
            starts.push(cue);
        }
    }
    // The last piece needs room before the end, or it goes and the one before reaches the end.
    while starts
        .last()
        .is_some_and(|&last| duration_ms - last < MIN_CUT_MS)
    {
        starts.pop();
    }

    let ends = starts.iter().skip(1).copied().chain([duration_ms]);
    starts
        .iter()
        .zip(ends)
        .map(|(&start_ms, end_ms)| Cut { start_ms, end_ms })
        .collect()
}

/// Where each sound of a recording starts, in milliseconds, for a recording of several sounds
/// with pauses between them. `levels` is how loud the recording is, one value from 0 to 1 per
/// `frame_ms`. A sound starts when the recording gets loud after at least [`GAP_MS`] of quiet,
/// and quiet is relative to the loudest moment, so the result does not depend on the volume.
pub fn onsets(levels: &[f32], frame_ms: u32) -> Vec<u32> {
    if frame_ms == 0 {
        return Vec::new();
    }
    let loudest = levels.iter().copied().fold(0.0_f32, f32::max);
    if loudest < SILENCE {
        return Vec::new();
    }
    let quiet = (loudest * QUIET_BELOW_LOUDEST).max(SILENCE);
    let gap_frames = GAP_MS.div_ceil(frame_ms) as usize;

    let mut found = Vec::new();
    // The start of the recording counts as a long quiet.
    let mut quiet_frames = gap_frames;
    for (index, &level) in levels.iter().enumerate() {
        if level < quiet {
            quiet_frames += 1;
            continue;
        }
        if quiet_frames >= gap_frames {
            let at = index as u32 * frame_ms;
            found.push(at.saturating_sub(PRE_ROLL_MS));
        }
        quiet_frames = 0;
    }
    found
}
