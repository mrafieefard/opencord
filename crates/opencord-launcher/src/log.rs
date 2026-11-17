//! `launcher.log` in the install folder: the last run's story, for when an
//! update goes wrong. Each run starts it afresh.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static FILE: Mutex<Option<File>> = Mutex::new(None);

pub fn open(path: &Path) {
    open_with(path, false);
}

/// Adds to the log the last start began: a prepare runs while the app that
/// start began is open, and its story is still wanted.
pub fn append(path: &Path) {
    open_with(path, true);
}

fn open_with(path: &Path, append: bool) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(path);
    if let Ok(file) = file
        && let Ok(mut slot) = FILE.lock()
    {
        *slot = Some(file);
    }
}

pub fn line(text: impl AsRef<str>) {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_secs_f64())
        .unwrap_or_default();
    let line = format!("[{seconds:.3}] {}\n", text.as_ref());
    eprint!("{line}");
    if let Ok(mut slot) = FILE.lock()
        && let Some(file) = slot.as_mut()
    {
        let _ = file.write_all(line.as_bytes());
    }
}
