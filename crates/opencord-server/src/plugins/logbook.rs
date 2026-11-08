//! A plugin's log (plan Task 7): the last lines in memory for its page,
//! and `log.txt` in its folder, written by a background task so logging
//! never waits on the disk.

use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use opencord_plugins::limits::{LOG_FILE_MAX_BYTES, LOG_LINE_MAX_CHARS, LOG_LINES};
use opencord_proto::v1 as proto;
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;

use crate::state::now_ms;

/// Lines a watcher may fall behind by before it misses some.
const WATCH_BUFFER: usize = 1_024;

pub struct PluginLog {
    plugin_id: String,
    lines: Mutex<Lines>,
    watchers: broadcast::Sender<proto::PluginLogLine>,
    /// To the file writer; `None` without a folder, or once closed.
    file: Mutex<Option<mpsc::Sender<String>>>,
    writer: Mutex<Option<JoinHandle<()>>>,
    /// Lines the writer couldn't keep up with.
    unwritten: Arc<AtomicU64>,
}

#[derive(Default)]
struct Lines {
    kept: VecDeque<proto::PluginLogLine>,
    next_seq: u64,
}

impl PluginLog {
    /// A log kept in memory and, with a folder, in its `log.txt`. Must be
    /// made inside the server's runtime when given a folder.
    pub fn new(plugin_id: &str, folder: Option<PathBuf>) -> Arc<PluginLog> {
        let unwritten = Arc::new(AtomicU64::new(0));
        let (file, writer) = match folder {
            Some(folder) => {
                let (sender, receiver) = mpsc::channel(LOG_LINES);
                let writer = tokio::spawn(write_lines(
                    plugin_id.to_owned(),
                    folder,
                    receiver,
                    unwritten.clone(),
                ));
                (Some(sender), Some(writer))
            }
            None => (None, None),
        };
        Arc::new(PluginLog {
            plugin_id: plugin_id.to_owned(),
            lines: Mutex::new(Lines {
                kept: VecDeque::new(),
                next_seq: 1,
            }),
            watchers: broadcast::channel(WATCH_BUFFER).0,
            file: Mutex::new(file),
            writer: Mutex::new(writer),
            unwritten,
        })
    }

    pub fn push(&self, level: proto::PluginLogLevel, text: &str) {
        let text: String = text.chars().take(LOG_LINE_MAX_CHARS).collect();
        let line = {
            let mut lines = lock(&self.lines);
            let line = proto::PluginLogLine {
                seq: lines.next_seq,
                at_ms: now_ms(),
                level: level as i32,
                text,
            };
            lines.next_seq += 1;
            if lines.kept.len() >= LOG_LINES {
                lines.kept.pop_front();
            }
            lines.kept.push_back(line.clone());
            line
        };
        if let Some(file) = &*lock(&self.file)
            && file.try_send(file_line(&line)).is_err()
        {
            self.unwritten.fetch_add(1, Ordering::Relaxed);
        }
        // Nobody watching is the usual case, not a failure.
        self.watchers.send(line).ok();
    }

    /// Up to `limit` lines before `before` (all of them without it),
    /// oldest first.
    pub fn lines(&self, before: Option<u64>, limit: usize) -> Vec<proto::PluginLogLine> {
        let lines = lock(&self.lines);
        let end = match before {
            Some(before) => lines.kept.partition_point(|line| line.seq < before),
            None => lines.kept.len(),
        };
        let start = end.saturating_sub(limit);
        lines.kept.range(start..end).cloned().collect()
    }

    /// Lines as they are pushed.
    pub fn subscribe(&self) -> broadcast::Receiver<proto::PluginLogLine> {
        self.watchers.subscribe()
    }

    /// Writes what is waiting to `log.txt` and stops writing it; later
    /// lines stay in memory only. Await it before deleting the folder.
    pub async fn close(&self) {
        drop(lock(&self.file).take());
        let writer = lock(&self.writer).take();
        if let Some(writer) = writer
            && let Err(error) = writer.await
        {
            tracing::warn!(plugin = %self.plugin_id, %error, "the plugin's log writer failed");
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn file_line(line: &proto::PluginLogLine) -> String {
    let level = match proto::PluginLogLevel::try_from(line.level) {
        Ok(proto::PluginLogLevel::Warn) => "WARN",
        Ok(proto::PluginLogLevel::Error) => "ERROR",
        Ok(proto::PluginLogLevel::Action) => "ACTION",
        _ => "INFO",
    };
    format!("{} {level} {}\n", utc(line.at_ms), line.text)
}

/// `2026-11-08T12:34:56.789Z`.
fn utc(at_ms: i64) -> String {
    let days = at_ms.div_euclid(86_400_000);
    let ms = at_ms.rem_euclid(86_400_000);
    // Howard Hinnant's days-to-civil.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1_000 % 60,
        ms % 1_000
    )
}

async fn write_lines(
    plugin_id: String,
    folder: PathBuf,
    mut lines: mpsc::Receiver<String>,
    unwritten: Arc<AtomicU64>,
) {
    let mut batch = Vec::new();
    while lines.recv_many(&mut batch, 256).await > 0 {
        let text: String = batch.drain(..).collect();
        let missed = unwritten.swap(0, Ordering::Relaxed);
        let folder = folder.clone();
        let written = tokio::task::spawn_blocking(move || append(&folder, &text, missed)).await;
        match written {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                tracing::warn!(plugin = %plugin_id, %error, "could not write the plugin's log.txt");
            }
            Err(error) => {
                tracing::warn!(plugin = %plugin_id, %error, "the plugin's log writer failed");
            }
        }
    }
}

/// Appends to `log.txt`, first moving it to `log.1.txt` when this would
/// take it past its size.
fn append(folder: &Path, text: &str, missed: u64) -> io::Result<()> {
    fs::create_dir_all(folder)?;
    let path = folder.join("log.txt");
    let size = fs::metadata(&path).map_or(0, |metadata| metadata.len());
    if size > 0 && size + text.len() as u64 > LOG_FILE_MAX_BYTES {
        fs::rename(&path, folder.join("log.1.txt"))?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    if missed > 0 {
        writeln!(
            file,
            "{missed} lines weren't written here: the log was too busy"
        )?;
    }
    file.write_all(text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_dates() {
        assert_eq!(utc(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(utc(1_762_605_296_789), "2025-11-08T12:34:56.789Z");
        assert_eq!(utc(951_782_400_000), "2000-02-29T00:00:00.000Z");
    }

    #[tokio::test]
    async fn the_log_keeps_its_last_lines_and_rotates() {
        let dir = tempfile::tempdir().unwrap();
        let log = PluginLog::new("p", None);
        let mut watcher = log.subscribe();
        log.push(
            proto::PluginLogLevel::Warn,
            &"y".repeat(LOG_LINE_MAX_CHARS + 10),
        );
        let watched = watcher.recv().await.unwrap();
        assert_eq!(watched.seq, 1);
        assert_eq!(watched.text.chars().count(), LOG_LINE_MAX_CHARS);
        for index in 0..LOG_LINES + 1 {
            log.push(proto::PluginLogLevel::Info, &index.to_string());
        }

        let all = log.lines(None, usize::MAX);
        assert_eq!(all.len(), LOG_LINES);
        // The warning and line "0" were pushed out.
        assert_eq!(all[0].text, "1");
        let before = log.lines(Some(all[10].seq), 3);
        let texts: Vec<&str> = before.iter().map(|line| line.text.as_str()).collect();
        assert_eq!(texts, ["8", "9", "10"]);

        let written = PluginLog::new("p", Some(dir.path().to_path_buf()));
        written.push(proto::PluginLogLevel::Warn, "careful");
        written.push(proto::PluginLogLevel::Info, "done");
        written.close().await;
        written.push(proto::PluginLogLevel::Info, "after closing");
        let text = fs::read_to_string(dir.path().join("log.txt")).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].ends_with("Z WARN careful"), "{}", lines[0]);
        assert!(lines[1].ends_with("Z INFO done"), "{}", lines[1]);

        let big = "z".repeat(LOG_LINE_MAX_CHARS);
        let lines = LOG_FILE_MAX_BYTES as usize / LOG_LINE_MAX_CHARS + 10;
        let rotating = PluginLog::new("p", Some(dir.path().to_path_buf()));
        for _ in 0..lines {
            rotating.push(proto::PluginLogLevel::Info, &big);
        }
        rotating.close().await;
        let current = fs::metadata(dir.path().join("log.txt")).unwrap().len();
        let old = fs::metadata(dir.path().join("log.1.txt")).unwrap().len();
        assert!(current < LOG_FILE_MAX_BYTES);
        assert!(old <= LOG_FILE_MAX_BYTES);
    }
}
