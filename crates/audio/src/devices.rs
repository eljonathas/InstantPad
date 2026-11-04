//! Everything that talks to cpal: listing devices and opening streams on them.

use std::fmt::Display;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    BufferSize, Device, DeviceId, ErrorKind, FromSample, I24, SampleFormat, Stream, StreamConfig,
    SupportedBufferSize,
};
use instantpad_application::message::{Failure, TextKey};
use instantpad_application::ports::{AudioDevice, StreamStop};
use rtrb::Producer;

use crate::ai::AiStage;
use crate::capture::CaptureProcessor;
use crate::controls::AtomicF32;
use crate::format::{CHANNELS, SAMPLE_RATE};
use crate::mixer::Mixer;
use crate::playback::{Conversion, Playback};
use crate::recording::RecordTap;
use crate::voice::VoiceControls;

const OUTPUT_BUFFER_MS: u32 = 30;
const CAPTURE_BUFFER_MS: u32 = 20;

pub(crate) fn microphones() -> Vec<AudioDevice> {
    describe(cpal::default_host().input_devices())
}

/// The output devices, also for the installer, which only needs to know whether the cable is one.
pub fn outputs() -> Vec<AudioDevice> {
    describe(cpal::default_host().output_devices())
}

fn describe(devices: Result<impl Iterator<Item = Device>, cpal::Error>) -> Vec<AudioDevice> {
    let mut described: Vec<_> = devices
        .into_iter()
        .flatten()
        .filter_map(|device| {
            Some(AudioDevice {
                id: device.id().ok()?.to_string(),
                name: device.description().ok()?.name().to_owned(),
            })
        })
        .collect();
    described.sort_by(|a, b| a.name.cmp(&b.name));
    described
}

fn unavailable(detail: impl Display) -> Failure {
    Failure::because(TextKey::DeviceUnavailable, detail)
}

fn find(id: &str) -> Result<Device, Failure> {
    let id: DeviceId = id.parse().map_err(unavailable)?;
    cpal::host_from_id(id.host())
        .map_err(unavailable)?
        .device_by_id(&id)
        .ok_or_else(|| unavailable("the device is not there"))
}

/// Asks for the buffer size of a target latency. WASAPI takes any size in shared mode, and the
/// other hosts refuse one outside their range.
fn buffer_for(milliseconds: u32, rate: u32, supported: &SupportedBufferSize) -> BufferSize {
    let frames = rate * milliseconds / 1000;
    match supported {
        _ if cfg!(windows) => BufferSize::Fixed(frames),
        SupportedBufferSize::Range { min, max } => BufferSize::Fixed(frames.clamp(*min, *max)),
        SupportedBufferSize::Unknown => BufferSize::Default,
    }
}

const RUNNING: u8 = 0;
const FAILED: u8 = 1;
const LOST: u8 = 2;

/// How a stream stopped, as its error callback saw it, for the app to read away from the audio
/// thread. Each stream has its own, so one that was replaced cannot speak for the new one.
#[derive(Default)]
pub(crate) struct StreamHealth(AtomicU8);

impl StreamHealth {
    /// What stopped the stream since the last call, told once.
    pub(crate) fn take_stop(&self) -> Option<StreamStop> {
        match self.0.swap(RUNNING, Ordering::Relaxed) {
            LOST => Some(StreamStop::Lost),
            FAILED => Some(StreamStop::Failed),
            _ => None,
        }
    }
}

/// The error callback of a stream. It runs on the audio thread, so it only marks what happened,
/// and the app opens the device again from its own thread.
pub(crate) fn watch(health: Arc<StreamHealth>) -> impl FnMut(cpal::Error) + Send + 'static {
    move |error| {
        let stop = match error.kind() {
            // A glitch, or a stream that goes on by itself.
            ErrorKind::Xrun | ErrorKind::RealtimeDenied | ErrorKind::DeviceChanged => return,
            // WASAPI reports AUDCLNT_E_DEVICE_INVALIDATED, which a device that is unplugged or
            // reconfigured gives, as `DeviceNotAvailable`, and AUDCLNT_E_RESOURCES_INVALIDATED as
            // `StreamInvalidated`. A busy device or a restarted audio service can come back too.
            ErrorKind::DeviceNotAvailable
            | ErrorKind::StreamInvalidated
            | ErrorKind::HostUnavailable
            | ErrorKind::DeviceBusy => LOST,
            _ => FAILED,
        };
        // A loss wins over a failure that came with it, so the device is still opened again.
        health.0.fetch_max(stop, Ordering::Relaxed);
    }
}

pub(crate) fn open_output(
    id: &str,
    mixer: Mixer,
    health: Arc<StreamHealth>,
) -> Result<Stream, Failure> {
    let device = find(id)?;
    let native = device.default_output_config().map_err(unavailable)?;

    // WASAPI converts the rate and the channels itself, so it is always asked for the engine
    // format. The other hosts get it when the device already runs like that, and otherwise the
    // engine adapts.
    let engine_format = cfg!(windows)
        || (usize::from(native.channels()) == CHANNELS && native.sample_rate() == SAMPLE_RATE);
    let (channels, rate, conversion) = if engine_format {
        (CHANNELS as u16, SAMPLE_RATE, None)
    } else {
        let conversion =
            Conversion::new(native.channels().into(), native.sample_rate()).map_err(unavailable)?;
        (native.channels(), native.sample_rate(), Some(conversion))
    };
    let config = StreamConfig {
        channels,
        sample_rate: rate,
        buffer_size: buffer_for(OUTPUT_BUFFER_MS, rate, native.buffer_size()),
    };

    let mut playback = Playback::new(mixer, conversion);
    let stream = device
        .build_output_stream(
            config,
            move |block: &mut [f32], _| playback.fill(block),
            watch(health),
            None,
        )
        .map_err(unavailable)?;
    stream.play().map_err(unavailable)?;
    Ok(stream)
}

pub(crate) fn open_capture(
    id: &str,
    rings: [Producer<f32>; 2],
    level: Arc<AtomicF32>,
    voice: Arc<VoiceControls>,
    tap: RecordTap,
    ai: AiStage,
    health: Arc<StreamHealth>,
) -> Result<Stream, Failure> {
    let device = find(id)?;
    // Capture only takes the formats the device has, so they are converted here.
    let native = device.default_input_config().map_err(unavailable)?;
    let config = StreamConfig {
        channels: native.channels(),
        sample_rate: native.sample_rate(),
        buffer_size: buffer_for(
            CAPTURE_BUFFER_MS,
            native.sample_rate(),
            native.buffer_size(),
        ),
    };
    let processor = CaptureProcessor::new(
        native.channels().into(),
        native.sample_rate(),
        rings,
        level,
        voice,
        tap,
        ai,
    )
    .map_err(unavailable)?;
    let on_error = watch(health);

    let stream = match native.sample_format() {
        SampleFormat::F32 => capture_stream::<f32>(&device, config, processor, on_error),
        SampleFormat::I16 => capture_stream::<i16>(&device, config, processor, on_error),
        SampleFormat::I24 => capture_stream::<I24>(&device, config, processor, on_error),
        SampleFormat::I32 => capture_stream::<i32>(&device, config, processor, on_error),
        other => {
            return Err(unavailable(format!(
                "the sample format {other} is not supported"
            )));
        }
    }
    .map_err(unavailable)?;
    stream.play().map_err(unavailable)?;
    Ok(stream)
}

fn capture_stream<T>(
    device: &Device,
    config: StreamConfig,
    mut processor: CaptureProcessor,
    on_error: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<Stream, cpal::Error>
where
    T: cpal::SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    device.build_input_stream(
        config,
        move |data: &[T], _| processor.push(data),
        on_error,
        None,
    )
}

#[cfg(test)]
mod tests {
    use instantpad_application::message::Message;

    use super::*;

    /// How the stream ends up after its callback saw `errors`.
    fn after(errors: &[ErrorKind]) -> Option<StreamStop> {
        let health = Arc::new(StreamHealth::default());
        let mut on_error = watch(health.clone());
        for &kind in errors {
            on_error(cpal::Error::new(kind));
        }
        health.take_stop()
    }

    #[test]
    fn a_device_that_went_away_or_was_reconfigured_is_lost() {
        // What WASAPI gives for AUDCLNT_E_DEVICE_INVALIDATED, 0x88890004.
        let invalidated = cpal::Error::with_message(
            ErrorKind::DeviceNotAvailable,
            "OS Error -2004287484 (FormatMessageW() returned error 317) (os error -2004287484)",
        );
        let health = Arc::new(StreamHealth::default());

        watch(health.clone())(invalidated);

        assert_eq!(health.take_stop(), Some(StreamStop::Lost));
        assert_eq!(
            after(&[ErrorKind::StreamInvalidated]),
            Some(StreamStop::Lost)
        );
        assert_eq!(after(&[ErrorKind::HostUnavailable]), Some(StreamStop::Lost));
    }

    #[test]
    fn a_stop_is_told_once() {
        let health = Arc::new(StreamHealth::default());
        watch(health.clone())(cpal::Error::new(ErrorKind::DeviceNotAvailable));

        assert_eq!(health.take_stop(), Some(StreamStop::Lost));
        assert_eq!(health.take_stop(), None);
    }

    #[test]
    fn glitches_and_reroutes_leave_the_stream_running() {
        assert_eq!(
            after(&[
                ErrorKind::Xrun,
                ErrorKind::RealtimeDenied,
                ErrorKind::DeviceChanged
            ]),
            None
        );
    }

    #[test]
    fn any_other_error_stops_the_stream_for_good() {
        assert_eq!(after(&[ErrorKind::BackendError]), Some(StreamStop::Failed));
        assert_eq!(
            after(&[ErrorKind::PermissionDenied]),
            Some(StreamStop::Failed)
        );
    }

    #[test]
    fn a_loss_wins_over_a_failure_that_came_with_it() {
        assert_eq!(
            after(&[ErrorKind::BackendError, ErrorKind::DeviceNotAvailable]),
            Some(StreamStop::Lost)
        );
        assert_eq!(
            after(&[ErrorKind::DeviceNotAvailable, ErrorKind::BackendError]),
            Some(StreamStop::Lost)
        );
    }

    #[test]
    fn a_device_id_that_is_not_one_is_unavailable() {
        let failure = find("not a device id").err().unwrap();

        assert!(matches!(
            failure.0,
            Message::Text {
                key: TextKey::DeviceUnavailable,
                ..
            }
        ));
    }

    #[test]
    fn a_device_id_of_a_host_that_does_not_exist_is_unavailable() {
        let failure = find("nohost:device").err().unwrap();

        assert!(matches!(
            failure.0,
            Message::Text {
                key: TextKey::DeviceUnavailable,
                ..
            }
        ));
    }
}
