//! `launcher.log` in the install folder: the last run's story, for when an
//! update goes wrong. Each run starts it afresh.

use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static FILE: Mutex<Option<File>> = Mutex::new(None);

pub fn open(path: &Path) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(file) = File::create(path)
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
