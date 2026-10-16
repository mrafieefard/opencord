//! How far a server's clock is from this device's (Phase 3 plan §14.3),
//! for music progress: the server says what time it is in `Ready` and in
//! every heartbeat ack. The sample with the shortest round trip of the
//! last few is the best estimate, since its answer took the least
//! uncertain time to arrive.

use std::collections::VecDeque;
use std::time::Duration;

/// Samples kept.
const SAMPLES: usize = 8;

#[derive(Debug, Default, Clone)]
pub(crate) struct ServerClock {
    /// (server − device in ms, round trip).
    samples: VecDeque<(i64, Duration)>,
}

impl ServerClock {
    /// The server said `server_ms` while the device's clock went from
    /// `sent_ms` to `received_ms` (the same when the round trip is not
    /// known, as for `Ready`).
    pub fn sample(&mut self, server_ms: i64, sent_ms: i64, received_ms: i64) {
        if server_ms <= 0 || received_ms < sent_ms {
            return;
        }
        let round_trip = received_ms - sent_ms;
        let offset = server_ms - (sent_ms + round_trip / 2);
        if self.samples.len() == SAMPLES {
            self.samples.pop_front();
        }
        self.samples.push_back((
            offset,
            Duration::from_millis(u64::try_from(round_trip).unwrap_or(0)),
        ));
    }

    /// Server time minus device time, in ms; 0 before any sample.
    pub fn offset_ms(&self) -> i64 {
        // Ready's sample has no round trip: any measured one is better.
        let measured = self.samples.iter().filter(|(_, rtt)| !rtt.is_zero());
        measured
            .min_by_key(|(_, rtt)| *rtt)
            .or_else(|| self.samples.back())
            .map_or(0, |(offset, _)| *offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shortest_round_trip_wins() {
        let mut clock = ServerClock::default();
        assert_eq!(clock.offset_ms(), 0);
        clock.sample(10_500, 10_000, 10_000);
        assert_eq!(clock.offset_ms(), 500);
        clock.sample(20_400, 20_000, 20_200);
        assert_eq!(clock.offset_ms(), 300);
        clock.sample(30_250, 30_000, 30_040);
        assert_eq!(clock.offset_ms(), 230);
        clock.sample(40_900, 40_000, 40_800);
        assert_eq!(clock.offset_ms(), 230);
        clock.sample(0, 1, 2);
        assert_eq!(clock.samples.len(), 4);
    }
}
