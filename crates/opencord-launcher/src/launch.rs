//! Starting the app, and the launcher replacing itself with the one a new
//! release carries.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use semver::Version;

/// Tells the app where its launcher is, so links, shortcuts and launch at
/// login go through the launcher and keep updating, and which version it
/// was installed as.
pub const LAUNCHER_ENV: &str = "OPENCORD_LAUNCHER";
pub const VERSION_ENV: &str = "OPENCORD_VERSION";

/// Starts the app at `executable`, passing on the launcher's own
/// arguments (a link, `--autostart`).
pub fn start(
    executable: &Path,
    args: &[OsString],
    launcher: Option<&Path>,
    version: &Version,
) -> io::Result<()> {
    let mut command = Command::new(executable);
    command.args(args).env(VERSION_ENV, version.to_string());
    if let Some(dir) = executable.parent() {
        command.current_dir(dir);
    }
    match launcher {
        Some(launcher) => command.env(LAUNCHER_ENV, launcher),
        None => command.env_remove(LAUNCHER_ENV),
    };
    command.spawn().map(drop)
}

/// Waits up to `limit` for process `pid` to end: the app asks for a
/// restart by starting the launcher and quitting, and the new app must
/// not find the old one still running and hand itself over to it.
pub fn wait_for_exit(pid: u32, limit: Duration) {
    let started = Instant::now();
    while running(pid) && started.elapsed() < limit {
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(unix)]
fn running(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    // SAFETY: signal 0 sends nothing; it only asks whether `pid` exists.
    let result = unsafe { libc::kill(pid, 0) };
    result == 0 || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(windows)]
fn running(pid: u32) -> bool {
    use windows::Win32::Foundation::{CloseHandle, WAIT_TIMEOUT};
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };
    // SAFETY: a handle opened here is closed here, and waiting on it for 0
    // ms only reads its state.
    unsafe {
        let Ok(process) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) else {
            return false;
        };
        let alive = WaitForSingleObject(process, 0) == WAIT_TIMEOUT;
        let _ = CloseHandle(process);
        alive
    }
}

/// Replaces the launcher at `installed` with `bundled` when they differ.
/// A running executable can't be overwritten on Windows, but it can be
/// renamed: the old one moves aside and is removed next time.
pub fn replace_self(installed: &Path, bundled: &Path) -> io::Result<bool> {
    let new = fs::read(bundled)?;
    if fs::read(installed).is_ok_and(|old| old == new) {
        return Ok(false);
    }
    let staged = installed.with_extension("new");
    fs::write(&staged, &new)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&staged, fs::Permissions::from_mode(0o755))?;
    }
    if cfg!(windows) && installed.exists() {
        let aside = installed.with_extension("old");
        let _ = fs::remove_file(&aside);
        fs::rename(installed, &aside)?;
    }
    fs::rename(&staged, installed)?;
    Ok(true)
}

/// Removes what [`replace_self`] moved aside.
pub fn tidy_after_replace(installed: &Path) {
    let _ = fs::remove_file(installed.with_extension("old"));
    let _ = fs::remove_file(installed.with_extension("new"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_launcher_is_replaced_only_when_it_differs() {
        let dir = tempfile::tempdir().unwrap();
        let installed = dir.path().join("opencord");
        let bundled = dir.path().join("opencord-launcher");
        fs::write(&installed, b"old").unwrap();
        fs::write(&bundled, b"new").unwrap();

        assert!(replace_self(&installed, &bundled).unwrap());
        assert_eq!(fs::read(&installed).unwrap(), b"new");
        assert!(!replace_self(&installed, &bundled).unwrap());

        tidy_after_replace(&installed);
        assert!(!installed.with_extension("new").exists());
        assert!(!installed.with_extension("old").exists());
    }

    #[cfg(unix)]
    #[test]
    fn waiting_ends_when_the_process_does() {
        let mut child = Command::new("sleep").arg("0.2").spawn().unwrap();
        let pid = child.id();
        assert!(running(pid));
        let reaper = std::thread::spawn(move || child.wait());
        wait_for_exit(pid, Duration::from_secs(5));
        reaper.join().unwrap().unwrap();
        assert!(!running(pid));
    }
}
