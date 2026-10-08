//! Everything heard, summed (plan §7.5): each person at their own volume
//! (0–200 %) unless locally muted, the master volume, then a limiter that
//! keeps loud moments from clipping. Deafened, nothing comes out.

/// The peak the limiter holds the mix under: -1 dBFS.
pub const CEILING: f32 = 0.891;
/// How far the limiter's gain moves back toward 1 each 10 ms: about 100 ms
/// to let go.
const RELEASE: f32 = 0.1;

#[derive(Debug)]
pub struct Mixer {
    /// The limiter's gain at the end of the last tick.
    gain: f32,
}

impl Default for Mixer {
    fn default() -> Self {
        Self::new()
    }
}

impl Mixer {
    pub fn new() -> Self {
        Self { gain: 1.0 }
    }

    /// Sums `sources`, each as (samples, volume), into `out` at `master`
    /// volume, limited.
    pub fn mix<'a>(
        &mut self,
        sources: impl IntoIterator<Item = (&'a [f32], f32)>,
        master: f32,
        out: &mut [f32],
    ) {
        out.fill(0.0);
        for (samples, volume) in sources {
            if volume == 0.0 {
                continue;
            }
            for (mixed, sample) in out.iter_mut().zip(samples) {
                *mixed += sample * volume;
            }
        }
        let mut peak = 0.0f32;
        for mixed in out.iter_mut() {
            *mixed *= master;
            peak = peak.max(mixed.abs());
        }

        // Instant attack, gradual release; the gain moves smoothly across
        // the tick when it rises, and never above what this tick allows.
        let allowed = if peak > CEILING { CEILING / peak } else { 1.0 };
        let (from, to) = if allowed < self.gain {
            (allowed, allowed)
        } else {
            (self.gain, self.gain + (allowed - self.gain) * RELEASE)
        };
        let steps = out.len().max(1) as f32;
        for (index, mixed) in out.iter_mut().enumerate() {
            let gain = from + (to - from) * (index as f32 / steps);
            *mixed = (*mixed * gain).clamp(-1.0, 1.0);
        }
        self.gain = to;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::TICK;

    fn constant(value: f32) -> Vec<f32> {
        vec![value; TICK]
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0, |peak, s| peak.max(s.abs()))
    }

    #[test]
    fn volumes_and_local_mutes_apply() {
        let mut mixer = Mixer::new();
        let loud_friend = constant(0.1);
        let muted = constant(0.3);
        let mut out = [0.0; TICK];

        mixer.mix([(&loud_friend[..], 2.0), (&muted[..], 0.0)], 1.0, &mut out);

        assert!(out.iter().all(|s| (s - 0.2).abs() < 1e-6), "{}", out[0]);
    }

    #[test]
    fn the_master_volume_scales_everything() {
        let mut mixer = Mixer::new();
        let voice = constant(0.2);
        let mut out = [0.0; TICK];

        mixer.mix([(&voice[..], 1.0)], 0.5, &mut out);

        assert!(out.iter().all(|s| (s - 0.1).abs() < 1e-6));
    }

    #[test]
    fn a_quiet_mix_passes_untouched() {
        let mut mixer = Mixer::new();
        let a = constant(0.1);
        let b = constant(-0.05);
        let mut out = [0.0; TICK];

        mixer.mix([(&a[..], 1.0), (&b[..], 1.0)], 1.0, &mut out);

        assert!(out.iter().all(|s| (s - 0.05).abs() < 1e-6));
    }

    #[test]
    fn a_loud_mix_never_clips() {
        let mut mixer = Mixer::new();
        let shout = constant(0.9);
        let mut out = [0.0; TICK];

        for _ in 0..20 {
            mixer.mix(
                [(&shout[..], 2.0), (&shout[..], 2.0), (&shout[..], 1.0)],
                2.0,
                &mut out,
            );
            assert!(peak(&out) <= 1.0, "{}", peak(&out));
        }

        assert!(peak(&out) <= CEILING + 0.01, "{}", peak(&out));
    }

    #[test]
    fn the_limiter_lets_go_after_a_loud_moment() {
        let mut mixer = Mixer::new();
        let shout = constant(0.9);
        let speech = constant(0.2);
        let mut out = [0.0; TICK];
        for _ in 0..10 {
            mixer.mix([(&shout[..], 2.0)], 1.0, &mut out);
        }

        for _ in 0..50 {
            mixer.mix([(&speech[..], 1.0)], 1.0, &mut out);
        }

        assert!((out[TICK - 1] - 0.2).abs() < 0.01, "{}", out[TICK - 1]);
    }

    #[test]
    fn nothing_mixes_to_silence() {
        let mut mixer = Mixer::new();
        let mut out = [0.5; TICK];

        mixer.mix(std::iter::empty::<(&[f32], f32)>(), 1.0, &mut out);

        assert!(out.iter().all(|s| *s == 0.0));
    }
}
