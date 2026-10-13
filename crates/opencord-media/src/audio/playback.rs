//! The receive side (plan §7.5): each person's packets through their own
//! jitter buffer and decoder, then the mixer. Volumes and local mutes are
//! the listener's choices and outlast people leaving and coming back.
//! Someone speaks from their first sound until 250 ms after their last;
//! while a priority speaker speaks, everyone else plays at 25 %. A watched
//! screen share's sound (plan §10.5) is stereo, with its own jitter buffer
//! and its own volume and mute, apart from the streamer's voice; voices and
//! sounds play the same on both sides.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

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
/// One tick of playing.
const TICK_TIME: Duration = Duration::from_millis(10);

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

/// A screen share's sound being watched: stereo, interleaved.
struct Stream {
    jitter: JitterBuffer,
    decoder: VoiceDecoder,
    /// Decoded, not yet played.
    pcm: VecDeque<f32>,
    decoded: Box<[f32]>,
    /// This tick's samples.
    tick: [f32; 2 * TICK],
}

impl Stream {
    fn new() -> Result<Self, CodecError> {
        Ok(Self {
            jitter: JitterBuffer::new(),
            decoder: VoiceDecoder::stereo()?,
            pcm: VecDeque::with_capacity(2 * (MAX_DECODED + TICK)),
            decoded: vec![0.0; 2 * MAX_DECODED].into_boxed_slice(),
            tick: [0.0; 2 * TICK],
        })
    }

    /// Decodes until the next tick is there, or silence.
    fn next_tick(&mut self, now: Instant) {
        while self.pcm.len() < 2 * TICK {
            let decoded = match self.jitter.next_frame(now) {
                Playout::Frame(packet) => self.decoder.decode(&packet, &mut self.decoded),
                Playout::Recover(next) => self.decoder.recover(&next, &mut self.decoded),
                Playout::Conceal => self.decoder.conceal(&mut self.decoded),
                Playout::Silence => break,
            };
            let frames = decoded
                .or_else(|_| self.decoder.conceal(&mut self.decoded))
                .unwrap_or(0);
            if frames == 0 {
                break;
            }
            self.pcm.extend(&self.decoded[..2 * frames]);
        }
        let ready = self.pcm.len().min(2 * TICK);
        for (slot, sample) in self.tick.iter_mut().zip(self.pcm.drain(..ready)) {
            *slot = sample;
        }
        self.tick[ready..].fill(0.0);
    }
}

/// A soundboard sound playing (plan §11.5): mono at 48 kHz, mixed in with
/// the voices, so the echo canceller hears it too.
struct Sound {
    samples: Arc<[f32]>,
    position: usize,
    gain: f32,
    /// Its next 10 ms; silence past its end.
    tick: [f32; TICK],
}

impl Sound {
    fn next_tick(&mut self) {
        let start = self.position.min(self.samples.len());
        let end = (start + TICK).min(self.samples.len());
        let length = end - start;
        self.tick[..length].copy_from_slice(&self.samples[start..end]);
        self.tick[length..].fill(0.0);
        self.position += TICK;
    }

    fn playing(&self) -> bool {
        self.position < self.samples.len()
    }
}

pub struct Playback {
    voices: HashMap<i64, Voice>,
    /// Watched screen shares' sound, by streamer.
    streams: HashMap<i64, Stream>,
    stream_listening: HashMap<i64, Listening>,
    /// A mono tick of everything heard, for [`Self::tick`].
    stereo: [f32; 2 * TICK],
    /// Soundboard sounds, until they end.
    sounds: Vec<Sound>,
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
            streams: HashMap::new(),
            stream_listening: HashMap::new(),
            stereo: [0.0; 2 * TICK],
            sounds: Vec::new(),
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

    /// A packet of `user_id`'s screen share sound.
    pub fn receive_stream(
        &mut self,
        user_id: i64,
        timestamp: u32,
        marker: bool,
        payload: Arc<[u8]>,
        arrived: Instant,
    ) {
        let stream = match self.streams.entry(user_id) {
            std::collections::hash_map::Entry::Occupied(entry) => entry.into_mut(),
            std::collections::hash_map::Entry::Vacant(entry) => match Stream::new() {
                Ok(stream) => entry.insert(stream),
                Err(_) => return,
            },
        };
        stream.jitter.insert(timestamp, marker, payload, arrived);
    }

    /// How long after arriving each watched stream's sound plays: its
    /// jitter buffer's delay plus a tick. Its pictures wait as long, so
    /// they stay with it (plan §10.5).
    pub fn stream_delays(&self) -> impl Iterator<Item = (i64, Duration)> + '_ {
        self.streams
            .iter()
            .map(|(user_id, stream)| (*user_id, stream.jitter.target_delay() + TICK_TIME))
    }

    /// `user_id`'s screen share is no longer watched here.
    pub fn remove_stream(&mut self, user_id: i64) {
        self.streams.remove(&user_id);
    }

    /// `user_id` left: their audio stops; the listener's choices stay.
    pub fn remove(&mut self, user_id: i64) {
        self.voices.remove(&user_id);
        self.streams.remove(&user_id);
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

    /// How loud `user_id`'s screen share sound is, 0–2 (200 %), apart
    /// from their voice.
    pub fn set_stream_volume(&mut self, user_id: i64, volume: f32) {
        self.stream_listening.entry(user_id).or_default().volume = volume.clamp(0.0, MAX_VOLUME);
    }

    pub fn set_stream_mute(&mut self, user_id: i64, muted: bool) {
        self.stream_listening.entry(user_id).or_default().muted = muted;
    }

    /// 0–2 (200 %).
    pub fn set_master(&mut self, volume: f32) {
        self.master = volume.clamp(0.0, MAX_VOLUME);
    }

    pub fn set_deafened(&mut self, deafened: bool) {
        self.deafened = deafened;
    }

    /// Plays a soundboard sound, mono at 48 kHz, at `gain` (0–1).
    pub fn play_sound(&mut self, samples: Arc<[f32]>, gain: f32) {
        self.sounds.push(Sound {
            samples,
            position: 0,
            gain: gain.clamp(0.0, 1.0),
            tick: [0.0; TICK],
        });
    }

    pub fn stop_sounds(&mut self) {
        self.sounds.clear();
    }

    /// The next 10 ms of everything heard, at 48 kHz, both sides mixed.
    pub fn tick(&mut self, now: Instant, out: &mut [f32]) {
        let mut stereo = std::mem::replace(&mut self.stereo, [0.0; 2 * TICK]);
        self.tick_stereo(now, &mut stereo);
        for (slot, frame) in out.iter_mut().zip(stereo.as_chunks::<2>().0) {
            *slot = (frame[0] + frame[1]) / 2.0;
        }
        self.stereo = stereo;
    }

    /// The next 10 ms of everything heard, at 48 kHz, interleaved stereo
    /// (`2 * TICK` samples).
    pub fn tick_stereo(&mut self, now: Instant, out: &mut [f32]) {
        for voice in self.voices.values_mut() {
            voice.next_tick(now);
        }
        for stream in self.streams.values_mut() {
            stream.next_tick(now);
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
        // Sounds go on while deafened, like everyone's voice.
        for sound in &mut self.sounds {
            sound.next_tick();
        }
        if self.deafened {
            out.fill(0.0);
        } else {
            let listening = &self.listening;
            let voices = self.voices.iter().map(|(user_id, voice)| {
                let choice = listening.get(user_id).copied().unwrap_or_default();
                let volume = if choice.muted { 0.0 } else { choice.volume };
                (&voice.tick[..], volume)
            });
            let sounds = self
                .sounds
                .iter()
                .map(|sound| (&sound.tick[..], sound.gain));
            let stream_listening = &self.stream_listening;
            let streams = self.streams.iter().map(|(user_id, stream)| {
                let choice = stream_listening.get(user_id).copied().unwrap_or_default();
                let volume = if choice.muted { 0.0 } else { choice.volume };
                (&stream.tick[..], volume)
            });
            self.mixer
                .mix_stereo(voices.chain(sounds), streams, self.master, out);
        }
        self.sounds.retain(Sound::playing);
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

    /// The tick-by-tick peak of `ticks` ticks.
    fn peaks(playback: &mut Playback, ticks: usize) -> Vec<f32> {
        let now = Instant::now();
        let mut out = [0.0; TICK];
        (0..ticks)
            .map(|_| {
                playback.tick(now, &mut out);
                out.iter().fold(0f32, |peak, sample| peak.max(sample.abs()))
            })
            .collect()
    }

    #[test]
    fn a_sound_plays_at_its_gain_until_it_ends() {
        let mut playback = Playback::new();
        // 25 ms of a steady 0.5.
        playback.play_sound(Arc::from(vec![0.5; 1_200]), 0.4);

        let heard = peaks(&mut playback, 4);

        assert_eq!(heard, [0.2, 0.2, 0.2, 0.0]);
        assert!(playback.sounds.is_empty());
    }

    /// How strongly `frequency` is in `samples` (Goertzel), as an amplitude.
    fn strength(samples: &[f32], frequency: f32) -> f32 {
        let coefficient = 2.0 * (std::f32::consts::TAU * frequency / SAMPLE_RATE as f32).cos();
        let (mut previous, mut before) = (0.0f32, 0.0f32);
        for sample in samples {
            let current = sample + coefficient * previous - before;
            before = previous;
            previous = current;
        }
        let power = previous * previous + before * before - coefficient * previous * before;
        2.0 * power.max(0.0).sqrt() / samples.len() as f32
    }

    #[test]
    fn someone_talking_during_a_sound_is_heard_together_with_it() {
        let start = Instant::now();
        let mut playback = Playback::new();
        let sound: Vec<f32> = (0..2 * SAMPLE_RATE as usize)
            .map(|n| 0.2 * (std::f32::consts::TAU * 1_000.0 * n as f32 / SAMPLE_RATE as f32).sin())
            .collect();
        playback.play_sound(Arc::from(sound), 1.0);
        let mut arrivals = talk(1, 300.0, 50, start, &[]).into_iter().peekable();

        let mut heard = Vec::new();
        let mut out = [0.0; TICK];
        for index in 0..80 {
            let now = start + Duration::from_millis(index * 10);
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
            if index >= 20 {
                heard.extend_from_slice(&out);
            }
        }

        let (voice, sound) = (strength(&heard, 300.0), strength(&heard, 1_000.0));
        let elsewhere = strength(&heard, 2_300.0);
        assert!(voice > 0.1, "the voice at {voice}");
        assert!(sound > 0.1, "the sound at {sound}");
        assert!(elsewhere < 0.01, "nothing else, {elsewhere}");
    }

    #[test]
    fn sounds_overlap_and_go_on_while_deafened() {
        let mut playback = Playback::new();
        playback.play_sound(Arc::from(vec![0.25; 4_800]), 1.0);
        playback.play_sound(Arc::from(vec![0.25; 2_400]), 1.0);

        let together = peaks(&mut playback, 2);
        playback.set_deafened(true);
        let deafened = peaks(&mut playback, 3);
        playback.set_deafened(false);
        let after = peaks(&mut playback, 6);

        assert_eq!(together, [0.5, 0.5]);
        assert_eq!(deafened, [0.0; 3]);
        // The short one ended while deafened; the long one plays on to its
        // end at 100 ms.
        assert_eq!(after, [0.25, 0.25, 0.25, 0.25, 0.25, 0.0]);
    }

    /// `user_id`'s screen share sound: 1 kHz left, 2.4 kHz right, as
    /// packets 20 ms apart from `start`.
    fn stream_packets(start: Instant, count: usize) -> Vec<(u32, Arc<[u8]>, Instant)> {
        let mut encoder = opus::Encoder::new(
            SAMPLE_RATE,
            opus::Channels::Stereo,
            opus::Application::Audio,
        )
        .unwrap();
        let mut packet = [0u8; MAX_PACKET];
        (0..count)
            .map(|index| {
                let pcm: Vec<f32> = (0..FRAME)
                    .flat_map(|n| {
                        let t = (index * FRAME + n) as f32 / SAMPLE_RATE as f32;
                        [
                            0.3 * (std::f32::consts::TAU * 1000.0 * t).sin(),
                            0.3 * (std::f32::consts::TAU * 2400.0 * t).sin(),
                        ]
                    })
                    .collect();
                let size = encoder.encode_float(&pcm, &mut packet).unwrap();
                (
                    (index * FRAME) as u32,
                    Arc::from(&packet[..size]),
                    start + Duration::from_millis(index as u64 * 20),
                )
            })
            .collect()
    }

    /// The level of `frequency` in one side of interleaved stereo.
    fn side_level(stereo: &[f32], side: usize, frequency: f32) -> f32 {
        let samples: Vec<f32> = stereo.iter().skip(side).step_by(2).copied().collect();
        let coefficient = 2.0 * (std::f32::consts::TAU * frequency / SAMPLE_RATE as f32).cos();
        let (mut s1, mut s2) = (0f32, 0f32);
        for sample in &samples {
            (s1, s2) = (sample + coefficient * s1 - s2, s1);
        }
        let power = (s1 * s1 + s2 * s2 - coefficient * s1 * s2).max(0.0);
        20.0 * (2.0 * power.sqrt() / samples.len() as f32)
            .max(1e-10)
            .log10()
    }

    /// Plays 100 packets of a stream from user 9 (and their voice, a
    /// 440 Hz tone) through `setup`; returns the last 1.5 s in stereo.
    fn hear_stream(setup: impl FnOnce(&mut Playback)) -> Vec<f32> {
        let mut playback = Playback::new();
        setup(&mut playback);
        let start = Instant::now();
        let packets = stream_packets(start, 100);
        let mut voice = VoiceEncoder::new(64_000).unwrap();
        let mut packet = [0u8; MAX_PACKET];
        let mut out = Vec::new();
        let mut stereo = [0.0; 2 * TICK];
        for tick in 0..200 {
            let now = start + Duration::from_millis(tick * 10);
            if tick % 2 == 0 {
                let (timestamp, payload, arrived) = packets[tick as usize / 2].clone();
                playback.receive_stream(9, timestamp, false, payload, arrived);
                let frame = tick as usize / 2;
                let tone: Vec<f32> = (0..FRAME)
                    .map(|n| {
                        let t = (frame * FRAME + n) as f32 / SAMPLE_RATE as f32;
                        0.2 * (std::f32::consts::TAU * 440.0 * t).sin()
                    })
                    .collect();
                let size = voice.encode(&tone, &mut packet).unwrap();
                playback.receive(9, timestamp, false, Arc::from(&packet[..size]), arrived);
            }
            playback.tick_stereo(now, &mut stereo);
            if tick >= 50 {
                out.extend_from_slice(&stereo);
            }
        }
        out
    }

    #[test]
    fn a_watched_stream_plays_in_stereo_at_its_own_volume_apart_from_the_voice() {
        let normal = hear_stream(|_| {});
        let half = hear_stream(|playback| playback.set_stream_volume(9, 0.5));
        let voice_muted = hear_stream(|playback| playback.set_local_mute(9, true));
        let stream_muted = hear_stream(|playback| playback.set_stream_mute(9, true));
        let deafened = hear_stream(|playback| playback.set_deafened(true));

        // The sides stay apart; the voice is on both.
        assert!(side_level(&normal, 0, 1000.0) > -20.0);
        assert!(side_level(&normal, 0, 2400.0) < -50.0);
        assert!(side_level(&normal, 1, 2400.0) > -20.0);
        assert!(side_level(&normal, 1, 1000.0) < -50.0);
        assert!(side_level(&normal, 0, 440.0) > -25.0 && side_level(&normal, 1, 440.0) > -25.0);
        let halved = side_level(&half, 0, 1000.0) - side_level(&normal, 0, 1000.0);
        assert!((halved + 6.0).abs() < 0.5, "half volume: {halved} dB");
        assert!((side_level(&half, 0, 440.0) - side_level(&normal, 0, 440.0)).abs() < 0.5);
        // Muting their voice keeps their stream, and the other way round.
        assert!(side_level(&voice_muted, 0, 440.0) < -60.0);
        assert!(side_level(&voice_muted, 0, 1000.0) > -20.0);
        assert!(side_level(&stream_muted, 0, 1000.0) < -60.0);
        assert!(side_level(&stream_muted, 0, 440.0) > -25.0);
        assert!(side_level(&deafened, 0, 1000.0) < -60.0);
    }

    #[test]
    fn a_stream_no_longer_watched_stops_and_its_volume_stays() {
        let mut playback = Playback::new();
        playback.set_stream_volume(9, 0.0);
        let start = Instant::now();
        for (timestamp, payload, arrived) in stream_packets(start, 10) {
            playback.receive_stream(9, timestamp, false, payload, arrived);
        }
        let delays: Vec<(i64, Duration)> = playback.stream_delays().collect();
        playback.remove_stream(9);
        let mut stereo = [0.0; 2 * TICK];
        playback.tick_stereo(start + Duration::from_millis(200), &mut stereo);

        // Its pictures wait as long as the sound: the jitter buffer's least
        // delay and a tick.
        assert_eq!(delays, [(9, Duration::from_millis(30))]);
        assert!(playback.streams.is_empty());
        assert_eq!(playback.stream_delays().count(), 0);
        assert_eq!(playback.stream_listening[&9].volume, 0.0);
    }
}
