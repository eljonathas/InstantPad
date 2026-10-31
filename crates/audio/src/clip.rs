//! A sound fully decoded to the engine format.

use std::fs::File;
use std::path::Path;

use instantpad_application::message::{Failure, TextKey};
use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error;
use symphonia::core::formats::TrackType;
use symphonia::core::formats::probe::Hint;
use symphonia::core::io::MediaSourceStream;

use crate::format::{CHANNELS, SAMPLE_RATE};

/// Frames the resampler works on at a time.
const RESAMPLE_CHUNK: usize = 1024;

/// How many bars the display draws for a sound.
const PEAK_BARS: usize = 44;

pub(crate) struct SoundClip {
    samples: Vec<f32>,
    peaks: Vec<f32>,
}

impl SoundClip {
    pub(crate) fn new(samples: Vec<f32>) -> Self {
        let peaks = peaks_of(&samples);
        Self { samples, peaks }
    }

    /// Interleaved stereo at the engine rate.
    pub(crate) fn samples(&self) -> &[f32] {
        &self.samples
    }

    /// The loudest moment of each stretch of the sound, from 0 to 1.
    pub(crate) fn peaks(&self) -> &[f32] {
        &self.peaks
    }

    pub(crate) fn seconds(&self) -> f32 {
        (self.samples.len() / CHANNELS) as f32 / SAMPLE_RATE as f32
    }

    pub(crate) fn load(path: &str) -> Result<Self, Failure> {
        let (rate, stereo) = decode(path)?;
        if stereo.is_empty() {
            return Err(Failure::text(TextKey::SoundHasNoAudio));
        }

        if rate == SAMPLE_RATE {
            return Ok(Self::new(stereo));
        }
        resample(rate, &stereo).map(Self::new).map_err(open_failed)
    }
}

/// Scaled to the loudest moment, so a quiet sound still fills the display.
fn peaks_of(samples: &[f32]) -> Vec<f32> {
    let stretch = samples.len().div_ceil(PEAK_BARS).max(1);
    let peaks: Vec<f32> = samples
        .chunks(stretch)
        .map(|part| {
            part.iter()
                .fold(0.0, |peak: f32, sample| peak.max(sample.abs()))
        })
        .collect();
    let loudest = peaks.iter().copied().fold(0.0, f32::max);
    if loudest == 0.0 {
        return peaks;
    }
    peaks.iter().map(|peak| peak / loudest).collect()
}

fn open_failed(detail: impl ToString) -> Failure {
    Failure::because(TextKey::SoundOpenFailed, detail)
}

/// Returns the sample rate of the file and its audio as interleaved stereo.
fn decode(path: &str) -> Result<(u32, Vec<f32>), Failure> {
    let file = File::open(path).map_err(open_failed)?;
    let mut hint = Hint::new();
    if let Some(extension) = Path::new(path).extension().and_then(|name| name.to_str()) {
        hint.with_extension(extension);
    }
    let source = MediaSourceStream::new(Box::new(file), Default::default());
    let mut format = symphonia::default::get_probe()
        .probe(&hint, source, Default::default(), Default::default())
        .map_err(open_failed)?;

    let no_audio = || Failure::text(TextKey::SoundHasNoAudio);
    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(no_audio)?;
    let parameters = track
        .codec_params
        .as_ref()
        .and_then(|parameters| parameters.audio())
        .ok_or_else(no_audio)?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(parameters, &AudioDecoderOptions::default())
        .map_err(open_failed)?;
    let track_id = track.id;

    let mut rate = SAMPLE_RATE;
    let mut decoded = Vec::new();
    let mut stereo = Vec::new();
    while let Some(packet) = format.next_packet().map_err(open_failed)? {
        if packet.track_id != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(buffer) => {
                rate = buffer.spec().rate();
                decoded.resize(buffer.samples_interleaved(), 0.0);
                buffer.copy_to_slice_interleaved(&mut decoded);
                append_as_stereo(&decoded, buffer.spec().channels().count(), &mut stereo);
            }
            // One bad packet costs a click, not the whole sound.
            Err(Error::DecodeError(_)) => {}
            Err(error) => return Err(open_failed(error)),
        }
    }
    Ok((rate, stereo))
}

/// Mono becomes stereo and anything beyond two channels keeps the first two.
fn append_as_stereo(decoded: &[f32], channels: usize, stereo: &mut Vec<f32>) {
    if channels == 1 {
        stereo.extend(decoded.iter().flat_map(|&sample| [sample, sample]));
    } else {
        for frame in decoded.chunks_exact(channels) {
            stereo.extend_from_slice(&frame[..CHANNELS]);
        }
    }
}

fn resample(rate: u32, stereo: &[f32]) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
    let frames = stereo.len() / CHANNELS;
    let mut resampler = Fft::<f32>::new(
        rate as usize,
        SAMPLE_RATE as usize,
        RESAMPLE_CHUNK,
        CHANNELS,
        FixedSync::Input,
    )?;
    let input = InterleavedSlice::new(stereo, CHANNELS, frames)?;
    Ok(resampler.process_all(&input, frames, None)?.take_data())
}

#[cfg(test)]
pub(crate) mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[test]
    fn the_peaks_follow_the_loud_and_quiet_parts_of_a_sound() {
        let quiet = vec![0.05; 48_000];
        let loud = vec![0.5; 48_000];
        let clip = SoundClip::new([quiet, loud].concat());

        let peaks = clip.peaks();

        assert!(peaks.len() <= PEAK_BARS);
        assert!((peaks[0] - 0.1).abs() < 0.001);
        assert_eq!(peaks[peaks.len() - 1], 1.0);
    }

    #[test]
    fn silence_has_flat_peaks() {
        let clip = SoundClip::new(vec![0.0; 1000]);

        assert!(clip.peaks().iter().all(|&peak| peak == 0.0));
    }

    #[test]
    fn a_clip_knows_how_long_it_lasts() {
        let clip = SoundClip::new(vec![0.0; SAMPLE_RATE as usize * CHANNELS / 2]);

        assert_eq!(clip.seconds(), 0.5);
    }

    /// A file in the temporary folder that goes away with the value.
    pub(crate) struct TempFile(PathBuf);

    impl TempFile {
        pub(crate) fn path(&self) -> &str {
            self.0.to_str().unwrap()
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    /// A 16-bit PCM WAV with `frames` frames, where channel `n` holds the constant `levels[n]`.
    pub(crate) fn wav(rate: u32, levels: &[i16], frames: usize) -> TempFile {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let name = format!(
            "instantpad-audio-{}-{}.wav",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let path = std::env::temp_dir().join(name);

        let channels = levels.len() as u16;
        let data_size = (frames * levels.len() * 2) as u32;
        let mut bytes = Vec::new();
        bytes.extend(b"RIFF");
        bytes.extend((36 + data_size).to_le_bytes());
        bytes.extend(b"WAVEfmt ");
        bytes.extend(16_u32.to_le_bytes());
        bytes.extend(1_u16.to_le_bytes());
        bytes.extend(channels.to_le_bytes());
        bytes.extend(rate.to_le_bytes());
        bytes.extend((rate * u32::from(channels) * 2).to_le_bytes());
        bytes.extend((channels * 2).to_le_bytes());
        bytes.extend(16_u16.to_le_bytes());
        bytes.extend(b"data");
        bytes.extend(data_size.to_le_bytes());
        for _ in 0..frames {
            for level in levels {
                bytes.extend(level.to_le_bytes());
            }
        }
        fs::write(&path, bytes).unwrap();
        TempFile(path)
    }

    fn level(sample: i16) -> f32 {
        f32::from(sample) / 32768.0
    }

    #[test]
    fn a_stereo_file_at_the_engine_rate_is_kept_as_it_is() {
        let file = wav(48_000, &[8192, -8192], 480);

        let clip = SoundClip::load(file.path()).unwrap();

        assert_eq!(clip.samples().len(), 960);
        assert_eq!(&clip.samples()[..2], [level(8192), level(-8192)]);
    }

    #[test]
    fn mono_becomes_stereo() {
        let file = wav(48_000, &[8192], 480);

        let clip = SoundClip::load(file.path()).unwrap();

        assert_eq!(clip.samples().len(), 960);
        assert!(clip.samples().iter().all(|&sample| sample == level(8192)));
    }

    #[test]
    fn more_than_two_channels_keep_the_first_two() {
        let file = wav(48_000, &[4096, 8192, 16384], 480);

        let clip = SoundClip::load(file.path()).unwrap();

        assert_eq!(clip.samples().len(), 960);
        assert_eq!(
            &clip.samples()[..4],
            [level(4096), level(8192), level(4096), level(8192)]
        );
    }

    #[test]
    fn another_sample_rate_is_resampled_to_the_engine_rate() {
        let file = wav(24_000, &[8192, 8192], 12_000);

        let clip = SoundClip::load(file.path()).unwrap();

        let frames = clip.samples().len() / CHANNELS;
        assert!(frames.abs_diff(24_000) <= RESAMPLE_CHUNK, "{frames} frames");
        // The middle of a constant signal is still that constant after resampling.
        assert!((clip.samples()[frames] - level(8192)).abs() < 0.01);
    }

    #[test]
    fn a_file_without_audio_has_no_audio() {
        let file = wav(48_000, &[0, 0], 0);

        let failure = SoundClip::load(file.path()).err().unwrap();

        assert_eq!(failure, Failure::text(TextKey::SoundHasNoAudio));
    }

    #[test]
    fn a_missing_file_fails_to_open() {
        let failure = SoundClip::load("/no/such/sound.mp3").err().unwrap();

        assert!(matches!(
            failure.0,
            instantpad_application::message::Message::Text {
                key: TextKey::SoundOpenFailed,
                ..
            }
        ));
    }

    #[test]
    fn a_file_that_is_not_audio_fails_to_open() {
        let file = wav(48_000, &[0], 4);
        fs::write(file.path(), b"not audio at all, just some text in a file").unwrap();

        let failure = SoundClip::load(file.path()).err().unwrap();

        assert!(matches!(
            failure.0,
            instantpad_application::message::Message::Text {
                key: TextKey::SoundOpenFailed,
                ..
            }
        ));
    }
}
