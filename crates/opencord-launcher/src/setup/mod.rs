//! Installing: on Windows and Linux the launcher is its own installer.
//! Run from anywhere but the install folder (from Downloads, as
//! `opencord-setup-…`), it copies itself in, sets the track it was built
//! for, and makes the app findable: shortcuts, `opencord://` links and an
//! uninstaller. macOS installs by dragging the app to Applications, so
//! there the track of a new disk image is noticed on its first launch
//! instead ([`adopt_track`]).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::layout::Layout;
use crate::track::Track;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;

/// The app's identity: the desktop file's name on Linux, the taskbar's
/// AppUserModelID on Windows. The Flutter app uses the same.
#[cfg(any(target_os = "linux", windows))]
pub const APP_ID: &str = "dev.opencord.opencord";

/// Whether this launcher, at `exe`, is an installer: on Windows and Linux,
/// anything outside the install folder is.
pub fn is_installer(layout: &Layout, exe: &Path) -> bool {
    if cfg!(target_os = "macos") {
        return false;
    }
    let installed = layout.launcher();
    let same = |a: &Path, b: &Path| match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    };
    !same(exe, &installed)
}

/// Installs the launcher at `exe` into `layout`, for `track`, and returns
/// where the installed launcher is.
pub fn install(layout: &Layout, exe: &Path, track: Track) -> io::Result<PathBuf> {
    fs::create_dir_all(layout.root())?;
    let installed = layout.launcher();
    // An older launcher may be running (Windows won't overwrite it), so
    // the copy goes in the way a self-update does.
    crate::launch::replace_self(&installed, exe)?;
    let settings = layout.read_settings().installed_from(track);
    layout.write_settings(&settings)?;
    register(layout, &installed)?;
    Ok(installed)
}

/// On macOS: a launcher from another track's disk image than the one
/// that set the track last sets its own, as an installer would.
pub fn adopt_track(layout: &Layout, track: Track) -> io::Result<()> {
    let settings = layout.read_settings();
    if settings.installer_track.as_deref() == Some(track.name()) {
        return Ok(());
    }
    layout.write_settings(&settings.installed_from(track))
}

/// Shortcuts, links and the uninstaller entry, pointing at `launcher`.
fn register(layout: &Layout, launcher: &Path) -> io::Result<()> {
    #[cfg(target_os = "linux")]
    {
        let _ = layout;
        linux::register(launcher)
    }
    #[cfg(windows)]
    return windows::register(layout, launcher);
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = (layout, launcher);
        Ok(())
    }
}

/// Removes the app: its shortcuts, links and install folder. The user's
/// data (servers, messages, settings) stays where the app keeps it.
pub fn uninstall(layout: &Layout) -> io::Result<()> {
    #[cfg(target_os = "linux")]
    linux::unregister(&layout.launcher());
    #[cfg(windows)]
    return windows::uninstall(layout);
    // `update.json` goes too: a later install sets its own track.
    #[cfg(not(windows))]
    fs::remove_dir_all(layout.root())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_installed_launcher_is_not_an_installer() {
        if cfg!(target_os = "macos") {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::new(dir.path().join("root"));
        fs::create_dir_all(layout.root()).unwrap();
        fs::write(layout.launcher(), b"x").unwrap();
        let elsewhere = dir.path().join("opencord-setup");
        fs::write(&elsewhere, b"x").unwrap();

        assert!(!is_installer(&layout, &layout.launcher()));
        assert!(is_installer(&layout, &elsewhere));
    }

    #[test]
    fn a_disk_image_of_another_track_sets_its_own_once() {
        let dir = tempfile::tempdir().unwrap();
        let layout = Layout::new(dir.path());

        adopt_track(&layout, Track::Canary).unwrap();
        assert_eq!(layout.read_settings().track(), Track::Canary);

        // The user picks beta in the app; the same launcher leaves it.
        let mut settings = layout.read_settings();
        settings.track = Some("beta".into());
        layout.write_settings(&settings).unwrap();
        adopt_track(&layout, Track::Canary).unwrap();
        assert_eq!(layout.read_settings().track(), Track::Beta);

        // A stable disk image installed over it sets stable.
        adopt_track(&layout, Track::Stable).unwrap();
        assert_eq!(layout.read_settings().track(), Track::Stable);
    }
}
