//! How much of a CPU core a plugin has used lately (design §26.8).

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use opencord_plugins::limits::{CPU_SHARE_MAX_PERMILLE, CPU_SHARE_WINDOW};

/// Samples closer together than this are one sample.
const RESOLUTION: Duration = Duration::from_millis(100);

/// A plugin's use of the CPU over the last [`CPU_SHARE_WINDOW`].
#[derive(Default)]
pub struct CpuShare {
    /// (when, Lua time used in total by then), oldest first.
    samples: VecDeque<(Instant, Duration)>,
    used: Duration,
}

impl CpuShare {
    /// `used` is the Lua time the plugin has used since it started.
    pub fn record(&mut self, now: Instant, used: Duration) {
        self.used = used;
        let is_new = self
            .samples
            .back()
            .is_none_or(|&(at, _)| now.saturating_duration_since(at) >= RESOLUTION);
        if is_new {
            self.samples.push_back((now, used));
        }
        // One sample at or before the window's start stays, as its baseline.
        while self
            .samples
            .get(1)
            .is_some_and(|&(at, _)| now.saturating_duration_since(at) >= CPU_SHARE_WINDOW)
        {
            self.samples.pop_front();
        }
    }

    /// How long the plugin's next event should wait: nothing under its
    /// share; past it, long enough to pay the excess back at its share.
    pub fn pause_for(&self) -> Duration {
        let Some(&(_, before)) = self.samples.front() else {
            return Duration::ZERO;
        };
        let allowed = CPU_SHARE_WINDOW.mul_f64(f64::from(CPU_SHARE_MAX_PERMILLE) / 1000.0);
        let excess = self.used.saturating_sub(before).saturating_sub(allowed);
        excess
            .mul_f64(1000.0 / f64::from(CPU_SHARE_MAX_PERMILLE))
            .min(CPU_SHARE_WINDOW)
    }
}

/// Chat and voice come first when the machine is busy. Best effort: a
/// system that refuses leaves the thread as it was.
pub fn lower_this_threads_priority() {
    if let Err(error) = lower_priority() {
        tracing::warn!(%error, "could not lower a plugin thread's priority");
    }
}

#[cfg(target_os = "linux")]
fn lower_priority() -> std::io::Result<()> {
    const NICE: libc::c_int = 10;
    // SAFETY: plain libc call with no pointers. On Linux `who` 0 with
    // PRIO_PROCESS is the calling thread, not the whole process.
    let current = unsafe { libc::getpriority(libc::PRIO_PROCESS, 0) };
    // SAFETY: as above.
    let done = unsafe { libc::setpriority(libc::PRIO_PROCESS, 0, current + NICE) };
    if done == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(target_os = "macos")]
fn lower_priority() -> std::io::Result<()> {
    // SAFETY: plain libc call with no pointers; it changes the calling thread.
    let done =
        unsafe { libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_UTILITY, 0) };
    if done == 0 {
        Ok(())
    } else {
        Err(std::io::Error::from_raw_os_error(done))
    }
}

#[cfg(windows)]
fn lower_priority() -> std::io::Result<()> {
    use std::ffi::c_void;

    const THREAD_PRIORITY_BELOW_NORMAL: i32 = -1;
    unsafe extern "system" {
        fn GetCurrentThread() -> *mut c_void;
        fn SetThreadPriority(thread: *mut c_void, priority: i32) -> i32;
    }
    // SAFETY: GetCurrentThread returns a pseudo-handle for this thread, which
    // SetThreadPriority takes; neither keeps it.
    let done = unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL) };
    if done != 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn lower_priority() -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: Duration = Duration::from_secs(1);

    #[test]
    fn under_the_share_nothing_waits() {
        let start = Instant::now();
        let mut share = CpuShare::default();

        share.record(start, Duration::ZERO);
        share.record(start + 4 * SECOND, 2 * SECOND);

        assert_eq!(share.pause_for(), Duration::ZERO);
    }

    #[test]
    fn a_quiet_plugin_is_not_held_up() {
        let start = Instant::now();
        let mut share = CpuShare::default();

        for second in 0..60 {
            share.record(start + second * SECOND, second * Duration::from_millis(100));
            assert_eq!(share.pause_for(), Duration::ZERO);
        }
    }

    #[test]
    fn over_the_share_waits_until_it_is_back_under() {
        let start = Instant::now();
        let mut share = CpuShare::default();
        share.record(start, Duration::ZERO);

        share.record(start + 3 * SECOND, 3 * SECOND);

        // Half a second over, paid back at a quarter of a core.
        assert_eq!(share.pause_for(), 2 * SECOND);
    }

    #[test]
    fn the_wait_is_never_longer_than_the_window() {
        let start = Instant::now();
        let mut share = CpuShare::default();
        share.record(start, Duration::ZERO);

        share.record(start + SECOND, 60 * SECOND);

        assert_eq!(share.pause_for(), CPU_SHARE_WINDOW);
    }

    #[test]
    fn an_old_burst_leaves_the_window() {
        let start = Instant::now();
        let mut share = CpuShare::default();
        share.record(start, Duration::ZERO);
        share.record(start + SECOND, 5 * SECOND);
        assert!(share.pause_for() > Duration::ZERO);

        share.record(start + 30 * SECOND, 5 * SECOND);

        assert_eq!(share.pause_for(), Duration::ZERO);
    }

    #[test]
    fn busy_loops_dont_grow_the_samples_without_bound() {
        let start = Instant::now();
        let mut share = CpuShare::default();

        for millisecond in 0..60_000 {
            share.record(start + Duration::from_millis(millisecond), Duration::ZERO);
        }

        assert!(share.samples.len() <= 102, "{}", share.samples.len());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn lowering_changes_only_this_thread() {
        // SAFETY: getpriority only reads; `who` 0 is the calling thread on Linux.
        let nice = || unsafe { libc::getpriority(libc::PRIO_PROCESS, 0) };
        let before = nice();

        let lowered = std::thread::spawn(move || {
            lower_this_threads_priority();
            nice()
        })
        .join()
        .unwrap();

        assert_eq!(lowered, before + 10);
        assert_eq!(nice(), before);
    }
}
