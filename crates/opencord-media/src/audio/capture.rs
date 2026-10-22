//! The send side (plan §7.2, §7.4): the microphone gate, then Opus. Voice
//! activity opens on a voice (automatic sensitivity: RNNoise's voice
//! probability) or above a level (manual), and stays open for 250 ms after;
//! push-to-talk opens while the key is held and for a release delay. The
//! priority speaker key opens it too, and marks the talk as priority. A talk
//! spurt starts with 20 ms of audio from before the gate opened, so the
//! start of a word is not cut, and its first frame is marked. Speaking
//! while muted is noticed, for the "You're muted" reminder.

use std::time::Duration;

use super::codec::{CodecError, MAX_PACKET, VoiceEncoder, is_dtx};
use super::{FRAME, TICK, dbfs};

/// How long voice activity keeps the gate open after the level drops.
pub const HANGOVER: Duration = Duration::from_millis(250);
/// Ticks from before the gate opened that start a talk spurt: 20 ms.
pub const PRE_ROLL_TICKS: usize = 2;
const TICK_DURATION: Duration = Duration::from_millis(10);

/// The level manual sensitivity starts at, and what automatic falls back
/// to while there is no voice probability.
pub const DEFAULT_THRESHOLD_DBFS: f32 = -45.0;
/// A voice probability at or above this is a voice.
const VOICE_PROBABILITY: f32 = 0.5;
/// Muted, half a second of voice is speaking (the reminder, plan §12).
const MUTED_SPEECH_TICKS: u32 = 50;
/// After a reminder, the next one waits at least 30 s.
const REMINDER_PAUSE_TICKS: u32 = 3_000;

/// How voice activity decides someone is speaking.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sensitivity {
    /// RNNoise's voice probability, whatever the level.
    Automatic,
    /// The level reaches the threshold.
    Manual { threshold_dbfs: f32 },
}

/// What processing could tell about a microphone tick.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Heard {
    /// How likely it is to be speech, from 0 to 1, when RNNoise looked.
    pub voice_probability: Option<f32>,
    /// Nothing but what is left of the echo of what was just played.
    pub echo_only: bool,
}

/// How the microphone opens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputMode {
    /// While there is a voice, and for the hangover.
    VoiceActivity(Sensitivity),
    /// While the key is held, and for the release delay.
    PushToTalk { release_delay: Duration },
}

impl InputMode {
    /// Whether this mode needs each tick's voice probability.
    pub fn wants_voice_probability(self) -> bool {
        self == Self::VoiceActivity(Sensitivity::Automatic)
    }
}

/// Decides, tick by tick, whether the microphone is open.
#[derive(Debug, Clone)]
pub struct Gate {
    mode: InputMode,
    held: bool,
    priority_held: bool,
    /// Ticks the gate stays open without a new reason to.
    open_for: u32,
    /// Ticks the talk stays priority after the key is let go.
    priority_for: u32,
}

impl Gate {
    pub fn new(mode: InputMode) -> Self {
        Self {
            mode,
            held: false,
            priority_held: false,
            open_for: 0,
            priority_for: 0,
        }
    }

    pub fn set_mode(&mut self, mode: InputMode) {
        self.mode = mode;
        self.open_for = 0;
    }

    pub fn set_push_to_talk(&mut self, held: bool) {
        self.held = held;
    }

    /// The priority speaker key: it opens the gate like push-to-talk, in
    /// either mode.
    pub fn set_priority_held(&mut self, held: bool) {
        self.priority_held = held;
    }

    /// Whether the priority key is held, or was let go within its delay.
    pub fn is_priority(&self) -> bool {
        self.priority_held || self.priority_for > 0
    }

    /// Whether the gate is open for a tick at `level_dbfs`. Voice activity
    /// never opens on the echo alone.
    pub fn tick(&mut self, level_dbfs: f32, heard: Heard) -> bool {
        // The priority key lets go after push-to-talk's release delay, or
        // voice activity's hangover.
        let priority_release = match self.mode {
            InputMode::PushToTalk { release_delay } => release_delay,
            InputMode::VoiceActivity(_) => HANGOVER,
        };
        if self.priority_held {
            self.priority_for = ticks(priority_release);
            self.open_for = self.open_for.max(ticks(priority_release));
            return true;
        }
        self.priority_for = self.priority_for.saturating_sub(1);
        let (reason, then) = match self.mode {
            InputMode::VoiceActivity(Sensitivity::Automatic) => {
                let voice = match heard.voice_probability {
                    Some(probability) => probability >= VOICE_PROBABILITY,
                    None => level_dbfs >= DEFAULT_THRESHOLD_DBFS,
                };
                (voice && !heard.echo_only, HANGOVER)
            }
            InputMode::VoiceActivity(Sensitivity::Manual { threshold_dbfs }) => {
                (level_dbfs >= threshold_dbfs && !heard.echo_only, HANGOVER)
            }
            InputMode::PushToTalk { release_delay } => (self.held, release_delay),
        };
        if reason {
            self.open_for = ticks(then);
            true
        } else if self.open_for > 0 {
            self.open_for -= 1;
            true
        } else {
            false
        }
    }
}

fn ticks(duration: Duration) -> u32 {
    u32::try_from(duration.as_millis() / TICK_DURATION.as_millis()).unwrap_or(u32::MAX)
}

/// One Opus frame to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedFrame {
    pub payload: Vec<u8>,
    /// Its first sample on the capture clock (48 kHz, counting from 0). It
    /// keeps counting through silence, so receivers see the pauses.
    pub position: u64,
    /// The first frame of a talk spurt.
    pub marker: bool,
    /// -dBov, as the RFC 6464 header extension carries it.
    pub audio_level: i8,
}

/// Microphone ticks in, Opus frames out.
pub struct Capture {
    gate: Gate,
    encoder: VoiceEncoder,
    muted: bool,
    /// The last ticks, for the pre-roll; the newest is last.
    history: [[f32; TICK]; PRE_ROLL_TICKS],
    history_len: usize,
    frame: [f32; FRAME],
    frame_len: usize,
    frame_position: u64,
    /// The next tick's first sample.
    position: u64,
    talking: bool,
    mark_next: bool,
    packet: [u8; MAX_PACKET],
    /// Ticks in a row the gate would have opened while muted.
    muted_speech: u32,
    /// Ticks until another reminder may come.
    reminder_pause: u32,
    speaking_while_muted: bool,
}

impl Capture {
    pub fn new(mode: InputMode, bitrate: u32) -> Result<Self, CodecError> {
        Ok(Self {
            gate: Gate::new(mode),
            encoder: VoiceEncoder::new(bitrate)?,
            muted: false,
            history: [[0.0; TICK]; PRE_ROLL_TICKS],
            history_len: 0,
            frame: [0.0; FRAME],
            frame_len: 0,
            frame_position: 0,
            position: 0,
            talking: false,
            mark_next: false,
            packet: [0; MAX_PACKET],
            muted_speech: 0,
            reminder_pause: 0,
            speaking_while_muted: false,
        })
    }

    pub fn set_mode(&mut self, mode: InputMode) {
        self.gate.set_mode(mode);
    }

    pub fn set_push_to_talk(&mut self, held: bool) {
        self.gate.set_push_to_talk(held);
    }

    pub fn set_priority_held(&mut self, held: bool) {
        self.gate.set_priority_held(held);
    }

    /// Whether the talk going on is the priority speaker's.
    pub fn is_priority(&self) -> bool {
        self.talking && self.gate.is_priority()
    }

    /// Whether someone spoke while muted since the last call; at most once
    /// every 30 s.
    pub fn take_speaking_while_muted(&mut self) -> bool {
        std::mem::take(&mut self.speaking_while_muted)
    }

    /// Self mute and deafen: nothing is sent.
    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
    }

    pub fn set_bitrate(&mut self, bitrate: u32) -> Result<(), CodecError> {
        self.encoder.set_bitrate(bitrate)
    }

    pub fn set_expected_loss(&mut self, percent: u8) -> Result<(), CodecError> {
        self.encoder.set_expected_loss(percent)
    }

    /// Whether a talk spurt is going on.
    pub fn is_talking(&self) -> bool {
        self.talking
    }

    /// The next 10 ms of microphone audio, 48 kHz mono, and what processing
    /// heard in it; returns a frame to send when one is complete.
    pub fn push(&mut self, tick: &[f32], heard: Heard) -> Option<EncodedFrame> {
        debug_assert_eq!(tick.len(), TICK);
        let position = self.position;
        self.position += TICK as u64;
        let wanted = self.gate.tick(dbfs(tick), heard);
        self.notice_muted_speech(wanted);
        let open = !self.muted && wanted;
        let sent = match (self.talking, open) {
            (false, true) => self.start_talking(position, tick),
            (true, true) => self.append(tick),
            (true, false) => {
                self.talking = false;
                self.flush()
            }
            (false, false) => None,
        };
        self.remember(tick);
        sent
    }

    fn notice_muted_speech(&mut self, wanted: bool) {
        self.reminder_pause = self.reminder_pause.saturating_sub(1);
        if !(self.muted && wanted) {
            self.muted_speech = 0;
            return;
        }
        self.muted_speech += 1;
        if self.muted_speech == MUTED_SPEECH_TICKS && self.reminder_pause == 0 {
            self.speaking_while_muted = true;
            self.reminder_pause = REMINDER_PAUSE_TICKS;
        }
    }

    fn start_talking(&mut self, position: u64, tick: &[f32]) -> Option<EncodedFrame> {
        self.talking = true;
        self.mark_next = true;
        let _ = self.encoder.reset();
        self.frame_len = 0;
        self.frame_position = position - (self.history_len * TICK) as u64;
        let mut sent = None;
        for index in PRE_ROLL_TICKS - self.history_len..PRE_ROLL_TICKS {
            let earlier = self.history[index];
            sent = sent.or(self.append(&earlier));
        }
        sent.or(self.append(tick))
    }

    fn append(&mut self, tick: &[f32]) -> Option<EncodedFrame> {
        self.frame[self.frame_len..self.frame_len + TICK].copy_from_slice(tick);
        self.frame_len += TICK;
        if self.frame_len < FRAME {
            return None;
        }
        self.frame_len = 0;
        self.encode()
    }

    /// Ends a talk spurt: a half-full frame is padded with silence.
    fn flush(&mut self) -> Option<EncodedFrame> {
        if self.frame_len == 0 {
            return None;
        }
        self.frame[self.frame_len..].fill(0.0);
        self.frame_len = 0;
        self.encode()
    }

    fn encode(&mut self) -> Option<EncodedFrame> {
        let position = self.frame_position;
        self.frame_position += FRAME as u64;
        let size = self.encoder.encode(&self.frame, &mut self.packet).ok()?;
        if is_dtx(size) {
            return None;
        }
        Some(EncodedFrame {
            payload: self.packet[..size].to_vec(),
            position,
            marker: std::mem::take(&mut self.mark_next),
            audio_level: dbfs(&self.frame).clamp(-127.0, 0.0) as i8,
        })
    }

    fn remember(&mut self, tick: &[f32]) {
        self.history.rotate_left(1);
        self.history[PRE_ROLL_TICKS - 1].copy_from_slice(tick);
        self.history_len = (self.history_len + 1).min(PRE_ROLL_TICKS);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::audio::{SAMPLE_RATE, TICK};

    const LOUD: f32 = 0.3;
    const QUIET: f32 = 0.0005;

    fn tick(amplitude: f32, index: usize) -> Vec<f32> {
        (0..TICK)
            .map(|i| {
                let t = (index * TICK + i) as f32 / SAMPLE_RATE as f32;
                amplitude * (std::f32::consts::TAU * 300.0 * t).sin()
            })
            .collect()
    }

    fn voice_activity() -> Capture {
        Capture::new(
            InputMode::VoiceActivity(Sensitivity::Manual {
                threshold_dbfs: -40.0,
            }),
            64_000,
        )
        .unwrap()
    }

    /// Pushes `amplitudes` as ticks; returns what was sent, by tick index.
    fn probability(voice_probability: f32) -> Heard {
        Heard {
            voice_probability: Some(voice_probability),
            echo_only: false,
        }
    }

    fn run(capture: &mut Capture, amplitudes: &[f32]) -> Vec<(usize, EncodedFrame)> {
        amplitudes
            .iter()
            .enumerate()
            .filter_map(|(index, amplitude)| {
                capture
                    .push(&tick(*amplitude, index), Heard::default())
                    .map(|frame| (index, frame))
            })
            .collect()
    }

    #[test]
    fn nothing_is_sent_while_it_is_quiet() {
        let mut capture = voice_activity();

        let sent = run(&mut capture, &[QUIET; 100]);

        assert!(sent.is_empty());
    }

    #[test]
    fn a_talk_spurt_starts_with_the_pre_roll_and_a_mark() {
        let mut capture = voice_activity();
        let mut amplitudes = vec![QUIET; 10];
        amplitudes.extend([LOUD; 10]);

        let sent = run(&mut capture, &amplitudes);

        let (_, first) = &sent[0];
        assert!(first.marker);
        assert_eq!(first.position, (10 - PRE_ROLL_TICKS) as u64 * TICK as u64);
        assert!(sent[1..].iter().all(|(_, frame)| !frame.marker));
        let positions: Vec<u64> = sent.iter().map(|(_, frame)| frame.position).collect();
        assert!(
            positions
                .windows(2)
                .all(|pair| pair[1] - pair[0] == FRAME as u64),
            "{positions:?}"
        );
    }

    #[test]
    fn voice_activity_stays_open_for_the_hangover() {
        let mut gate = Gate::new(InputMode::VoiceActivity(Sensitivity::Manual {
            threshold_dbfs: -40.0,
        }));
        let hangover_ticks = (HANGOVER.as_millis() / 10) as usize;

        let loud = gate.tick(-20.0, Heard::default());
        let after: Vec<bool> = (0..hangover_ticks + 5)
            .map(|_| gate.tick(-60.0, Heard::default()))
            .collect();

        assert!(loud);
        assert!(after[..hangover_ticks].iter().all(|open| *open));
        assert!(after[hangover_ticks..].iter().all(|open| !open));
    }

    #[test]
    fn a_talk_spurt_after_a_pause_is_marked_and_placed_after_it() {
        let mut capture = voice_activity();
        let mut amplitudes = vec![LOUD; 10];
        amplitudes.extend([QUIET; 60]);
        amplitudes.extend([LOUD; 4]);

        let sent = run(&mut capture, &amplitudes);

        let (_, second) = sent.iter().find(|(index, _)| *index >= 70).unwrap();
        assert!(second.marker);
        assert_eq!(second.position, (70 - PRE_ROLL_TICKS) as u64 * TICK as u64);
    }

    #[test]
    fn muted_sends_nothing() {
        let mut capture = voice_activity();
        capture.set_muted(true);

        let sent = run(&mut capture, &[LOUD; 50]);

        assert!(sent.is_empty());
    }

    #[test]
    fn push_to_talk_sends_while_held_and_for_the_release_delay() {
        let mut capture = Capture::new(
            InputMode::PushToTalk {
                release_delay: Duration::from_millis(100),
            },
            64_000,
        )
        .unwrap();

        let before = run(&mut capture, &[LOUD; 10]);
        capture.set_push_to_talk(true);
        let held = run(&mut capture, &[QUIET; 10]);
        capture.set_push_to_talk(false);
        let released = run(&mut capture, &[LOUD; 30]);

        assert!(before.is_empty());
        assert!(!held.is_empty());
        let after_release = released.last().unwrap().0;
        assert!(
            (9..=11).contains(&after_release),
            "sent until tick {after_release}"
        );
    }

    #[test]
    fn frames_carry_their_level() {
        let mut capture = voice_activity();

        let sent = run(&mut capture, &[LOUD; 10]);

        let level = sent.last().unwrap().1.audio_level;
        assert!((-16..=-8).contains(&level), "{level}");
    }

    #[test]
    fn automatic_sensitivity_follows_the_voice_probability() {
        let mut gate = Gate::new(InputMode::VoiceActivity(Sensitivity::Automatic));
        let hangover_ticks = (HANGOVER.as_millis() / 10) as usize;

        let loud_noise = gate.tick(-10.0, probability(0.1));
        let quiet_voice = gate.tick(-50.0, probability(0.9));
        let after: Vec<bool> = (0..hangover_ticks + 1)
            .map(|_| gate.tick(-50.0, probability(0.1)))
            .collect();

        assert!(!loud_noise);
        assert!(quiet_voice);
        assert!(after[..hangover_ticks].iter().all(|open| *open));
        assert!(!after[hangover_ticks]);
    }

    #[test]
    fn automatic_sensitivity_without_a_probability_goes_by_the_default_level() {
        let mut gate = Gate::new(InputMode::VoiceActivity(Sensitivity::Automatic));

        let quiet = gate.tick(DEFAULT_THRESHOLD_DBFS - 1.0, Heard::default());
        let loud = gate.tick(DEFAULT_THRESHOLD_DBFS, Heard::default());

        assert!(!quiet);
        assert!(loud);
    }

    #[test]
    fn the_priority_key_sends_even_when_quiet_and_marks_it() {
        let mut capture = voice_activity();
        let hangover_ticks = (HANGOVER.as_millis() / 10) as usize;

        capture.set_priority_held(true);
        let held = run(&mut capture, &[QUIET; 10]);
        let priority_while_held = capture.is_priority();
        capture.set_priority_held(false);
        let released = run(&mut capture, &[QUIET; 40]);

        assert!(!held.is_empty());
        assert!(priority_while_held);
        let after_release = released.last().unwrap().0;
        assert!(
            (hangover_ticks - 1..=hangover_ticks + 1).contains(&after_release),
            "sent until tick {after_release}"
        );
        assert!(!capture.is_priority());
    }

    #[test]
    fn talking_without_the_priority_key_is_not_priority() {
        let mut capture = voice_activity();

        run(&mut capture, &[LOUD; 10]);

        assert!(capture.is_talking());
        assert!(!capture.is_priority());
    }

    #[test]
    fn speaking_while_muted_is_noticed_once() {
        let mut capture = voice_activity();
        capture.set_muted(true);

        let sent = run(&mut capture, &[LOUD; 100]);
        let first = capture.take_speaking_while_muted();
        run(&mut capture, &[LOUD; 100]);
        let again = capture.take_speaking_while_muted();

        assert!(sent.is_empty());
        assert!(first);
        assert!(!again, "noticed again within the pause");
    }

    #[test]
    fn a_short_sound_while_muted_is_not_speaking() {
        let mut capture = voice_activity();
        capture.set_muted(true);

        let mut amplitudes = vec![LOUD; 10];
        amplitudes.extend([QUIET; 90]);
        run(&mut capture, &amplitudes);

        assert!(!capture.take_speaking_while_muted());
    }

    #[test]
    fn speaking_unmuted_is_not_noticed() {
        let mut capture = voice_activity();

        run(&mut capture, &[LOUD; 100]);

        assert!(!capture.take_speaking_while_muted());
    }

    #[test]
    fn the_echo_alone_never_opens_voice_activity() {
        let echo = Heard {
            voice_probability: Some(0.9),
            echo_only: true,
        };
        let mut automatic = Gate::new(InputMode::VoiceActivity(Sensitivity::Automatic));
        let mut manual = Gate::new(InputMode::VoiceActivity(Sensitivity::Manual {
            threshold_dbfs: -60.0,
        }));
        let mut push_to_talk = Gate::new(InputMode::PushToTalk {
            release_delay: Duration::ZERO,
        });
        push_to_talk.set_push_to_talk(true);

        assert!(!automatic.tick(-20.0, echo));
        assert!(!manual.tick(-20.0, echo));
        assert!(
            push_to_talk.tick(-20.0, echo),
            "holding the key still sends"
        );
    }
}
