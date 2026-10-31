//! The session log for the app (D82): started before the core opens its
//! data, written to by Flutter, exported from Settings → About.

use std::path::Path;

use flutter_rust_bridge::frb;

use crate::api::types::CoreError;
use crate::log::{self, Level};

/// One line of the log's header, from the app: its version, track…
pub struct LogFact {
    pub key: String,
    pub value: String,
}

/// This session's file, and the last session's when it didn't end
/// cleanly (it crashed or was killed).
pub struct SessionLogFiles {
    pub current: String,
    pub previous_crashed: Option<String>,
}

pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl From<LogLevel> for Level {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Debug => Level::DEBUG,
            LogLevel::Info => Level::INFO,
            LogLevel::Warn => Level::WARN,
            LogLevel::Error => Level::ERROR,
        }
    }
}

/// Starts this launch's log in `dir` (the app's `logs` folder). The
/// machine's quick facts follow the app's in the header; its graphics
/// cards, audio devices and capture support are written just after, from
/// a thread of their own. Safe to call again: later calls return the
/// first session.
#[frb(sync)]
pub fn session_log_start(dir: String, facts: Vec<LogFact>) -> Result<SessionLogFiles, CoreError> {
    let first = log::session().is_none();
    let mut header: Vec<(String, String)> = facts
        .into_iter()
        .map(|fact| (fact.key, fact.value))
        .collect();
    header.push(("core version".into(), super::system::core_version()));
    header.extend(opencord_media::system::facts());
    let session = log::start(Path::new(&dir), &header).map_err(storage)?;
    if first {
        let _ = std::thread::Builder::new()
            .name("opencord-log-machine".into())
            .spawn(|| {
                for (key, value) in opencord_media::system::machine() {
                    tracing::info!(target: "opencord_core::machine", "{key}: {value}");
                }
            });
    }
    Ok(SessionLogFiles {
        current: session.current.to_string_lossy().into_owned(),
        previous_crashed: session
            .previous_crashed
            .map(|path| path.to_string_lossy().into_owned()),
    })
}

/// A line from the app, under `app::<area>`.
#[frb(sync)]
pub fn session_log_write(level: LogLevel, area: String, message: String) {
    log::write_app(level.into(), &area, &message);
}

/// Copies the session log at `source` to `destination`, flushing this
/// session's first when it's the one.
pub fn session_log_export(source: String, destination: String) -> Result<(), CoreError> {
    if log::session().is_some_and(|session| session.current == Path::new(&source)) {
        log::flush();
    }
    std::fs::copy(&source, &destination).map_err(storage)?;
    Ok(())
}

/// Marks the session as ended cleanly; the app calls it as it quits.
#[frb(sync)]
pub fn session_log_end() {
    log::end();
}

fn storage(error: std::io::Error) -> CoreError {
    CoreError::Storage {
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_copies_flushed_file() {
        let session = crate::log::tests::test_session();
        session_log_write(LogLevel::Info, "about".into(), "exporting soon".into());
        let out = tempfile::tempdir().unwrap();
        let destination = out.path().join("opencord-log.txt");
        session_log_export(
            session.current.to_string_lossy().into_owned(),
            destination.to_string_lossy().into_owned(),
        )
        .unwrap();
        let text = std::fs::read_to_string(destination).unwrap();
        assert!(text.contains("INFO  app::about  exporting soon"), "{text}");
    }

    #[test]
    fn export_to_a_missing_folder_is_an_error() {
        let session = crate::log::tests::test_session();
        let result = session_log_export(
            session.current.to_string_lossy().into_owned(),
            "/nonexistent-folder-for-opencord/x.txt".into(),
        );
        assert!(matches!(result, Err(CoreError::Storage { .. })));
    }
}
