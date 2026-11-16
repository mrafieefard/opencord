//! Opencord's launcher: what the Start menu, the dock, the desktop's app
//! list and `opencord://` links start (docs/updater.md). It starts the
//! installed app at once, switching first to an update the app has
//! prepared. It waits for GitHub, with a splash, only to install, to
//! switch tracks and after a start that never got going. On Windows and
//! Linux it is also the installer.
//!
//! ```text
//! opencord [ARGS…]                 start the app with ARGS
//! opencord --uninstall [--quiet]   remove the app (Windows, Linux)
//! opencord --after-pid PID …       wait for PID to end first (a restart)
//! opencord --launcher-root DIR …   install into DIR (for testing)
//! ```

#![cfg_attr(windows, windows_subsystem = "windows")]

mod github;
mod launch;
mod log;
mod setup;
mod splash;
mod update;

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ed25519_dalek::VerifyingKey;
use github::{Github, Reqwest};
use opencord_update::REPOSITORY;
use opencord_update::layout::{self, Installed, Layout};
use opencord_update::markers::Mark;
use opencord_update::pending;
use opencord_update::release;
use opencord_update::signature::{self, KeyError};
use opencord_update::track::Track;
use update::{Source, Status};

/// How long a restart waits for the old app to quit.
const RESTART_WAIT: Duration = Duration::from_secs(15);
/// Between tries when nothing is installed and GitHub can't be reached.
const RETRY_EVERY: Duration = Duration::from_secs(5);
/// How long a start without the network may take (switching to a waiting
/// update) before the splash shows.
const SPLASH_AFTER: Duration = Duration::from_secs(1);

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

/// How this launcher was started.
struct Start {
    exe: PathBuf,
    /// Run from outside the install folder (Windows, Linux).
    installer: bool,
    /// Opening a link, which never waits for GitHub.
    link: bool,
    /// Started by the app to restart it (`--after-pid`), so an update
    /// restart's `restart.json` counts.
    restart: bool,
    /// For the app.
    forward: Vec<OsString>,
}

/// What a start reaches outside the install folder: the system's
/// shortcuts, GitHub and the app's process. Tests stand in for all three.
trait Outside {
    /// Installs this launcher for `track`; returns where it now is.
    fn install(&self, layout: &Layout, exe: &Path, track: Track) -> io::Result<PathBuf>;
    /// Where releases come from, or `None` when this launcher can't update.
    fn source(&self, platform: &'static str) -> Option<Box<dyn Source>>;
    /// Starts the app.
    fn spawn(&self, command: Command) -> io::Result<()>;
}

/// The real system.
struct System;

impl Outside for System {
    fn install(&self, layout: &Layout, exe: &Path, track: Track) -> io::Result<PathBuf> {
        setup::install(layout, exe, track)
    }

    fn source(&self, platform: &'static str) -> Option<Box<dyn Source>> {
        github(platform, signature::built_in_key())
    }

    fn spawn(&self, mut command: Command) -> io::Result<()> {
        command.spawn().map(drop)
    }
}

/// What runs under the splash, returning whether the app started.
type Work = Box<dyn FnOnce(&dyn Fn(Status)) -> bool + Send>;

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
    let start = Start {
        installer: setup::is_installer(&layout, &exe),
        link: args.opens_a_link(),
        restart: args.after_pid.is_some(),
        exe,
        forward: args.forward,
    };
    let started = run(layout, start, Box::new(System), splash::show_after);
    if !started {
        std::process::exit(1);
    }
}

/// Starts the app, going online first only when it must (design: "The
/// launcher's start"). The work runs under `show`, the splash: at once
/// when it goes online, and after [`SPLASH_AFTER`] otherwise. Returns
/// whether the app started.
fn run(
    layout: Layout,
    start: Start,
    outside: Box<dyn Outside + Send>,
    show: impl FnOnce(Duration, Work) -> bool,
) -> bool {
    if cfg!(target_os = "macos")
        && !start.installer
        && !start.link
        && let Err(error) = setup::adopt_track(&layout, Track::built_in())
    {
        log::line(format!("can't write update.json: {error}"));
    }
    let online = goes_online(&layout, &start);
    let splash_after = if online { Duration::ZERO } else { SPLASH_AFTER };
    show(
        splash_after,
        Box::new(move |report| {
            if online {
                start_online(&layout, &start, &*outside, report)
            } else {
                start_offline(&layout, &start, &*outside)
            }
        }),
    )
}

/// Whether this start waits for GitHub: nothing is installed, it is the
/// installer, a track switch is owed, or the last start never got as far
/// as "started" (a release that crashes on start). A link waits only
/// when nothing is installed.
fn goes_online(layout: &Layout, start: &Start) -> bool {
    if start.installer || layout.current().is_none() {
        return true;
    }
    if start.link {
        return false;
    }
    if update::switching_tracks(layout, layout.read_settings().track()) {
        log::line("a track switch is owed");
        return true;
    }
    if layout.last_start_crashed() {
        log::line("the last start never got as far as started");
        return true;
    }
    false
}

/// Installs if this is the installer, updates from GitHub, and starts
/// the app.
fn start_online(
    layout: &Layout,
    start: &Start,
    outside: &dyn Outside,
    report: &dyn Fn(Status),
) -> bool {
    if start.installer {
        report(Status::Installing);
        match outside.install(layout, &start.exe, Track::built_in()) {
            Ok(installed) => {
                log::line(format!("installed the launcher at {}", installed.display()))
            }
            Err(error) => {
                log::line(format!("installing failed: {error}"));
                return false;
            }
        }
    }

    let track = layout.read_settings().track();
    log::line(format!(
        "track {track}, installed {:?}",
        layout.current().map(|v| v.to_string())
    ));
    match release::PLATFORM {
        Some(platform) => match outside.source(platform) {
            Some(source) => update_until_installed(layout, track, platform, &*source, report),
            None => log::line("not updating; starting the installed version"),
        },
        None => log::line("this system has no builds to update from"),
    }
    report(Status::Starting);
    start_installed(layout, start, outside)
}

/// Starts the app without the network, switching first to a waiting
/// update. While the app runs, this start is handed over to it and its
/// version keeps running, so nothing is switched: `installed.json` would
/// name a version that never ran.
fn start_offline(layout: &Layout, start: &Start, outside: &dyn Outside) -> bool {
    if layout.app_running() {
        log::line("the app is running: switching nothing");
    } else {
        switch_to_waiting(layout, start.restart);
    }
    start_installed(layout, start, outside)
}

/// Switches `installed.json` to the waiting update if it should start
/// now, updates the launcher from it, and prunes. Holds the install lock,
/// so a prepare still unpacking is waited for, never pruned.
fn switch_to_waiting(layout: &Layout, restart: bool) {
    let track = layout.read_settings().track();
    let note = if restart { layout.read_restart() } else { None };
    let _lock = match update::lock_install(layout) {
        Ok(lock) => lock,
        Err(error) => {
            log::line(format!("can't lock the install folder: {error}"));
            return;
        }
    };
    let Some(version) = pending::switchable(layout, track, now_ms(), note.as_ref()) else {
        return;
    };
    let installed = Installed {
        version: Some(version.clone()),
        track: Some(track.name().to_owned()),
    };
    if let Err(error) = layout.write_installed(&installed) {
        log::line(format!("can't switch to the waiting {version}: {error}"));
        return;
    }
    log::line(format!("switched to the waiting {version}"));
    update_launcher(layout, &version);
    if let Err(error) = layout.remove_pending() {
        log::line(format!("can't remove pending.json: {error}"));
    }
    layout.prune();
}

/// GitHub, with releases checked against `key`. A key this launcher can't
/// read means no updates: the installed version starts, and unsigned
/// releases are never taken instead.
fn github(
    platform: &'static str,
    key: Result<Option<VerifyingKey>, KeyError>,
) -> Option<Box<dyn Source>> {
    let key = match key {
        Ok(key) => key,
        Err(error) => {
            log::line(format!("the built-in update key can't be read: {error}"));
            return None;
        }
    };
    match Reqwest::new() {
        Ok(http) => Some(Box::new(Github::new(http, REPOSITORY, key, platform))),
        Err(error) => {
            log::line(format!("no HTTP client: {error}"));
            None
        }
    }
}

/// Updates. If nothing is installed yet, keeps trying until something is;
/// otherwise a failed update leaves the installed version to start.
fn update_until_installed(
    layout: &Layout,
    track: Track,
    platform: &'static str,
    source: &dyn Source,
    report: &dyn Fn(Status),
) {
    loop {
        match update_once(layout, source, track, platform, report) {
            Ok(_) => return,
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

/// One update from GitHub, holding the install lock, then the launcher's
/// own update and pruning.
fn update_once(
    layout: &Layout,
    source: &dyn Source,
    track: Track,
    platform: &str,
    report: &dyn Fn(Status),
) -> Result<Option<semver::Version>, update::Error> {
    let _lock = update::lock_install(layout)?;
    let updated = update::update(layout, source, track, platform, report)?;
    match &updated {
        Some(version) => {
            log::line(format!("installed {version}"));
            update_launcher(layout, version);
        }
        None => log::line("up to date"),
    }
    layout.prune();
    Ok(updated)
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

/// Starts the installed version, if there is one, marking it in
/// `launching.json` first.
fn start_installed(layout: &Layout, start: &Start, outside: &dyn Outside) -> bool {
    let Some(version) = layout.current() else {
        log::line("nothing is installed to start");
        return false;
    };
    // The installed launcher, which links and shortcuts point at (on macOS,
    // the one in Applications: this one).
    let launcher = if cfg!(target_os = "macos") {
        start.exe.clone()
    } else {
        layout.launcher()
    };
    let app = layout::app_executable(&layout.version_dir(&version));
    // A start while the app runs is handed over to it and starts no new
    // version, so it isn't marked.
    if !layout.app_running() {
        let mark = Mark {
            version: version.clone(),
            at: now_ms(),
        };
        if let Err(error) = layout.write_launching(&mark) {
            log::line(format!("can't write launching.json: {error}"));
        }
    }
    match outside.spawn(launch::command(
        &app,
        &start.forward,
        Some(&launcher),
        &version,
    )) {
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

/// Now, in Unix milliseconds, as the install folder's files keep time.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_millis()).unwrap_or(u64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::fs::{self, OpenOptions};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use opencord_update::layout::Settings;
    use opencord_update::pending::Pending;
    use opencord_update::release::Manifest;
    use opencord_update::restart::RestartNote;
    use semver::Version;

    use super::*;

    fn args(list: &[&str]) -> Args {
        Args::parse(list.iter().map(OsString::from))
    }

    fn v(version: &str) -> Version {
        Version::parse(version).unwrap()
    }

    /// The world outside the install folder. GitHub answers "offline"
    /// when `online` allows it, and asking for it panics otherwise;
    /// installing touches nothing; the app's commands are kept, not run.
    #[derive(Clone, Default)]
    struct Fake {
        online: bool,
        asked: Arc<AtomicUsize>,
        installs: Arc<AtomicUsize>,
        spawned: Arc<Mutex<Vec<Command>>>,
    }

    impl Fake {
        fn online() -> Self {
            Self {
                online: true,
                ..Self::default()
            }
        }

        fn asked(&self) -> usize {
            self.asked.load(Ordering::SeqCst)
        }

        /// The one app started, as (program, OPENCORD_VERSION,
        /// OPENCORD_LAUNCHER_FEATURES).
        fn started(&self) -> (PathBuf, Option<String>, Option<String>) {
            let spawned = self.spawned.lock().unwrap();
            assert_eq!(spawned.len(), 1, "one app is started");
            let command = &spawned[0];
            let env = |name: &str| {
                command
                    .get_envs()
                    .find(|(key, _)| *key == name)
                    .and_then(|(_, value)| value)
                    .map(|value| value.to_string_lossy().into_owned())
            };
            (
                PathBuf::from(command.get_program()),
                env(launch::VERSION_ENV),
                env(launch::FEATURES_ENV),
            )
        }
    }

    impl Outside for Fake {
        fn install(&self, layout: &Layout, _: &Path, _: Track) -> io::Result<PathBuf> {
            self.installs.fetch_add(1, Ordering::SeqCst);
            Ok(layout.launcher())
        }

        fn source(&self, _: &'static str) -> Option<Box<dyn Source>> {
            assert!(self.online, "asked for GitHub on the no-network path");
            Some(Box::new(Offline {
                asked: self.asked.clone(),
            }))
        }

        fn spawn(&self, command: Command) -> io::Result<()> {
            self.spawned.lock().unwrap().push(command);
            Ok(())
        }
    }

    /// GitHub, unreachable.
    struct Offline {
        asked: Arc<AtomicUsize>,
    }

    impl Source for Offline {
        fn latest(&self, _: Track) -> Result<Manifest, update::Error> {
            self.asked.fetch_add(1, Ordering::SeqCst);
            Err(update::Error::Network("offline".into()))
        }

        fn download(&self, _: &str, _: &mut dyn io::Write) -> Result<(), update::Error> {
            panic!("nothing to download from an unreachable GitHub");
        }
    }

    fn plain() -> Start {
        Start {
            exe: PathBuf::from("/elsewhere/opencord"),
            installer: false,
            link: false,
            restart: false,
            forward: Vec::new(),
        }
    }

    /// A whole app folder for `version`.
    fn whole_app(layout: &Layout, version: &str) {
        let exe = layout::app_executable(&layout.version_dir(&v(version)));
        fs::create_dir_all(exe.parent().unwrap()).unwrap();
        fs::write(exe, version).unwrap();
    }

    fn app_of(layout: &Layout, version: &str) -> PathBuf {
        layout::app_executable(&layout.version_dir(&v(version)))
    }

    /// An install folder on the beta track with `version` installed, and
    /// its last start having got as far as "started".
    fn installed(dir: &Path, version: &str) -> Layout {
        let layout = Layout::new(dir);
        layout
            .write_settings(&Settings {
                track: Some("beta".into()),
                ..Settings::default()
            })
            .unwrap();
        whole_app(&layout, version);
        layout
            .write_installed(&Installed {
                version: Some(v(version)),
                track: Some("beta".into()),
            })
            .unwrap();
        layout
            .write_launching(&Mark {
                version: v(version),
                at: 1,
            })
            .unwrap();
        layout
            .write_started(&Mark {
                version: v(version),
                at: 2,
            })
            .unwrap();
        layout
    }

    fn waiting(layout: &Layout, version: &str, prepared_at: u64) {
        layout
            .write_pending(&Pending {
                version: v(version),
                track: "beta".into(),
                prepared_at,
                confirmed_at: None,
            })
            .unwrap();
    }

    /// What one start did.
    struct Ran {
        started: bool,
        splash_after: Duration,
        statuses: Vec<Status>,
    }

    /// Runs a start as `main` does, with `fake` for the world outside and
    /// the work run here in place of the splash.
    fn launch(layout: &Layout, start: Start, fake: &Fake) -> Ran {
        let statuses = RefCell::new(Vec::new());
        let mut splash_after = None;
        let started = run(
            layout.clone(),
            start,
            Box::new(fake.clone()),
            |after, work| {
                splash_after = Some(after);
                work(&|status| statuses.borrow_mut().push(status))
            },
        );
        Ran {
            started,
            splash_after: splash_after.expect("the work ran under the splash"),
            statuses: statuses.into_inner(),
        }
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

    // Fails if `github` takes an unreadable key as no key and updates
    // unsigned (`let key = key.unwrap_or(None)`).
    #[test]
    fn an_unreadable_key_never_updates() {
        assert!(github("linux-x64", Err(KeyError)).is_none());
        // The other side: no key at all still updates (unsigned builds).
        assert!(github("linux-x64", Ok(None)).is_some());
    }

    // Fails if `goes_online` goes online with something installed and
    // nothing owed (its final `false` becomes `true`): the fake panics.
    #[test]
    fn a_plain_start_with_nothing_waiting_makes_no_request() {
        let dir = tempfile::tempdir().unwrap();
        let layout = installed(dir.path(), "0.0.1-beta.1");
        let fake = Fake::default();

        let ran = launch(&layout, plain(), &fake);

        assert!(ran.started);
        assert_eq!(ran.splash_after, SPLASH_AFTER);
        assert!(ran.statuses.is_empty());
        assert_eq!(fake.started().0, app_of(&layout, "0.0.1-beta.1"));
        // Marked before it started.
        let launching = layout.read_launching().unwrap();
        assert_eq!(launching.version, v("0.0.1-beta.1"));
        assert_ne!(launching.at, 1);
    }

    // Fails if `start_offline` skips `switch_to_waiting`, and if
    // `switch_to_waiting` leaves out `update_launcher` or `layout.prune()`.
    // (Without `remove_pending` it still passes: prune drops a
    // `pending.json` the installed version has reached.)
    #[test]
    fn a_fresh_waiting_update_is_switched_to_without_a_request() {
        let dir = tempfile::tempdir().unwrap();
        let layout = installed(dir.path(), "0.0.1-beta.1");
        whole_app(&layout, "0.0.1-beta.0");
        whole_app(&layout, "0.0.1-beta.2");
        waiting(&layout, "0.0.1-beta.2", now_ms());
        let bundled = layout::bundled_launcher(&layout.version_dir(&v("0.0.1-beta.2")));
        if let Some(bundled) = &bundled {
            fs::write(layout.launcher(), b"old launcher").unwrap();
            fs::write(bundled, b"new launcher").unwrap();
        }
        let fake = Fake::default();

        let ran = launch(&layout, plain(), &fake);

        assert!(ran.started);
        assert_eq!(ran.splash_after, SPLASH_AFTER);
        assert_eq!(fake.started().0, app_of(&layout, "0.0.1-beta.2"));
        assert_eq!(layout.read_installed().version, Some(v("0.0.1-beta.2")));
        assert_eq!(layout.read_installed().track.as_deref(), Some("beta"));
        assert_eq!(layout.read_launching().unwrap().version, v("0.0.1-beta.2"));
        assert!(layout.read_pending().is_none());
        // Pruned: the switched-from version stays, the one before it goes.
        assert!(layout.is_whole(&v("0.0.1-beta.1")));
        assert!(!layout.version_dir(&v("0.0.1-beta.0")).exists());
        if bundled.is_some() {
            assert_eq!(fs::read(layout.launcher()).unwrap(), b"new launcher");
        }
    }

    // Fails if `switch_to_waiting` switches without asking
    // `pending::switchable` (writes `pending.version` straight in).
    #[test]
    fn a_half_unpacked_waiting_update_is_not_switched_to() {
        let dir = tempfile::tempdir().unwrap();
        let layout = installed(dir.path(), "0.0.1-beta.1");
        waiting(&layout, "0.0.1-beta.2", now_ms());
        // Still unpacking: the folder has no app yet, and the partial one
        // beside it isn't a version.
        fs::create_dir_all(layout.version_dir(&v("0.0.1-beta.2"))).unwrap();
        let partial = layout::app_executable(&layout.partial_dir(&v("0.0.1-beta.2")));
        fs::create_dir_all(partial.parent().unwrap()).unwrap();
        fs::write(&partial, b"half").unwrap();
        let fake = Fake::default();

        let ran = launch(&layout, plain(), &fake);

        assert!(ran.started);
        assert_eq!(fake.started().0, app_of(&layout, "0.0.1-beta.1"));
        assert_eq!(layout.read_installed().version, Some(v("0.0.1-beta.1")));
        // Left alone for the prepare to finish.
        assert!(layout.read_pending().is_some());
        assert!(partial.is_file());
    }

    // Fails if `switch_to_waiting` checks `pending::switchable` before
    // taking `update::lock_install` (or doesn't take it).
    #[test]
    fn a_switch_waits_for_a_prepare_in_progress() {
        let dir = tempfile::tempdir().unwrap();
        let layout = installed(dir.path(), "0.0.1-beta.1");
        let prepare = update::lock_install(&layout).unwrap();
        let fake = Fake::default();

        let starting = {
            let (layout, fake) = (layout.clone(), fake.clone());
            std::thread::spawn(move || launch(&layout, plain(), &fake).started)
        };
        std::thread::sleep(Duration::from_millis(500));
        assert!(!starting.is_finished(), "the start waits for the prepare");
        // The prepare finishes: the folder is whole, then pending.json.
        whole_app(&layout, "0.0.1-beta.2");
        waiting(&layout, "0.0.1-beta.2", now_ms());
        drop(prepare);

        assert!(starting.join().unwrap());
        assert_eq!(fake.started().0, app_of(&layout, "0.0.1-beta.2"));
        assert_eq!(layout.read_installed().version, Some(v("0.0.1-beta.2")));
    }

    // Fails if `goes_online` ignores `layout.last_start_crashed()`.
    #[test]
    fn a_start_that_never_started_goes_online() {
        let dir = tempfile::tempdir().unwrap();
        let layout = installed(dir.path(), "0.0.1-beta.1");
        fs::remove_file(layout.started_file()).unwrap();
        let fake = Fake::online();

        let ran = launch(&layout, plain(), &fake);

        assert_eq!(fake.asked(), 1);
        assert_eq!(ran.splash_after, Duration::ZERO);
        assert_eq!(ran.statuses.first(), Some(&Status::Checking));
        // GitHub can't be reached, so the installed version starts.
        assert!(ran.started);
        assert_eq!(fake.started().0, app_of(&layout, "0.0.1-beta.1"));

        // The other side: once it has started, the next start stays
        // offline (this fake panics if asked).
        layout
            .write_started(&Mark {
                version: v("0.0.1-beta.1"),
                at: now_ms(),
            })
            .unwrap();
        assert!(launch(&layout, plain(), &Fake::default()).started);
    }

    // Fails if `goes_online` ignores `update::switching_tracks`.
    #[test]
    fn a_track_switch_goes_online() {
        let dir = tempfile::tempdir().unwrap();
        let layout = installed(dir.path(), "0.0.1-beta.1");
        layout
            .write_settings(&Settings {
                track: Some("canary".into()),
                ..Settings::default()
            })
            .unwrap();
        let fake = Fake::online();

        let ran = launch(&layout, plain(), &fake);

        assert_eq!(fake.asked(), 1);
        assert_eq!(ran.splash_after, Duration::ZERO);
        assert!(ran.started);
    }

    // Fails if `goes_online` drops `if start.link { return false }`: a
    // link after a start that never started would wait for GitHub.
    #[test]
    fn a_link_never_waits_for_github() {
        let dir = tempfile::tempdir().unwrap();
        let layout = installed(dir.path(), "0.0.1-beta.1");
        fs::remove_file(layout.started_file()).unwrap();
        let fake = Fake::default();
        let start = Start {
            link: true,
            forward: vec![OsString::from("opencord://host/invite/x")],
            ..plain()
        };

        let ran = launch(&layout, start, &fake);

        assert!(ran.started);
        assert_eq!(ran.splash_after, SPLASH_AFTER);
        assert_eq!(fake.started().0, app_of(&layout, "0.0.1-beta.1"));
    }

    // Fails if `goes_online` ignores `start.installer`.
    #[test]
    fn the_installer_goes_online() {
        let dir = tempfile::tempdir().unwrap();
        let layout = installed(dir.path(), "0.0.1-beta.1");
        let fake = Fake::online();
        let start = Start {
            installer: true,
            ..plain()
        };

        let ran = launch(&layout, start, &fake);

        assert_eq!(fake.installs.load(Ordering::SeqCst), 1);
        assert_eq!(fake.asked(), 1);
        assert_eq!(ran.splash_after, Duration::ZERO);
        assert_eq!(ran.statuses.first(), Some(&Status::Installing));
        assert!(ran.started);
    }

    // Fails if `switch_to_waiting` never reads `restart.json`
    // (`note` is always `None`), and if it reads it without
    // `--after-pid` (the first, plain start would switch).
    #[test]
    fn an_update_restart_switches_an_old_waiting_update() {
        let dir = tempfile::tempdir().unwrap();
        let layout = installed(dir.path(), "0.0.1-beta.1");
        whole_app(&layout, "0.0.1-beta.2");
        // Prepared twice as long ago as a waiting update stays fresh.
        let stale = now_ms() - 2 * u64::try_from(pending::FRESH_FOR.as_millis()).unwrap();
        waiting(&layout, "0.0.1-beta.2", stale);
        layout
            .write_restart(&RestartNote {
                from: v("0.0.1-beta.1"),
                to: v("0.0.1-beta.2"),
                at: now_ms(),
                voice: None,
                window: None,
                cover: false,
            })
            .unwrap();

        // A plain start leaves a stale waiting update alone.
        let fake = Fake::default();
        assert!(launch(&layout, plain(), &fake).started);
        assert_eq!(fake.started().0, app_of(&layout, "0.0.1-beta.1"));
        assert!(layout.read_pending().is_some());

        // The app's update restart takes it.
        let fake = Fake::default();
        let restart = Start {
            restart: true,
            ..plain()
        };
        assert!(launch(&layout, restart, &fake).started);
        assert_eq!(fake.started().0, app_of(&layout, "0.0.1-beta.2"));
        assert_eq!(layout.read_installed().version, Some(v("0.0.1-beta.2")));
        assert!(layout.read_pending().is_none());
    }

    // Fails if `launch::command` doesn't set `FEATURES_ENV`.
    #[test]
    fn the_app_is_started_with_the_features_variable() {
        let dir = tempfile::tempdir().unwrap();
        let layout = installed(dir.path(), "0.0.1-beta.1");
        let fake = Fake::default();

        assert!(launch(&layout, plain(), &fake).started);

        let (_, version, features) = fake.started();
        assert_eq!(version.as_deref(), Some("0.0.1-beta.1"));
        assert_eq!(features.as_deref(), Some("prepare cover"));
    }

    // Fails if `start_offline` ignores `layout.app_running()`, and if
    // `start_installed` writes `launching.json` while the app runs.
    #[test]
    fn a_start_while_the_app_runs_switches_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let layout = installed(dir.path(), "0.0.1-beta.1");
        whole_app(&layout, "0.0.1-beta.2");
        waiting(&layout, "0.0.1-beta.2", now_ms());
        let app = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(layout.app_lock())
            .unwrap();
        app.lock().unwrap();
        let fake = Fake::default();

        let ran = launch(&layout, plain(), &fake);

        assert!(ran.started);
        // Handed to the running app as today: same version, nothing written.
        assert_eq!(fake.started().0, app_of(&layout, "0.0.1-beta.1"));
        assert_eq!(layout.read_installed().version, Some(v("0.0.1-beta.1")));
        assert!(layout.read_pending().is_some());
        assert_eq!(layout.read_launching().unwrap().at, 1);
    }
}
