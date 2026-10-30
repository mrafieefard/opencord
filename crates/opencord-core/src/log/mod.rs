//! The session log (D82, docs/logs.md): every launch writes one file in
//! the app's `logs` folder with the core's `tracing` events and the app's
//! own lines, for a user to export and send when something goes wrong.
//!
//! Events are formatted where they happen and handed to one writer thread
//! over a bounded channel, so no audio or video thread waits on the disk;
//! when the channel is full a line is dropped and counted instead.

pub mod alias;
mod crash;
mod format;
mod scrub;
mod writer;

use std::io::{self, BufWriter};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

pub use tracing::Level;
use tracing::Subscriber;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};
use tracing_subscriber::util::SubscriberInitExt;

use format::Fields;
pub use writer::Session;
use writer::{ENDED, Writer};

/// Bytes a session may take before only warnings and errors go in.
const CAP: u64 = 20_000_000;
/// Lines waiting for the writer before new ones are dropped.
const QUEUE: usize = 8192;
/// What gets in: Opencord's own crates at INFO, its media and connection
/// code at DEBUG, and other crates' warnings and errors.
const LEVELS: &str = "warn,opencord_core=info,opencord_media=info,opencord_voice=info,\
    opencord_common=info,opencord_media::video=debug,opencord_media::share_audio=debug,\
    opencord_core::connection=debug,opencord_core::voice=debug,opencord_core::media=debug,\
    opencord_core::video=debug";

enum Message {
    Line {
        level: Level,
        prefix: String,
        body: String,
    },
    Raw(String),
    Flush(mpsc::Sender<()>),
}

struct State {
    session: Session,
    sender: SyncSender<Message>,
    started: Instant,
}

static STATE: OnceLock<State> = OnceLock::new();
static DROPPED: AtomicU64 = AtomicU64::new(0);

/// Starts this launch's session in `dir` with `header`'s lines at the
/// top. A second call returns the first session and does nothing else.
pub fn start(dir: &Path, header: &[(String, String)]) -> io::Result<Session> {
    static STARTING: Mutex<()> = Mutex::new(());
    let _guard = STARTING.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(state) = STATE.get() {
        return Ok(state.session.clone());
    }
    let started = Instant::now();
    let header: Vec<(String, String)> = header
        .iter()
        .map(|(key, value)| (key.clone(), alias::scrub(value, home())))
        .collect();
    let (file, session) = writer::open_session(dir, &header, &format::local_now().stamp())?;
    let crash_file = file.try_clone()?;
    let (sender, receiver) = mpsc::sync_channel(QUEUE);
    std::thread::Builder::new()
        .name("opencord-log".into())
        .spawn(move || run(Writer::new(BufWriter::new(file), CAP), receiver))?;
    let _ = STATE.set(State {
        session: session.clone(),
        sender,
        started,
    });
    crash::install(crash_file, started);
    let filter = EnvFilter::new(LEVELS);
    if tracing_subscriber::registry()
        .with(SessionLayer.with_filter(filter))
        .try_init()
        .is_err()
    {
        write_app(
            Level::WARN,
            "log",
            "another tracing subscriber was set; core events are missing",
        );
    }
    Ok(session)
}

/// A line from the app (Flutter): `app::<area>` is its target.
pub fn write_app(level: Level, area: &str, message: &str) {
    send(level, &format!("app::{area}"), message, "");
}

/// Waits (up to 2 s) until everything sent so far is on disk.
pub fn flush() {
    let Some(state) = STATE.get() else {
        return;
    };
    let (done, wait) = mpsc::channel();
    if state.sender.send(Message::Flush(done)).is_ok() {
        let _ = wait.recv_timeout(Duration::from_secs(2));
    }
}

/// Marks the session as having ended cleanly, and flushes.
pub fn end() {
    let Some(state) = STATE.get() else {
        return;
    };
    let _ = state.sender.send(Message::Raw(format!(
        "{}{}",
        now_prefix(),
        format::body(Level::INFO, "log", ENDED, "")
    )));
    flush();
}

/// This session, once started.
pub fn session() -> Option<Session> {
    STATE.get().map(|state| state.session.clone())
}

fn send(level: Level, target: &str, message: &str, fields: &str) {
    let Some(state) = STATE.get() else {
        return;
    };
    let body = alias::scrub(&format::body(level, target, message, fields), home());
    let line = Message::Line {
        level,
        prefix: now_prefix(),
        body,
    };
    if state.sender.try_send(line).is_err() {
        DROPPED.fetch_add(1, Ordering::Relaxed);
    }
}

/// The user's home folder, which reads `~` in the log.
fn home() -> Option<&'static str> {
    static HOME: OnceLock<Option<String>> = OnceLock::new();
    HOME.get_or_init(|| {
        std::env::var(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
            .ok()
            .filter(|home| home.len() > 1)
    })
    .as_deref()
}

/// The time part of a line written now.
fn now_prefix() -> String {
    let elapsed = STATE
        .get()
        .map(|state| state.started.elapsed())
        .unwrap_or_default();
    format::prefix(&format::local_now(), elapsed)
}

fn run<W: io::Write>(mut writer: Writer<W>, receiver: Receiver<Message>) {
    loop {
        let message = receiver.recv_timeout(Duration::from_secs(1));
        let dropped = DROPPED.swap(0, Ordering::Relaxed);
        if dropped > 0 {
            writer.write_raw(&format!(
                "{}{}",
                now_prefix(),
                format::body(
                    Level::WARN,
                    "log",
                    &format!("{dropped} lines were dropped: the log fell behind"),
                    ""
                )
            ));
        }
        match message {
            Ok(Message::Line {
                level,
                prefix,
                body,
            }) => {
                writer.write(level, &body, &prefix, Instant::now());
                if level <= Level::WARN {
                    let _ = writer.flush();
                }
            }
            Ok(Message::Raw(line)) => {
                writer.write_raw(&line);
                let _ = writer.flush();
            }
            Ok(Message::Flush(done)) => {
                let _ = writer.flush();
                let _ = done.send(());
            }
            Err(RecvTimeoutError::Timeout) => {
                writer.tick(Instant::now());
                let _ = writer.flush();
            }
            Err(RecvTimeoutError::Disconnected) => {
                let _ = writer.flush();
                return;
            }
        }
    }
}

/// Sends every event that passes the filter to the writer.
struct SessionLayer;

impl<S: Subscriber> Layer<S> for SessionLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _context: Context<'_, S>) {
        let mut fields = Fields::default();
        event.record(&mut fields);
        let metadata = event.metadata();
        let target = fields.log_target.as_deref().unwrap_or(metadata.target());
        send(*metadata.level(), target, &fields.message, &fields.fields);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::OnceLock;

    use super::*;

    /// One session for the whole test binary: the subscriber is global.
    pub(crate) fn test_session() -> &'static Session {
        static DIR: OnceLock<tempfile::TempDir> = OnceLock::new();
        static SESSION: OnceLock<Session> = OnceLock::new();
        SESSION.get_or_init(|| {
            let dir = DIR.get_or_init(|| tempfile::tempdir().unwrap());
            start(dir.path(), &[("app version".into(), "test".into())]).unwrap()
        })
    }

    #[test]
    fn second_start_returns_same_session() {
        let session = test_session().clone();
        let other = tempfile::tempdir().unwrap();
        assert_eq!(start(other.path(), &[]).unwrap(), session);
        assert_eq!(std::fs::read_dir(other.path()).unwrap().count(), 0);
    }

    #[test]
    fn events_and_app_lines_reach_the_file() {
        let session = test_session();
        tracing::info!(target: "opencord_core::connection", step = "tls", "connecting");
        tracing::debug!(target: "hyper::proto", "chatter from another crate");
        write_app(Level::WARN, "share", "picked a source");
        flush();
        let text = std::fs::read_to_string(&session.current).unwrap();
        assert!(
            text.contains("INFO  core::connection  connecting  step=tls"),
            "{text}"
        );
        assert!(text.contains("WARN  app::share  picked a source"), "{text}");
        assert!(!text.contains("chatter"), "{text}");
    }

    #[test]
    fn lines_are_scrubbed_before_the_disk() {
        let session = test_session();
        tracing::warn!(target: "reqwest::connect", "error for url (https://secret-host.example.org/x?token=1)");
        write_app(Level::INFO, "servers", "added 203.0.113.9:7700");
        flush();
        let text = std::fs::read_to_string(&session.current).unwrap();
        assert!(!text.contains("example.org"), "{text}");
        assert!(!text.contains("203.0.113.9"), "{text}");
        assert!(text.contains("added public-ip-"), "{text}");
    }
}
