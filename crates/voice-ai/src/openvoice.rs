//! OpenVoice: listens for the tone color of a voice and gives it to another speech.

use ort::session::Session;
use ort::value::{Tensor, TensorRef};

use crate::stft::Stft;

/// The rate the model works at.
pub(crate) const RATE: usize = 22_050;
/// Samples between two frames of the spectrogram.
pub(crate) const HOP: usize = 256;
const FRAME: usize = 1024;
const PAD: usize = 384;
const BINS: usize = FRAME / 2 + 1;
/// How much of the original voice survives the conversion.
const TAU: f32 = 0.3;

/// What the model reads: the linear magnitude of the speech.
pub(crate) struct Spectrogram {
    stft: Stft,
}

impl Spectrogram {
    pub(crate) fn new() -> Self {
        Self {
            stft: Stft::new(FRAME, HOP, PAD),
        }
    }

    /// One row of bins per frame.
    fn frames(&mut self, speech: &[f32]) -> Vec<f32> {
        let mut magnitudes = vec![0.0; self.stft.frame_count(speech.len()) * BINS];
        self.stft.run(speech, |index, spectrum| {
            let row = &mut magnitudes[index * BINS..][..BINS];
            for (magnitude, bin) in row.iter_mut().zip(spectrum) {
                *magnitude = (bin.norm_sqr() + 1e-6).sqrt();
            }
        });
        magnitudes
    }
}

/// The voiceprint of the whole speech.
pub(crate) fn voiceprint(session: &mut Session, speech: &[f32]) -> ort::Result<Vec<f32>> {
    let frames = Spectrogram::new().frames(speech);
    let count = frames.len() / BINS;
    let outputs = session.run(ort::inputs![
        "input" => Tensor::from_array(([1, count, BINS], frames))?,
    ])?;
    let (_, tone) = outputs["tone_embedding"].try_extract_tensor::<f32>()?;
    Ok(tone.to_vec())
}

/// Speech with the voice of `source` made into the voice of `target`, as long as `speech`,
/// which must be a whole number of hops.
pub(crate) fn convert(
    session: &mut Session,
    spectrogram: &mut Spectrogram,
    speech: &[f32],
    source: &[f32],
    target: &[f32],
) -> ort::Result<Vec<f32>> {
    let count = speech.len() / HOP;
    let frames = spectrogram.frames(speech);
    let outputs = session.run(ort::inputs![
        "audio" => Tensor::from_array(([1, BINS, count], bins_first(&frames)))?,
        "audio_length" => Tensor::from_array(([1], vec![count as i64]))?,
        "src_tone" => TensorRef::from_array_view(([1, source.len(), 1], source))?,
        "dest_tone" => TensorRef::from_array_view(([1, target.len(), 1], target))?,
        "tau" => Tensor::from_array(([1], vec![TAU]))?,
    ])?;
    let (_, converted) = outputs["converted_audio"].try_extract_tensor::<f32>()?;
    Ok(converted.to_vec())
}

/// The conversion reads a row per bin, where the voiceprint reads a row per frame.
fn bins_first(frames: &[f32]) -> Vec<f32> {
    let count = frames.len() / BINS;
    let mut bins = vec![0.0; frames.len()];
    for (index, row) in frames.as_chunks::<BINS>().0.iter().enumerate() {
        for (bin, &magnitude) in row.iter().enumerate() {
            bins[bin * count + index] = magnitude;
        }
    }
    bins
}

#[cfg(test)]
mod tests {
    use std::f32::consts::TAU;

    use super::*;

    fn tone() -> Vec<f32> {
        (0..2048)
            .map(|n| 0.5 * (TAU * 440.0 * n as f32 / RATE as f32).sin())
            .collect()
    }

    #[test]
    fn the_spectrogram_of_a_tone_matches_the_reference() {
        let frames = Spectrogram::new().frames(&tone());

        assert_eq!(frames.len(), 8 * BINS);
        let close = |value: f32, expected: f32| (value - expected).abs() < expected * 1e-3;
        assert!(
            close(frames[3 * BINS + 20], 113.21689),
            "{}",
            frames[3 * BINS + 20]
        );
        assert!(close(frames[0], 6.803779), "{}", frames[0]);
    }

    #[test]
    fn the_conversion_reads_one_row_per_bin() {
        let frames = Spectrogram::new().frames(&tone());

        let bins = bins_first(&frames);

        assert_eq!(bins[20 * 8 + 3], frames[3 * BINS + 20]);
        assert_eq!(bins[0], frames[0]);
        assert_eq!(bins[BINS * 8 - 1], frames[8 * BINS - 1]);
    }
}
