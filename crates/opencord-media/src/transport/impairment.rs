//! A network impairment shim for tests (plan §15): loss, latency, jitter,
//! reordering and a bandwidth cap on a voice connection's packets, so tests
//! need neither root nor `tc`. Built on str0m-netem, a Sans-IO emulator
//! from str0m's authors. Only with the `testing` feature.

use std::time::{Duration, Instant};

use str0m_netem::{Bitrate, DataSize, Input, Netem, NetemConfig, Output, Probability, RandomLoss};

/// What happens to packets in one direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Impairment {
    /// Share of packets lost, 0 to 1.
    pub loss: f32,
    pub latency: Duration,
    /// Each packet's delay varies by up to this much either way.
    pub jitter: Duration,
    /// Every this many packets one skips the delay, arriving early; 0
    /// never.
    pub reorder_gap: u32,
    /// The link's capacity in bits per second; 0 unlimited.
    pub rate: u64,
    /// How much the link buffers before dropping, as time at its rate.
    pub queue: Duration,
}

impl Default for Impairment {
    fn default() -> Self {
        Self {
            loss: 0.0,
            latency: Duration::ZERO,
            jitter: Duration::ZERO,
            reorder_gap: 0,
            rate: 0,
            queue: Duration::from_millis(200),
        }
    }
}

impl Impairment {
    fn config(&self, seed: u64) -> NetemConfig {
        let mut config = NetemConfig::new()
            .latency(self.latency)
            .jitter(self.jitter)
            .seed(seed);
        if self.loss > 0.0 {
            config = config.loss(RandomLoss::new(Probability::new(self.loss.clamp(0.0, 1.0))));
        }
        if self.reorder_gap > 0 {
            config = config.reorder_gap(self.reorder_gap);
        }
        if self.rate > 0 {
            let buffer = (self.rate as f64 * self.queue.as_secs_f64() / 8.0) as i64;
            config = config.link(Bitrate::bps(self.rate), DataSize::bytes(buffer.max(1500)));
        }
        config
    }
}

/// Packets in one direction, held back as the impairment says.
pub(crate) struct Shim {
    netem: Netem<Vec<u8>>,
    seed: u64,
}

impl Shim {
    pub fn new(impairment: &Impairment, seed: u64) -> Self {
        Self {
            netem: Netem::new(impairment.config(seed)),
            seed,
        }
    }

    /// Changes the impairment; packets already held keep their times.
    pub fn set(&mut self, impairment: &Impairment) {
        self.netem.set_config(impairment.config(self.seed));
    }

    pub fn push(&mut self, now: Instant, packet: Vec<u8>) {
        self.netem.handle_input(Input::Packet(now, packet));
    }

    /// A packet due by `now`.
    pub fn pop(&mut self, now: Instant) -> Option<Vec<u8>> {
        self.netem.handle_input(Input::Timeout(now));
        match self.netem.poll_output()? {
            Output::Packet(packet) => Some(packet),
            Output::Timeout(_) => None,
        }
    }

    /// When the next packet is due.
    pub fn next_due(&self) -> Option<Instant> {
        (!self.netem.is_empty()).then(|| self.netem.poll_timeout())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drain(shim: &mut Shim, now: Instant) -> usize {
        std::iter::from_fn(|| shim.pop(now)).count()
    }

    #[test]
    fn without_impairment_packets_pass_at_once() {
        let mut shim = Shim::new(&Impairment::default(), 1);
        let now = Instant::now();

        shim.push(now, vec![1, 2, 3]);

        assert_eq!(shim.pop(now), Some(vec![1, 2, 3]));
        assert_eq!(shim.next_due(), None);
    }

    #[test]
    fn latency_holds_packets_back() {
        let impairment = Impairment {
            latency: Duration::from_millis(50),
            ..Impairment::default()
        };
        let mut shim = Shim::new(&impairment, 1);
        let now = Instant::now();

        shim.push(now, vec![0; 100]);

        assert_eq!(drain(&mut shim, now + Duration::from_millis(49)), 0);
        assert_eq!(shim.next_due(), Some(now + Duration::from_millis(50)));
        assert_eq!(drain(&mut shim, now + Duration::from_millis(50)), 1);
    }

    #[test]
    fn a_bandwidth_cap_spreads_packets_out() {
        let impairment = Impairment {
            rate: 500_000,
            ..Impairment::default()
        };
        let mut shim = Shim::new(&impairment, 1);
        let now = Instant::now();

        // 10 packets of 1250 bytes: 100 kbit, 200 ms at 500 kbps.
        for _ in 0..10 {
            shim.push(now, vec![0; 1250]);
        }

        let by_100_ms = drain(&mut shim, now + Duration::from_millis(100));
        let by_250_ms = drain(&mut shim, now + Duration::from_millis(250));
        assert!((4..=6).contains(&by_100_ms), "{by_100_ms} by 100 ms");
        assert_eq!(by_100_ms + by_250_ms, 10);
    }

    #[test]
    fn a_full_queue_drops_what_does_not_fit() {
        let impairment = Impairment {
            rate: 100_000,
            queue: Duration::from_millis(100),
            ..Impairment::default()
        };
        let mut shim = Shim::new(&impairment, 1);
        let now = Instant::now();

        // 100 ms at 100 kbps is 1250 bytes of queue.
        for _ in 0..20 {
            shim.push(now, vec![0; 500]);
        }

        let delivered = drain(&mut shim, now + Duration::from_secs(10));
        assert!(delivered < 10, "{delivered} delivered");
    }

    #[test]
    fn loss_drops_about_that_share() {
        let impairment = Impairment {
            loss: 0.5,
            ..Impairment::default()
        };
        let mut shim = Shim::new(&impairment, 7);
        let now = Instant::now();

        for _ in 0..1000 {
            shim.push(now, vec![0; 10]);
        }

        let delivered = drain(&mut shim, now);
        assert!((400..=600).contains(&delivered), "{delivered} delivered");
    }

    #[test]
    fn the_impairment_can_be_lifted() {
        let capped = Impairment {
            rate: 100_000,
            ..Impairment::default()
        };
        let mut shim = Shim::new(&capped, 1);
        let now = Instant::now();
        shim.push(now, vec![0; 1250]);
        assert_eq!(drain(&mut shim, now + Duration::from_millis(150)), 1);

        shim.set(&Impairment::default());
        let later = now + Duration::from_secs(1);
        for _ in 0..10 {
            shim.push(later, vec![0; 1250]);
        }

        assert_eq!(drain(&mut shim, later), 10);
    }
}
