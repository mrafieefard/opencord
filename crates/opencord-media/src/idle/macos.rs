//! macOS: seconds since the last input event of any kind, as the HID
//! system sees it (`CGEventSourceSecondsSinceLastEventType`).

use std::time::Duration;

use objc2_core_graphics::{CGEventSource, CGEventSourceStateID, CGEventType};

/// `kCGAnyInputEventType`.
const ANY_INPUT: CGEventType = CGEventType(u32::MAX);

pub async fn idle_time() -> Option<Duration> {
    let seconds = CGEventSource::seconds_since_last_event_type(
        CGEventSourceStateID::HIDSystemState,
        ANY_INPUT,
    );
    Duration::try_from_secs_f64(seconds).ok()
}
