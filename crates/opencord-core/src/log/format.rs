//! One event as one line: `14:32:07.412 +2.118s INFO  media::video  message  key=value`.

use std::fmt::{Debug, Write as _};
use std::time::Duration;

use tracing::Level;
use tracing::field::{Field, Visit};

/// The wall clock in local time.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LocalTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub millis: u16,
}

impl LocalTime {
    /// For file names: `2026-10-30T14-32-05`.
    pub(crate) fn stamp(&self) -> String {
        format!(
            "{:04}-{:02}-{:02}T{:02}-{:02}-{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

#[cfg(unix)]
pub(crate) fn local_now() -> LocalTime {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let seconds = now.as_secs() as libc::time_t;
    // SAFETY: localtime_r writes only into `tm`, which outlives the call.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&seconds, &mut tm) };
    LocalTime {
        year: (tm.tm_year + 1900) as u16,
        month: (tm.tm_mon + 1) as u8,
        day: tm.tm_mday as u8,
        hour: tm.tm_hour as u8,
        minute: tm.tm_min as u8,
        second: tm.tm_sec as u8,
        millis: now.subsec_millis() as u16,
    }
}

#[cfg(windows)]
pub(crate) fn local_now() -> LocalTime {
    // SAFETY: GetLocalTime has no preconditions.
    let time = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    LocalTime {
        year: time.wYear,
        month: time.wMonth as u8,
        day: time.wDay as u8,
        hour: time.wHour as u8,
        minute: time.wMinute as u8,
        second: time.wSecond as u8,
        millis: time.wMilliseconds,
    }
}

/// The time part of a line: the clock, then the time since the session
/// started.
pub(crate) fn prefix(time: &LocalTime, elapsed: Duration) -> String {
    format!(
        "{:02}:{:02}:{:02}.{:03} +{:.3}s ",
        time.hour,
        time.minute,
        time.second,
        time.millis,
        elapsed.as_secs_f64()
    )
}

/// The rest of a line; `fields` is ` key=value…` or empty. Lines after a
/// message's first are indented, so each event still starts a line with
/// its time.
pub(crate) fn body(level: Level, target: &str, message: &str, fields: &str) -> String {
    let mut body = format!(
        "{:<5} {}  {}",
        level.as_str(),
        short_target(target),
        message
    );
    if !fields.is_empty() {
        body.push(' ');
        body.push_str(fields);
    }
    body.replace('\n', "\n    ")
}

/// `opencord_media::video` reads `media::video`.
fn short_target(target: &str) -> &str {
    target.strip_prefix("opencord_").unwrap_or(target)
}

/// An event's message and fields; a `log` crate record's target replaces
/// the event's.
#[derive(Default)]
pub(crate) struct Fields {
    pub message: String,
    pub fields: String,
    pub log_target: Option<String>,
}

impl Visit for Fields {
    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "message" => self.message.push_str(value),
            "log.target" => self.log_target = Some(value.to_owned()),
            name if name.starts_with("log.") => {}
            name if value.contains(char::is_whitespace) || value.is_empty() => {
                let _ = write!(self.fields, " {name}={value:?}");
            }
            name => {
                let _ = write!(self.fields, " {name}={value}");
            }
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        match field.name() {
            "message" => {
                let _ = write!(self.message, "{value:?}");
            }
            name if name.starts_with("log.") => {}
            name => {
                let _ = write!(self.fields, " {name}={value:?}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tracing::Level;

    use super::*;

    #[test]
    fn prefix_has_clock_and_elapsed() {
        let time = LocalTime {
            year: 2026,
            month: 10,
            day: 30,
            hour: 14,
            minute: 32,
            second: 7,
            millis: 412,
        };
        assert_eq!(
            prefix(&time, Duration::from_millis(2118)),
            "14:32:07.412 +2.118s "
        );
        assert_eq!(time.stamp(), "2026-10-30T14-32-07");
    }

    #[test]
    fn body_has_level_target_message_and_fields() {
        assert_eq!(
            body(
                Level::INFO,
                "opencord_media::video::screen::windows",
                "capture api opened",
                " size=2560x1440 api=wgc"
            ),
            "INFO  media::video::screen::windows  capture api opened  size=2560x1440 api=wgc"
        );
        assert_eq!(
            body(Level::WARN, "rustls::conn", "bad", ""),
            "WARN  rustls::conn  bad"
        );
    }

    #[test]
    fn lines_after_the_first_are_indented() {
        assert_eq!(
            body(Level::ERROR, "app::start", "failed\n#0 main", ""),
            "ERROR app::start  failed\n    #0 main"
        );
    }

    #[test]
    fn local_time_is_sane() {
        let now = local_now();
        assert!(now.year >= 2026 && (1..=12).contains(&now.month) && now.hour < 24);
    }
}
