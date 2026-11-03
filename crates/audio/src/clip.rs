//! A sound fully decoded to the engine format.

use std::fs::File;
use std::ops::Range;
use std::path::Path;

use instantpad_application::message::{Failure, TextKey};
use instantpad_domain::cuts::Cut;
use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error;
use symphonia::core::formats::TrackType;
use symphonia::core::formats::probe::Hint;
use symphonia::core::io::MediaSourceStream;

use crate::format::{CHANNELS, MAX_SPEECH_SECONDS, SAMPLE_RATE, downmix};

/// Frames the resampler works on at a time.
const RESAMPLE_CHUNK: usize = 1024;

/// How many bars the display draws for a sound.
const PEAK_BARS: usize = 44;

pub(crate) struct SoundClip {
    samples: Vec<f32>,
    peaks: Vec<f32>,
    /// The loudest sample, so a part of the sound is drawn at the scale of the whole.
    loudest: f32,
}

impl SoundClip {
    pub(crate) fn new(samples: Vec<f32>) -> Self {
        let peaks = peaks_of(&samples);
        let loudest = samples
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        Self {
            samples,
            peaks,
            loudest,
        }
    }

    pub(crate) fn duration_ms(&self) -> u32 {
        (self.samples.len() / CHANNELS * 1000 / SAMPLE_RATE as usize) as u32
    }

    /// The samples of `cut`, or of the whole clip. A cut that reaches past the end stops there.
    pub(crate) fn range(&self, cut: Option<Cut>) -> Range<usize> {
        let Some(cut) = cut else {
            return 0..self.samples.len();
        };
        let at = |ms: u32| {
            (ms as usize * SAMPLE_RATE as usize / 1000 * CHANNELS).min(self.samples.len())
        };
        at(cut.start_ms)..at(cut.end_ms)
    }

    /// The display bars of `range`, from 0 to 1.
    pub(crate) fn peaks_of(&self, range: Range<usize>) -> Vec<f32> {
        peaks_of(&self.samples[range])
    }

    /// How loud each of `bars` stretches of the part from `start_ms` to `end_ms` gets, from 0
    /// to 1 of the loudest moment of the whole clip, so zooming in does not change the scale.
    pub(crate) fn waveform(&self, start_ms: u32, end_ms: u32, bars: usize) -> Vec<f32> {
        let range = self.range(Some(Cut {
            start_ms,
            end_ms: end_ms.max(start_ms),
        }));
        let part = &self.samples[range];
        if bars == 0 || self.loudest == 0.0 {
            return vec![0.0; bars];
        }
        let frames = part.len() / CHANNELS;
        (0..bars)
            .map(|bar| {
                let from = frames * bar / bars * CHANNELS;
                let to = (frames * (bar + 1) / bars * CHANNELS).max(from);
                part[from..to]
                    .iter()
                    .fold(0.0_f32, |peak, sample| peak.max(sample.abs()))
                    / self.loudest
            })
            .collect()
    }

    /// How loud the clip is along the way, one value per `frame_ms`, from 0 to 1.
    pub(crate) fn levels(&self, frame_ms: u32) -> Vec<f32> {
        let frame = (frame_ms as usize * SAMPLE_RATE as usize / 1000).max(1) * CHANNELS;
        self.samples
            .chunks(frame)
            .map(|part| {
                part.iter()
                    .fold(0.0_f32, |peak, sample| peak.max(sample.abs()))
            })
            .collect()
    }

    /// Interleaved stereo at the engine rate.
    pub(crate) fn samples(&self) -> &[f32] {
        &self.samples
    }

    /// The loudest moment of each stretch of the sound, from 0 to 1.
    pub(crate) fn peaks(&self) -> &[f32] {
        &self.peaks
    }

    pub(crate) fn load(path: &str) -> Result<Self, Failure> {
        load_stereo(path, None).map(Self::new)
    }
}

/// A file of speech for the AI to learn from, as mono at the engine rate. Longer ones are
/// refused while decoding, so a whole podcast never fills the memory.
pub(crate) fn load_speech(path: &str) -> Result<Vec<f32>, Failure> {
    let stereo = load_stereo(path, Some(MAX_SPEECH_SECONDS))?;
    Ok(downmix(&stereo).collect())
}

/// The audio of a file as stereo at the engine rate. `max_seconds` refuses a longer file.
fn load_stereo(path: &str, max_seconds: Option<u32>) -> Result<Vec<f32>, Failure> {
    let (rate, stereo) = decode(path, max_seconds)?;
    if stereo.is_empty() {
        return Err(Failure::text(TextKey::SoundHasNoAudio));
    }

    if rate == SAMPLE_RATE {
        return Ok(stereo);
    }
    resample(rate, &stereo).map_err(open_failed)
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

/// Returns the sample rate of the file and its audio as interleaved stereo. A file that lasts
/// longer than `max_seconds` is refused as soon as that is clear.
fn decode(path: &str, max_seconds: Option<u32>) -> Result<(u32, Vec<f32>), Failure> {
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
                let frames = stereo.len() / CHANNELS;
                if max_seconds.is_some_and(|seconds| frames > seconds as usize * rate as usize) {
                    return Err(Failure::text(TextKey::SampleTooLong));
                }
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

    /// A second of stereo at the engine rate: quiet for the first half, loud for the second.
    fn half_quiet_half_loud() -> SoundClip {
        let half = SAMPLE_RATE as usize / 2 * CHANNELS;
        SoundClip::new([vec![0.1; half], vec![0.8; half]].concat())
    }

    #[test]
    fn a_cut_is_the_samples_between_its_start_and_its_end() {
        let clip = half_quiet_half_loud();
        let per_ms = SAMPLE_RATE as usize / 1000 * CHANNELS;

        assert_eq!(clip.range(None), 0..clip.samples().len());
        let cut = Cut {
            start_ms: 250,
            end_ms: 750,
        };
        assert_eq!(clip.range(Some(cut)), 250 * per_ms..750 * per_ms);
        let past_the_end = Cut {
            start_ms: 900,
            end_ms: 5000,
        };
        assert_eq!(
            clip.range(Some(past_the_end)),
            900 * per_ms..clip.samples().len()
        );
        assert_eq!(clip.duration_ms(), 1000);
    }

    #[test]
    fn the_waveform_of_a_part_keeps_the_scale_of_the_whole_sound() {
        let clip = half_quiet_half_loud();

        let whole = clip.waveform(0, 1000, 4);
        let quiet_half = clip.waveform(0, 500, 2);

        let close = |a: f32, b: f32| (a - b).abs() < 1e-6;
        assert!(close(whole[0], 0.125) && close(whole[3], 1.0), "{whole:?}");
        // Zoomed into the quiet half, it is still drawn quiet.
        assert!(
            quiet_half.iter().all(|&bar| close(bar, 0.125)),
            "{quiet_half:?}"
        );
        assert_eq!(clip.waveform(0, 1000, 0), Vec::<f32>::new());
        assert_eq!(clip.waveform(2000, 3000, 3), [0.0; 3]);
    }

    #[test]
    fn the_levels_measure_each_frame_of_the_sound() {
        let clip = half_quiet_half_loud();

        let levels = clip.levels(10);

        assert_eq!(levels.len(), 100);
        assert!((levels[0] - 0.1).abs() < 1e-6 && (levels[99] - 0.8).abs() < 1e-6);
    }

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

        assert_eq!(clip.duration_ms(), 500);
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
    fn speech_is_mono_at_the_engine_rate() {
        let file = wav(24_000, &[8192, 0], 12_000);

        let speech = load_speech(file.path()).unwrap();

        assert!(
            speech.len().abs_diff(24_000) <= RESAMPLE_CHUNK,
            "{}",
            speech.len()
        );
        assert!((speech[speech.len() / 2] - level(4096)).abs() < 0.01);
    }

    #[test]
    fn speech_longer_than_a_sample_can_be_is_refused() {
        let file = wav(8000, &[1000], 8000 * (MAX_SPEECH_SECONDS as usize + 1));

        let failure = load_speech(file.path()).err().unwrap();

        assert_eq!(failure, Failure::text(TextKey::SampleTooLong));
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
