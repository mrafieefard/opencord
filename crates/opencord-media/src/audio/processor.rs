//! The audio processing tick (plan §7.1): device samples in, Opus frames
//! out; Opus packets in, device samples out. The engine's thread runs it;
//! nothing here touches a device, so tests drive it directly. The mic test
//! plays what would be sent back through the speaker, Opus and all.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

use rtrb::{Consumer, Producer};

use super::capture::{Capture, EncodedFrame, InputMode};
use super::codec::CodecError;
use super::convert::{ConvertError, FromDevice, ToDevice};
use super::playback::{MAX_VOLUME, Playback};
use super::processing::{ProcessingSettings, VoiceProcessing};
use super::{TICK, dbfs};

/// Who the mic test plays as; never a real user (ids are positive).
const MIC_TEST_USER: i64 = -1;
/// The shared sound kept for the echo canceller, at most: 300 ms.
const SHARED_KEPT: usize = 30 * TICK;

/// What the listener and speaker chose.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProcessorSettings {
    pub mode: InputMode,
    pub processing: ProcessingSettings,
    /// Bits per second, from the channel.
    pub bitrate: u32,
    /// 0–2 (200 %).
    pub input_volume: f32,
    /// 0–2 (200 %).
    pub output_volume: f32,
}

#[derive(Debug, thiserror::Error)]
pub enum ProcessorError {
    #[error(transparent)]
    Codec(#[from] CodecError),
    #[error(transparent)]
    Convert(#[from] ConvertError),
    #[error("the audio thread stopped")]
    Stopped,
}

pub struct Processor {
    capture: Capture,
    processing: VoiceProcessing,
    playback: Playback,
    from_device: FromDevice,
    to_device: ToDevice,
    output_channels: usize,
    input_volume: f32,
    muted: bool,
    deafened: bool,
    tick: [f32; TICK],
    /// What plays, both sides.
    stereo: [f32; 2 * TICK],
    /// This device's shared screen sound, mixed to mono, not yet in the
    /// echo canceller's reference (plan §10.6).
    shared: VecDeque<f32>,
    mic_test: bool,
    /// Frames for the mic test, played from the next `play`.
    mic_test_frames: Vec<EncodedFrame>,
    /// The loudest microphone tick since the last look, after processing
    /// and the input volume.
    input_level: Option<f32>,
    capture_tap: Option<Producer<f32>>,
    silence_call: bool,
    local_media: Option<Consumer<f32>>,
}

impl Processor {
    /// `input` and `output` are each device's sample rate and channels.
    pub fn new(
        settings: ProcessorSettings,
        input: (u32, u16),
        output: (u32, u16),
    ) -> Result<Self, ProcessorError> {
        let mut playback = Playback::new();
        playback.set_master(settings.output_volume);
        let mut processing = VoiceProcessing::new(settings.processing);
        processing.set_voice_analysis(settings.mode.wants_voice_probability());
        Ok(Self {
            capture: Capture::new(settings.mode, settings.bitrate)?,
            processing,
            playback,
            from_device: FromDevice::new(input.0, input.1)?,
            to_device: ToDevice::new(output.0, output.1)?,
            output_channels: usize::from(output.1.max(1)),
            input_volume: settings.input_volume.clamp(0.0, MAX_VOLUME),
            muted: false,
            deafened: false,
            tick: [0.0; TICK],
            stereo: [0.0; 2 * TICK],
            shared: VecDeque::with_capacity(SHARED_KEPT),
            mic_test: false,
            mic_test_frames: Vec::new(),
            input_level: None,
            capture_tap: None,
            silence_call: false,
            local_media: None,
        })
    }

    /// Attaches or detaches a capture tap. When `silence_call` is true,
    /// processed ticks still reach the tap and the input level meter, but
    /// the call receives silence.
    pub fn set_capture_tap(&mut self, tap: Option<Producer<f32>>, silence_call: bool) {
        self.capture_tap = tap;
        self.silence_call = silence_call;
    }

    /// Sets or clears an interleaved stereo source (48 kHz) to mix into
    /// local playback and the echo canceller's reference.
    pub fn set_local_media(&mut self, source: Option<Consumer<f32>>) {
        self.local_media = source;
    }

    /// A new microphone, at its rate and channels.
    pub fn set_input_device(&mut self, rate: u32, channels: u16) -> Result<(), ProcessorError> {
        self.from_device = FromDevice::new(rate, channels)?;
        Ok(())
    }

    /// A new speaker, at its rate and channels.
    pub fn set_output_device(&mut self, rate: u32, channels: u16) -> Result<(), ProcessorError> {
        self.to_device = ToDevice::new(rate, channels)?;
        self.output_channels = usize::from(channels.max(1));
        Ok(())
    }

    /// Interleaved microphone samples; `send` gets each frame to send.
    /// Each tick is cleaned up, then the input volume applies (plan §7.2).
    pub fn capture(&mut self, interleaved: &[f32], mut send: impl FnMut(EncodedFrame)) {
        self.from_device.push(interleaved);
        while self.from_device.pop_tick(&mut self.tick) {
            let heard = self.processing.capture(&mut self.tick);
            if self.input_volume != 1.0 {
                for sample in &mut self.tick {
                    *sample *= self.input_volume;
                }
            }
            // A whole tick or none: a reader that falls behind loses ticks,
            // never parts of one.
            if let Some(tap) = &mut self.capture_tap
                && let Ok(chunk) = tap.write_chunk_uninit(self.tick.len())
            {
                chunk.fill_from_iter(self.tick.iter().copied());
            }
            let level = dbfs(&self.tick);
            self.input_level = Some(self.input_level.map_or(level, |loudest| loudest.max(level)));
            if self.silence_call {
                self.tick.fill(0.0);
            }
            if let Some(frame) = self.capture.push(&self.tick, heard) {
                if self.mic_test {
                    self.mic_test_frames.push(frame.clone());
                }
                send(frame);
            }
        }
    }

    /// A packet from `user_id`.
    pub fn receive(
        &mut self,
        user_id: i64,
        timestamp: u32,
        marker: bool,
        payload: Arc<[u8]>,
        arrived: Instant,
    ) {
        self.playback
            .receive(user_id, timestamp, marker, payload, arrived);
    }

    /// Fills interleaved speaker samples, playing as many ticks as that
    /// takes; returns the frames written.
    pub fn play(&mut self, now: Instant, interleaved: &mut [f32]) -> usize {
        self.deliver_mic_test(now);
        let wanted = interleaved.len() / self.output_channels;
        while self.to_device.buffered() < wanted {
            self.playback.tick_stereo(now, &mut self.stereo);
            if let Some(source) = &mut self.local_media {
                for sample in &mut self.stereo {
                    match source.pop() {
                        Ok(s) => *sample += s,
                        Err(_) => break,
                    }
                }
            }
            // Everything played is what the echo canceller listens for,
            // and the sound this device shares, which its speakers play
            // too: the microphone must not send it again (plan §10.6).
            let shared = self.shared.len().min(TICK);
            let mut shared = self.shared.drain(..shared);
            for (slot, frame) in self.tick.iter_mut().zip(self.stereo.as_chunks::<2>().0) {
                *slot = (frame[0] + frame[1]) / 2.0 + shared.next().unwrap_or(0.0);
            }
            drop(shared);
            self.processing.render(&self.tick);
            self.to_device.push_tick(&self.stereo);
        }
        self.to_device.pop(interleaved)
    }

    /// One tick of playback with no speaker to hear it: everyone's audio
    /// moves on in time, and nothing is an echo to cancel.
    pub fn play_unheard(&mut self, now: Instant) {
        self.deliver_mic_test(now);
        self.playback.tick_stereo(now, &mut self.stereo);
        if let Some(source) = &mut self.local_media {
            for _ in 0..self.stereo.len() {
                if source.pop().is_err() {
                    break;
                }
            }
        }
        self.shared.clear();
    }

    /// The sound this device shares, as captured (interleaved stereo at
    /// 48 kHz), for the echo canceller.
    pub fn shared_sound(&mut self, stereo: &[f32]) {
        self.shared.extend(
            stereo
                .as_chunks::<2>()
                .0
                .iter()
                .map(|frame| (frame[0] + frame[1]) / 2.0),
        );
        let over = self.shared.len().saturating_sub(SHARED_KEPT);
        self.shared.drain(..over);
    }

    /// A packet of `user_id`'s screen share sound.
    pub fn receive_stream(
        &mut self,
        user_id: i64,
        timestamp: u32,
        marker: bool,
        payload: Arc<[u8]>,
        arrived: Instant,
    ) {
        self.playback
            .receive_stream(user_id, timestamp, marker, payload, arrived);
    }

    pub fn remove_stream(&mut self, user_id: i64) {
        self.playback.remove_stream(user_id);
    }

    /// See [`Playback::stream_delays`].
    pub fn stream_delays(&self) -> Vec<(i64, std::time::Duration)> {
        self.playback.stream_delays().collect()
    }

    pub fn set_stream_volume(&mut self, user_id: i64, volume: f32) {
        self.playback.set_stream_volume(user_id, volume);
    }

    pub fn set_stream_mute(&mut self, user_id: i64, muted: bool) {
        self.playback.set_stream_mute(user_id, muted);
    }

    fn deliver_mic_test(&mut self, now: Instant) {
        for frame in self.mic_test_frames.drain(..) {
            self.playback.receive(
                MIC_TEST_USER,
                frame.position as u32,
                frame.marker,
                Arc::from(frame.payload),
                now,
            );
        }
    }

    /// A soundboard sound, mono at 48 kHz, at `gain` (0–1).
    pub fn play_sound(&mut self, samples: Arc<[f32]>, gain: f32) {
        self.playback.play_sound(samples, gain);
    }

    pub fn stop_sounds(&mut self) {
        self.playback.stop_sounds();
    }

    pub fn remove_user(&mut self, user_id: i64) {
        self.playback.remove(user_id);
    }

    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
        self.capture.set_muted(self.muted || self.deafened);
    }

    /// Deafened hears nothing and sends nothing.
    pub fn set_deafened(&mut self, deafened: bool) {
        self.deafened = deafened;
        self.playback.set_deafened(deafened);
        self.capture.set_muted(self.muted || self.deafened);
    }

    pub fn set_mode(&mut self, mode: InputMode) {
        self.capture.set_mode(mode);
        self.processing
            .set_voice_analysis(mode.wants_voice_probability());
    }

    pub fn set_processing(&mut self, settings: ProcessingSettings) {
        self.processing.set_settings(settings);
    }

    pub fn set_push_to_talk(&mut self, held: bool) {
        self.capture.set_push_to_talk(held);
    }

    pub fn set_input_volume(&mut self, volume: f32) {
        self.input_volume = volume.clamp(0.0, MAX_VOLUME);
    }

    pub fn set_output_volume(&mut self, volume: f32) {
        self.playback.set_master(volume);
    }

    pub fn set_user_volume(&mut self, user_id: i64, volume: f32) {
        self.playback.set_volume(user_id, volume);
    }

    pub fn set_local_mute(&mut self, user_id: i64, muted: bool) {
        self.playback.set_local_mute(user_id, muted);
    }

    pub fn set_bitrate(&mut self, bitrate: u32) -> Result<(), ProcessorError> {
        Ok(self.capture.set_bitrate(bitrate)?)
    }

    pub fn set_expected_loss(&mut self, percent: u8) -> Result<(), ProcessorError> {
        Ok(self.capture.set_expected_loss(percent)?)
    }

    /// Whether the microphone is sending a talk spurt.
    pub fn is_talking(&self) -> bool {
        self.capture.is_talking()
    }

    /// The priority speaker key.
    pub fn set_priority_held(&mut self, held: bool) {
        self.capture.set_priority_held(held);
    }

    /// Whether this talk spurt is the priority speaker's.
    pub fn is_priority(&self) -> bool {
        self.capture.is_priority()
    }

    /// Whether `user_id` speaks with priority (the node relays it).
    pub fn set_priority(&mut self, user_id: i64, priority: bool) {
        self.playback.set_priority(user_id, priority);
    }

    /// Hear what would be sent.
    pub fn set_mic_test(&mut self, on: bool) {
        self.mic_test = on;
        if !on {
            self.mic_test_frames.clear();
            self.playback.remove(MIC_TEST_USER);
        }
    }

    /// Adds to `changes` who started or stopped speaking since the last
    /// call.
    pub fn speaking_changes(&mut self, changes: &mut Vec<(i64, bool)>) {
        self.playback.speaking_changes(changes);
        changes.retain(|(user_id, _)| *user_id != MIC_TEST_USER);
    }

    /// The loudest microphone tick, in dBFS, since the last call.
    pub fn take_input_level(&mut self) -> Option<f32> {
        self.input_level.take()
    }

    /// Whether the microphone heard speaking while muted (at most every
    /// 30 s).
    pub fn take_speaking_while_muted(&mut self) -> bool {
        self.capture.take_speaking_while_muted()
    }

    /// Whether High noise suppression gave way to Standard.
    pub fn take_noise_suppression_fallback(&mut self) -> bool {
        self.processing.take_fallback()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::audio::capture::{InputMode, Sensitivity};
    use crate::audio::fixtures::{at_level, speech};
    use crate::audio::processing::NoiseSuppression;
    use crate::audio::{SAMPLE_RATE, TICK, dbfs};

    /// Manual sensitivity and no cleanup, so levels pass as they are.
    fn settings() -> ProcessorSettings {
        ProcessorSettings {
            mode: InputMode::VoiceActivity(Sensitivity::Manual {
                threshold_dbfs: -50.0,
            }),
            processing: ProcessingSettings {
                echo_cancellation: false,
                noise_suppression: NoiseSuppression::Off,
                automatic_gain: false,
            },
            bitrate: 64_000,
            input_volume: 1.0,
            output_volume: 1.0,
        }
    }

    /// 10 ms of a tone at 48 kHz stereo, as a microphone would deliver it.
    fn microphone_tick(index: usize, amplitude: f32) -> Vec<f32> {
        (0..TICK)
            .flat_map(|i| {
                let t = (index * TICK + i) as f32 / SAMPLE_RATE as f32;
                let value = amplitude * (std::f32::consts::TAU * 400.0 * t).sin();
                [value, value]
            })
            .collect()
    }

    /// `ticks` ticks of a tone through `sender`; returns the frames.
    fn speak(sender: &mut Processor, ticks: usize, amplitude: f32) -> Vec<EncodedFrame> {
        let mut frames = Vec::new();
        for index in 0..ticks {
            sender.capture(&microphone_tick(index, amplitude), |frame| {
                frames.push(frame)
            });
        }
        frames
    }

    /// Delivers `frames` from user 1 on time and plays `ticks` ticks on a
    /// stereo speaker; returns the level of the second half.
    fn hear(listener: &mut Processor, frames: &[EncodedFrame], ticks: usize) -> f32 {
        let start = Instant::now();
        let mut played = Vec::new();
        let mut next = 0;
        for index in 0..ticks {
            let now = start + Duration::from_millis(index as u64 * 10);
            while next < frames.len() && frames[next].position / TICK as u64 <= index as u64 {
                let frame = &frames[next];
                listener.receive(
                    1,
                    frame.position as u32,
                    frame.marker,
                    Arc::from(&frame.payload[..]),
                    start + Duration::from_millis(frame.position / 48),
                );
                next += 1;
            }
            let mut out = vec![0.0; 2 * TICK];
            let written = listener.play(now, &mut out);
            if index >= ticks / 2 {
                played.extend_from_slice(&out[..2 * written]);
            }
        }
        dbfs(&played)
    }

    #[test]
    fn what_one_says_the_other_hears() {
        let mut alice = Processor::new(settings(), (SAMPLE_RATE, 2), (SAMPLE_RATE, 2)).unwrap();
        let mut bob = Processor::new(settings(), (SAMPLE_RATE, 2), (SAMPLE_RATE, 2)).unwrap();

        let frames = speak(&mut alice, 100, 0.2);
        let level = hear(&mut bob, &frames, 100);

        assert_eq!(frames.len(), 50);
        assert!(
            (level - dbfs(&microphone_tick(0, 0.2))).abs() < 3.0,
            "{level}"
        );
    }

    #[test]
    fn the_input_volume_scales_the_microphone() {
        let mut quiet = settings();
        quiet.input_volume = 0.5;
        let mut normal = Processor::new(settings(), (SAMPLE_RATE, 2), (SAMPLE_RATE, 2)).unwrap();
        let mut halved = Processor::new(quiet, (SAMPLE_RATE, 2), (SAMPLE_RATE, 2)).unwrap();
        let mut listener = Processor::new(settings(), (SAMPLE_RATE, 2), (SAMPLE_RATE, 2)).unwrap();

        let loud = hear(&mut listener, &speak(&mut normal, 100, 0.2), 100);
        let mut listener = Processor::new(settings(), (SAMPLE_RATE, 2), (SAMPLE_RATE, 2)).unwrap();
        let soft = hear(&mut listener, &speak(&mut halved, 100, 0.2), 100);

        assert!((loud - soft - 6.0).abs() < 1.5, "{loud} against {soft}");
    }

    #[test]
    fn muted_sends_nothing_and_deafened_hears_nothing() {
        let mut alice = Processor::new(settings(), (SAMPLE_RATE, 2), (SAMPLE_RATE, 2)).unwrap();
        let mut bob = Processor::new(settings(), (SAMPLE_RATE, 2), (SAMPLE_RATE, 2)).unwrap();
        let frames = speak(&mut alice, 100, 0.2);

        alice.set_muted(true);
        let muted = speak(&mut alice, 100, 0.2);
        bob.set_deafened(true);
        let deafened = hear(&mut bob, &frames, 100);

        assert!(muted.is_empty());
        assert_eq!(deafened, crate::audio::SILENT_DBFS);
    }

    #[test]
    fn devices_at_other_rates_work_too() {
        let mut alice = Processor::new(settings(), (SAMPLE_RATE, 2), (SAMPLE_RATE, 2)).unwrap();
        let mut bob = Processor::new(settings(), (44_100, 1), (44_100, 2)).unwrap();

        let frames = speak(&mut alice, 100, 0.2);
        let start = Instant::now();
        let mut written = 0;
        for (index, frame) in frames.iter().enumerate() {
            bob.receive(
                1,
                frame.position as u32,
                frame.marker,
                Arc::from(&frame.payload[..]),
                start + Duration::from_millis(index as u64 * 20),
            );
        }
        for index in 0..100u64 {
            let mut out = vec![0.0; 2 * 441];
            written += bob.play(start + Duration::from_millis(index * 10), &mut out);
        }

        assert!(
            written.abs_diff(44_100_usize) < 2 * 441,
            "{written} frames for a second"
        );
    }

    /// A small deterministic generator, so impaired runs repeat exactly.
    struct XorShift(u64);

    impl XorShift {
        fn next(&mut self) -> f64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 11) as f64 / (1u64 << 53) as f64
        }
    }

    /// Sends ticks of microphone audio through `sender`; returns each frame
    /// with the tick it was ready at.
    fn capture(sender: &mut Processor, ticks: &[Vec<f32>]) -> Vec<(usize, EncodedFrame)> {
        let mut frames = Vec::new();
        for (index, tick) in ticks.iter().enumerate() {
            sender.capture(tick, |frame| frames.push((index, frame)));
        }
        frames
    }

    /// Plays `ticks` ticks on a stereo speaker, delivering each frame when
    /// it arrives (in ms after start); returns each tick's level.
    fn play(
        listener: &mut Processor,
        mut arrivals: Vec<(u64, EncodedFrame)>,
        ticks: usize,
    ) -> Vec<f32> {
        let start = Instant::now();
        arrivals.sort_by_key(|(at, _)| *at);
        let mut arrivals = arrivals.into_iter().peekable();
        let mut levels = Vec::new();
        for index in 0..ticks {
            let now_ms = index as u64 * 10;
            while let Some((at, frame)) = arrivals.next_if(|(at, _)| *at <= now_ms) {
                listener.receive(
                    1,
                    frame.position as u32,
                    frame.marker,
                    Arc::from(&frame.payload[..]),
                    start + Duration::from_millis(at),
                );
            }
            let mut out = vec![0.0; 2 * TICK];
            let written = listener.play(start + Duration::from_millis(now_ms), &mut out);
            levels.push(dbfs(&out[..2 * written]));
        }
        levels
    }

    #[test]
    fn speech_survives_5_percent_loss_and_40_ms_of_jitter() {
        let mut alice = Processor::new(settings(), (SAMPLE_RATE, 2), (SAMPLE_RATE, 2)).unwrap();
        let ticks: Vec<Vec<f32>> = (0..300).map(|index| microphone_tick(index, 0.2)).collect();
        let frames = capture(&mut alice, &ticks);
        let mut random = XorShift(0x5eed);
        let mut lost = 0;
        let mut arrivals: Vec<(u64, EncodedFrame)> = Vec::new();
        for (tick, frame) in frames {
            if random.next() < 0.05 {
                lost += 1;
                continue;
            }
            let ready_ms = (tick as u64 + 1) * 10;
            arrivals.push((ready_ms + (random.next() * 40.0) as u64, frame));
        }
        let mut bob = Processor::new(settings(), (SAMPLE_RATE, 2), (SAMPLE_RATE, 2)).unwrap();

        let levels = play(&mut bob, arrivals, 340);

        // From when playing settles until the speech ends, nothing drops
        // out: every lost or late frame was recovered or concealed. Opus
        // fades back in over a frame after concealing, so a dip is not a
        // dropout; near silence is.
        let speech = &levels[40..300];
        let gaps = speech.iter().filter(|db| **db < -60.0).count();
        assert!(lost >= 5, "the network lost only {lost} frames");
        assert_eq!(gaps, 0, "{gaps} silent ticks");
        let loud = speech.iter().filter(|db| **db > -22.0).count();
        assert!(
            loud * 10 >= speech.len() * 9,
            "{loud} of {} ticks at speech level",
            speech.len()
        );
    }

    #[test]
    fn the_software_path_adds_little_latency() {
        let mut alice = Processor::new(settings(), (SAMPLE_RATE, 2), (SAMPLE_RATE, 2)).unwrap();
        // Quiet, then a click at tick 50.
        let mut ticks: Vec<Vec<f32>> = (0..50).map(|_| vec![0.0; 2 * TICK]).collect();
        ticks.extend((50..60).map(|index| microphone_tick(index, 0.5)));
        let frames = capture(&mut alice, &ticks);
        let arrivals = frames
            .into_iter()
            .map(|(tick, frame)| ((tick as u64 + 1) * 10, frame))
            .collect();
        let mut bob = Processor::new(settings(), (SAMPLE_RATE, 2), (SAMPLE_RATE, 2)).unwrap();

        let levels = play(&mut bob, arrivals, 100);

        let heard_at = levels.iter().position(|db| *db > -30.0).unwrap();
        let latency_ms = (heard_at - 50) * 10;
        // Framing, the shortest jitter delay and decoding; devices add
        // their buffers on top (plan §16.1: 150 ms with them).
        assert!(latency_ms <= 60, "{latency_ms} ms");
    }

    /// What everyone runs: echo cancellation, `noise_suppression`, gain
    /// control and automatic sensitivity.
    fn defaults(noise_suppression: NoiseSuppression) -> ProcessorSettings {
        ProcessorSettings {
            mode: InputMode::VoiceActivity(Sensitivity::Automatic),
            processing: ProcessingSettings {
                noise_suppression,
                ..ProcessingSettings::default()
            },
            ..settings()
        }
    }

    /// Alice talks; Bob's microphone hears his speaker 40 ms later, 10 dB
    /// down, and from tick `bob_from` on Bob himself, someone else reading.
    /// Returns the ticks after the first second at which Bob sent a frame.
    fn sent_by_bob(bob_settings: ProcessorSettings, bob_from: Option<usize>) -> Vec<usize> {
        let mut alice = Processor::new(settings(), (SAMPLE_RATE, 1), (SAMPLE_RATE, 1)).unwrap();
        let mut bob = Processor::new(bob_settings, (SAMPLE_RATE, 1), (SAMPLE_RATE, 1)).unwrap();
        let voice = at_level(&speech(), -20.0);
        let ticks: Vec<Vec<f32>> = voice
            .as_chunks::<TICK>()
            .0
            .iter()
            .map(|tick| tick.to_vec())
            .collect();
        // Bob's voice: the reading backwards, so it is no echo of Alice's.
        let bob_voice: Vec<f32> = at_level(&speech(), -25.0).into_iter().rev().collect();
        let bob_ticks = bob_voice.len() / TICK;
        let arrivals: Vec<(u64, EncodedFrame)> = capture(&mut alice, &ticks)
            .into_iter()
            .map(|(tick, frame)| ((tick as u64 + 1) * 10, frame))
            .collect();
        let start = Instant::now();
        let mut arrivals = arrivals.into_iter().peekable();
        let mut room: VecDeque<Vec<f32>> = std::iter::repeat_n(vec![0.0; TICK], 4).collect();
        let end = ticks.len().max(bob_from.unwrap_or(0) + bob_ticks) + 40;
        let mut sent = Vec::new();
        for index in 0..end {
            let now_ms = index as u64 * 10;
            while let Some((at, frame)) = arrivals.next_if(|(at, _)| *at <= now_ms) {
                bob.receive(
                    1,
                    frame.position as u32,
                    frame.marker,
                    Arc::from(&frame.payload[..]),
                    start + Duration::from_millis(at),
                );
            }
            let mut played = vec![0.0; TICK];
            bob.play(start + Duration::from_millis(now_ms), &mut played);
            room.push_back(played.iter().map(|s| s * 0.316).collect());
            let mut heard = room.pop_front().unwrap();
            if let Some(from) = bob_from
                && let Some(own) = index
                    .checked_sub(from)
                    .and_then(|at| bob_voice.get(at * TICK..(at + 1) * TICK))
            {
                for (sample, own) in heard.iter_mut().zip(own) {
                    *sample += own;
                }
            }
            bob.capture(&heard, |_| {
                if index >= 100 {
                    sent.push(index);
                }
            });
        }
        sent
    }

    #[test]
    fn what_bob_plays_does_not_go_back_to_alice() {
        crate::audio::deep_filter::preload().unwrap();
        let mut only_echo_cancellation = settings();
        only_echo_cancellation.processing.echo_cancellation = true;

        let without = sent_by_bob(settings(), None).len();

        assert!(
            without > 100,
            "the test has no echo to remove: {without} frames"
        );
        for bob in [
            defaults(NoiseSuppression::Standard),
            defaults(NoiseSuppression::High),
            only_echo_cancellation,
        ] {
            let sent = sent_by_bob(bob, None);
            assert!(
                sent.is_empty(),
                "{} frames of echo went back ({bob:?})",
                sent.len()
            );
        }
    }

    /// Bob shares his screen's sound: an app plays a reading on his
    /// speakers, which his microphone hears 40 ms later, 10 dB down.
    /// Returns the ticks after the first second at which Bob sent a frame.
    fn sent_while_sharing(echo_cancellation: bool, told: bool) -> Vec<usize> {
        let mut bob_settings = settings();
        bob_settings.processing.echo_cancellation = echo_cancellation;
        let mut bob = Processor::new(bob_settings, (SAMPLE_RATE, 1), (SAMPLE_RATE, 1)).unwrap();
        let shared = at_level(&speech(), -20.0);
        let start = Instant::now();
        let mut room: VecDeque<Vec<f32>> = std::iter::repeat_n(vec![0.0; TICK], 4).collect();
        let mut sent = Vec::new();
        for (index, tick) in shared.as_chunks::<TICK>().0.iter().enumerate() {
            if told {
                let stereo: Vec<f32> = tick.iter().flat_map(|s| [*s, *s]).collect();
                bob.shared_sound(&stereo);
            }
            let mut played = vec![0.0; TICK];
            bob.play(
                start + Duration::from_millis(index as u64 * 10),
                &mut played,
            );
            room.push_back(tick.iter().map(|s| s * 0.316).collect());
            let heard = room.pop_front().unwrap();
            bob.capture(&heard, |_| {
                if index >= 100 {
                    sent.push(index);
                }
            });
        }
        sent
    }

    #[test]
    fn the_sound_bob_shares_is_not_sent_again_through_his_microphone() {
        let without_canceller = sent_while_sharing(false, true);
        let not_told = sent_while_sharing(true, false);
        let told = sent_while_sharing(true, true);

        assert!(
            without_canceller.len() > 100 && not_told.len() > 100,
            "the test has no echo to remove: {}, {}",
            without_canceller.len(),
            not_told.len()
        );
        // The reading starts near the end of the first second; the
        // canceller takes a few hundred milliseconds to find the echo,
        // then nothing of it goes out.
        assert!(
            told.iter().all(|tick| *tick < 150),
            "the shared sound went out at {told:?}"
        );
    }

    #[test]
    fn bob_talking_over_alice_is_still_sent() {
        // Alice reads until about tick 350; Bob starts at 150.
        let sent = sent_by_bob(defaults(NoiseSuppression::Standard), Some(150));

        let over_alice = sent
            .iter()
            .filter(|tick| (150..350).contains(*tick))
            .count();
        // 100 frames would be all of it; Bob's reading has pauses too, and
        // the echo canceller turns a voice down a little while both talk.
        assert!(
            over_alice >= 75,
            "only {over_alice} frames while both talked"
        );
    }

    /// Captures `ticks` ticks of a tone and plays as much, 10 ms apart;
    /// returns each played tick's level.
    fn test_microphone(processor: &mut Processor, ticks: usize) -> Vec<f32> {
        let start = Instant::now();
        (0..ticks)
            .map(|index| {
                processor.capture(&microphone_tick(index, 0.3), |_| {});
                let mut out = vec![0.0; 2 * TICK];
                processor.play(start + Duration::from_millis(index as u64 * 10), &mut out);
                dbfs(&out)
            })
            .collect()
    }

    #[test]
    fn the_mic_test_plays_the_microphone_back() {
        let mut processor = Processor::new(settings(), (SAMPLE_RATE, 2), (SAMPLE_RATE, 2)).unwrap();
        processor.set_mic_test(true);

        let levels = test_microphone(&mut processor, 100);

        let heard = levels[50..].iter().sum::<f32>() / 50.0;
        let spoken = dbfs(&microphone_tick(0, 0.3));
        assert!((heard - spoken).abs() < 3.0, "{heard} against {spoken}");
    }

    #[test]
    fn without_the_mic_test_nothing_comes_back() {
        let mut processor = Processor::new(settings(), (SAMPLE_RATE, 2), (SAMPLE_RATE, 2)).unwrap();
        processor.set_mic_test(true);
        test_microphone(&mut processor, 50);

        processor.set_mic_test(false);
        let levels = test_microphone(&mut processor, 50);

        assert!(levels[10..].iter().all(|db| *db < -100.0), "{levels:?}");
    }

    #[test]
    fn the_mic_test_is_not_someone_speaking() {
        let mut processor = Processor::new(settings(), (SAMPLE_RATE, 2), (SAMPLE_RATE, 2)).unwrap();
        processor.set_mic_test(true);
        test_microphone(&mut processor, 50);

        let mut changes = Vec::new();
        processor.speaking_changes(&mut changes);

        assert!(changes.is_empty(), "{changes:?}");
    }

    #[test]
    fn the_input_level_is_the_loudest_tick_since_the_last_look() {
        let mut quiet = settings();
        quiet.input_volume = 0.5;
        let mut processor = Processor::new(quiet, (SAMPLE_RATE, 2), (SAMPLE_RATE, 2)).unwrap();

        let before = processor.take_input_level();
        // Some ticks first, so the high-pass filter has settled.
        for index in 0..20 {
            processor.capture(&microphone_tick(index, 0.3), |_| {});
        }
        processor.take_input_level();
        processor.capture(&microphone_tick(20, 0.1), |_| {});
        processor.capture(&microphone_tick(21, 0.3), |_| {});
        let level = processor.take_input_level().unwrap();
        let again = processor.take_input_level();

        // The tone at 0.3, halved by the input volume.
        let expected = dbfs(&microphone_tick(1, 0.3)) - 6.0;
        assert_eq!(before, None);
        assert!((level - expected).abs() < 0.5, "{level} against {expected}");
        assert_eq!(again, None);
    }

    #[test]
    fn holding_the_priority_key_talks_with_priority() {
        let mut processor = Processor::new(settings(), (SAMPLE_RATE, 2), (SAMPLE_RATE, 2)).unwrap();

        processor.set_priority_held(true);
        processor.capture(&microphone_tick(0, 0.0), |_| {});

        assert!(processor.is_talking());
        assert!(processor.is_priority());
    }

    #[test]
    fn without_a_speaker_playback_keeps_time_and_is_not_an_echo() {
        let mut alice = Processor::new(settings(), (SAMPLE_RATE, 2), (SAMPLE_RATE, 2)).unwrap();
        let mut bob = Processor::new(
            defaults(NoiseSuppression::Standard),
            (SAMPLE_RATE, 2),
            (SAMPLE_RATE, 2),
        )
        .unwrap();
        let frames = speak(&mut alice, 50, 0.3);
        let start = Instant::now();
        let mut changes = Vec::new();
        let mut stopped_at = None;

        for index in 0..100 {
            let now = start + Duration::from_millis(index as u64 * 10);
            for frame in frames
                .iter()
                .filter(|frame| frame.position / TICK as u64 == index as u64)
            {
                bob.receive(
                    1,
                    frame.position as u32,
                    frame.marker,
                    Arc::from(&frame.payload[..]),
                    now,
                );
            }
            bob.play_unheard(now);
            changes.clear();
            bob.speaking_changes(&mut changes);
            if changes.contains(&(1, false)) {
                stopped_at = Some(index);
            }
            // Bob's own voice must not be taken for an echo of Alice's.
            let heard = bob
                .processing
                .capture(&mut microphone_tick(index, 0.3)[..TICK].to_vec());
            assert!(!heard.echo_only, "tick {index}");
        }

        // Alice's half second, the jitter buffer's wait, then 250 ms.
        let stopped_at = stopped_at.expect("speaking never stopped");
        assert!(
            (75..=90).contains(&stopped_at),
            "stopped at tick {stopped_at}"
        );
    }

    /// Plan §16.1's audio processing budget, measured: run with
    /// `--release --ignored --nocapture`.
    #[test]
    #[ignore = "a measurement; run in a release build"]
    fn processing_time_per_tick() {
        const TICKS: usize = 3_000;
        crate::audio::deep_filter::preload().unwrap();
        let far = at_level(&speech(), -20.0);
        let near: Vec<f32> = at_level(&speech(), -25.0).into_iter().rev().collect();
        let noise = at_level(&crate::audio::fixtures::noise("fan", far.len()), -40.0);
        let tick_at = |index: usize| {
            let start = (index * TICK) % (far.len() - TICK);
            start..start + TICK
        };
        for mode in [NoiseSuppression::Standard, NoiseSuppression::High] {
            // The voice processing alone: high-pass filter, echo
            // cancellation, noise suppression, gain control.
            let mut processing = VoiceProcessing::new(ProcessingSettings {
                noise_suppression: mode,
                ..ProcessingSettings::default()
            });
            processing.set_voice_analysis(true);
            let started = Instant::now();
            for index in 0..TICKS {
                let range = tick_at(index);
                processing.render(&far[range.clone()]);
                let mut tick: Vec<f32> = (0..TICK)
                    .map(|i| {
                        far[range.start + i] * 0.316
                            + near[range.start + i]
                            + noise[range.start + i]
                    })
                    .collect();
                processing.capture(&mut tick);
            }
            let processing_us = started.elapsed().as_secs_f64() * 1e6 / TICKS as f64;

            // The whole audio tick: that, Opus both ways, one voice heard.
            let mut alice = Processor::new(settings(), (SAMPLE_RATE, 1), (SAMPLE_RATE, 1)).unwrap();
            let mut bob =
                Processor::new(defaults(mode), (SAMPLE_RATE, 1), (SAMPLE_RATE, 1)).unwrap();
            let mut frames = Vec::new();
            for index in 0..TICKS {
                alice.capture(&far[tick_at(index)], |frame| frames.push((index, frame)));
            }
            let start = Instant::now();
            let mut arrivals = frames.into_iter().peekable();
            let mut out = vec![0.0; TICK];
            let started = Instant::now();
            for index in 0..TICKS {
                let now = start + Duration::from_millis(index as u64 * 10);
                while let Some((_, frame)) = arrivals.next_if(|(at, _)| *at <= index) {
                    bob.receive(
                        1,
                        frame.position as u32,
                        frame.marker,
                        Arc::from(&frame.payload[..]),
                        now,
                    );
                }
                bob.play(now, &mut out);
                let range = tick_at(index);
                let heard: Vec<f32> = (0..TICK)
                    .map(|i| out[i] * 0.316 + near[range.start + i] + noise[range.start + i])
                    .collect();
                bob.capture(&heard, |_| {});
            }
            let whole_us = started.elapsed().as_secs_f64() * 1e6 / TICKS as f64;
            println!(
                "{mode:?}: processing {processing_us:.0} us per 10 ms tick ({:.1} % of a core), whole audio tick {whole_us:.0} us ({:.1} %)",
                processing_us / 100.0,
                whole_us / 100.0
            );
        }
    }

    #[test]
    fn the_tap_gets_processed_ticks_and_silence_reaches_the_call() {
        use crate::audio::codec::VoiceDecoder;
        use rtrb::RingBuffer;

        let mut processor = Processor::new(settings(), (SAMPLE_RATE, 2), (SAMPLE_RATE, 2)).unwrap();
        processor.set_mode(InputMode::PushToTalk {
            release_delay: Duration::ZERO,
        });
        processor.set_push_to_talk(true);
        let (producer, mut consumer) = RingBuffer::new(25 * TICK);
        processor.set_capture_tap(Some(producer), true);

        let mut sent = Vec::new();
        for index in 0..20 {
            processor.capture(&microphone_tick(index, 0.5), |frame| sent.push(frame));
        }

        // Tap receives 480-sample ticks of processed audio (sine wave)
        let mut tapped = Vec::new();
        while let Ok(sample) = consumer.pop() {
            tapped.push(sample);
        }
        assert_eq!(tapped.len(), 20 * TICK);
        assert!(dbfs(&tapped) > -30.0);

        // Every encoded frame sent to the call decodes to silence (< -60 dBFS)
        assert!(!sent.is_empty());
        let mut decoder = VoiceDecoder::new().unwrap();
        let mut pcm = vec![0.0; 960];
        for frame in sent {
            let n = decoder.decode(&frame.payload, &mut pcm).unwrap();
            let energy = dbfs(&pcm[..n]);
            assert!(
                energy < -60.0,
                "expected silence (< -60 dBFS), but frame had {energy} dBFS"
            );
        }
    }

    #[test]
    fn local_media_is_played_and_heard_by_the_echo_canceller() {
        use rtrb::RingBuffer;

        let sent_while_local_media = |echo_cancellation: bool,
                                      with_media: bool|
         -> (Vec<usize>, f32) {
            let mut bob_settings = settings();
            bob_settings.processing.echo_cancellation = echo_cancellation;
            let mut bob = Processor::new(bob_settings, (SAMPLE_RATE, 1), (SAMPLE_RATE, 1)).unwrap();
            let speech_samples = at_level(&speech(), -20.0);
            let start = Instant::now();
            let mut room: VecDeque<Vec<f32>> = std::iter::repeat_n(vec![0.0; TICK], 4).collect();
            let mut sent = Vec::new();
            let mut total_played = Vec::new();

            if with_media {
                let (mut producer, consumer) =
                    RingBuffer::new(speech_samples.len() * 2 + 10 * TICK);
                for s in &speech_samples {
                    let _ = producer.push(*s);
                    let _ = producer.push(*s);
                }
                bob.set_local_media(Some(consumer));
            }

            for (index, _) in speech_samples.as_chunks::<TICK>().0.iter().enumerate() {
                let mut played = vec![0.0; TICK];
                bob.play(
                    start + Duration::from_millis(index as u64 * 10),
                    &mut played,
                );
                if with_media {
                    total_played.extend_from_slice(&played);
                }
                room.push_back(played.iter().map(|s| s * 0.316).collect());
                let heard = room.pop_front().unwrap();
                bob.capture(&heard, |_| {
                    if index >= 100 {
                        sent.push(index);
                    }
                });
            }
            (sent, dbfs(&total_played))
        };

        let (without_canceller, level_without) = sent_while_local_media(false, true);
        let (with_canceller, level_with) = sent_while_local_media(true, true);

        // Output contains the samples
        assert!(level_without > -30.0, "level_without was {level_without}");
        assert!(level_with > -30.0, "level_with was {level_with}");

        // Without canceller, the speaker bleeding sends frames; with canceller, echo is removed
        assert!(
            without_canceller.len() > 100,
            "expected echo without canceller, got {}",
            without_canceller.len()
        );
        assert!(
            with_canceller.iter().all(|tick| *tick < 150),
            "local media echo was sent after adaptation at {with_canceller:?}"
        );
    }
}
