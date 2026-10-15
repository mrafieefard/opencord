//! When domains are checked by themselves (Phase 3 plan §6.4): at start-up,
//! every 12 hours with up to 30 minutes of random delay, and right after
//! the public address changes.

use std::sync::Arc;
use std::time::Duration;

use crate::state::AppState;

const EVERY: Duration = Duration::from_secs(12 * 3600);
const JITTER_S: u64 = 30 * 60;
/// Start-up checks wait for the public address lookup a moment.
const AFTER_START: Duration = Duration::from_secs(5);

/// Checks every domain now and then, until the server stops.
pub fn start(state: &Arc<AppState>) {
    let state = Arc::clone(state);
    tokio::spawn(async move {
        let mut addresses = state.network.public_ips();
        addresses.mark_unchanged();
        let mut next = tokio::time::Instant::now() + AFTER_START;
        loop {
            tokio::select! {
                () = tokio::time::sleep_until(next) => {
                    super::free::follow(&state).await;
                    check_all(&state);
                    next = tokio::time::Instant::now() + EVERY + jitter();
                }
                changed = addresses.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    // Free subdomains follow first; then everyone is
                    // checked against the new address.
                    super::free::follow(&state).await;
                    check_all(&state);
                }
                () = state.shutdown.cancelled() => return,
            }
        }
    });
}

fn check_all(state: &Arc<AppState>) {
    for domain in state.domains.all() {
        state.domains.start_check(state, domain.id);
    }
}

fn jitter() -> Duration {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).expect("the system has randomness");
    Duration::from_secs(u64::from_ne_bytes(bytes) % JITTER_S)
}
