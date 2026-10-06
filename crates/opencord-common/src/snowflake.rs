//! 64-bit, time-sortable ids.
//!
//! Layout, from the most significant bit: 1 unused bit (ids stay positive in
//! an `i64`), 41 bits of milliseconds since [`EPOCH_MS`], 10 bits of worker
//! id, 12 bits of per-millisecond sequence.

use std::sync::{Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

/// 2026-01-01T00:00:00Z in Unix milliseconds.
pub const EPOCH_MS: i64 = 1_767_225_600_000;
pub const MAX_WORKER_ID: u16 = (1 << WORKER_BITS) - 1;

const WORKER_BITS: u32 = 10;
const SEQUENCE_BITS: u32 = 12;
const TIMESTAMP_SHIFT: u32 = WORKER_BITS + SEQUENCE_BITS;
const MAX_SEQUENCE: u16 = (1 << SEQUENCE_BITS) - 1;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("worker id {0} is above the maximum of {MAX_WORKER_ID}")]
pub struct WorkerIdOutOfRange(pub u16);

/// Unix-millisecond timestamp encoded in a Snowflake id.
pub fn timestamp_ms(id: i64) -> i64 {
    (id >> TIMESTAMP_SHIFT) + EPOCH_MS
}

#[derive(Debug)]
pub struct SnowflakeGenerator {
    worker_id: u16,
    state: Mutex<State>,
}

#[derive(Debug, Default)]
struct State {
    /// Milliseconds since [`EPOCH_MS`] of the last issued id.
    elapsed_ms: i64,
    sequence: u16,
}

impl SnowflakeGenerator {
    pub fn new(worker_id: u16) -> Result<Self, WorkerIdOutOfRange> {
        if worker_id > MAX_WORKER_ID {
            return Err(WorkerIdOutOfRange(worker_id));
        }
        Ok(Self {
            worker_id,
            state: Mutex::new(State::default()),
        })
    }

    /// Next id. Never repeats and always increases, even if the system clock
    /// goes backwards or more than 4096 ids are requested within a millisecond.
    pub fn next_id(&self) -> i64 {
        self.next_id_at(now_ms())
    }

    fn next_id_at(&self, now_ms: i64) -> i64 {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        let elapsed_ms = (now_ms - EPOCH_MS).max(0);
        if elapsed_ms > state.elapsed_ms {
            state.elapsed_ms = elapsed_ms;
            state.sequence = 0;
        } else if state.sequence == MAX_SEQUENCE {
            state.elapsed_ms += 1;
            state.sequence = 0;
        } else {
            state.sequence += 1;
        }
        (state.elapsed_ms << TIMESTAMP_SHIFT)
            | (i64::from(self.worker_id) << SEQUENCE_BITS)
            | i64::from(state.sequence)
    }
}

fn now_ms() -> i64 {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from(since_epoch.as_millis()).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Arc;
    use std::thread;

    use super::*;

    const T0: i64 = EPOCH_MS + 1_000_000;

    #[test]
    fn rejects_worker_ids_above_ten_bits() {
        assert_eq!(
            SnowflakeGenerator::new(1024).unwrap_err(),
            WorkerIdOutOfRange(1024)
        );
        assert!(SnowflakeGenerator::new(MAX_WORKER_ID).is_ok());
    }

    #[test]
    fn encodes_the_timestamp() {
        let generator = SnowflakeGenerator::new(5).unwrap();

        let id = generator.next_id_at(T0);

        assert!(id > 0);
        assert_eq!(timestamp_ms(id), T0);
    }

    #[test]
    fn ids_increase_within_and_across_milliseconds() {
        let generator = SnowflakeGenerator::new(1).unwrap();

        let ids = [
            generator.next_id_at(T0),
            generator.next_id_at(T0),
            generator.next_id_at(T0 + 1),
            generator.next_id_at(T0 + 50),
        ];

        assert!(ids.windows(2).all(|pair| pair[0] < pair[1]), "{ids:?}");
    }

    #[test]
    fn ids_keep_increasing_when_the_clock_goes_backwards() {
        let generator = SnowflakeGenerator::new(1).unwrap();

        let before = generator.next_id_at(T0);
        let after = generator.next_id_at(T0 - 5_000);

        assert!(after > before);
    }

    #[test]
    fn sequence_overflow_moves_into_the_next_millisecond() {
        let generator = SnowflakeGenerator::new(1).unwrap();

        let ids: Vec<i64> = (0..=u32::from(MAX_SEQUENCE) + 1)
            .map(|_| generator.next_id_at(T0))
            .collect();

        assert!(ids.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(timestamp_ms(*ids.last().unwrap()), T0 + 1);
    }

    #[test]
    fn different_workers_never_collide() {
        let a = SnowflakeGenerator::new(1).unwrap();
        let b = SnowflakeGenerator::new(2).unwrap();

        assert_ne!(a.next_id_at(T0), b.next_id_at(T0));
    }

    #[test]
    fn ids_are_unique_across_threads() {
        let generator = Arc::new(SnowflakeGenerator::new(3).unwrap());

        let handles: Vec<_> = (0..8)
            .map(|_| {
                let generator = Arc::clone(&generator);
                thread::spawn(move || (0..5_000).map(|_| generator.next_id()).collect::<Vec<_>>())
            })
            .collect();
        let ids: Vec<i64> = handles
            .into_iter()
            .flat_map(|handle| handle.join().unwrap())
            .collect();

        let unique: HashSet<i64> = ids.iter().copied().collect();
        assert_eq!(unique.len(), ids.len());
    }
}
