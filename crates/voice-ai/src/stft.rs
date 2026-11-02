//! The short-time Fourier transform both models listen through.

use std::f64::consts::PI;
use std::sync::Arc;

use realfft::num_complex::Complex;
use realfft::{RealFftPlanner, RealToComplex};

pub(crate) struct Stft {
    size: usize,
    hop: usize,
    pad: usize,
    window: Vec<f32>,
    fft: Arc<dyn RealToComplex<f32>>,
    frame: Vec<f32>,
    spectrum: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
}

impl Stft {
    /// Frames of `size` samples every `hop`, after `pad` reflected samples on each side. The
    /// window is a periodic Hann, as the models were trained with.
    pub(crate) fn new(size: usize, hop: usize, pad: usize) -> Self {
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(size);
        let window = (0..size)
            .map(|n| (0.5 - 0.5 * (2.0 * PI * n as f64 / size as f64).cos()) as f32)
            .collect();
        Self {
            size,
            hop,
            pad,
            window,
            frame: fft.make_input_vec(),
            spectrum: fft.make_output_vec(),
            scratch: fft.make_scratch_vec(),
            fft,
        }
    }

    pub(crate) fn frame_count(&self, samples: usize) -> usize {
        ((samples + 2 * self.pad).max(self.size) - self.size) / self.hop + 1
    }

    /// Hands the spectrum of every frame to `each`, along with the number of the frame.
    pub(crate) fn run(&mut self, samples: &[f32], mut each: impl FnMut(usize, &[Complex<f32>])) {
        let mut padded = reflect_padded(samples, self.pad);
        padded.resize(padded.len().max(self.size), 0.0);

        for index in 0..self.frame_count(samples.len()) {
            let frame = &padded[index * self.hop..];
            for ((slot, &sample), &weight) in self.frame.iter_mut().zip(frame).zip(&self.window) {
                *slot = sample * weight;
            }
            self.fft
                .process_with_scratch(&mut self.frame, &mut self.spectrum, &mut self.scratch)
                .expect("the buffers are sized for the transform");
            each(index, &self.spectrum);
        }
    }
}

/// Mirrors the signal around its first and last sample, without repeating them.
fn reflect_padded(samples: &[f32], pad: usize) -> Vec<f32> {
    if samples.is_empty() {
        return vec![0.0; 2 * pad];
    }
    let last = samples.len() - 1;
    let reflect = |position: isize| {
        let period = (2 * last) as isize;
        if period == 0 {
            return 0;
        }
        let folded = position.rem_euclid(period) as usize;
        if folded <= last {
            folded
        } else {
            2 * last - folded
        }
    };
    (0..samples.len() + 2 * pad)
        .map(|index| samples[reflect(index as isize - pad as isize)])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padding_mirrors_the_signal_without_repeating_its_edges() {
        let padded = reflect_padded(&[1.0, 2.0, 3.0, 4.0], 3);

        assert_eq!(padded, [4.0, 3.0, 2.0, 1.0, 2.0, 3.0, 4.0, 3.0, 2.0, 1.0]);
    }

    #[test]
    fn padding_a_signal_shorter_than_the_padding_keeps_mirroring() {
        let padded = reflect_padded(&[1.0, 2.0], 3);

        assert_eq!(padded, [2.0, 1.0, 2.0, 1.0, 2.0, 1.0, 2.0, 1.0]);
    }

    #[test]
    fn padding_nothing_gives_silence() {
        assert_eq!(reflect_padded(&[], 2), [0.0; 4]);
        assert_eq!(reflect_padded(&[5.0], 2), [5.0; 5]);
    }

    #[test]
    fn a_signal_shorter_than_a_frame_still_has_one_frame() {
        let mut stft = Stft::new(8, 4, 0);
        let mut frames = 0;

        stft.run(&[1.0; 3], |_, _| frames += 1);

        assert_eq!(stft.frame_count(3), 1);
        assert_eq!(frames, 1);
    }

    #[test]
    fn frames_follow_the_hop() {
        let mut stft = Stft::new(8, 4, 2);
        let mut seen = Vec::new();

        stft.run(&[1.0; 20], |index, spectrum| {
            seen.push((index, spectrum.len()))
        });

        assert_eq!(stft.frame_count(20), 5);
        assert_eq!(seen, [(0, 5), (1, 5), (2, 5), (3, 5), (4, 5)]);
    }
}
