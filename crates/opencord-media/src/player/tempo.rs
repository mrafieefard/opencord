//! The player's speed (D87): decoded stereo at 48 kHz on its way out, as it
//! is at 1×, through the WSOLA time stretcher at the other speeds, which
//! keeps the pitch, as voice messages' does.

use std::collections::VecDeque;
use std::num::{NonZeroU16, NonZeroU32};

use wsola::TimeStretch;

/// Input pushed into the stretcher at a time, in samples (21 ms of stereo).
const BLOCK: usize = 2_048;

pub struct Tempo {
    speed: f32,
    stretch: TimeStretch,
    /// Interleaved stereo ready to write, at the current speed.
    ready: VecDeque<f32>,
    /// The stretcher gave up its tail at the end of the stream.
    flushed: bool,
}

impl Tempo {
    pub fn new() -> Result<Self, String> {
        let stretch = TimeStretch::new(
            const { NonZeroU32::new(48_000).unwrap() },
            const { NonZeroU16::new(2).unwrap() },
        )
        .map_err(|e| e.to_string())?;
        Ok(Self {
            speed: 1.0,
            stretch,
            ready: VecDeque::new(),
            flushed: false,
        })
    }

    /// Back to 1×, what the stretcher holds comes out first, so nothing is
    /// lost.
    pub fn set_speed(&mut self, speed: f32) {
        if speed == self.speed || !speed.is_finite() || speed <= 0.0 {
            return;
        }
        if speed == 1.0 {
            self.drain_stretcher();
        }
        self.speed = speed;
        self.stretch.set_tempo(speed);
    }

    /// Everything the stretcher holds, into `ready`. A flushed stretcher
    /// keeps its input and would play it again: it starts afresh.
    fn drain_stretcher(&mut self) {
        self.ready.extend(self.stretch.flush());
        self.stretch.reset();
    }

    /// Readies up to `samples` from `pending`, which `end` says will get no
    /// more.
    pub fn fill(&mut self, pending: &mut VecDeque<f32>, samples: usize, end: bool) {
        if self.speed == 1.0 {
            let take = samples.saturating_sub(self.ready.len()).min(pending.len());
            self.ready.extend(pending.drain(..take));
            return;
        }
        while self.ready.len() < samples {
            let before = self.ready.len();
            if !self.flushed {
                self.ready.extend(self.stretch.pull(samples - before));
            }
            if self.ready.len() > before {
                continue;
            }
            if !pending.is_empty() {
                let block: Vec<f32> = pending.drain(..BLOCK.min(pending.len())).collect();
                self.stretch.push(&block);
                self.flushed = false;
            } else if end && !self.flushed {
                self.drain_stretcher();
                self.flushed = true;
            } else {
                break;
            }
        }
    }

    pub fn ready(&self) -> &VecDeque<f32> {
        &self.ready
    }

    /// Takes the first `samples` written.
    pub fn consume(&mut self, samples: usize) {
        self.ready.drain(..samples.min(self.ready.len()));
    }

    /// Nothing ready and nothing held back.
    pub fn is_empty(&self) -> bool {
        self.ready.is_empty() && (self.speed == 1.0 || self.stretch.buffered() == 0)
    }

    /// After a seek: what was readied belongs to the old place.
    pub fn reset(&mut self) {
        self.stretch.reset();
        self.ready.clear();
        self.flushed = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `seconds` of a 440 Hz tone, interleaved stereo at 48 kHz.
    fn tone(seconds: f32) -> VecDeque<f32> {
        let frames = (48_000.0 * seconds) as usize;
        (0..frames)
            .flat_map(|i| {
                let s = (i as f32 * 440.0 * std::f32::consts::TAU / 48_000.0).sin() * 0.5;
                [s, s]
            })
            .collect()
    }

    /// Everything out of `tempo` from `input`, in writes of 60 ms.
    fn play(tempo: &mut Tempo, mut input: VecDeque<f32>) -> Vec<f32> {
        let mut out = Vec::new();
        loop {
            tempo.fill(&mut input, 2_880 * 2, true);
            if tempo.ready().is_empty() {
                break;
            }
            let n = tempo.ready().len();
            out.extend(tempo.ready().iter().copied());
            tempo.consume(n);
        }
        out
    }

    /// Rising zero crossings of the left channel, per second of output.
    fn crossings_per_second(stereo: &[f32]) -> f32 {
        let left: Vec<f32> = stereo.iter().step_by(2).copied().collect();
        let rising = left
            .windows(2)
            .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
            .count();
        rising as f32 / (left.len() as f32 / 48_000.0)
    }

    #[test]
    fn at_1x_the_sound_passes_untouched() {
        let mut tempo = Tempo::new().unwrap();
        let input = tone(0.5);
        let out = play(&mut tempo, input.clone());
        assert_eq!(out, input.into_iter().collect::<Vec<_>>());
    }

    #[test]
    fn at_2x_a_tone_keeps_its_pitch_and_halves_its_time() {
        let mut tempo = Tempo::new().unwrap();
        tempo.set_speed(2.0);
        let out = play(&mut tempo, tone(2.0));
        let seconds = out.len() as f32 / 2.0 / 48_000.0;
        assert!((0.95..1.05).contains(&seconds), "{seconds} s");
        let pitch = crossings_per_second(&out);
        assert!((430.0..450.0).contains(&pitch), "{pitch} Hz");
    }

    #[test]
    fn at_3x_a_tone_keeps_its_pitch_and_takes_a_third() {
        let mut tempo = Tempo::new().unwrap();
        tempo.set_speed(3.0);
        let out = play(&mut tempo, tone(3.0));
        let seconds = out.len() as f32 / 2.0 / 48_000.0;
        assert!((0.95..1.05).contains(&seconds), "{seconds} s");
        let pitch = crossings_per_second(&out);
        assert!((430.0..450.0).contains(&pitch), "{pitch} Hz");
    }

    #[test]
    fn back_to_1x_nothing_held_is_lost() {
        let mut tempo = Tempo::new().unwrap();
        tempo.set_speed(2.0);
        let mut input = tone(1.0);
        tempo.fill(&mut input, 4_000, false);
        tempo.set_speed(1.0);
        assert!(tempo.is_empty() || !tempo.ready().is_empty());
        let out = play(&mut tempo, input);
        // Half a second's worth went through at 2× (≈ 0.25 s), the rest at 1×.
        let seconds = out.len() as f32 / 2.0 / 48_000.0;
        assert!(seconds > 0.7, "{seconds} s");
    }

    #[test]
    fn switching_speeds_plays_nothing_twice() {
        let mut tempo = Tempo::new().unwrap();
        tempo.set_speed(2.0);
        let mut input = tone(2.0);
        tempo.fill(&mut input, 4_000, false);
        let mut out: Vec<f32> = tempo.ready().iter().copied().collect();
        tempo.consume(out.len());
        tempo.set_speed(1.0);
        tempo.set_speed(2.0);
        out.extend(play(&mut tempo, input));
        // All of it at 2×: a second, give or take what the switch let out.
        let seconds = out.len() as f32 / 2.0 / 48_000.0;
        assert!((0.95..1.1).contains(&seconds), "{seconds} s");
    }
}
