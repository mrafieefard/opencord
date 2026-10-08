//! The receive side (plan §7.5): each person's packets through their own
//! jitter buffer and decoder, then the mixer. Volumes and local mutes are
//! the listener's choices and outlast people leaving and coming back.
//! Someone speaks from their first sound until 250 ms after their last;
//! while a priority speaker speaks, everyone else plays at 25 %.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Instant;

use super::codec::{CodecError, MAX_DECODED, VoiceDecoder};
use super::jitter::{JitterBuffer, Playout};
use super::mixer::Mixer;
use super::{TICK, dbfs};

/// The loudest a listener can set someone, or everything: 200 %.
pub const MAX_VOLUME: f32 = 2.0;
/// A decoded tick at least this loud is someone speaking.
const SPEAKING_DBFS: f32 = -60.0;
/// Speaking ends after 250 ms quieter than that.
const SPEAKING_ENDS_TICKS: u32 = 25;
/// Everyone else while a priority speaker speaks: −12 dB.
const DUCKED: f32 = 0.25;

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
    speaking: bool,
    /// Ticks since the last loud one.
    quiet_ticks: u32,
    /// The gain priority speakers left this voice at, last tick.
    duck: f32,
}

impl Voice {
    fn new() -> Result<Self, CodecError> {
        Ok(Self {
            jitter: JitterBuffer::new(),
            decoder: VoiceDecoder::new()?,
            pcm: VecDeque::with_capacity(MAX_DECODED + TICK),
            decoded: vec![0.0; MAX_DECODED].into_boxed_slice(),
            tick: [0.0; TICK],
            speaking: false,
            quiet_ticks: 0,
            duck: 1.0,
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
        if dbfs(&self.tick) >= SPEAKING_DBFS {
            self.speaking = true;
            self.quiet_ticks = 0;
        } else if self.speaking {
            self.quiet_ticks += 1;
            self.speaking = self.quiet_ticks < SPEAKING_ENDS_TICKS;
        }
    }

    /// Moves this voice to `gain` across the tick, so the change does not
    /// click.
    fn duck_to(&mut self, gain: f32) {
        let from = self.duck;
        if from == 1.0 && gain == 1.0 {
            return;
        }
        let steps = TICK as f32;
        for (index, sample) in self.tick.iter_mut().enumerate() {
            *sample *= from + (gain - from) * (index as f32 / steps);
        }
        self.duck = gain;
    }
}

pub struct Playback {
    voices: HashMap<i64, Voice>,
    listening: HashMap<i64, Listening>,
    /// Who speaks with priority now, as the node relays it.
    priority: HashSet<i64>,
    /// Who was last reported speaking.
    reported: HashSet<i64>,
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
            priority: HashSet::new(),
            reported: HashSet::new(),
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
        self.priority.remove(&user_id);
    }

    /// Whether `user_id` speaks as the priority speaker.
    pub fn set_priority(&mut self, user_id: i64, priority: bool) {
        if priority {
            self.priority.insert(user_id);
        } else {
            self.priority.remove(&user_id);
        }
    }

    /// Adds to `changes` who started or stopped speaking since the last
    /// call.
    pub fn speaking_changes(&mut self, changes: &mut Vec<(i64, bool)>) {
        for (user_id, voice) in &self.voices {
            if voice.speaking != self.reported.contains(user_id) {
                changes.push((*user_id, voice.speaking));
            }
        }
        for (user_id, speaking) in changes.iter() {
            if *speaking {
                self.reported.insert(*user_id);
            } else {
                self.reported.remove(user_id);
            }
        }
        let voices = &self.voices;
        self.reported.retain(|user_id| {
            let here = voices.contains_key(user_id);
            if !here {
                changes.push((*user_id, false));
            }
            here
        });
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
        let priority = &self.priority;
        let ducking = self
            .voices
            .iter()
            .any(|(user_id, voice)| voice.speaking && priority.contains(user_id));
        for (user_id, voice) in &mut self.voices {
            let ducked = ducking && !priority.contains(user_id);
            voice.duck_to(if ducked { DUCKED } else { 1.0 });
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

    /// Like `listen`, with who started or stopped speaking at each tick.
    fn watch(
        playback: &mut Playback,
        mut arrivals: Vec<Arrival>,
        start: Instant,
        ticks: usize,
    ) -> Vec<(f32, Vec<(i64, bool)>)> {
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
                let mut changes = Vec::new();
                playback.speaking_changes(&mut changes);
                (dbfs(&out), changes)
            })
            .collect()
    }

    fn ticks_with(watched: &[(f32, Vec<(i64, bool)>)], change: (i64, bool)) -> Vec<usize> {
        (0..watched.len())
            .filter(|tick| watched[*tick].1.contains(&change))
            .collect()
    }

    #[test]
    fn someone_speaks_from_their_first_sound_until_250_ms_after_the_last() {
        let start = Instant::now();

        let watched = watch(
            &mut Playback::new(),
            talk(1, 300.0, 25, start, &[]),
            start,
            150,
        );

        let sounding: Vec<usize> = (0..watched.len())
            .filter(|tick| watched[*tick].0 >= -60.0)
            .collect();
        let started = ticks_with(&watched, (1, true));
        let stopped = ticks_with(&watched, (1, false));
        assert_eq!(started, vec![sounding[0]]);
        assert_eq!(stopped, vec![sounding.last().unwrap() + 25]);
    }

    #[test]
    fn only_who_changed_is_listed() {
        let start = Instant::now();
        let mut both = talk(1, 300.0, 75, start, &[]);
        both.extend(talk(2, 700.0, 25, start + Duration::from_millis(500), &[]));

        let watched = watch(&mut Playback::new(), both, start, 100);

        let second = ticks_with(&watched, (2, true));
        assert_eq!(second.len(), 1);
        assert_eq!(watched[second[0]].1, vec![(2, true)]);
    }

    #[test]
    fn someone_muted_locally_still_shows_speaking() {
        let start = Instant::now();
        let mut playback = Playback::new();
        playback.set_local_mute(1, true);

        let watched = watch(&mut playback, talk(1, 300.0, 25, start, &[]), start, 50);

        assert_eq!(ticks_with(&watched, (1, true)).len(), 1);
    }

    #[test]
    fn someone_who_leaves_while_speaking_stops_speaking() {
        let start = Instant::now();
        let mut playback = Playback::new();
        watch(&mut playback, talk(1, 300.0, 50, start, &[]), start, 20);

        playback.remove(1);
        let mut changes = Vec::new();
        playback.speaking_changes(&mut changes);

        assert_eq!(changes, vec![(1, false)]);
    }

    #[test]
    fn a_priority_speaker_lowers_everyone_else_to_a_quarter() {
        let start = Instant::now();
        let heard = |priority: bool| {
            let mut playback = Playback::new();
            // Only the other person is heard, to measure them.
            playback.set_local_mute(1, true);
            playback.set_priority(1, priority);
            let mut both = talk(1, 300.0, 50, start, &[]);
            both.extend(talk(2, 700.0, 50, start, &[]));
            level(&listen(&mut playback, both, start, 100))
        };

        let ducked = heard(true);
        let plain = heard(false);

        assert!(
            (plain - ducked - 12.0).abs() < 1.0,
            "{ducked} against {plain}"
        );
    }

    #[test]
    fn everyone_comes_back_up_when_the_priority_speaker_stops() {
        let start = Instant::now();
        let mut playback = Playback::new();
        playback.set_local_mute(1, true);
        playback.set_priority(1, true);
        let mut both = talk(1, 300.0, 25, start, &[]);
        both.extend(talk(2, 700.0, 75, start, &[]));

        let levels = listen(&mut playback, both, start, 160);
        let plain = listen(
            &mut Playback::new(),
            talk(2, 700.0, 75, start, &[]),
            start,
            160,
        );

        // A second after the priority speaker's last word.
        let after: f32 = levels[100..140].iter().sum::<f32>() / 40.0;
        let expected: f32 = plain[100..140].iter().sum::<f32>() / 40.0;
        assert!((after - expected).abs() < 0.5, "{after} against {expected}");
    }
}
