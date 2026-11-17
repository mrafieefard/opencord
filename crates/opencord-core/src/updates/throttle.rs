//! The voice throttle (design: "The app: downloading, and the voice
//! throttle"): while the user is in a call, the update download slows
//! down whenever it raises the call's ping or makes it lose packets.
//!
//! - **The readings** are the voice stats the core gets once a second
//!   while a call is connected. `media.rs` publishes each one with
//!   [`publish_voice_reading`], and `None` when the call's session ends,
//!   to one process-wide `watch` channel (one call per process).
//! - **[`Controller`]** turns readings into a [`Rate`]: a pure state
//!   machine following the design's numbers.
//! - **[`VoiceThrottle`]** follows the readings in a task of its own, so
//!   the baseline can come from the readings before a download starts,
//!   and enforces the rate with a token bucket on the chunk reads.

use std::collections::VecDeque;
use std::fmt;
use std::future::Future;
use std::pin::{Pin, pin};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use tokio::sync::{Notify, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use super::Throttle;
use super::download::CHUNK_TIMEOUT;

/// The baseline is the median ping of this many readings.
const BASELINE_READINGS: usize = 5;
/// The rate a download starts at in a call.
const START_RATE: u64 = 256 << 10;
/// Halving stops here; hurting at it pauses.
const FLOOR_RATE: u64 = 32 << 10;
/// A reading hurts with its ping this far over the baseline…
const HURT_PING_MS: u32 = 30;
/// …or with more than this much lost coming in.
const HURT_LOSS_PERCENT: f32 = 2.0;
/// Calm readings in a row that raise the rate by a quarter.
const CALM_TO_GROW: u8 = 2;
/// Calm readings in a row that end a pause at the floor.
const CALM_TO_RESUME: u8 = 3;
/// No chunk has waited on the throttle for this long: the download is
/// over. A chunk read between two takes lasts at most [`CHUNK_TIMEOUT`].
const IDLE_AFTER: Duration = Duration::from_secs(2 * CHUNK_TIMEOUT.as_secs());

/// One second of a call's stats, as far as the throttle cares.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoiceReading {
    pub ping_ms: Option<u32>,
    pub loss_in_percent: Option<f32>,
}

/// The call's latest reading, or `None` outside a call.
static READINGS: LazyLock<watch::Sender<Option<VoiceReading>>> =
    LazyLock::new(|| watch::Sender::new(None));

/// A connected call's reading (`media.rs`, once a second), or `None` when
/// its session ends.
pub(crate) fn publish_voice_reading(reading: Option<VoiceReading>) {
    // Kept even with no one following, so a throttle made later starts
    // from the call's state.
    READINGS.send_replace(reading);
}

/// The readings, for [`VoiceThrottle::new`].
pub fn voice_readings() -> watch::Receiver<Option<VoiceReading>> {
    READINGS.subscribe()
}

/// How fast the download may go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Rate {
    Unlimited,
    /// Bytes a second.
    Limited(u64),
    Paused,
}

impl fmt::Display for Rate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unlimited => f.write_str("unlimited"),
            Self::Limited(rate) => write!(f, "{} KiB/s", rate >> 10),
            Self::Paused => f.write_str("paused"),
        }
    }
}

/// Where the call is, for the throttle.
#[derive(Debug, Default)]
enum Call {
    #[default]
    Out,
    /// In a call with no baseline yet: its pings so far, oldest first.
    /// While nothing downloads, only the latest [`BASELINE_READINGS`] are
    /// kept, so a download takes its baseline from the readings just
    /// before it.
    Gathering(VecDeque<u32>),
    Limited {
        baseline: u32,
        rate: u64,
        calm: u8,
    },
    /// Hurting at the floor.
    Paused {
        baseline: u32,
        calm: u8,
    },
}

/// Turns readings into a rate, following the design's rules.
#[derive(Debug, Default)]
pub(crate) struct Controller {
    downloading: bool,
    call: Call,
}

impl Controller {
    pub(crate) fn rate(&self) -> Rate {
        match &self.call {
            Call::Out => Rate::Unlimited,
            Call::Gathering(_) | Call::Paused { .. } => Rate::Paused,
            Call::Limited { rate, .. } => Rate::Limited(*rate),
        }
    }

    pub(crate) fn downloading(&self) -> bool {
        self.downloading
    }

    /// A download starts: at full speed outside a call, at
    /// [`START_RATE`] with a baseline, or paused until it has one.
    pub(crate) fn start(&mut self) -> Rate {
        self.downloading = true;
        self.take_baseline();
        self.rate()
    }

    /// The download is over. What a call did meanwhile says nothing about
    /// the next one, which starts again from a baseline.
    pub(crate) fn stop(&mut self) {
        self.downloading = false;
        if !matches!(self.call, Call::Out) {
            self.call = Call::Gathering(VecDeque::new());
        }
    }

    pub(crate) fn on_reading(&mut self, reading: Option<VoiceReading>) -> Rate {
        let Some(reading) = reading else {
            // Leaving the call: full speed at once.
            self.call = Call::Out;
            return self.rate();
        };
        if matches!(self.call, Call::Out) {
            self.call = Call::Gathering(VecDeque::new());
        }
        // A reading with no ping is neither calm nor hurting.
        let Some(ping) = reading.ping_ms else {
            return self.rate();
        };
        match &mut self.call {
            Call::Out => {}
            Call::Gathering(pings) => {
                pings.push_back(ping);
                if !self.downloading && pings.len() > BASELINE_READINGS {
                    pings.pop_front();
                }
                if self.downloading {
                    self.take_baseline();
                }
            }
            Call::Limited {
                baseline,
                rate,
                calm,
            } => {
                if hurts(*baseline, ping, reading.loss_in_percent) {
                    self.call = if *rate <= FLOOR_RATE {
                        Call::Paused {
                            baseline: *baseline,
                            calm: 0,
                        }
                    } else {
                        Call::Limited {
                            baseline: *baseline,
                            rate: (*rate / 2).max(FLOOR_RATE),
                            calm: 0,
                        }
                    };
                } else {
                    *calm += 1;
                    if *calm == CALM_TO_GROW {
                        *rate = rate.saturating_add(*rate / 4);
                        *calm = 0;
                    }
                }
            }
            Call::Paused { baseline, calm } => {
                if hurts(*baseline, ping, reading.loss_in_percent) {
                    *calm = 0;
                } else {
                    *calm += 1;
                    if *calm == CALM_TO_RESUME {
                        self.call = Call::Limited {
                            baseline: *baseline,
                            rate: FLOOR_RATE,
                            calm: 0,
                        };
                    }
                }
            }
        }
        self.rate()
    }

    /// Gathering with [`BASELINE_READINGS`] pings in: the median is the
    /// baseline, and the download goes on at [`START_RATE`].
    fn take_baseline(&mut self) {
        let Call::Gathering(pings) = &self.call else {
            return;
        };
        if pings.len() < BASELINE_READINGS {
            return;
        }
        let mut sorted: Vec<u32> = pings.iter().copied().collect();
        sorted.sort_unstable();
        self.call = Call::Limited {
            baseline: sorted[sorted.len() / 2],
            rate: START_RATE,
            calm: 0,
        };
    }
}

/// Whether a reading with a ping hurts the call.
fn hurts(baseline: u32, ping: u32, loss_in_percent: Option<f32>) -> bool {
    let lossy = loss_in_percent.is_some_and(|loss| loss.is_finite() && loss > HURT_LOSS_PERCENT);
    ping > baseline.saturating_add(HURT_PING_MS) || lossy
}

/// Enforces [`Rate::Limited`]: it holds up to one second of the rate, and
/// a take bigger than that goes into debt, which later takes wait off.
#[derive(Debug)]
struct Bucket {
    rate: Rate,
    tokens: f64,
    at: Instant,
}

impl Bucket {
    fn new(now: Instant) -> Self {
        Self {
            rate: Rate::Unlimited,
            tokens: 0.0,
            at: now,
        }
    }

    fn set(&mut self, rate: Rate, now: Instant) {
        self.refill(now);
        match rate {
            // A limit after full speed starts empty: no burst.
            Rate::Unlimited => self.tokens = 0.0,
            // A debt from before a pause or a change is still owed.
            Rate::Limited(per_second) => self.tokens = self.tokens.min(per_second as f64),
            Rate::Paused => {}
        }
        self.rate = rate;
    }

    fn refill(&mut self, now: Instant) {
        if let Rate::Limited(per_second) = self.rate {
            let earned = per_second as f64 * now.duration_since(self.at).as_secs_f64();
            self.tokens = (self.tokens + earned).min(per_second as f64);
        }
        self.at = now;
    }

    /// Takes `bytes` now, or says how long to wait before trying again:
    /// `None` while paused, until the rate changes.
    fn take(&mut self, bytes: usize, now: Instant) -> Result<(), Option<Duration>> {
        self.refill(now);
        match self.rate {
            Rate::Unlimited => Ok(()),
            Rate::Paused => Err(None),
            Rate::Limited(per_second) => {
                let per_second = per_second as f64;
                let need = (bytes as f64).min(per_second);
                if self.tokens >= need {
                    self.tokens -= bytes as f64;
                    return Ok(());
                }
                // The rate is at least the floor and the tokens finite, so
                // this is a finite, positive number of seconds.
                let seconds = (need - self.tokens) / per_second;
                Err(Some(
                    Duration::try_from_secs_f64(seconds).unwrap_or(Duration::from_secs(1)),
                ))
            }
        }
    }
}

struct Inner {
    controller: Controller,
    bucket: Bucket,
    /// Takes waiting now, and when the last one returned.
    taking: usize,
    last_take: Option<Instant>,
}

impl Inner {
    /// Ends the download once nothing has taken for [`IDLE_AFTER`].
    fn settle(&mut self, now: Instant) {
        let quiet = self
            .last_take
            .is_none_or(|at| now.duration_since(at) >= IDLE_AFTER);
        if self.controller.downloading() && self.taking == 0 && quiet {
            self.controller.stop();
            self.apply(now);
        }
    }

    /// Gives the bucket the controller's rate.
    fn apply(&mut self, now: Instant) {
        let rate = self.controller.rate();
        if rate != self.bucket.rate && self.controller.downloading() {
            tracing::info!(%rate, "update download rate");
        }
        self.bucket.set(rate, now);
    }
}

struct Shared {
    inner: Mutex<Inner>,
    /// Woken whenever a reading changed the rate.
    changed: Notify,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn on_reading(&self, reading: Option<VoiceReading>) {
        let mut inner = self.lock();
        let now = Instant::now();
        inner.settle(now);
        inner.controller.on_reading(reading);
        inner.apply(now);
        drop(inner);
        self.changed.notify_waiters();
    }
}

/// One take in progress; when it returns or is dropped, the download is
/// still on for [`IDLE_AFTER`].
struct Taking<'a> {
    shared: &'a Shared,
}

impl<'a> Taking<'a> {
    fn begin(shared: &'a Shared) -> Self {
        let mut inner = shared.lock();
        let now = Instant::now();
        inner.settle(now);
        if !inner.controller.downloading() {
            let rate = inner.controller.start();
            tracing::info!(%rate, "update download starts");
            // Nothing saved up from the last download: no burst.
            inner.bucket = Bucket::new(now);
            inner.bucket.set(rate, now);
        }
        inner.taking += 1;
        Self { shared }
    }
}

impl Drop for Taking<'_> {
    fn drop(&mut self) {
        let mut inner = self.shared.lock();
        inner.taking -= 1;
        inner.last_take = Some(Instant::now());
    }
}

/// The update download's [`Throttle`] while the user may be in a call.
pub struct VoiceThrottle {
    shared: Arc<Shared>,
    follower: JoinHandle<()>,
}

impl VoiceThrottle {
    /// A throttle following `readings` ([`voice_readings`]) from now on.
    ///
    /// # Panics
    ///
    /// Outside a Tokio runtime: its follower is a task.
    pub fn new(readings: watch::Receiver<Option<VoiceReading>>) -> Self {
        let shared = Arc::new(Shared {
            inner: Mutex::new(Inner {
                controller: Controller::default(),
                bucket: Bucket::new(Instant::now()),
                taking: 0,
                last_take: None,
            }),
            changed: Notify::new(),
        });
        let follower = tokio::spawn(follow(Arc::clone(&shared), readings));
        Self { shared, follower }
    }

    async fn take_bytes(&self, bytes: usize) {
        let _taking = Taking::begin(&self.shared);
        loop {
            // Listening before looking, so a reading in between still wakes.
            let mut changed = pin!(self.shared.changed.notified());
            changed.as_mut().enable();
            let wait = self.shared.lock().bucket.take(bytes, Instant::now());
            match wait {
                Ok(()) => return,
                Err(None) => changed.await,
                Err(Some(wait)) => {
                    tokio::select! {
                        () = changed => {}
                        () = tokio::time::sleep(wait) => {}
                    }
                }
            }
        }
    }
}

impl Drop for VoiceThrottle {
    fn drop(&mut self) {
        self.follower.abort();
    }
}

impl Throttle for VoiceThrottle {
    fn take(&self, bytes: usize) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(self.take_bytes(bytes))
    }
}

/// Feeds every reading to the controller, the one there now first.
async fn follow(shared: Arc<Shared>, mut readings: watch::Receiver<Option<VoiceReading>>) {
    loop {
        let reading = *readings.borrow_and_update();
        shared.on_reading(reading);
        if readings.changed().await.is_err() {
            // Nothing can publish any more, so no call is reporting.
            shared.on_reading(None);
            return;
        }
    }
}

#[cfg(test)]
#[path = "throttle_tests.rs"]
mod tests;
