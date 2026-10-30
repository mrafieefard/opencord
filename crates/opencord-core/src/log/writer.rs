//! The session's file: opened with its header, older sessions pruned, and
//! written line by line with repeats folded and a size cap.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tracing::Level;

/// Sessions kept, the new one included.
const KEPT: usize = 10;
/// The last line of a session that ended cleanly.
pub(crate) const ENDED: &str = "session ended";
/// How long an identical line keeps being folded into the one before.
const FOLD: Duration = Duration::from_secs(1);

/// This session's file, and the last one's if it didn't end cleanly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub current: PathBuf,
    pub previous_crashed: Option<PathBuf>,
}

/// Creates `dir`, keeps the newest sessions so this one makes [`KEPT`],
/// and opens the new file named after `stamp` (`2026-10-30T14-32-05`)
/// with its header written.
pub(crate) fn open_session(
    dir: &Path,
    header: &[(String, String)],
    stamp: &str,
) -> io::Result<(File, Session)> {
    fs::create_dir_all(dir)?;
    let mut old: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| is_session(path))
        .collect();
    old.sort();
    let previous_crashed = old.last().filter(|path| !ended_cleanly(path)).cloned();
    let excess = old.len().saturating_sub(KEPT - 1);
    for path in &old[..excess] {
        let _ = fs::remove_file(path);
    }

    let (mut file, current) = create_new(dir, stamp)?;
    let mut text = String::from("=== Opencord session log ===\n");
    text.push_str(&format!("started: {}\n", readable(stamp)));
    for (key, value) in header {
        // A value over several lines (the launcher's log) is indented
        // under its key.
        if value.contains('\n') {
            text.push_str(&format!("{key}:\n"));
            for line in value.lines() {
                text.push_str(&format!("    {line}\n"));
            }
        } else {
            text.push_str(&format!("{key}: {value}\n"));
        }
    }
    text.push_str("===\n");
    file.write_all(text.as_bytes())?;
    Ok((
        file,
        Session {
            current,
            previous_crashed,
        },
    ))
}

fn is_session(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("opencord-") && name.ends_with(".log"))
}

/// A new file for `stamp`; a second session in the same second gets `-2`.
fn create_new(dir: &Path, stamp: &str) -> io::Result<(File, PathBuf)> {
    for attempt in 1.. {
        let name = match attempt {
            1 => format!("opencord-{stamp}.log"),
            n => format!("opencord-{stamp}-{n}.log"),
        };
        let path = dir.join(name);
        match OpenOptions::new().create_new(true).append(true).open(&path) {
            Ok(file) => return Ok((file, path)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists && attempt < 100 => {}
            Err(error) => return Err(error),
        }
    }
    unreachable!("the loop returns")
}

fn ended_cleanly(path: &Path) -> bool {
    let Ok(mut file) = File::open(path) else {
        return true;
    };
    let length = file.metadata().map(|meta| meta.len()).unwrap_or(0);
    let _ = file.seek(SeekFrom::Start(length.saturating_sub(256)));
    let mut tail = Vec::new();
    let _ = file.read_to_end(&mut tail);
    String::from_utf8_lossy(&tail).trim_end().ends_with(ENDED)
}

/// `2026-10-30T14-32-05` as `2026-10-30 14:32:05`.
fn readable(stamp: &str) -> String {
    match stamp.split_once('T') {
        Some((date, time)) => format!("{date} {}", time.replace('-', ":")),
        None => stamp.to_owned(),
    }
}

/// Lines into `out`: an identical line within [`FOLD`] of the one written
/// is counted instead, and past `cap` bytes only warnings and errors go in.
pub(crate) struct Writer<W: Write> {
    out: W,
    cap: u64,
    written: u64,
    capped: bool,
    /// The body of the last line written, when it was written, and how
    /// many times it has come again since.
    last: Option<(String, Instant, u32)>,
}

impl<W: Write> Writer<W> {
    pub(crate) fn new(out: W, cap: u64) -> Self {
        Self {
            out,
            cap,
            written: 0,
            capped: false,
            last: None,
        }
    }

    /// Writes `prefix` (the time) and `body` (the level onwards).
    pub(crate) fn write(&mut self, level: Level, body: &str, prefix: &str, now: Instant) {
        if let Some((last, at, repeats)) = &mut self.last
            && last == body
            && now.duration_since(*at) < FOLD
        {
            *repeats += 1;
            return;
        }
        self.settle_repeats();
        // tracing orders levels by verbosity: WARN and ERROR are the
        // smallest.
        if self.capped && level > Level::WARN {
            return;
        }
        if !self.capped && self.written >= self.cap {
            self.capped = true;
            self.put(&format!(
                "{prefix}WARN  log  size limit reached ({} MB); only warnings and errors from now on",
                self.cap / 1_000_000
            ));
            if level > Level::WARN {
                return;
            }
        }
        self.put(&format!("{prefix}{body}"));
        self.last = Some((body.to_owned(), now, 0));
    }

    /// Writes a pending repeat count once its line is [`FOLD`] old, so a
    /// loop that never stops still shows up.
    pub(crate) fn tick(&mut self, now: Instant) {
        if self
            .last
            .as_ref()
            .is_some_and(|(_, at, _)| now.duration_since(*at) >= FOLD)
        {
            self.settle_repeats();
        }
    }

    /// Writes a line as it is, past folding and the cap: crash reports
    /// and the session's end.
    pub(crate) fn write_raw(&mut self, line: &str) {
        self.settle_repeats();
        self.put(line);
    }

    pub(crate) fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }

    fn settle_repeats(&mut self) {
        if let Some((_, _, repeats)) = self.last.take()
            && repeats > 0
        {
            self.put(&format!("    (repeated {repeats} times)"));
        }
    }

    fn put(&mut self, line: &str) {
        if self.out.write_all(line.as_bytes()).is_ok() && self.out.write_all(b"\n").is_ok() {
            self.written += line.len() as u64 + 1;
        }
    }

    #[cfg(test)]
    pub(crate) fn into_inner(mut self) -> W {
        self.settle_repeats();
        self.out
    }
}
#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{Duration, Instant};

    use tracing::Level;

    use super::*;

    const HEADER: &[(&str, &str)] = &[("app version", "1.2.3")];

    fn header() -> Vec<(String, String)> {
        HEADER
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    fn names(dir: &std::path::Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn rotation_keeps_ten() {
        let dir = tempfile::tempdir().unwrap();
        for day in 10..22 {
            fs::write(
                dir.path()
                    .join(format!("opencord-2026-10-{day}T10-00-00.log")),
                "x  session ended\n",
            )
            .unwrap();
        }
        fs::write(dir.path().join("notes.txt"), "kept").unwrap();
        let (_, session) = open_session(dir.path(), &header(), "2026-10-30T14-32-05").unwrap();
        let names = names(dir.path());
        assert_eq!(
            names.iter().filter(|name| name.ends_with(".log")).count(),
            10
        );
        assert!(names.contains(&"notes.txt".to_string()));
        assert!(names.contains(&"opencord-2026-10-21T10-00-00.log".to_string()));
        assert!(!names.contains(&"opencord-2026-10-12T10-00-00.log".to_string()));
        assert_eq!(
            session.current,
            dir.path().join("opencord-2026-10-30T14-32-05.log")
        );
    }

    #[test]
    fn unclean_previous_session_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("opencord-2026-10-29T10-00-00.log"),
            "x  session ended\n",
        )
        .unwrap();
        let crashed = dir.path().join("opencord-2026-10-30T10-00-00.log");
        fs::write(&crashed, "x  sharing a screen\n").unwrap();
        let (_, session) = open_session(dir.path(), &header(), "2026-10-30T14-32-05").unwrap();
        assert_eq!(session.previous_crashed, Some(crashed));

        let (_, next) = open_session(dir.path(), &header(), "2026-10-30T15-00-00").unwrap();
        // The session before it has only its header: still unclean, so
        // reported; a clean one is not.
        assert!(next.previous_crashed.is_some());
        fs::write(&session.current, "x  session ended\n").unwrap();
        fs::remove_file(&next.current).unwrap();
        let (_, last) = open_session(dir.path(), &header(), "2026-10-30T16-00-00").unwrap();
        assert_eq!(last.previous_crashed, None);
    }

    #[test]
    fn header_comes_first() {
        let dir = tempfile::tempdir().unwrap();
        let (_, session) = open_session(dir.path(), &header(), "2026-10-30T14-32-05").unwrap();
        let text = fs::read_to_string(session.current).unwrap();
        assert!(text.starts_with("=== Opencord session log ===\n"), "{text}");
        assert!(text.contains("\napp version: 1.2.3\n"), "{text}");
        assert!(text.contains("\nstarted: 2026-10-30 14:32:05\n"), "{text}");
    }

    #[test]
    fn same_second_gets_its_own_file() {
        let dir = tempfile::tempdir().unwrap();
        let (_, first) = open_session(dir.path(), &header(), "2026-10-30T14-32-05").unwrap();
        let (_, second) = open_session(dir.path(), &header(), "2026-10-30T14-32-05").unwrap();
        assert_ne!(first.current, second.current);
    }

    #[test]
    fn start_in_unwritable_folder_returns_error_and_app_continues() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("logs");
        fs::write(&file, "not a folder").unwrap();
        assert!(open_session(&file, &header(), "2026-10-30T14-32-05").is_err());
    }

    #[test]
    fn cap_switches_to_warnings() {
        let mut writer = Writer::new(Vec::new(), 1000);
        let now = Instant::now();
        for index in 0..100 {
            writer.write(Level::INFO, &format!("info line number {index}"), "", now);
        }
        writer.write(Level::WARN, "a warning", "", now);
        writer.write(Level::INFO, "late info", "", now);
        let text = String::from_utf8(writer.into_inner()).unwrap();
        assert!(text.len() < 1300, "{}", text.len());
        assert_eq!(text.matches("size limit reached").count(), 1, "{text}");
        assert!(text.contains("a warning"));
        assert!(!text.contains("late info"));
    }

    #[test]
    fn repeats_fold() {
        let mut writer = Writer::new(Vec::new(), u64::MAX);
        let start = Instant::now();
        for step in 0..5 {
            writer.write(
                Level::WARN,
                "a frame could not be read",
                &format!("10:00:00.{step:03} "),
                start + Duration::from_millis(step * 100),
            );
        }
        writer.write(
            Level::INFO,
            "something else",
            "",
            start + Duration::from_millis(600),
        );
        let text = String::from_utf8(writer.into_inner()).unwrap();
        assert_eq!(
            text.matches("a frame could not be read").count(),
            1,
            "{text}"
        );
        assert!(text.contains("(repeated 4 times)"), "{text}");
        assert!(
            text.find("(repeated").unwrap() < text.find("something else").unwrap(),
            "{text}"
        );
    }

    #[test]
    fn repeats_are_counted_when_a_second_passes() {
        let mut writer = Writer::new(Vec::new(), u64::MAX);
        let start = Instant::now();
        writer.write(Level::WARN, "again", "", start);
        writer.write(Level::WARN, "again", "", start + Duration::from_millis(10));
        writer.tick(start + Duration::from_millis(1100));
        writer.write(
            Level::WARN,
            "again",
            "",
            start + Duration::from_millis(1200),
        );
        let text = String::from_utf8(writer.into_inner()).unwrap();
        assert!(text.contains("(repeated 1 times)"), "{text}");
        assert_eq!(text.matches("again").count(), 2, "{text}");
    }
}
