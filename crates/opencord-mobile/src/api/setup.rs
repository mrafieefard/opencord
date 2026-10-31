//! Opening the core, its version, events and local settings.

use std::path::Path;

use crate::MobileError;
use crate::events::MobileEvent;
use crate::mobile::{self, mobile};

/// Opens the core's local data in `data_dir` and keeps attachments in
/// `cache_dir`. Safe to call again (after a JavaScript reload); later calls
/// do nothing.
#[uniffi::export]
pub fn core_init(data_dir: String, cache_dir: String) -> Result<(), MobileError> {
    mobile::init(Path::new(&data_dir), Path::new(&cache_dir))
}

#[uniffi::export]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// Starts a new event generation; a `next_events` still waiting under an
/// older one returns an empty batch at once. Call once per JavaScript start.
#[uniffi::export]
pub fn events_attach() -> Result<u64, MobileError> {
    Ok(mobile()?.attach())
}

/// Waits for at least one event, then returns what is ready, up to `max`
/// (1–1024), in order. Empty only when a newer generation has attached.
#[uniffi::export]
pub async fn next_events(generation: u64, max: u32) -> Result<Vec<MobileEvent>, MobileError> {
    Ok(mobile()?.next_events(generation, max).await)
}

/// A setting kept in the core's local store.
#[uniffi::export]
pub fn settings_get(key: String) -> Result<Option<String>, MobileError> {
    Ok(mobile()?.client().setting(&key))
}

/// `None` removes it.
#[uniffi::export]
pub fn settings_set(key: String, value: Option<String>) -> Result<(), MobileError> {
    Ok(mobile()?.client().set_setting(&key, value)?)
}
