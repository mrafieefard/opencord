//! Where a player's sound goes (Milestone A6/A7): mixed into an active call,
//! through the chosen speaker, or a null sink that paces time in tests.

use std::sync::Arc;
use std::time::{Duration, Instant};

use opencord_media::audio::convert::ToDevice;
use opencord_media::audio::device::Speaker;
use opencord_media::audio::engine::AudioEngine;
use opencord_media::audio::{SAMPLE_RATE, TICK};
use opencord_media::player::AudioOut;

use crate::api::types::{AudioSettings, CoreError};

pub(crate) const AHEAD: Duration = Duration::from_millis(60);

pub(crate) fn ahead(rate: u32, channels: u16) -> usize {
    (rate as usize * usize::from(channels) * AHEAD.as_millis() as usize) / 1000
}

/// Where the player's sound goes.
pub(crate) enum OutputSink {
    /// Mixed into the call's playback, which the echo canceller hears too
    /// (interleaved stereo, 48 kHz). Dropping it detaches it from the call.
    Call {
        producer: rtrb::Producer<f32>,
        engine: Option<Arc<AudioEngine>>,
    },
    Device {
        speaker: Speaker,
        to_device: Box<ToDevice>,
        volume: f32,
        scratch: Vec<f32>,
        pending: Vec<f32>,
    },
    /// No device (tests, or devices turned off): time passes as if played.
    Null {
        start: Option<Instant>,
        frames_written: usize,
    },
}

impl OutputSink {
    /// The output for the player thread, opened on it (cpal streams aren't
    /// `Send`): the call when there is one, else the chosen speaker.
    pub fn open(
        engine: Option<Arc<AudioEngine>>,
        settings: &AudioSettings,
        open_devices: bool,
    ) -> Result<Self, CoreError> {
        if let Some(engine) = engine {
            let (producer, consumer) = rtrb::RingBuffer::new(2 * ahead(SAMPLE_RATE, 2));
            engine.set_local_media(Some(consumer));
            return Ok(Self::Call {
                producer,
                engine: Some(engine),
            });
        }
        if !open_devices {
            return Ok(Self::Null {
                start: None,
                frames_written: 0,
            });
        }
        let unavailable = |error: &dyn std::fmt::Display| CoreError::InvalidInput {
            message: format!("The speakers couldn't be opened: {error}"),
        };
        let speaker =
            Speaker::open(settings.output_device.as_deref()).map_err(|e| unavailable(&e))?;
        let to_device =
            ToDevice::new(speaker.rate, speaker.channels).map_err(|e| unavailable(&e))?;
        Ok(Self::Device {
            speaker,
            to_device: Box::new(to_device),
            volume: settings.output_volume,
            scratch: Vec::new(),
            pending: Vec::new(),
        })
    }

    /// Interleaved 48 kHz frames queued and not yet played.
    pub fn queued_frames(&self) -> usize {
        match self {
            Self::Call { producer, .. } => {
                let queued_samples = producer.buffer().capacity() - producer.slots();
                queued_samples / 2
            }
            Self::Device {
                speaker,
                to_device,
                pending,
                ..
            } => {
                let dev_buffered = pending.len() / 2 + to_device.buffered();
                let spk_samples = speaker.queued();
                let spk_frames = (spk_samples as u64 * 48000
                    / (speaker.rate as u64 * usize::from(speaker.channels) as u64))
                    as usize;
                dev_buffered + spk_frames
            }
            Self::Null {
                start,
                frames_written,
            } => {
                let elapsed = start.as_ref().map_or(Duration::ZERO, |s| s.elapsed());
                let consumed_frames = (elapsed.as_secs_f64() * 48000.0) as usize;
                frames_written.saturating_sub(consumed_frames)
            }
        }
    }

    /// Whether one more tick fits without running more than [`AHEAD`] in
    /// front of the listener. While it doesn't, the player waits; nothing
    /// is ever dropped.
    pub fn has_room(&self) -> bool {
        match self {
            Self::Call { producer, .. } => {
                let queued = producer.buffer().capacity() - producer.slots();
                queued + 2 * TICK <= ahead(SAMPLE_RATE, 2)
            }
            Self::Device { speaker, .. } => {
                let tick = (speaker.rate as usize / 100 + 1) * usize::from(speaker.channels);
                speaker.queued() + tick <= ahead(speaker.rate, speaker.channels)
                    && speaker.samples.slots() >= tick
            }
            Self::Null { .. } => self.queued_frames() + TICK <= 2880,
        }
    }

    /// Writes stereo 48 kHz samples; returns the number of samples accepted.
    pub fn write_stereo(&mut self, stereo_48k: &[f32]) -> usize {
        match self {
            Self::Call { producer, .. } => {
                let slots = producer.slots() / 2 * 2;
                let count = stereo_48k.len().min(slots);
                if count > 0 {
                    if let Ok(chunk) = producer.write_chunk_uninit(count) {
                        chunk.fill_from_iter(stereo_48k[..count].iter().copied());
                        count
                    } else {
                        0
                    }
                } else {
                    0
                }
            }
            Self::Device {
                speaker,
                to_device,
                volume,
                scratch,
                pending,
            } => {
                pending.extend_from_slice(stereo_48k);
                let mut tick = [0.0f32; 2 * TICK];
                while pending.len() >= 2 * TICK {
                    tick.copy_from_slice(&pending[..2 * TICK]);
                    pending.drain(..2 * TICK);
                    to_device.push_tick(&tick);
                    let channels = usize::from(speaker.channels);
                    scratch.resize(to_device.buffered() * channels, 0.0);
                    let samples = to_device.pop(scratch) * channels;
                    for sample in &mut scratch[..samples] {
                        *sample *= *volume;
                    }
                    if let Ok(chunk) = speaker.samples.write_chunk_uninit(samples) {
                        chunk.fill_from_iter(scratch[..samples].iter().copied());
                    }
                }
                stereo_48k.len()
            }
            Self::Null {
                start,
                frames_written,
            } => {
                if start.is_none() {
                    *start = Some(Instant::now());
                }
                let frames = stereo_48k.len() / 2;
                *frames_written += frames;
                std::thread::sleep(Duration::from_millis(10));
                stereo_48k.len()
            }
        }
    }

    /// Writes up to one tick of mono samples; a short last tick is padded
    /// with silence.
    pub fn write_mono(&mut self, mono: &[f32]) {
        let mut stereo = [0.0f32; 2 * TICK];
        for (frame, &sample) in stereo.as_chunks_mut::<2>().0.iter_mut().zip(mono) {
            *frame = [sample, sample];
        }
        self.write_stereo(&stereo);
    }

    /// Convenience for voice player: writes up to one tick of mono samples.
    pub fn write(&mut self, mono: &[f32]) {
        self.write_mono(mono);
    }
}

impl AudioOut for OutputSink {
    fn write(&mut self, stereo_48k: &[f32]) -> usize {
        self.write_stereo(stereo_48k)
    }

    fn queued_frames(&self) -> usize {
        self.queued_frames()
    }
}

impl Drop for OutputSink {
    fn drop(&mut self) {
        if let Self::Call {
            engine: Some(engine),
            ..
        } = self
        {
            engine.set_local_media(None);
        }
    }
}
