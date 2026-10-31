//! Everything that talks to cpal: listing devices and opening streams on them.

use std::fmt::Display;
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    BufferSize, Device, DeviceId, ErrorKind, FromSample, I24, SampleFormat, Stream, StreamConfig,
    SupportedBufferSize,
};
use instantpad_application::events::AppEvent;
use instantpad_application::message::{Failure, Message, TextKey};
use instantpad_application::ports::{AudioDevice, EventSink};
use rtrb::Producer;

use crate::capture::CaptureProcessor;
use crate::controls::AtomicF32;
use crate::format::{CHANNELS, SAMPLE_RATE};
use crate::mixer::Mixer;
use crate::playback::{Conversion, Playback};
use crate::voice::VoiceControls;

const OUTPUT_BUFFER_MS: u32 = 30;
const CAPTURE_BUFFER_MS: u32 = 20;

pub(crate) fn microphones() -> Vec<AudioDevice> {
    describe(cpal::default_host().input_devices())
}

pub(crate) fn outputs() -> Vec<AudioDevice> {
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

/// Reports a stream that stopped while running. Xruns are glitches, not failures.
fn report(events: Arc<dyn EventSink>, key: TextKey) -> impl FnMut(cpal::Error) + Send + 'static {
    move |error| {
        if error.kind() != ErrorKind::Xrun {
            events.emit(AppEvent::error(Message::with(
                key,
                [Message::plain(error.to_string())],
            )));
        }
    }
}

pub(crate) fn open_output(
    id: &str,
    mixer: Mixer,
    events: Arc<dyn EventSink>,
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
            report(events, TextKey::OutputStopped),
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
    events: Arc<dyn EventSink>,
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
    )
    .map_err(unavailable)?;
    let on_error = report(events, TextKey::MicStopped);

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
    use instantpad_test_support::fakes::RecordingEvents;

    use super::*;

    #[test]
    fn a_stream_error_reaches_the_user() {
        let events = Arc::new(RecordingEvents::default());
        let mut on_error = report(events.clone(), TextKey::OutputStopped);

        on_error(cpal::Error::with_message(
            ErrorKind::DeviceNotAvailable,
            "unplugged",
        ));

        assert_eq!(
            *events.0.lock().unwrap(),
            [AppEvent::error(Message::with(
                TextKey::OutputStopped,
                [Message::plain("unplugged")]
            ))]
        );
    }

    #[test]
    fn xruns_are_ignored() {
        let events = Arc::new(RecordingEvents::default());
        let mut on_error = report(events.clone(), TextKey::MicStopped);

        on_error(cpal::Error::new(ErrorKind::Xrun));

        assert!(events.0.lock().unwrap().is_empty());
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
