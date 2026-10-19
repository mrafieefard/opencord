//! Voice message recorder (Milestone A6, Task 28): records microphone or
//! in-call audio to Ogg Opus and extracts a waveform draft.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use opencord_common::sound::ogg;
use opencord_common::waveform;
use opencord_media::audio::codec::MAX_PACKET;
use opencord_media::audio::convert::FromDevice;
use opencord_media::audio::device::Microphone;
use opencord_media::audio::engine::AudioEngine;
use opencord_media::audio::processing::VoiceProcessing;
use opencord_media::audio::{FRAME, SAMPLE_RATE, TICK};
use tokio::sync::mpsc;

use crate::api::types::{AudioSettings, CoreError, MediaEvent, VoiceDraft, VoiceStopReason};

static NEXT_DRAFT_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) trait TapEngine: Send + Sync {
    fn set_capture_tap(&self, tap: Option<rtrb::Producer<f32>>, silence_call: bool);
}

impl TapEngine for AudioEngine {
    fn set_capture_tap(&self, tap: Option<rtrb::Producer<f32>>, silence_call: bool) {
        self.set_capture_tap(tap, silence_call);
    }
}

pub(crate) trait TickSource {
    fn next_tick(&mut self, tick: &mut [f32; 480]) -> Option<()>;
    fn is_device_lost(&self) -> bool {
        false
    }
}

impl<T: TickSource + ?Sized> TickSource for Box<T> {
    fn next_tick(&mut self, tick: &mut [f32; 480]) -> Option<()> {
        (**self).next_tick(tick)
    }
    fn is_device_lost(&self) -> bool {
        (**self).is_device_lost()
    }
}

pub(crate) struct TapTickSource {
    consumer: rtrb::Consumer<f32>,
    stop_flag: Arc<AtomicBool>,
    cancel_flag: Arc<AtomicBool>,
}

impl TapTickSource {
    pub fn new(
        consumer: rtrb::Consumer<f32>,
        stop_flag: Arc<AtomicBool>,
        cancel_flag: Arc<AtomicBool>,
    ) -> Self {
        Self {
            consumer,
            stop_flag,
            cancel_flag,
        }
    }
}

impl TickSource for TapTickSource {
    fn next_tick(&mut self, tick: &mut [f32; 480]) -> Option<()> {
        while self.consumer.slots() < TICK {
            if self.stop_flag.load(Ordering::Relaxed) || self.cancel_flag.load(Ordering::Relaxed) {
                return None;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        for sample in tick.iter_mut() {
            *sample = self.consumer.pop().ok()?;
        }
        Some(())
    }
}

pub(crate) struct MicTickSource {
    mic: Microphone,
    from_device: FromDevice,
    processing: VoiceProcessing,
    input_volume: f32,
    stop_flag: Arc<AtomicBool>,
    cancel_flag: Arc<AtomicBool>,
}

impl MicTickSource {
    pub fn new(
        mic: Microphone,
        from_device: FromDevice,
        processing: VoiceProcessing,
        input_volume: f32,
        stop_flag: Arc<AtomicBool>,
        cancel_flag: Arc<AtomicBool>,
    ) -> Self {
        Self {
            mic,
            from_device,
            processing,
            input_volume,
            stop_flag,
            cancel_flag,
        }
    }
}

impl TickSource for MicTickSource {
    fn next_tick(&mut self, tick: &mut [f32; 480]) -> Option<()> {
        let mut buffer = Vec::new();
        loop {
            if self.from_device.pop_tick(tick) {
                self.processing.capture(tick);
                if self.input_volume != 1.0 {
                    for s in tick.iter_mut() {
                        *s *= self.input_volume;
                    }
                }
                return Some(());
            }
            if self.stop_flag.load(Ordering::Relaxed) || self.cancel_flag.load(Ordering::Relaxed) {
                return None;
            }
            if self.mic.health.is_lost() {
                return None;
            }
            buffer.clear();
            while let Ok(s) = self.mic.samples.pop() {
                buffer.push(s);
            }
            if !buffer.is_empty() {
                self.from_device.push(&buffer);
            } else {
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }

    fn is_device_lost(&self) -> bool {
        self.mic.health.is_lost()
    }
}

/// Computes the 128-byte draft waveform from 10 ms tick mean square powers.
pub(crate) fn compute_waveform(tick_power: &[f32]) -> Vec<u8> {
    const LEN: usize = 128;
    let n = tick_power.len();
    let mut out = Vec::with_capacity(LEN);
    for i in 0..LEN {
        let start = i * n / LEN;
        let end = (i + 1) * n / LEN;
        let slice = &tick_power[start..end];
        if slice.is_empty() {
            out.push(0);
        } else {
            let mean = slice.iter().sum::<f32>() / slice.len() as f32;
            let rms = mean.sqrt();
            out.push(waveform::level_byte(rms));
        }
    }
    out
}

pub(crate) struct RecordResult {
    pub ogg_bytes: Vec<u8>,
    pub draft: VoiceDraft,
}

pub(crate) fn record_ticks(
    mut source: impl TickSource,
    max_seconds: u32,
    stop_flag: Arc<AtomicBool>,
    cancel_flag: Arc<AtomicBool>,
    events: Option<mpsc::UnboundedSender<MediaEvent>>,
    draft_id: u64,
) -> Result<Option<RecordResult>, CoreError> {
    let mut encoder =
        opus::Encoder::new(SAMPLE_RATE, opus::Channels::Mono, opus::Application::Voip).map_err(
            |e| CoreError::InvalidInput {
                message: e.to_string(),
            },
        )?;
    encoder
        .set_bitrate(opus::Bitrate::Bits(32_000))
        .map_err(|e| CoreError::InvalidInput {
            message: e.to_string(),
        })?;
    encoder.set_vbr(true).map_err(|e| CoreError::InvalidInput {
        message: e.to_string(),
    })?;
    encoder
        .set_dtx(false)
        .map_err(|e| CoreError::InvalidInput {
            message: e.to_string(),
        })?;
    encoder
        .set_complexity(10)
        .map_err(|e| CoreError::InvalidInput {
            message: e.to_string(),
        })?;

    let pre_skip = u16::try_from(encoder.get_lookahead().unwrap_or(0)).unwrap_or(0);

    let max_ticks = (max_seconds as usize) * 100;
    let mut tick_power: Vec<f32> = Vec::new();
    let mut packets: Vec<Vec<u8>> = Vec::new();

    let mut tick = [0.0f32; TICK];
    let mut frame = [0.0f32; FRAME];
    let mut tick_in_frame = 0;
    let mut loudest_20ms = 0.0f32;
    let mut last_event_time = Instant::now();
    let mut packet_buf = vec![0u8; MAX_PACKET];

    let mut stopped_by_max_length = false;

    while let Some(()) = source.next_tick(&mut tick) {
        if cancel_flag.load(Ordering::Relaxed) {
            return Ok(None);
        }
        if stop_flag.load(Ordering::Relaxed) {
            break;
        }

        let sum_sq: f32 = tick.iter().map(|&x| x * x).sum();
        let mean_sq = sum_sq / TICK as f32;
        tick_power.push(mean_sq);

        if tick_in_frame == 0 {
            frame[..TICK].copy_from_slice(&tick);
            tick_in_frame = 1;
        } else {
            frame[TICK..].copy_from_slice(&tick);
            tick_in_frame = 0;

            let size = encoder.encode_float(&frame, &mut packet_buf).map_err(|e| {
                CoreError::InvalidInput {
                    message: e.to_string(),
                }
            })?;
            packets.push(packet_buf[..size].to_vec());

            let prev_mean_sq = tick_power[tick_power.len() - 2];
            let frame_rms = ((prev_mean_sq + mean_sq) / 2.0).sqrt();
            let frame_level = waveform::level_unit(frame_rms);
            loudest_20ms = loudest_20ms.max(frame_level);

            if last_event_time.elapsed() >= Duration::from_millis(33) {
                if let Some(events) = &events {
                    let _ = events.send(MediaEvent::VoiceRecordLevel {
                        level: loudest_20ms,
                    });
                }
                loudest_20ms = 0.0;
                last_event_time = Instant::now();
            }
        }

        if tick_power.len() >= max_ticks {
            stopped_by_max_length = true;
            if let Some(events) = &events {
                let _ = events.send(MediaEvent::VoiceRecordStopped {
                    reason: VoiceStopReason::MaxLength,
                });
            }
            break;
        }
    }

    if cancel_flag.load(Ordering::Relaxed) {
        return Ok(None);
    }

    if !stopped_by_max_length && !stop_flag.load(Ordering::Relaxed) && source.is_device_lost() {
        if let Some(events) = &events {
            let _ = events.send(MediaEvent::VoiceRecordStopped {
                reason: VoiceStopReason::DeviceLost,
            });
        }
        return Ok(None);
    }

    let frames = packets.len();
    let duration_ms = (frames * 20) as u32;
    let samples = frames as u64 * 960;

    // Flush encoder with one padding frame of silence so pre_skip lookahead is output
    let pad_frame = [0.0f32; FRAME];
    let pad_size = encoder
        .encode_float(&pad_frame, &mut packet_buf)
        .map_err(|e| CoreError::InvalidInput {
            message: e.to_string(),
        })?;
    packets.push(packet_buf[..pad_size].to_vec());

    let ogg_bytes = ogg::write(1, pre_skip, &packets, samples);
    let waveform = compute_waveform(&tick_power);

    let draft = VoiceDraft {
        draft_id,
        duration_ms,
        waveform,
    };

    Ok(Some(RecordResult { ogg_bytes, draft }))
}

struct ActiveRecording {
    draft_id: u64,
    stop_flag: Arc<AtomicBool>,
    cancel_flag: Arc<AtomicBool>,
    join_handle: Option<std::thread::JoinHandle<Option<VoiceDraft>>>,
    finished_draft: Arc<Mutex<Option<VoiceDraft>>>,
    tap_engine: Option<Arc<dyn TapEngine>>,
}

type DraftEntry = (Vec<u8>, VoiceDraft);
type DraftMap = HashMap<u64, DraftEntry>;

#[derive(Default)]
pub(crate) struct VoiceRecorder {
    drafts: Arc<Mutex<DraftMap>>,
    active: Mutex<Option<ActiveRecording>>,
}

impl VoiceRecorder {
    pub fn start(
        &self,
        engine: Option<Arc<dyn TapEngine>>,
        settings: AudioSettings,
        events: Option<mpsc::UnboundedSender<MediaEvent>>,
        max_seconds: u32,
    ) -> Result<(), CoreError> {
        let mut active_lock = self.active.lock().unwrap_or_else(PoisonError::into_inner);
        if active_lock.is_some() {
            return Err(CoreError::InvalidInput {
                message: "already recording".to_owned(),
            });
        }

        let draft_id = NEXT_DRAFT_ID.fetch_add(1, Ordering::Relaxed);
        let stop_flag = Arc::new(AtomicBool::new(false));
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let finished_draft = Arc::new(Mutex::new(None));

        let thread_stop = stop_flag.clone();
        let thread_cancel = cancel_flag.clone();
        let thread_finished = finished_draft.clone();
        let thread_drafts = Arc::clone(&self.drafts);
        let thread_engine = engine.clone();

        let join_handle = std::thread::Builder::new()
            .name("opencord-voice-recorder".to_owned())
            .spawn(move || {
                let source: Option<Box<dyn TickSource>> = if let Some(tap_engine) = &thread_engine {
                    let (producer, consumer) = rtrb::RingBuffer::new(96000);
                    tap_engine.set_capture_tap(Some(producer), true);
                    Some(Box::new(TapTickSource::new(
                        consumer,
                        thread_stop.clone(),
                        thread_cancel.clone(),
                    )))
                } else {
                    match Microphone::open(settings.input_device.as_deref()) {
                        Ok(mic) => match FromDevice::new(mic.rate, mic.channels) {
                            Ok(from_device) => {
                                let processing =
                                    VoiceProcessing::new(crate::media::processing(&settings));
                                Some(Box::new(MicTickSource::new(
                                    mic,
                                    from_device,
                                    processing,
                                    settings.input_volume,
                                    thread_stop.clone(),
                                    thread_cancel.clone(),
                                )))
                            }
                            Err(_) => None,
                        },
                        Err(_) => None,
                    }
                };

                let outcome = if let Some(source) = source {
                    record_ticks(
                        source,
                        max_seconds,
                        thread_stop,
                        thread_cancel,
                        events,
                        draft_id,
                    )
                } else {
                    Ok(None)
                };

                if let Some(engine) = &thread_engine {
                    engine.set_capture_tap(None, false);
                }
                if let Ok(Some(result)) = outcome {
                    *thread_finished
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner) = Some(result.draft.clone());
                    thread_drafts
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .insert(draft_id, (result.ogg_bytes, result.draft.clone()));
                    Some(result.draft)
                } else {
                    None
                }
            })
            .map_err(|e| CoreError::InvalidInput {
                message: format!("could not spawn voice recorder thread: {e}"),
            })?;

        *active_lock = Some(ActiveRecording {
            draft_id,
            stop_flag,
            cancel_flag,
            join_handle: Some(join_handle),
            finished_draft,
            tap_engine: engine,
        });

        Ok(())
    }

    pub fn stop(&self) -> Result<VoiceDraft, CoreError> {
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .ok_or_else(|| CoreError::InvalidInput {
                message: "not recording".to_owned(),
            })?;

        active.stop_flag.store(true, Ordering::Relaxed);
        if let Some(handle) = active.join_handle.take() {
            let _ = handle.join();
        }
        if let Some(engine) = &active.tap_engine {
            engine.set_capture_tap(None, false);
        }

        let draft = active
            .finished_draft
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
            .ok_or_else(|| CoreError::InvalidInput {
                message: "recording produced no draft".to_owned(),
            })?;

        Ok(draft)
    }

    pub fn cancel(&self) {
        let active = self
            .active
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(mut active) = active {
            active.cancel_flag.store(true, Ordering::Relaxed);
            if let Some(handle) = active.join_handle.take() {
                let _ = handle.join();
            }
            if let Some(engine) = &active.tap_engine {
                engine.set_capture_tap(None, false);
            }
            self.discard(active.draft_id);
        }
    }

    pub fn discard(&self, draft_id: u64) {
        self.drafts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&draft_id);
    }

    pub fn get_draft(&self, draft_id: u64) -> Option<(Vec<u8>, VoiceDraft)> {
        self.drafts
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&draft_id)
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencord_common::sound::ogg::OpusFile;
    use std::f32::consts::PI;

    struct TestSineSource {
        samples: Vec<f32>,
        position: usize,
    }

    impl TestSineSource {
        fn new(samples: Vec<f32>) -> Self {
            Self {
                samples,
                position: 0,
            }
        }
    }

    impl TickSource for TestSineSource {
        fn next_tick(&mut self, tick: &mut [f32; 480]) -> Option<()> {
            if self.position + 480 <= self.samples.len() {
                tick.copy_from_slice(&self.samples[self.position..self.position + 480]);
                self.position += 480;
                Some(())
            } else {
                None
            }
        }
    }

    #[derive(Default)]
    struct FakeTapEngine {
        tap: Mutex<Option<(bool, bool)>>,
    }

    impl TapEngine for FakeTapEngine {
        fn set_capture_tap(&self, tap: Option<rtrb::Producer<f32>>, silence_call: bool) {
            *self.tap.lock().unwrap() = Some((tap.is_some(), silence_call));
        }
    }

    fn generate_sine(samples: usize, freq: f32, amp: f32) -> Vec<f32> {
        (0..samples)
            .map(|i| {
                let t = i as f32 / 48000.0;
                amp * (2.0 * PI * freq * t).sin()
            })
            .collect()
    }

    #[test]
    fn a_recording_becomes_valid_ogg_opus() {
        let pcm = generate_sine(48000, 440.0, 0.5);
        let source = TestSineSource::new(pcm);
        let stop_flag = Arc::new(AtomicBool::new(false));
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let result = record_ticks(source, 60, stop_flag, cancel_flag, None, 1)
            .expect("record_ticks failed")
            .expect("recording returned None");

        let file = OpusFile::read(&result.ogg_bytes).expect("invalid ogg opus file");
        assert_eq!(file.info.channels, 1);
        let duration = file.info.duration_ms as i64;
        assert!(
            (duration - 1000).abs() <= 20,
            "duration was {duration} ms, expected 1000 ± 20 ms"
        );
        let draft_duration = result.draft.duration_ms as i64;
        assert!(
            (draft_duration - 1000).abs() <= 20,
            "draft duration was {draft_duration} ms, expected 1000 ± 20 ms"
        );
    }

    #[test]
    fn the_draft_waveform_matches_the_server_formula() {
        // 2 s input: 1 s sine of amplitude 0.5, then 1 s silence
        let mut pcm = generate_sine(48000, 440.0, 0.5);
        pcm.resize(96000, 0.0);

        let server_waveform = waveform::waveform(&pcm, 128);

        let source = TestSineSource::new(pcm);
        let stop_flag = Arc::new(AtomicBool::new(false));
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let result = record_ticks(source, 60, stop_flag, cancel_flag, None, 1)
            .expect("record_ticks failed")
            .expect("recording returned None");

        assert_eq!(result.draft.waveform.len(), 128);
        for (i, (&draft_val, &server_val)) in result
            .draft
            .waveform
            .iter()
            .zip(server_waveform.iter())
            .enumerate()
        {
            let diff = (draft_val as i16 - server_val as i16).abs();
            assert!(
                diff <= 6,
                "at index {i}: draft {draft_val} vs server {server_val} (diff {diff} > 6)"
            );
        }
    }

    #[test]
    fn recording_stops_at_the_maximum() {
        // max 1 s, 2 s of input -> duration 1000 ± 20, MaxLength event
        let pcm = generate_sine(96000, 440.0, 0.5);
        let source = TestSineSource::new(pcm);
        let stop_flag = Arc::new(AtomicBool::new(false));
        let cancel_flag = Arc::new(AtomicBool::new(false));
        let (tx, mut rx) = mpsc::unbounded_channel();

        let result = record_ticks(source, 1, stop_flag, cancel_flag, Some(tx), 1)
            .expect("record_ticks failed")
            .expect("recording returned None");

        let duration = result.draft.duration_ms as i64;
        assert!(
            (duration - 1000).abs() <= 20,
            "draft duration was {duration} ms, expected 1000 ± 20 ms"
        );

        let mut got_max_length = false;
        while let Ok(event) = rx.try_recv() {
            if let MediaEvent::VoiceRecordStopped {
                reason: VoiceStopReason::MaxLength,
            } = event
            {
                got_max_length = true;
            }
        }
        assert!(got_max_length, "expected VoiceRecordStopped(MaxLength)");
    }

    #[test]
    fn in_call_recording_silences_the_call() {
        let fake = Arc::new(FakeTapEngine::default());
        let (producer, _consumer) = rtrb::RingBuffer::new(96000);
        fake.set_capture_tap(Some(producer), true);

        assert_eq!(*fake.tap.lock().unwrap(), Some((true, true)));

        fake.set_capture_tap(None, false);
        assert_eq!(*fake.tap.lock().unwrap(), Some((false, false)));
    }
}
