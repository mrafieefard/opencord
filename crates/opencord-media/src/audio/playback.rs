//! The receive side (plan §7.5): each person's packets through their own
//! jitter buffer and decoder, then the mixer. Volumes and local mutes are
//! the listener's choices and outlast people leaving and coming back.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::Instant;

use super::TICK;
use super::codec::{CodecError, MAX_DECODED, VoiceDecoder};
use super::jitter::{JitterBuffer, Playout};
use super::mixer::Mixer;

/// The loudest a listener can set someone, or everything: 200 %.
pub const MAX_VOLUME: f32 = 2.0;

/// How the listener wants to hear someone.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Listening {
    volume: f32,
    muted: bool,
}

impl Default for Listening {
    fn default() -> Self {
        Self {
            volume: 1.0,
            muted: false,
        }
    }
}

/// Someone being heard.
struct Voice {
    jitter: JitterBuffer,
    decoder: VoiceDecoder,
    /// Decoded, not yet played.
    pcm: VecDeque<f32>,
    decoded: Box<[f32]>,
    /// This tick's samples.
    tick: [f32; TICK],
}

impl Voice {
    fn new() -> Result<Self, CodecError> {
        Ok(Self {
            jitter: JitterBuffer::new(),
            decoder: VoiceDecoder::new()?,
            pcm: VecDeque::with_capacity(MAX_DECODED + TICK),
            decoded: vec![0.0; MAX_DECODED].into_boxed_slice(),
            tick: [0.0; TICK],
        })
    }

    /// Decodes until the next tick is there, or silence.
    fn next_tick(&mut self, now: Instant) {
        while self.pcm.len() < TICK {
            let decoded = match self.jitter.next_frame(now) {
                Playout::Frame(packet) => self.decoder.decode(&packet, &mut self.decoded),
                Playout::Recover(next) => self.decoder.recover(&next, &mut self.decoded),
                Playout::Conceal => self.decoder.conceal(&mut self.decoded),
                Playout::Silence => break,
            };
            // A packet Opus cannot read is a lost one.
            let samples = decoded
                .or_else(|_| self.decoder.conceal(&mut self.decoded))
                .unwrap_or(0);
            if samples == 0 {
                break;
            }
            self.pcm.extend(&self.decoded[..samples]);
        }
        let ready = self.pcm.len().min(TICK);
        for (slot, sample) in self.tick.iter_mut().zip(self.pcm.drain(..ready)) {
            *slot = sample;
        }
        self.tick[ready..].fill(0.0);
    }
}

pub struct Playback {
    voices: HashMap<i64, Voice>,
    listening: HashMap<i64, Listening>,
    mixer: Mixer,
    master: f32,
    deafened: bool,
}

impl Default for Playback {
    fn default() -> Self {
        Self::new()
    }
}

impl Playback {
    pub fn new() -> Self {
        Self {
            voices: HashMap::new(),
            listening: HashMap::new(),
            mixer: Mixer::new(),
            master: 1.0,
            deafened: false,
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
        let voice = match self.voices.entry(user_id) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => match Voice::new() {
                Ok(voice) => entry.insert(voice),
                Err(_) => return,
            },
        };
        voice.jitter.insert(timestamp, marker, payload, arrived);
    }

    /// `user_id` left: their audio stops; the listener's choices stay.
    pub fn remove(&mut self, user_id: i64) {
        self.voices.remove(&user_id);
    }

    /// 0–2 (200 %).
    pub fn set_volume(&mut self, user_id: i64, volume: f32) {
        self.listening.entry(user_id).or_default().volume = volume.clamp(0.0, MAX_VOLUME);
    }

    pub fn set_local_mute(&mut self, user_id: i64, muted: bool) {
        self.listening.entry(user_id).or_default().muted = muted;
    }

    /// 0–2 (200 %).
    pub fn set_master(&mut self, volume: f32) {
        self.master = volume.clamp(0.0, MAX_VOLUME);
    }

    pub fn set_deafened(&mut self, deafened: bool) {
        self.deafened = deafened;
    }

    /// The next 10 ms of everything heard, at 48 kHz mono.
    pub fn tick(&mut self, now: Instant, out: &mut [f32]) {
        for voice in self.voices.values_mut() {
            voice.next_tick(now);
        }
        if self.deafened {
            out.fill(0.0);
            return;
        }
        let listening = &self.listening;
        let sources = self.voices.iter().map(|(user_id, voice)| {
            let choice = listening.get(user_id).copied().unwrap_or_default();
            let volume = if choice.muted { 0.0 } else { choice.volume };
            (&voice.tick[..], volume)
        });
        self.mixer.mix(sources, self.master, out);
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::audio::codec::{MAX_PACKET, VoiceEncoder};
    use crate::audio::{FRAME, SAMPLE_RATE, SILENT_DBFS, dbfs};

    /// A packet and when it arrives.
    struct Arrival {
        at: Instant,
        user_id: i64,
        timestamp: u32,
        marker: bool,
        payload: Arc<[u8]>,
    }

    fn tone(frequency: f32, frame: usize) -> Vec<f32> {
        (0..FRAME)
            .map(|i| {
                let t = (frame * FRAME + i) as f32 / SAMPLE_RATE as f32;
                0.2 * (std::f32::consts::TAU * frequency * t).sin()
            })
            .collect()
    }

    /// `frames` packets of a tone from `user_id`, on time from `start`,
    /// except the frames in `lost`.
    fn talk(
        user_id: i64,
        frequency: f32,
        frames: usize,
        start: Instant,
        lost: &[usize],
    ) -> Vec<Arrival> {
        let mut encoder = VoiceEncoder::new(64_000).unwrap();
        let mut packet = [0u8; MAX_PACKET];
        (0..frames)
            .filter_map(|frame| {
                let size = encoder
                    .encode(&tone(frequency, frame), &mut packet)
                    .unwrap();
                (!lost.contains(&frame)).then(|| Arrival {
                    at: start + Duration::from_millis(frame as u64 * 20),
                    user_id,
                    timestamp: (frame * FRAME) as u32,
                    marker: frame == 0,
                    payload: Arc::from(&packet[..size]),
                })
            })
            .collect()
    }

    /// Plays `ticks` ticks from `start`, delivering packets as they arrive;
    /// returns each tick's level.
    fn listen(
        playback: &mut Playback,
        mut arrivals: Vec<Arrival>,
        start: Instant,
        ticks: usize,
    ) -> Vec<f32> {
        arrivals.sort_by_key(|arrival| arrival.at);
        let mut arrivals = arrivals.into_iter().peekable();
        let mut out = [0.0; TICK];
        (0..ticks)
            .map(|index| {
                let now = start + Duration::from_millis(index as u64 * 10);
                while let Some(arrival) = arrivals.next_if(|arrival| arrival.at <= now) {
                    playback.receive(
                        arrival.user_id,
                        arrival.timestamp,
                        arrival.marker,
                        arrival.payload,
                        arrival.at,
                    );
                }
                playback.tick(now, &mut out);
                dbfs(&out)
            })
            .collect()
    }

    /// The level over ticks 20–80, while everyone talks.
    fn level(levels: &[f32]) -> f32 {
        let power: f32 = levels[20..80].iter().map(|db| 10f32.powf(db / 10.0)).sum();
        10.0 * (power / 60.0).log10()
    }

    #[test]
    fn two_people_are_heard_together() {
        let start = Instant::now();
        let mut both = talk(1, 300.0, 50, start, &[]);
        both.extend(talk(2, 700.0, 50, start, &[]));

        let together = level(&listen(&mut Playback::new(), both, start, 100));
        let alone = level(&listen(
            &mut Playback::new(),
            talk(1, 300.0, 50, start, &[]),
            start,
            100,
        ));

        assert!(alone > -25.0, "{alone}");
        assert!(together > alone + 2.0, "{together} against {alone}");
    }

    #[test]
    fn a_local_mute_silences_one_person() {
        let start = Instant::now();
        let mut playback = Playback::new();
        playback.set_local_mute(2, true);
        let mut both = talk(1, 300.0, 50, start, &[]);
        both.extend(talk(2, 700.0, 50, start, &[]));

        let heard = level(&listen(&mut playback, both, start, 100));
        let alone = level(&listen(
            &mut Playback::new(),
            talk(1, 300.0, 50, start, &[]),
            start,
            100,
        ));

        assert!((heard - alone).abs() < 0.5, "{heard} against {alone}");
    }

    #[test]
    fn a_persons_volume_applies_and_lasts_after_they_leave() {
        let start = Instant::now();
        let mut playback = Playback::new();
        playback.set_volume(1, 2.0);
        listen(&mut playback, talk(1, 300.0, 50, start, &[]), start, 100);
        playback.remove(1);
        let later = start + Duration::from_secs(2);

        let doubled = level(&listen(
            &mut playback,
            talk(1, 300.0, 50, later, &[]),
            later,
            100,
        ));
        let plain = level(&listen(
            &mut Playback::new(),
            talk(1, 300.0, 50, later, &[]),
            later,
            100,
        ));

        assert!(
            (doubled - plain - 6.0).abs() < 1.0,
            "{doubled} against {plain}"
        );
    }

    #[test]
    fn deafened_hears_nothing() {
        let start = Instant::now();
        let mut playback = Playback::new();
        playback.set_deafened(true);

        let levels = listen(&mut playback, talk(1, 300.0, 50, start, &[]), start, 100);

        assert!(levels.iter().all(|db| *db == SILENT_DBFS));
    }

    #[test]
    fn the_master_volume_applies() {
        let start = Instant::now();
        let mut playback = Playback::new();
        playback.set_master(0.5);

        let halved = level(&listen(
            &mut playback,
            talk(1, 300.0, 50, start, &[]),
            start,
            100,
        ));
        let plain = level(&listen(
            &mut Playback::new(),
            talk(1, 300.0, 50, start, &[]),
            start,
            100,
        ));

        assert!(
            (plain - halved - 6.0).abs() < 1.0,
            "{halved} against {plain}"
        );
    }

    #[test]
    fn lost_packets_are_filled_in() {
        let start = Instant::now();
        let lost = [5, 15, 25, 35, 45];

        let levels = listen(
            &mut Playback::new(),
            talk(1, 300.0, 50, start, &lost),
            start,
            100,
        );

        // Opus fades back in over a frame or two after a concealed frame, so
        // a gap here means near silence, not a quieter moment.
        let gaps: Vec<usize> = (4..95).filter(|tick| levels[*tick] < -60.0).collect();
        assert!(gaps.is_empty(), "silent ticks: {gaps:?}");
    }
}
