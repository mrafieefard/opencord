//! The controller against scripted readings, and [`VoiceThrottle`] itself
//! on Tokio's paused clock.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;
use tokio::time::Instant;

use super::super::Throttle;
use super::{Controller, FLOOR_RATE, IDLE_AFTER, Rate, START_RATE, VoiceReading, VoiceThrottle};

const KIB: u64 = 1 << 10;

fn ping(ms: u32) -> Option<VoiceReading> {
    Some(VoiceReading {
        ping_ms: Some(ms),
        loss_in_percent: Some(0.0),
    })
}

fn lossy(ms: u32, loss_in_percent: f32) -> Option<VoiceReading> {
    Some(VoiceReading {
        ping_ms: Some(ms),
        loss_in_percent: Some(loss_in_percent),
    })
}

fn no_ping(loss_in_percent: f32) -> Option<VoiceReading> {
    Some(VoiceReading {
        ping_ms: None,
        loss_in_percent: Some(loss_in_percent),
    })
}

/// A download in a call whose baseline is 50 ms, at the start rate.
fn limited() -> Controller {
    let mut controller = Controller::default();
    for _ in 0..5 {
        controller.on_reading(ping(50));
    }
    assert_eq!(controller.start(), Rate::Limited(START_RATE));
    controller
}

/// `limited()`, halved down to the floor by three hurting readings.
fn at_the_floor() -> Controller {
    let mut controller = limited();
    for rate in [128 * KIB, 64 * KIB, 32 * KIB] {
        assert_eq!(controller.on_reading(ping(200)), Rate::Limited(rate));
    }
    controller
}

// Fails if `Controller::rate` gives `Call::Out` anything but `Unlimited`,
// or if `start` limits a download outside a call.
#[test]
fn outside_a_call_it_is_unlimited() {
    let mut controller = Controller::default();
    assert_eq!(controller.start(), Rate::Unlimited);
    for _ in 0..3 {
        assert_eq!(controller.on_reading(None), Rate::Unlimited);
    }
}

// Fails if `Controller::start` ignores the readings from before the
// download (`take_baseline` not called there), or if `START_RATE` isn't
// 256 KiB/s.
#[test]
fn a_call_starts_at_256_kib() {
    let mut controller = Controller::default();
    for _ in 0..5 {
        controller.on_reading(ping(50));
    }
    assert_eq!(controller.start(), Rate::Limited(256 * KIB));

    // The other side: four readings aren't a baseline yet.
    let mut short = Controller::default();
    for _ in 0..4 {
        short.on_reading(ping(50));
    }
    assert_eq!(short.start(), Rate::Paused);
    assert_eq!(short.on_reading(ping(50)), Rate::Limited(256 * KIB));
}

// Fails if `hurts` compares with `>=` (80 ms is the baseline + 30, calm),
// or if a hurting reading in `Call::Limited` doesn't halve.
#[test]
fn one_hurting_reading_halves() {
    let mut controller = limited();
    assert_eq!(controller.on_reading(ping(80)), Rate::Limited(256 * KIB));
    assert_eq!(controller.on_reading(ping(81)), Rate::Limited(128 * KIB));
}

// Fails without `.max(FLOOR_RATE)` in the halving: 40 KiB/s would halve
// to 20.
#[test]
fn it_never_goes_under_32_kib() {
    let mut controller = limited();
    // 256 → 320 by growing, then halved: 160, 80, 40, then the floor.
    controller.on_reading(ping(50));
    assert_eq!(controller.on_reading(ping(50)), Rate::Limited(320 * KIB));
    for rate in [160 * KIB, 80 * KIB, 40 * KIB, 32 * KIB] {
        assert_eq!(controller.on_reading(ping(200)), Rate::Limited(rate));
    }
    assert_eq!(FLOOR_RATE, 32 * KIB);
}

// Fails if `Call::Paused` doesn't reset its count on a hurting reading,
// or if `CALM_TO_RESUME` isn't 3.
#[test]
fn hurting_at_the_floor_pauses_until_3_calm() {
    let mut controller = at_the_floor();
    assert_eq!(controller.on_reading(ping(200)), Rate::Paused);
    assert_eq!(controller.on_reading(ping(50)), Rate::Paused);
    assert_eq!(controller.on_reading(ping(50)), Rate::Paused);
    // Hurting again starts the count over.
    assert_eq!(controller.on_reading(ping(200)), Rate::Paused);
    assert_eq!(controller.on_reading(ping(50)), Rate::Paused);
    assert_eq!(controller.on_reading(ping(50)), Rate::Paused);
    assert_eq!(controller.on_reading(ping(50)), Rate::Limited(32 * KIB));
}

// Fails if the rate grows on every calm reading (`CALM_TO_GROW` 1), by
// another factor than 1.25, or if a hurting reading keeps the count.
#[test]
fn two_calm_readings_grow_by_a_quarter() {
    let mut controller = limited();
    assert_eq!(controller.on_reading(ping(50)), Rate::Limited(256 * KIB));
    assert_eq!(controller.on_reading(ping(50)), Rate::Limited(320 * KIB));
    assert_eq!(controller.on_reading(ping(50)), Rate::Limited(320 * KIB));
    assert_eq!(controller.on_reading(ping(50)), Rate::Limited(400 * KIB));
    // One calm, one hurting, one calm: no growth.
    controller.on_reading(ping(50));
    assert_eq!(controller.on_reading(ping(200)), Rate::Limited(200 * KIB));
    assert_eq!(controller.on_reading(ping(50)), Rate::Limited(200 * KIB));
    assert_eq!(controller.on_reading(ping(50)), Rate::Limited(250 * KIB));
}

// Fails if `on_reading(None)` leaves `Call::Paused` as it was.
#[test]
fn leaving_the_call_is_unlimited_at_once() {
    let mut controller = at_the_floor();
    assert_eq!(controller.on_reading(ping(200)), Rate::Paused);
    assert_eq!(controller.on_reading(None), Rate::Unlimited);
}

// Fails if a call joined during a download isn't paused while it gathers
// (`Call::Gathering`'s rate), or if the baseline is the mean (88 ms) or
// the first reading instead of the median (60 ms).
#[test]
fn joining_partway_pauses_for_a_baseline() {
    let mut controller = Controller::default();
    assert_eq!(controller.start(), Rate::Unlimited);
    // Joined: no ping until it connects.
    assert_eq!(controller.on_reading(no_ping(0.0)), Rate::Paused);
    for ms in [40, 90, 50, 200] {
        assert_eq!(controller.on_reading(ping(ms)), Rate::Paused);
    }
    assert_eq!(controller.on_reading(ping(60)), Rate::Limited(256 * KIB));
    assert_eq!(controller.on_reading(ping(90)), Rate::Limited(256 * KIB));
    assert_eq!(controller.on_reading(ping(91)), Rate::Limited(128 * KIB));
}

// Fails if `hurts` looks only at the ping, or compares the loss with `>=`.
#[test]
fn loss_over_2_percent_hurts_without_ping_rising() {
    let mut controller = limited();
    assert_eq!(
        controller.on_reading(lossy(50, 2.0)),
        Rate::Limited(256 * KIB)
    );
    assert_eq!(
        controller.on_reading(lossy(50, 2.5)),
        Rate::Limited(128 * KIB)
    );
}

// Fails if a reading without a ping is judged by its loss.
#[test]
fn a_reading_without_a_ping_is_neither_calm_nor_hurting() {
    let mut controller = limited();
    assert_eq!(
        controller.on_reading(no_ping(50.0)),
        Rate::Limited(256 * KIB)
    );
    // Nor calm: a calm one after it is the first of two.
    assert_eq!(controller.on_reading(ping(50)), Rate::Limited(256 * KIB));
    assert_eq!(
        controller.on_reading(no_ping(0.0)),
        Rate::Limited(256 * KIB)
    );
    assert_eq!(controller.on_reading(ping(50)), Rate::Limited(320 * KIB));
}

// Fails if `Controller::stop` keeps the last download's rate and
// baseline.
#[test]
fn the_next_download_takes_a_new_baseline() {
    let mut controller = limited();
    controller.on_reading(ping(50));
    assert_eq!(controller.on_reading(ping(50)), Rate::Limited(320 * KIB));
    controller.stop();
    for _ in 0..5 {
        controller.on_reading(ping(100));
    }
    assert_eq!(controller.start(), Rate::Limited(256 * KIB));
    assert_eq!(controller.on_reading(ping(130)), Rate::Limited(256 * KIB));
    assert_eq!(controller.on_reading(ping(131)), Rate::Limited(128 * KIB));
}

/// A throttle on its own readings channel, with five 50 ms readings sent a
/// second apart before any download.
async fn throttle_in_a_call() -> (Arc<VoiceThrottle>, watch::Sender<Option<VoiceReading>>) {
    let (readings, receiver) = watch::channel(None);
    let throttle = Arc::new(VoiceThrottle::new(receiver));
    for _ in 0..5 {
        readings.send_replace(ping(50));
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    (throttle, readings)
}

/// A take that never returns fails the test rather than hanging it: the
/// paused clock jumps to this deadline once nothing else can run.
const STUCK: Duration = Duration::from_secs(600);

async fn take(throttle: &VoiceThrottle, bytes: u64) {
    tokio::time::timeout(STUCK, throttle.take(bytes as usize))
        .await
        .expect("a take never returned");
}

/// How long taking `total` bytes in `piece`s takes.
async fn time_to_take(throttle: &VoiceThrottle, total: u64, piece: u64) -> Duration {
    let started = Instant::now();
    let mut taken = 0;
    while taken < total {
        take(throttle, piece).await;
        taken += piece;
    }
    started.elapsed()
}

#[track_caller]
fn assert_about(elapsed: Duration, expected: Duration) {
    let low = expected.mul_f64(0.95);
    let high = expected.mul_f64(1.05);
    assert!(
        (low..=high).contains(&elapsed),
        "took {elapsed:?}, not {expected:?} ± 5 %"
    );
}

// Fails if a limit starts with a full bucket (`Bucket::set` filling it
// after `Unlimited`, so a 1 s burst: 9 s), or if it refills at another
// rate.
#[tokio::test(start_paused = true)]
async fn the_bucket_holds_the_rate_over_ten_seconds() {
    let (throttle, _readings) = throttle_in_a_call().await;
    let elapsed = time_to_take(&throttle, 10 * START_RATE, 64 * KIB).await;
    assert_about(elapsed, Duration::from_secs(10));
}

// Fails if a take bigger than the bucket waits only for a full bucket
// (`tokens -= need` instead of the bytes taken): 1 MiB a second, 4× the
// rate.
#[tokio::test(start_paused = true)]
async fn a_chunk_bigger_than_the_bucket_waits_off_its_debt() {
    let (throttle, _readings) = throttle_in_a_call().await;
    // 1 s for the first 256 KiB, then 4 s for each MiB owed.
    let elapsed = time_to_take(&throttle, 3 << 20, 1 << 20).await;
    assert_about(elapsed, Duration::from_secs(9));
}

// Fails if a reading doesn't wake a paused take (no `notify_waiters` in
// `Shared::on_reading`).
#[tokio::test(start_paused = true)]
async fn a_paused_take_goes_on_when_the_call_ends() {
    let (readings, receiver) = watch::channel(None);
    let throttle = Arc::new(VoiceThrottle::new(receiver));
    // Joined a moment before the download: one reading isn't a baseline.
    readings.send_replace(ping(50));
    tokio::time::sleep(Duration::from_secs(1)).await;

    let taking = Arc::clone(&throttle);
    let take = tokio::spawn(async move { taking.take(1 << 20).await });
    tokio::time::sleep(Duration::from_secs(10)).await;
    assert!(!take.is_finished(), "a take went on without a baseline");

    readings.send_replace(None);
    tokio::time::timeout(Duration::from_millis(10), take)
        .await
        .expect("leaving the call didn't wake the take")
        .expect("the take panicked");
}

// Fails if `Inner::settle` never ends a download: the next one, after a
// quiet minute, would go on at the grown 625 KiB/s from a full bucket
// (256 KiB at once) instead of starting again at 256 KiB/s.
#[tokio::test(start_paused = true)]
async fn a_download_after_a_quiet_minute_starts_again_at_256_kib() {
    let (throttle, readings) = throttle_in_a_call().await;
    take(&throttle, 64 * KIB).await;
    // Eight calm readings: 256 → 625 KiB/s.
    for _ in 0..8 {
        readings.send_replace(ping(50));
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    // The bucket filled to a second of the rate meanwhile.
    take(&throttle, 625 * KIB).await;
    assert_about(
        time_to_take(&throttle, 625 * KIB, 25 * KIB).await,
        Duration::from_secs(1),
    );

    tokio::time::sleep(IDLE_AFTER + Duration::from_secs(1)).await;
    for _ in 0..5 {
        readings.send_replace(ping(50));
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    assert_about(
        time_to_take(&throttle, 256 * KIB, 16 * KIB).await,
        Duration::from_secs(1),
    );
}
