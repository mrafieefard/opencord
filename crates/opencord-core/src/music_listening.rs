//! How this person hears the channel's music (Phase 3 plan §19.8): one
//! volume for every server, mute, which also stops the voice node sending
//! it, and optionally the music 10 dB lower while people talk, back 300 ms
//! after the last voice.

use std::collections::HashSet;
use std::time::Duration;

use tokio::time::Instant;

/// −10 dB.
const DUCKED: f32 = 0.316;
const RELEASE: Duration = Duration::from_millis(300);

/// What the person chose; kept on this device, never sent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MusicListening {
    /// 0–2 (200 %).
    pub volume: f32,
    pub muted: bool,
    /// Lower the music while people talk.
    pub duck: bool,
}

impl Default for MusicListening {
    fn default() -> Self {
        Self {
            volume: 0.5,
            muted: false,
            duck: false,
        }
    }
}

impl MusicListening {
    pub fn clamped(volume: f32, muted: bool, duck: bool) -> Self {
        Self {
            volume: if volume.is_finite() {
                volume.clamp(0.0, 2.0)
            } else {
                0.5
            },
            muted,
            duck,
        }
    }
}

/// Whether someone talks, for lowering the music.
#[derive(Debug, Default)]
pub struct MusicDuck {
    speaking: HashSet<i64>,
    talking: bool,
    lowered: bool,
    release_at: Option<Instant>,
}

impl MusicDuck {
    pub fn speaking(&mut self, user_id: i64, speaking: bool) {
        if speaking {
            self.speaking.insert(user_id);
        } else {
            self.speaking.remove(&user_id);
        }
    }

    pub fn talking(&mut self, talking: bool) {
        self.talking = talking;
    }

    pub fn clear(&mut self) {
        self.speaking.clear();
        self.talking = false;
    }

    pub fn release_at(&self) -> Option<Instant> {
        self.release_at
    }

    /// The music's gain now, and whether it changed since the last call.
    pub fn update(&mut self, listening: MusicListening, now: Instant) -> (f32, bool) {
        let voices = listening.duck && (self.talking || !self.speaking.is_empty());
        let before = self.lowered;
        if voices {
            self.lowered = true;
            self.release_at = None;
        } else if self.lowered {
            let release_at = *self.release_at.get_or_insert(now + RELEASE);
            if !listening.duck || now >= release_at {
                self.lowered = false;
                self.release_at = None;
            }
        }
        let gain = listening.volume * if self.lowered { DUCKED } else { 1.0 };
        (gain, before != self.lowered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn music_is_lowered_while_people_talk_and_comes_back_after_300_ms() {
        let listening = MusicListening::clamped(1.5, false, true);
        let mut duck = MusicDuck::default();
        let start = Instant::now();
        assert_eq!(duck.update(listening, start), (1.5, false));
        duck.speaking(7, true);
        let (gain, changed) = duck.update(listening, start);
        assert!((gain - 1.5 * DUCKED).abs() < 1e-6 && changed);
        duck.speaking(7, false);
        assert!(duck.update(listening, start).0 < 1.0);
        assert_eq!(duck.release_at(), Some(start + RELEASE));
        assert_eq!(duck.update(listening, start + RELEASE), (1.5, true));
    }

    #[test]
    fn without_ducking_the_volume_is_the_volume() {
        let listening = MusicListening::clamped(9.0, false, false);
        let mut duck = MusicDuck::default();
        duck.talking(true);
        assert_eq!(duck.update(listening, Instant::now()).0, 2.0);
        assert_eq!(MusicListening::clamped(f32::NAN, true, false).volume, 0.5);
    }
}
