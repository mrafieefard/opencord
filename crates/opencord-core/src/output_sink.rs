//! Where a player's sound goes (Milestones A6 and A7): mixed into the call,
//! through the chosen speaker, or nowhere at the pace it would play. The
//! voice message player and the media player share it.

use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

use opencord_media::audio::convert::ToDevice;
use opencord_media::audio::device::Speaker;
use opencord_media::audio::engine::AudioEngine;
use opencord_media::audio::{SAMPLE_RATE, TICK};
use opencord_media::player::AudioOut;

use crate::api::types::{AudioSettings, CoreError};

/// How far ahead of the listener a player writes: enough to ride out
/// scheduling, short enough that pause, seek and speed answer at once.
pub(crate) const AHEAD: Duration = Duration::from_millis(60);

pub(crate) fn ahead(rate: u32, channels: u16) -> usize {
    (rate as usize * usize::from(channels) * AHEAD.as_millis() as usize) / 1000
}

/// The call's one slot for a local player's sound: what [`AudioEngine`]
/// offers, and what tests stand in for.
pub(crate) trait CallMix: Send + Sync {
    /// Mixes `source` into the call in place of any other; its id.
    fn attach(&self, source: rtrb::Consumer<f32>) -> u64;
    /// Detaches `id`, unless another source took its place.
    fn detach(&self, id: u64);
    fn holds(&self, id: u64) -> bool;
}

impl CallMix for AudioEngine {
    fn attach(&self, source: rtrb::Consumer<f32>) -> u64 {
        self.attach_local_media(source)
    }

    fn detach(&self, id: u64) {
        self.detach_local_media(id);
    }

    fn holds(&self, id: u64) -> bool {
        self.has_local_media(id)
    }
}

pub(crate) struct OutputSink {
    route: Route,
    settings: AudioSettings,
    open_devices: bool,
}

enum Route {
    /// Mixed into the call's playback, which the echo canceller hears too
    /// (interleaved stereo, 48 kHz). Attached on the first write, and again
    /// whenever another player took the call's one slot meanwhile; held
    /// weakly, so leaving the call still closes its devices.
    Call {
        engine: Weak<dyn CallMix>,
        attached: Option<(u64, rtrb::Producer<f32>)>,
    },
    Device {
        // Boxed: WASAPI's speaker is far larger than the other routes.
        speaker: Box<Speaker>,
        to_device: Box<ToDevice>,
        volume: f32,
        scratch: Vec<f32>,
        /// Less than a tick, waiting for the rest of it.
        pending: Vec<f32>,
    },
    /// No device (tests, or devices turned off): time passes as if played.
    Null {
        start: Option<Instant>,
        frames_written: usize,
    },
}

impl OutputSink {
    /// The call when there is one, else the chosen speaker.
    pub fn open(
        engine: Option<Arc<dyn CallMix>>,
        settings: &AudioSettings,
        open_devices: bool,
    ) -> Result<Self, CoreError> {
        let route = match engine {
            Some(engine) => Route::Call {
                engine: Arc::downgrade(&engine),
                attached: None,
            },
            None => Route::outside_calls(settings, open_devices)?,
        };
        Ok(Self {
            route,
            settings: settings.clone(),
            open_devices,
        })
    }

    /// Interleaved 48 kHz frames queued and not yet played.
    pub fn queued_frames(&self) -> usize {
        match &self.route {
            Route::Call { engine, attached } => match attached {
                Some((id, producer)) if still_attached(engine, *id) => {
                    (producer.buffer().capacity() - producer.slots()) / 2
                }
                // Detached: the next write starts an empty buffer.
                _ => 0,
            },
            Route::Device {
                speaker,
                to_device,
                pending,
                ..
            } => {
                let at_device = speaker.queued() as u64 * u64::from(SAMPLE_RATE)
                    / (u64::from(speaker.rate) * u64::from(speaker.channels));
                pending.len() / 2 + to_device.buffered() + at_device as usize
            }
            Route::Null {
                start,
                frames_written,
            } => {
                let elapsed = start.map_or(Duration::ZERO, |s| s.elapsed());
                let played = (elapsed.as_secs_f64() * f64::from(SAMPLE_RATE)) as usize;
                frames_written.saturating_sub(played)
            }
        }
    }

    /// Whether one more tick fits without running more than [`AHEAD`] in
    /// front of the listener. While it doesn't, the player waits; nothing
    /// is ever dropped.
    pub fn has_room(&self) -> bool {
        match &self.route {
            Route::Device { speaker, .. } => {
                let tick = (speaker.rate as usize / 100 + 1) * usize::from(speaker.channels);
                speaker.queued() + tick <= ahead(speaker.rate, speaker.channels)
                    && speaker.samples.slots() >= tick
            }
            Route::Call { .. } | Route::Null { .. } => {
                2 * (self.queued_frames() + TICK) <= ahead(SAMPLE_RATE, 2)
            }
        }
    }

    /// Writes interleaved stereo at 48 kHz; returns the samples taken.
    pub fn write_stereo(&mut self, stereo: &[f32]) -> usize {
        self.follow_call();
        match &mut self.route {
            Route::Call {
                attached: Some((_, producer)),
                ..
            } => {
                let count = stereo.len().min(producer.slots() / 2 * 2);
                match producer.write_chunk_uninit(count) {
                    Ok(chunk) => chunk.fill_from_iter(stereo[..count].iter().copied()),
                    Err(_) => 0,
                }
            }
            Route::Call { attached: None, .. } => 0,
            Route::Device {
                speaker,
                to_device,
                volume,
                scratch,
                pending,
            } => {
                pending.extend_from_slice(stereo);
                let channels = usize::from(speaker.channels);
                let mut tick = [0.0f32; 2 * TICK];
                while pending.len() >= 2 * TICK {
                    tick.copy_from_slice(&pending[..2 * TICK]);
                    pending.drain(..2 * TICK);
                    to_device.push_tick(&tick);
                    scratch.resize(to_device.buffered() * channels, 0.0);
                    let samples = to_device.pop(scratch) * channels;
                    for sample in &mut scratch[..samples] {
                        *sample *= *volume;
                    }
                    if let Ok(chunk) = speaker.samples.write_chunk_uninit(samples) {
                        chunk.fill_from_iter(scratch[..samples].iter().copied());
                    }
                }
                stereo.len()
            }
            Route::Null {
                start,
                frames_written,
            } => {
                start.get_or_insert_with(Instant::now);
                *frames_written += stereo.len() / 2;
                stereo.len()
            }
        }
    }

    /// Writes up to one tick of mono samples; a short last tick is padded
    /// with silence.
    pub fn write(&mut self, mono: &[f32]) {
        let mut stereo = [0.0f32; 2 * TICK];
        for (frame, &sample) in stereo.as_chunks_mut::<2>().0.iter_mut().zip(mono) {
            *frame = [sample, sample];
        }
        self.write_stereo(&stereo);
    }

    /// Before writing into a call: attaches when this player doesn't hold
    /// the call's slot (first write, or another player took it), and moves
    /// to the speaker once the call is over.
    fn follow_call(&mut self) {
        let Route::Call { engine, attached } = &mut self.route else {
            return;
        };
        let Some(call) = engine.upgrade() else {
            self.route =
                Route::outside_calls(&self.settings, self.open_devices).unwrap_or_else(|error| {
                    tracing::warn!(%error, "the call ended and the speakers couldn't be opened");
                    Route::Null {
                        start: None,
                        frames_written: 0,
                    }
                });
            return;
        };
        if attached.as_ref().is_some_and(|(id, _)| call.holds(*id)) {
            return;
        }
        let (producer, consumer) = rtrb::RingBuffer::new(2 * ahead(SAMPLE_RATE, 2));
        *attached = Some((call.attach(consumer), producer));
    }
}

impl Route {
    fn outside_calls(settings: &AudioSettings, open_devices: bool) -> Result<Self, CoreError> {
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
            speaker: Box::new(speaker),
            to_device: Box::new(to_device),
            volume: settings.output_volume,
            scratch: Vec::new(),
            pending: Vec::new(),
        })
    }
}

fn still_attached(engine: &Weak<dyn CallMix>, id: u64) -> bool {
    engine.upgrade().is_some_and(|call| call.holds(id))
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
        if let Route::Call {
            engine,
            attached: Some((id, _)),
        } = &self.route
            && let Some(call) = engine.upgrade()
        {
            call.detach(*id);
        }
    }
}
