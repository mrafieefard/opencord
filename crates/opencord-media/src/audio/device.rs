//! Microphones and speakers through cpal (plan §7.6): WASAPI on Windows,
//! Core Audio on macOS, PipeWire on Linux with ALSA as the fallback. The
//! device callbacks run on the system's real-time audio threads and only
//! move samples to or from lock-free rings: they never allocate, lock, log
//! or block. A device that goes away is flagged for the engine, which
//! falls back to the default one.

use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{
    BufferSize, Device, ErrorKind, FromSample, InputCallbackInfo, OutputCallbackInfo, Sample,
    SampleFormat, SizedSample, StreamConfig, SupportedBufferSize, SupportedStreamConfig,
};
use rtrb::{Consumer, Producer, RingBuffer};

use super::SAMPLE_RATE;

/// How much audio the rings between a device and processing hold.
const RING_MS: u32 = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    /// Stable across runs where the system allows; what settings store.
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceList {
    pub inputs: Vec<DeviceInfo>,
    pub outputs: Vec<DeviceInfo>,
    pub default_input: Option<String>,
    pub default_output: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum DeviceError {
    #[error("there is no {0}")]
    NoDevice(&'static str),
    #[error("the {kind} {id:?} is not connected")]
    NotFound { kind: &'static str, id: String },
    #[error("the {kind} uses sample format {format}, which is not supported")]
    Format {
        kind: &'static str,
        format: SampleFormat,
    },
    #[error("could not open the {kind}: {source}")]
    Open {
        kind: &'static str,
        source: cpal::Error,
    },
}

/// Microphones and speakers the system offers now.
pub fn devices() -> DeviceList {
    let host = cpal::default_host();
    let id_of = |device: &Device| device.id().ok().map(|id| id.to_string());
    let info = |device: &Device| {
        let id = id_of(device)?;
        let name = device
            .description()
            .map(|description| description.name().to_owned())
            .unwrap_or_else(|_| id.clone());
        Some(DeviceInfo { id, name })
    };
    let mut list = DeviceList {
        default_input: host.default_input_device().as_ref().and_then(id_of),
        default_output: host.default_output_device().as_ref().and_then(id_of),
        ..DeviceList::default()
    };
    if let Ok(devices) = host.devices() {
        for device in devices {
            let Some(found) = info(&device) else {
                continue;
            };
            if device.supports_input() {
                list.inputs.push(found.clone());
            }
            if device.supports_output() {
                list.outputs.push(found);
            }
        }
    }
    list
}

/// What the callbacks tell the engine.
#[derive(Debug, Default)]
pub struct Health {
    lost: AtomicBool,
    /// Callbacks that found the ring full (microphone) or short (speaker).
    glitches: AtomicU64,
}

impl Health {
    /// The device went away; the engine opens another.
    pub fn is_lost(&self) -> bool {
        self.lost.load(Ordering::Relaxed)
    }

    pub fn glitches(&self) -> u64 {
        self.glitches.load(Ordering::Relaxed)
    }

    fn report(&self, error: &cpal::Error) {
        if matches!(
            error.kind(),
            ErrorKind::DeviceNotAvailable
                | ErrorKind::StreamInvalidated
                | ErrorKind::HostUnavailable
        ) {
            self.lost.store(true, Ordering::Relaxed);
        }
    }
}

/// An open microphone; its samples, interleaved, arrive in `samples`.
pub struct Microphone {
    _stream: cpal::Stream,
    pub samples: Consumer<f32>,
    pub rate: u32,
    pub channels: u16,
    pub name: String,
    pub health: Arc<Health>,
}

/// An open speaker; it plays what is written to `samples`, interleaved.
pub struct Speaker {
    _stream: cpal::Stream,
    pub samples: Producer<f32>,
    pub rate: u32,
    pub channels: u16,
    pub name: String,
    pub health: Arc<Health>,
}

impl Microphone {
    /// The microphone with `id`, or the system's default.
    pub fn open(id: Option<&str>) -> Result<Self, DeviceError> {
        const KIND: &str = "microphone";
        let host = cpal::default_host();
        let device = find(&host, id, KIND, |host| host.default_input_device())?;
        let open = |source| DeviceError::Open { kind: KIND, source };
        let supported = choose(
            device.default_input_config().map_err(open)?,
            device.supported_input_configs().map_err(open)?,
        );
        let config = stream_config(&supported);
        let (producer, samples) = RingBuffer::new(ring_capacity(&config));
        let health = Arc::new(Health::default());
        let stream = match supported.sample_format() {
            SampleFormat::F32 => build_input::<f32>(&device, config, producer, &health),
            SampleFormat::I16 => build_input::<i16>(&device, config, producer, &health),
            SampleFormat::U16 => build_input::<u16>(&device, config, producer, &health),
            SampleFormat::I32 => build_input::<i32>(&device, config, producer, &health),
            SampleFormat::U32 => build_input::<u32>(&device, config, producer, &health),
            SampleFormat::F64 => build_input::<f64>(&device, config, producer, &health),
            SampleFormat::I8 => build_input::<i8>(&device, config, producer, &health),
            SampleFormat::U8 => build_input::<u8>(&device, config, producer, &health),
            format => return Err(DeviceError::Format { kind: KIND, format }),
        }
        .map_err(open)?;
        stream.play().map_err(open)?;
        Ok(Self {
            _stream: stream,
            samples,
            rate: config.sample_rate,
            channels: config.channels,
            name: name_of(&device),
            health,
        })
    }
}

impl Speaker {
    /// The speaker with `id`, or the system's default.
    pub fn open(id: Option<&str>) -> Result<Self, DeviceError> {
        const KIND: &str = "speaker";
        let host = cpal::default_host();
        let device = find(&host, id, KIND, |host| host.default_output_device())?;
        let open = |source| DeviceError::Open { kind: KIND, source };
        let supported = choose(
            device.default_output_config().map_err(open)?,
            device.supported_output_configs().map_err(open)?,
        );
        let config = stream_config(&supported);
        let (samples, consumer) = RingBuffer::new(ring_capacity(&config));
        let health = Arc::new(Health::default());
        let stream = match supported.sample_format() {
            SampleFormat::F32 => build_output::<f32>(&device, config, consumer, &health),
            SampleFormat::I16 => build_output::<i16>(&device, config, consumer, &health),
            SampleFormat::U16 => build_output::<u16>(&device, config, consumer, &health),
            SampleFormat::I32 => build_output::<i32>(&device, config, consumer, &health),
            SampleFormat::U32 => build_output::<u32>(&device, config, consumer, &health),
            SampleFormat::F64 => build_output::<f64>(&device, config, consumer, &health),
            SampleFormat::I8 => build_output::<i8>(&device, config, consumer, &health),
            SampleFormat::U8 => build_output::<u8>(&device, config, consumer, &health),
            format => return Err(DeviceError::Format { kind: KIND, format }),
        }
        .map_err(open)?;
        stream.play().map_err(open)?;
        Ok(Self {
            _stream: stream,
            samples,
            rate: config.sample_rate,
            channels: config.channels,
            name: name_of(&device),
            health,
        })
    }

    /// Interleaved samples queued and not played yet.
    pub fn queued(&self) -> usize {
        self.samples.buffer().capacity() - self.samples.slots()
    }
}

fn find(
    host: &cpal::Host,
    id: Option<&str>,
    kind: &'static str,
    default: impl Fn(&cpal::Host) -> Option<Device>,
) -> Result<Device, DeviceError> {
    match id {
        None => default(host).ok_or(DeviceError::NoDevice(kind)),
        Some(id) => cpal::DeviceId::from_str(id)
            .ok()
            .and_then(|parsed| host.device_by_id(&parsed))
            .ok_or_else(|| DeviceError::NotFound {
                kind,
                id: id.to_owned(),
            }),
    }
}

fn name_of(device: &Device) -> String {
    device
        .description()
        .map(|description| description.name().to_owned())
        .unwrap_or_default()
}

/// The device's default configuration, at 48 kHz if it can do that, which
/// saves resampling.
fn choose(
    default: SupportedStreamConfig,
    supported: impl Iterator<Item = cpal::SupportedStreamConfigRange>,
) -> SupportedStreamConfig {
    if default.sample_rate() == SAMPLE_RATE {
        return default;
    }
    supported
        .filter(|range| {
            range.channels() == default.channels()
                && range.sample_format() == default.sample_format()
        })
        .find_map(|range| range.try_with_sample_rate(SAMPLE_RATE))
        .unwrap_or(default)
}

/// 10 ms device buffers where the device allows it, for low latency.
fn stream_config(supported: &SupportedStreamConfig) -> StreamConfig {
    let ten_ms = supported.sample_rate() / 100;
    let buffer_size = match supported.buffer_size() {
        SupportedBufferSize::Range { min, max } if (*min..=*max).contains(&ten_ms) => {
            BufferSize::Fixed(ten_ms)
        }
        _ => BufferSize::Default,
    };
    StreamConfig {
        channels: supported.channels(),
        sample_rate: supported.sample_rate(),
        buffer_size,
    }
}

fn ring_capacity(config: &StreamConfig) -> usize {
    (config.sample_rate * RING_MS / 1000) as usize * usize::from(config.channels.max(1))
}

fn build_input<T>(
    device: &Device,
    config: StreamConfig,
    mut ring: Producer<f32>,
    health: &Arc<Health>,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let data_health = Arc::clone(health);
    let error_health = Arc::clone(health);
    device.build_input_stream(
        config,
        move |data: &[T], _: &InputCallbackInfo| {
            let room = ring.slots().min(data.len());
            if room < data.len() {
                data_health.glitches.fetch_add(1, Ordering::Relaxed);
            }
            if let Ok(chunk) = ring.write_chunk_uninit(room) {
                chunk.fill_from_iter(data[..room].iter().map(|sample| f32::from_sample(*sample)));
            }
        },
        move |error| error_health.report(&error),
        None,
    )
}

fn build_output<T>(
    device: &Device,
    config: StreamConfig,
    mut ring: Consumer<f32>,
    health: &Arc<Health>,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample + FromSample<f32>,
{
    let data_health = Arc::clone(health);
    let error_health = Arc::clone(health);
    device.build_output_stream(
        config,
        move |data: &mut [T], _: &OutputCallbackInfo| {
            let ready = ring.slots().min(data.len());
            if let Ok(chunk) = ring.read_chunk(ready) {
                for (out, sample) in data.iter_mut().zip(chunk) {
                    *out = T::from_sample(sample);
                }
            }
            if ready < data.len() {
                data_health.glitches.fetch_add(1, Ordering::Relaxed);
                for out in &mut data[ready..] {
                    *out = T::EQUILIBRIUM;
                }
            }
        },
        move |error| error_health.report(&error),
        None,
    )
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    /// Needs a microphone and a speaker; run with `--ignored` on a
    /// machine that has them.
    #[test]
    #[ignore = "needs audio devices"]
    fn the_default_devices_open_and_move_samples() {
        let list = devices();
        let mut microphone = Microphone::open(None).unwrap();
        let mut speaker = Speaker::open(None).unwrap();

        let mut captured = 0;
        for _ in 0..50 {
            std::thread::sleep(Duration::from_millis(10));
            let ready = microphone.samples.slots();
            if let Ok(chunk) = microphone.samples.read_chunk(ready) {
                captured += chunk.into_iter().count();
            }
            let room = speaker.samples.slots();
            if let Ok(chunk) = speaker.samples.write_chunk_uninit(room) {
                chunk.fill_from_iter(std::iter::repeat(0.0));
            }
        }

        assert!(
            !list.inputs.is_empty() && !list.outputs.is_empty(),
            "{list:?}"
        );
        let expected = microphone.rate as usize * usize::from(microphone.channels) / 2;
        assert!(
            captured > expected / 2,
            "{captured} samples in half a second"
        );
        assert!(!microphone.health.is_lost() && !speaker.health.is_lost());
    }
}
