//! Opencord's launcher: what the Start menu, the dock, the desktop's app
//! list and `opencord://` links start (docs/updater.md). It shows a splash,
//! brings the app up to the newest build of the user's track from GitHub,
//! and starts it. On Windows and Linux it is also the installer.
//!
//! ```text
//! opencord [ARGS…]                 update, then start the app with ARGS
//! opencord --uninstall [--quiet]   remove the app (Windows, Linux)
//! opencord --after-pid PID …       wait for PID to end first (a restart)
//! opencord --launcher-root DIR …   install into DIR (for testing)
//! ```

#![cfg_attr(windows, windows_subsystem = "windows")]

mod archive;
mod github;
mod launch;
mod layout;
mod log;
mod release;
mod setup;
mod signature;
mod splash;
mod track;
mod update;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use github::{Github, REPOSITORY, Reqwest};
use layout::Layout;
use track::Track;
use update::Status;

/// How long a restart waits for the old app to quit.
const RESTART_WAIT: Duration = Duration::from_secs(15);
/// Between tries when nothing is installed and GitHub can't be reached.
const RETRY_EVERY: Duration = Duration::from_secs(5);

#[derive(Debug, Default, PartialEq)]
struct Args {
    root: Option<PathBuf>,
    uninstall: bool,
    after_pid: Option<u32>,
    /// Everything else, for the app: links, `--autostart`.
    forward: Vec<OsString>,
}

impl Args {
    fn parse(args: impl IntoIterator<Item = OsString>) -> Self {
        let mut parsed = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--uninstall") => parsed.uninstall = true,
                Some("--launcher-root") => parsed.root = args.next().map(PathBuf::from),
                Some("--after-pid") => {
                    parsed.after_pid = args.next().and_then(|pid| pid.to_str()?.parse().ok());
                }
                _ => parsed.forward.push(arg),
            }
        }
        parsed
    }

    /// Whether the app is being opened for a link: it starts at once, and
    /// updates wait for the next plain launch.
    fn opens_a_link(&self) -> bool {
        self.forward.iter().any(|arg| {
            arg.to_str().is_some_and(|arg| {
                let arg = arg.to_ascii_lowercase();
                arg.starts_with("opencord:") || arg.starts_with("opencords:")
            })
        })
    }
}

fn main() {
    let args = Args::parse(std::env::args_os().skip(1));
    let root = args
        .root
        .clone()
        .or_else(|| std::env::var_os("OPENCORD_LAUNCHER_ROOT").map(PathBuf::from))
        .or_else(Layout::default_root);
    let Some(root) = root else {
        eprintln!("Opencord can't find a folder to install into.");
        std::process::exit(1);
    };
    let layout = Layout::new(root);
    log::open(&layout.log_file());
    log::line(format!(
        "opencord-launcher {} ({} track built in), installing into {}",
        env!("CARGO_PKG_VERSION"),
        Track::built_in(),
        layout.root().display()
    ));

    if args.uninstall {
        if let Err(error) = setup::uninstall(&layout) {
            log::line(format!("uninstalling failed: {error}"));
            std::process::exit(1);
        }
        return;
    }
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(error) => {
            log::line(format!("can't tell where the launcher is: {error}"));
            std::process::exit(1);
        }
    };
    launch::tidy_after_replace(&layout.launcher());

    if let Some(pid) = args.after_pid {
        launch::wait_for_exit(pid, RESTART_WAIT);
    }
    let installer = setup::is_installer(&layout, &exe);
    if !installer && args.opens_a_link() && start_installed(&layout, &exe, &args.forward) {
        return;
    }
    let started = splash::show(move |report| run(&layout, &exe, installer, &args.forward, report));
    if !started {
        std::process::exit(1);
    }
}

/// Installs if this is the installer, updates, and starts the app.
/// Returns whether the app started.
fn run(
    layout: &Layout,
    exe: &Path,
    installer: bool,
    forward: &[OsString],
    report: &dyn Fn(Status),
) -> bool {
    if installer {
        report(Status::Installing);
        match setup::install(layout, exe, Track::built_in()) {
            Ok(installed) => {
                log::line(format!("installed the launcher at {}", installed.display()))
            }
            Err(error) => {
                log::line(format!("installing failed: {error}"));
                return false;
            }
        }
    } else if cfg!(target_os = "macos")
        && let Err(error) = setup::adopt_track(layout, Track::built_in())
    {
        log::line(format!("can't write update.json: {error}"));
    }

    let track = layout.read_settings().track();
    log::line(format!(
        "track {track}, installed {:?}",
        layout.current().map(|v| v.to_string())
    ));
    match release::PLATFORM {
        Some(platform) => update_until_installed(layout, track, platform, report),
        None => log::line("this system has no builds to update from"),
    }
    report(Status::Starting);
    start_installed(layout, exe, forward)
}

/// Updates. If nothing is installed yet, keeps trying until something is;
/// otherwise a failed update leaves the installed version to start.
fn update_until_installed(
    layout: &Layout,
    track: Track,
    platform: &'static str,
    report: &dyn Fn(Status),
) {
    let source = match Reqwest::new() {
        Ok(http) => Github::new(http, REPOSITORY, signature::built_in_key(), platform),
        Err(error) => {
            log::line(format!("no HTTP client: {error}"));
            return;
        }
    };
    loop {
        match update::update(layout, &source, track, platform, report) {
            Ok(Some(version)) => {
                log::line(format!("installed {version}"));
                update_launcher(layout, &version);
                layout.prune();
                return;
            }
            Ok(None) => {
                log::line("up to date");
                layout.prune();
                return;
            }
            Err(error) => {
                log::line(format!("update failed: {error}"));
                if layout.current().is_some() {
                    return;
                }
                report(Status::Retrying {
                    offline: matches!(error, update::Error::Network(_)),
                });
                std::thread::sleep(RETRY_EVERY);
            }
        }
    }
}

/// Replaces the installed launcher with the one `version` carries.
fn update_launcher(layout: &Layout, version: &semver::Version) {
    let Some(bundled) = layout::bundled_launcher(&layout.version_dir(version)) else {
        return;
    };
    if !bundled.is_file() {
        return;
    }
    match launch::replace_self(&layout.launcher(), &bundled) {
        Ok(true) => log::line("updated the launcher"),
        Ok(false) => {}
        Err(error) => log::line(format!("can't update the launcher: {error}")),
    }
}

/// Starts the newest installed version, if there is one.
fn start_installed(layout: &Layout, exe: &Path, forward: &[OsString]) -> bool {
    let Some(version) = layout.current() else {
        log::line("nothing is installed to start");
        return false;
    };
    // The installed launcher, which links and shortcuts point at (on macOS,
    // the one in Applications: this one).
    let launcher = if cfg!(target_os = "macos") {
        exe.to_path_buf()
    } else {
        layout.launcher()
    };
    let app = layout::app_executable(&layout.version_dir(&version));
    match launch::start(&app, forward, Some(&launcher), &version) {
        Ok(()) => {
            log::line(format!("started {}", app.display()));
            true
        }
        Err(error) => {
            log::line(format!("can't start {}: {error}", app.display()));
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Args {
        Args::parse(list.iter().map(OsString::from))
    }

    #[test]
    fn the_launchers_own_flags_are_taken_and_the_rest_passed_on() {
        let parsed = args(&[
            "--after-pid",
            "42",
            "--autostart",
            "--launcher-root",
            "/tmp/x",
            "opencord://a",
        ]);
        assert_eq!(parsed.after_pid, Some(42));
        assert_eq!(parsed.root, Some(PathBuf::from("/tmp/x")));
        assert_eq!(
            parsed.forward,
            [
                OsString::from("--autostart"),
                OsString::from("opencord://a")
            ]
        );
        assert!(!parsed.uninstall);
        assert!(args(&["--uninstall"]).uninstall);
    }

    #[test]
    fn a_link_opens_at_once() {
        assert!(args(&["OPENCORDS://host/invite/x"]).opens_a_link());
        assert!(args(&["opencord:host"]).opens_a_link());
        assert!(!args(&["--autostart"]).opens_a_link());
        assert!(!args(&[]).opens_a_link());
    }
}
