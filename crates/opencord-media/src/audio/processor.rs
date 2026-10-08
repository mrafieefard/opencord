//! The audio processing tick (plan §7.1): device samples in, Opus frames
//! out; Opus packets in, device samples out. The engine's thread runs it;
//! nothing here touches a device, so tests drive it directly.

use std::sync::Arc;
use std::time::Instant;

use super::TICK;
use super::capture::{Capture, EncodedFrame, InputMode};
use super::codec::CodecError;
use super::convert::{ConvertError, FromDevice, ToDevice};
use super::playback::{MAX_VOLUME, Playback};

/// What the listener and speaker chose.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProcessorSettings {
    pub mode: InputMode,
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
}

pub struct Processor {
    capture: Capture,
    playback: Playback,
    from_device: FromDevice,
    to_device: ToDevice,
    output_channels: usize,
    input_volume: f32,
    muted: bool,
    deafened: bool,
    tick: [f32; TICK],
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
        Ok(Self {
            capture: Capture::new(settings.mode, settings.bitrate)?,
            playback,
            from_device: FromDevice::new(input.0, input.1)?,
            to_device: ToDevice::new(output.0, output.1)?,
            output_channels: usize::from(output.1.max(1)),
            input_volume: settings.input_volume.clamp(0.0, MAX_VOLUME),
            muted: false,
            deafened: false,
            tick: [0.0; TICK],
        })
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
    pub fn capture(&mut self, interleaved: &[f32], mut send: impl FnMut(EncodedFrame)) {
        self.from_device.push(interleaved);
        while self.from_device.pop_tick(&mut self.tick) {
            if self.input_volume != 1.0 {
                for sample in &mut self.tick {
                    *sample *= self.input_volume;
                }
            }
            if let Some(frame) = self.capture.push(&self.tick) {
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
        let wanted = interleaved.len() / self.output_channels;
        while self.to_device.buffered() < wanted {
            self.playback.tick(now, &mut self.tick);
            self.to_device.push_tick(&self.tick);
        }
        self.to_device.pop(interleaved)
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
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::audio::capture::InputMode;
    use crate::audio::{SAMPLE_RATE, TICK, dbfs};

    fn settings() -> ProcessorSettings {
        ProcessorSettings {
            mode: InputMode::VoiceActivity {
                threshold_dbfs: -50.0,
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
}
