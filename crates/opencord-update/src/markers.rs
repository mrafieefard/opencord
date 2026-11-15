//! App running markers and startup crash detection.

use std::fs;
use std::io;
use std::path::PathBuf;

use semver::Version;
use serde::{Deserialize, Serialize};

use crate::layout::Layout;

/// A marker for app launch or successful startup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mark {
    pub version: Version,
    pub at: u64,
}

impl Layout {
    pub fn app_lock(&self) -> PathBuf {
        self.root().join("app.lock")
    }

    pub fn prepare_lock(&self) -> PathBuf {
        self.root().join("prepare.lock")
    }

    pub fn launching_file(&self) -> PathBuf {
        self.root().join("launching.json")
    }

    pub fn started_file(&self) -> PathBuf {
        self.root().join("started.json")
    }

    pub fn read_launching(&self) -> Option<Mark> {
        let bytes = fs::read(self.launching_file()).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    pub fn write_launching(&self, mark: &Mark) -> io::Result<()> {
        self.write_json(&self.launching_file(), mark)
    }

    pub fn read_started(&self) -> Option<Mark> {
        let bytes = fs::read(self.started_file()).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    pub fn write_started(&self, mark: &Mark) -> io::Result<()> {
        self.write_json(&self.started_file(), mark)
    }

    /// Whether the app is currently running (holds an exclusive lock on app.lock).
    pub fn app_running(&self) -> bool {
        let path = self.app_lock();
        if !path.exists() {
            return false;
        }
        let Ok(file) = fs::OpenOptions::new().read(true).write(true).open(&path) else {
            return false;
        };
        match file.try_lock() {
            Ok(()) => {
                let _ = file.unlock();
                false
            }
            Err(_) => true,
        }
    }

    /// Whether the previous launch crashed before reaching the started mark.
    pub fn last_start_crashed(&self) -> bool {
        let Some(current) = self.current() else {
            return false;
        };
        let Some(launching) = self.read_launching() else {
            return false;
        };
        if launching.version != current {
            return false;
        }
        match self.read_started() {
            Some(started) => started.version != current,
            None => true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::OpenOptions;

    fn fake_app(layout: &Layout, version: &Version) {
        let dir = layout.version_dir(version);
        fs::create_dir_all(&dir).unwrap();
        let exe = crate::layout::app_executable(&dir);
        if let Some(parent) = exe.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(exe, b"app").unwrap();
    }

    // Fails if last_start_crashed returns false when started mark is missing
    #[test]
    fn a_launch_without_a_started_mark_is_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::new(dir.path());
        let version = Version::parse("0.0.1").unwrap();
        fake_app(&layout, &version);

        layout
            .write_launching(&Mark {
                version: version.clone(),
                at: 1000,
            })
            .unwrap();

        assert!(layout.last_start_crashed());
    }

    // Fails if last_start_crashed returns false when started mark has an older version
    #[test]
    fn a_started_mark_for_another_version_is_too() {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::new(dir.path());
        let version_old = Version::parse("0.0.1").unwrap();
        let version_new = Version::parse("0.0.2").unwrap();
        fake_app(&layout, &version_old);
        fake_app(&layout, &version_new);

        layout
            .write_launching(&Mark {
                version: version_new.clone(),
                at: 2000,
            })
            .unwrap();
        layout
            .write_started(&Mark {
                version: version_old,
                at: 1000,
            })
            .unwrap();

        assert!(layout.last_start_crashed());
    }

    // Fails if app_running returns false when app.lock is held
    #[test]
    fn app_running_sees_a_held_lock() {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::new(dir.path());
        fs::create_dir_all(layout.root()).unwrap();

        assert!(!layout.app_running());

        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(layout.app_lock())
            .unwrap();
        lock_file.lock().unwrap();

        assert!(layout.app_running());

        lock_file.unlock().unwrap();
        assert!(!layout.app_running());
    }
}
