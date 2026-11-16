//! The waiting update and download notes.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use semver::Version;
use serde::{Deserialize, Serialize};

use crate::layout::Layout;
use crate::restart::RestartNote;
use crate::track::Track;

/// How long a waiting update stays fresh.
pub const FRESH_FOR: Duration = Duration::from_secs(24 * 60 * 60);

/// A downloaded and prepared update waiting for restart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pending {
    pub version: Version,
    pub track: String,
    pub prepared_at: u64,
    pub confirmed_at: Option<u64>,
}

impl Pending {
    /// Whether this waiting update was prepared or confirmed recently.
    pub fn is_fresh(&self, now: u64) -> bool {
        let reference = self.prepared_at.max(self.confirmed_at.unwrap_or(0));
        now.saturating_sub(reference) < FRESH_FOR.as_millis() as u64
    }
}

/// A download note written beside `downloads/<file>`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadNote {
    pub tag: String,
    pub version: Version,
    pub track: String,
    pub file: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

impl Layout {
    pub fn pending_file(&self) -> PathBuf {
        self.root().join("pending.json")
    }

    pub fn read_pending(&self) -> Option<Pending> {
        let bytes = fs::read(self.pending_file()).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    pub fn write_pending(&self, pending: &Pending) -> io::Result<()> {
        self.write_json(&self.pending_file(), pending)
    }

    pub fn remove_pending(&self) -> io::Result<()> {
        match fs::remove_file(self.pending_file()) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Sets `confirmed_at` only when the waiting version matches `version`.
    pub fn confirm_pending(&self, version: &Version, now: u64) -> io::Result<bool> {
        let Some(mut pending) = self.read_pending() else {
            return Ok(false);
        };
        if pending.version != *version {
            return Ok(false);
        }
        pending.confirmed_at = Some(now);
        self.write_pending(&pending)?;
        Ok(true)
    }

    pub fn read_download_note(&self, path: &Path) -> Option<DownloadNote> {
        let bytes = fs::read(path).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    pub fn write_download_note(&self, note: &DownloadNote) -> io::Result<()> {
        if note.file.contains('/')
            || note.file.contains('\\')
            || note.file.contains("..")
            || note.file.is_empty()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid file name",
            ));
        }
        let path = self.downloads().join(format!("{}.json", note.file));
        self.write_json(&path, note)
    }
}

/// Whether the waiting update should be switched to now.
pub fn switchable(
    layout: &Layout,
    track: Track,
    now: u64,
    restart: Option<&RestartNote>,
) -> Option<Version> {
    let pending = layout.read_pending()?;
    if !layout.is_whole(&pending.version) {
        return None;
    }
    if pending.track != track.name() {
        return None;
    }
    if let Some(current) = layout.current() {
        if current == pending.version {
            return None;
        }
        if Track::of(&current) == Some(track) && current > pending.version {
            return None;
        }
    }
    let restart_matches = restart.is_some_and(|r| r.is_fresh(now) && r.to == pending.version);
    if pending.is_fresh(now) || restart_matches {
        Some(pending.version)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_app(layout: &Layout, version: &Version) {
        let dir = layout.version_dir(version);
        fs::create_dir_all(&dir).unwrap();
        let exe = crate::layout::app_executable(&dir);
        if let Some(parent) = exe.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(exe, b"app").unwrap();
    }

    fn fake_installed(layout: &Layout, version: &Version) {
        fake_app(layout, version);
        layout
            .write_installed(&crate::layout::Installed {
                version: Some(version.clone()),
                track: Some(Track::built_in().name().to_owned()),
            })
            .unwrap();
    }

    // Fails if pending.rs:115 pending.is_fresh(now) || restart_matches becomes restart_matches
    #[test]
    fn a_fresh_waiting_update_is_switchable() {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::new(dir.path());
        let v_installed = Version::parse("0.0.1").unwrap();
        fake_installed(&layout, &v_installed);
        let version = Version::parse("0.0.2").unwrap();
        fake_app(&layout, &version);

        let pending = Pending {
            version: version.clone(),
            track: Track::built_in().name().to_owned(),
            prepared_at: 1000,
            confirmed_at: None,
        };
        layout.write_pending(&pending).unwrap();

        assert_eq!(
            switchable(&layout, Track::built_in(), 2000, None),
            Some(version)
        );
    }

    // Fails if pending.rs:30 now - reference < FRESH_FOR.as_millis() as u64 becomes true
    #[test]
    fn one_older_than_a_day_is_not() {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::new(dir.path());
        let v_installed = Version::parse("0.0.1").unwrap();
        fake_installed(&layout, &v_installed);
        let version = Version::parse("0.0.2").unwrap();
        fake_app(&layout, &version);

        let prepared_at = 1000;
        let day_ms = FRESH_FOR.as_millis() as u64;
        let pending = Pending {
            version: version.clone(),
            track: Track::built_in().name().to_owned(),
            prepared_at,
            confirmed_at: None,
        };
        layout.write_pending(&pending).unwrap();

        let expired = prepared_at + day_ms + 1;
        assert_eq!(switchable(&layout, Track::built_in(), expired, None), None);
    }

    // Fails if pending.rs:29 reference = self.prepared_at.max(self.confirmed_at.unwrap_or(0)) becomes reference = self.prepared_at
    #[test]
    fn a_recent_confirmation_keeps_it_fresh() {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::new(dir.path());
        let v_installed = Version::parse("0.0.1").unwrap();
        fake_installed(&layout, &v_installed);
        let version = Version::parse("0.0.2").unwrap();
        fake_app(&layout, &version);

        let prepared_at = 1000;
        let day_ms = FRESH_FOR.as_millis() as u64;
        let confirmed_at = prepared_at + day_ms + 5000;
        let pending = Pending {
            version: version.clone(),
            track: Track::built_in().name().to_owned(),
            prepared_at,
            confirmed_at: Some(confirmed_at),
        };
        layout.write_pending(&pending).unwrap();

        let now = confirmed_at + 1000;
        assert!(pending.is_fresh(now));
        assert_eq!(
            switchable(&layout, Track::built_in(), now, None),
            Some(version)
        );
    }

    // Fails if pending.rs:115 pending.is_fresh(now) || restart_matches becomes pending.is_fresh(now)
    #[test]
    fn an_update_restart_switches_an_old_one_but_only_its_own_version() {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::new(dir.path());
        let v_installed = Version::parse("0.0.1").unwrap();
        fake_installed(&layout, &v_installed);
        let version = Version::parse("0.0.2").unwrap();
        fake_app(&layout, &version);

        let day_ms = FRESH_FOR.as_millis() as u64;
        let pending = Pending {
            version: version.clone(),
            track: Track::built_in().name().to_owned(),
            prepared_at: 1000,
            confirmed_at: None,
        };
        layout.write_pending(&pending).unwrap();

        let now = 1000 + day_ms + 100_000; // Not fresh
        assert!(!pending.is_fresh(now));

        // Fresh restart note matching this version
        let restart_matching = RestartNote {
            from: Version::parse("0.0.1").unwrap(),
            to: version.clone(),
            at: now - 1000,
            voice: None,
            window: None,
            cover: false,
        };
        assert_eq!(
            switchable(&layout, Track::built_in(), now, Some(&restart_matching)),
            Some(version)
        );

        // Restart note for a different version does NOT allow it
        let restart_other = RestartNote {
            from: Version::parse("0.0.1").unwrap(),
            to: Version::parse("0.0.3").unwrap(),
            at: now - 1000,
            voice: None,
            window: None,
            cover: false,
        };
        assert_eq!(
            switchable(&layout, Track::built_in(), now, Some(&restart_other)),
            None
        );
    }

    // Fails if pending.rs:103 pending.track != track.name() check is removed
    #[test]
    fn a_waiting_update_for_another_track_is_not_switched_to() {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::new(dir.path());
        let version = Version::parse("0.0.2-canary.1").unwrap();
        fake_app(&layout, &version);

        let pending = Pending {
            version,
            track: "canary".to_owned(),
            prepared_at: 1000,
            confirmed_at: None,
        };
        layout.write_pending(&pending).unwrap();

        assert_eq!(switchable(&layout, Track::Beta, 2000, None), None);
    }

    // Fails if pending.rs:100 !layout.is_whole(&pending.version) check is removed
    #[test]
    fn a_half_unpacked_one_is_not_switchable() {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::new(dir.path());
        let version = Version::parse("0.0.2").unwrap();
        // Do NOT create the fake app executable, so is_whole is false

        let pending = Pending {
            version,
            track: Track::built_in().name().to_owned(),
            prepared_at: 1000,
            confirmed_at: None,
        };
        layout.write_pending(&pending).unwrap();

        assert_eq!(switchable(&layout, Track::built_in(), 2000, None), None);
    }

    // Fails if pending.rs:110 current > pending.version check is removed
    #[test]
    fn one_older_than_the_installed_version_is_not_switchable() {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::new(dir.path());
        let v_installed = Version::parse("0.0.3").unwrap();
        let v_pending = Version::parse("0.0.2").unwrap();
        fake_app(&layout, &v_installed);
        fake_app(&layout, &v_pending);

        layout
            .write_installed(&crate::layout::Installed {
                version: Some(v_installed),
                track: Some(Track::built_in().name().to_owned()),
            })
            .unwrap();

        let pending = Pending {
            version: v_pending,
            track: Track::built_in().name().to_owned(),
            prepared_at: 1000,
            confirmed_at: None,
        };
        layout.write_pending(&pending).unwrap();

        assert_eq!(switchable(&layout, Track::built_in(), 2000, None), None);
    }

    // Fails if pending.rs:74 pending.version != *version check is removed
    #[test]
    fn confirm_only_touches_the_named_version() {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::new(dir.path());
        let version = Version::parse("0.0.2").unwrap();

        let pending = Pending {
            version: version.clone(),
            track: Track::built_in().name().to_owned(),
            prepared_at: 1000,
            confirmed_at: None,
        };
        layout.write_pending(&pending).unwrap();

        let other_version = Version::parse("0.0.3").unwrap();
        let confirmed_other = layout.confirm_pending(&other_version, 5000).unwrap();
        assert!(!confirmed_other);
        assert_eq!(layout.read_pending().unwrap().confirmed_at, None);

        let confirmed_self = layout.confirm_pending(&version, 5000).unwrap();
        assert!(confirmed_self);
        assert_eq!(layout.read_pending().unwrap().confirmed_at, Some(5000));
    }

    // Fails if pending.rs:88 note.file validation is removed
    #[test]
    fn write_download_note_validates_file_and_writes_in_downloads() {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::new(dir.path());
        let note = DownloadNote {
            tag: "v0.0.1".into(),
            version: Version::parse("0.0.1").unwrap(),
            track: "beta".into(),
            file: "opencord.tar.gz".into(),
            url: "https://example.com/a".into(),
            size: 100,
            sha256: "0".repeat(64),
        };
        layout.write_download_note(&note).unwrap();
        let note_path = layout.downloads().join("opencord.tar.gz.json");
        assert!(note_path.is_file());
        assert_eq!(layout.read_download_note(&note_path).unwrap(), note);

        let mut bad_note = note.clone();
        bad_note.file = "../evil.tar.gz".into();
        assert!(layout.write_download_note(&bad_note).is_err());
        bad_note.file = "sub/dir.tar.gz".into();
        assert!(layout.write_download_note(&bad_note).is_err());
        bad_note.file = "sub\\dir.tar.gz".into();
        assert!(layout.write_download_note(&bad_note).is_err());
    }
}
