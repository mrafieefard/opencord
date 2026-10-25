//! The install folder, one per user, like Discord's:
//!
//! ```text
//! <root>/
//!   opencord, Opencord.exe     the launcher (Linux, Windows; on macOS it is
//!                              /Applications/Opencord.app)
//!   update.json                the track; the app's settings write it too
//!   app-0.0.2-beta.3/          one folder per installed version, as its
//!   app-0.0.2-beta.4/          archive unpacks: opencord/ or Opencord.app/
//!   downloads/
//!   launcher.log               the last run's log
//! ```
//!
//! A version folder appears whole or not at all: it is unpacked under
//! `.partial` and renamed. The newest one is the installed version; the
//! one before it stays, and older ones go.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use semver::Version;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::track::Track;

const VERSION_PREFIX: &str = "app-";
const PARTIAL: &str = ".partial";

/// The launcher's file name in the install folder.
#[cfg(windows)]
pub const LAUNCHER_FILE: &str = "Opencord.exe";
#[cfg(not(windows))]
pub const LAUNCHER_FILE: &str = "opencord";

/// The launcher a release carries inside its app, for the installed one
/// to update itself from.
#[cfg(windows)]
const BUNDLED_LAUNCHER: &str = "opencord-launcher.exe";
#[cfg(not(windows))]
const BUNDLED_LAUNCHER: &str = "opencord-launcher";

#[derive(Debug, Clone)]
pub struct Layout {
    root: PathBuf,
}

impl Layout {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// `%LOCALAPPDATA%\Opencord`, `~/Library/Application Support/Opencord`,
    /// or `$XDG_DATA_HOME/opencord` (`~/.local/share/opencord`). The app's
    /// own data lives elsewhere (`dev.opencord…`), so an uninstall leaves
    /// it alone.
    pub fn default_root() -> Option<PathBuf> {
        let env = |name| std::env::var_os(name).filter(|value| !value.is_empty());
        if cfg!(windows) {
            env("LOCALAPPDATA").map(|dir| PathBuf::from(dir).join("Opencord"))
        } else if cfg!(target_os = "macos") {
            env("HOME").map(|home| PathBuf::from(home).join("Library/Application Support/Opencord"))
        } else {
            env("XDG_DATA_HOME")
                .map(PathBuf::from)
                .or_else(|| env("HOME").map(|home| PathBuf::from(home).join(".local/share")))
                .map(|dir| dir.join("opencord"))
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn launcher(&self) -> PathBuf {
        self.root.join(LAUNCHER_FILE)
    }

    pub fn settings_file(&self) -> PathBuf {
        self.root.join("update.json")
    }

    pub fn downloads(&self) -> PathBuf {
        self.root.join("downloads")
    }

    pub fn log_file(&self) -> PathBuf {
        self.root.join("launcher.log")
    }

    pub fn version_dir(&self, version: &Version) -> PathBuf {
        self.root.join(format!("{VERSION_PREFIX}{version}"))
    }

    pub fn partial_dir(&self, version: &Version) -> PathBuf {
        self.root
            .join(format!("{VERSION_PREFIX}{version}{PARTIAL}"))
    }

    /// The installed versions, newest first.
    pub fn versions(&self) -> Vec<Version> {
        let Ok(entries) = fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut versions: Vec<Version> = entries
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .filter_map(|entry| {
                let name = entry.file_name().into_string().ok()?;
                Version::parse(name.strip_prefix(VERSION_PREFIX)?).ok()
            })
            .filter(|version| app_executable(&self.version_dir(version)).is_file())
            .collect();
        versions.sort_by(|a, b| b.cmp(a));
        versions
    }

    pub fn current(&self) -> Option<Version> {
        self.versions().into_iter().next()
    }

    /// Removes every version but the newest two, and anything half
    /// unpacked or half downloaded. A folder still in use (Windows) stays
    /// until the next time.
    pub fn prune(&self) {
        let keep: Vec<PathBuf> = self
            .versions()
            .iter()
            .take(2)
            .map(|version| self.version_dir(version))
            .collect();
        let Ok(entries) = fs::read_dir(&self.root) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(VERSION_PREFIX) && !keep.contains(&path) {
                let _ = fs::remove_dir_all(&path);
            }
        }
        let _ = fs::remove_dir_all(self.downloads());
    }

    pub fn read_settings(&self) -> Settings {
        fs::read(self.settings_file())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Writes `update.json` whole, keeping keys this launcher doesn't know.
    pub fn write_settings(&self, settings: &Settings) -> io::Result<()> {
        fs::create_dir_all(&self.root)?;
        let path = self.settings_file();
        let partial = path.with_extension("json.partial");
        fs::write(&partial, serde_json::to_vec_pretty(settings)?)?;
        fs::rename(&partial, &path)
    }
}

/// `update.json`. The app's settings change `track`; installers set both.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track: Option<String>,
    /// The track of the installer that last set `track` (macOS, where
    /// installing is dragging the app over: a launcher from another
    /// track's disk image sets its own).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installer_track: Option<String>,
    #[serde(flatten)]
    pub other: Map<String, Value>,
}

impl Settings {
    pub fn track(&self) -> Track {
        self.track
            .as_deref()
            .and_then(Track::parse)
            .unwrap_or_else(Track::built_in)
    }

    /// What an installer of `track` leaves: that track, and a note of it.
    pub fn installed_from(&self, track: Track) -> Self {
        Self {
            track: Some(track.name().to_owned()),
            installer_track: Some(track.name().to_owned()),
            other: self.other.clone(),
        }
    }
}

/// The app's executable inside a version folder.
pub fn app_executable(version_dir: &Path) -> PathBuf {
    if cfg!(windows) {
        version_dir.join("opencord").join("opencord.exe")
    } else if cfg!(target_os = "macos") {
        version_dir.join("Opencord.app/Contents/MacOS/Opencord")
    } else {
        version_dir.join("opencord").join("opencord")
    }
}

/// The launcher a version carries, which the installed launcher updates
/// itself from. macOS has none: its launcher is the app in Applications,
/// replaced by installing a newer disk image.
pub fn bundled_launcher(version_dir: &Path) -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        return None;
    }
    Some(version_dir.join("opencord").join(BUNDLED_LAUNCHER))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn install(layout: &Layout, version: &str) {
        let dir = layout.version_dir(&Version::parse(version).unwrap());
        let exe = app_executable(&dir);
        fs::create_dir_all(exe.parent().unwrap()).unwrap();
        fs::write(exe, b"app").unwrap();
    }

    #[test]
    fn versions_are_whole_folders_newest_first() {
        let root = tempfile::tempdir().unwrap();
        let layout = Layout::new(root.path());
        install(&layout, "0.0.2-beta.3");
        install(&layout, "0.0.2");
        install(&layout, "0.0.2-canary.1");
        // Half unpacked, empty, and not a version: none count.
        fs::create_dir_all(layout.partial_dir(&Version::parse("0.0.3").unwrap())).unwrap();
        fs::create_dir_all(root.path().join("app-0.0.4")).unwrap();
        fs::create_dir_all(root.path().join("app-latest")).unwrap();

        let names: Vec<String> = layout.versions().iter().map(Version::to_string).collect();
        assert_eq!(names, ["0.0.2", "0.0.2-canary.1", "0.0.2-beta.3"]);
        assert_eq!(layout.current().unwrap().to_string(), "0.0.2");
    }

    #[test]
    fn pruning_keeps_the_newest_two() {
        let root = tempfile::tempdir().unwrap();
        let layout = Layout::new(root.path());
        for version in ["0.0.1", "0.0.2", "0.0.3"] {
            install(&layout, version);
        }
        fs::create_dir_all(layout.partial_dir(&Version::parse("0.0.4").unwrap())).unwrap();
        fs::create_dir_all(layout.downloads()).unwrap();
        layout.write_settings(&Settings::default()).unwrap();

        layout.prune();

        let names: Vec<String> = layout.versions().iter().map(Version::to_string).collect();
        assert_eq!(names, ["0.0.3", "0.0.2"]);
        assert!(
            !layout
                .partial_dir(&Version::parse("0.0.4").unwrap())
                .exists()
        );
        assert!(!layout.downloads().exists());
        assert!(layout.settings_file().exists());
    }

    #[test]
    fn settings_keep_what_they_dont_know() {
        let root = tempfile::tempdir().unwrap();
        let layout = Layout::new(root.path());
        fs::write(
            layout.settings_file(),
            br#"{"track":"canary","later":{"a":1}}"#,
        )
        .unwrap();

        let settings = layout.read_settings();
        assert_eq!(settings.track(), Track::Canary);
        layout
            .write_settings(&settings.installed_from(Track::Beta))
            .unwrap();

        let written: Value =
            serde_json::from_slice(&fs::read(layout.settings_file()).unwrap()).unwrap();
        assert_eq!(written["track"], "beta");
        assert_eq!(written["installer_track"], "beta");
        assert_eq!(written["later"]["a"], 1);
    }

    #[test]
    fn missing_or_broken_settings_fall_back_to_the_built_in_track() {
        let root = tempfile::tempdir().unwrap();
        let layout = Layout::new(root.path());
        assert_eq!(layout.read_settings().track(), Track::built_in());
        fs::write(layout.settings_file(), b"{not json").unwrap();
        assert_eq!(layout.read_settings().track(), Track::built_in());
        fs::write(layout.settings_file(), br#"{"track":"nightly"}"#).unwrap();
        assert_eq!(layout.read_settings().track(), Track::built_in());
    }
}
