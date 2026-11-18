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
    let logs_dir = Path::new(&cache_dir).join("logs");
    let header = [
        (
            "app version".to_owned(),
            env!("CARGO_PKG_VERSION").to_owned(),
        ),
        (
            "system".to_owned(),
            format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        ),
        ("model".to_owned(), phone_model()),
    ];
    let _ = opencord_core::log::start(&logs_dir, &header);

    mobile::init(Path::new(&data_dir), Path::new(&cache_dir))
}

fn phone_model() -> String {
    #[cfg(target_os = "android")]
    {
        if let Ok(prop) = std::fs::read_to_string("/system/build.prop") {
            for line in prop.lines() {
                if let Some(model) = line.strip_prefix("ro.product.model=") {
                    return model.trim().to_owned();
                }
            }
        }
    }
    std::env::var("DEVICE_MODEL").unwrap_or_else(|_| "unknown".to_owned())
}

#[uniffi::export]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// The current session's file path, flushed first.
#[uniffi::export]
pub fn logs_export() -> Result<String, MobileError> {
    opencord_core::log::flush();
    let session = opencord_core::log::session().ok_or_else(|| MobileError::Storage {
        message: "no session log running".to_owned(),
    })?;
    Ok(session.current.to_string_lossy().into_owned())
}

/// The previous session's file path when it ended unexpectedly.
#[uniffi::export]
pub fn logs_crashed() -> Option<String> {
    opencord_core::log::session().and_then(|session| {
        session
            .previous_crashed
            .map(|p| p.to_string_lossy().into_owned())
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    // Fails if core_init does not start a session log or logs_export does not return its path.
    #[test]
    fn a_session_log_starts_at_init() {
        crate::test_support::init_once();

        let exported = logs_export().unwrap();
        assert!(
            exported.ends_with(".log"),
            "exported path should be a log file: {exported}"
        );
        assert!(
            std::path::Path::new(&exported).exists(),
            "log file must exist"
        );
    }

    // Fails if logs_export does not flush pending log lines to disk before returning.
    #[test]
    fn export_flushes_first() {
        crate::test_support::init_once();

        let unique = "test-flush-line-12345";
        tracing::info!("{unique}");

        let exported = logs_export().unwrap();
        let contents = std::fs::read_to_string(&exported).unwrap();
        assert!(
            contents.contains(unique),
            "log file must contain flushed line"
        );
    }
}
