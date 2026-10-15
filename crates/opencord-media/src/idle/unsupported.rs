//! No idle time on this system.

use std::time::Duration;

pub async fn idle_time() -> Option<Duration> {
    None
}
