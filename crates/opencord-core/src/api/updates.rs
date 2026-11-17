//! Desktop update management API for the Flutter app (design: "The app: checking",
//! plan Task 10).

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use std::collections::HashMap;
use std::collections::VecDeque;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use std::fs::{self, File, OpenOptions, TryLockError};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use std::path::PathBuf;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use std::sync::Arc;
use std::sync::{Mutex, PoisonError};
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use std::time::Duration;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use std::time::{SystemTime, UNIX_EPOCH};

use flutter_rust_bridge::frb;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use semver::Version;
use tokio::task::JoinHandle;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
use opencord_update::layout::Layout;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use opencord_update::markers::Mark;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use opencord_update::track::Track;

use crate::frb_generated::StreamSink;
#[cfg(not(any(target_os = "android", target_os = "ios")))]
use crate::updates::{CheckOutcome, UpdateError, Updater, VoiceThrottle};

#[cfg(not(any(target_os = "android", target_os = "ios")))]
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(15);
#[cfg(not(any(target_os = "android", target_os = "ios")))]
const CHECK_INTERVAL: Duration = Duration::from_secs(4 * 60 * 60);

/// Events emitted during background checks and downloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateEvent {
    Downloaded {
        version: String,
        download_json: String,
    },
    AlreadyWaiting {
        version: String,
    },
    Failed {
        kind: String,
        message: String,
    },
}

#[frb(ignore)]
struct UpdateState {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    updater: Option<Arc<Updater>>,
    task: Option<JoinHandle<()>>,
    sink: Option<StreamSink<UpdateEvent>>,
    backlog: VecDeque<UpdateEvent>,
}

impl UpdateState {
    fn new() -> Self {
        Self {
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            updater: None,
            task: None,
            sink: None,
            backlog: VecDeque::new(),
        }
    }
}

static STATE: Mutex<Option<UpdateState>> = Mutex::new(None);
#[cfg(not(any(target_os = "android", target_os = "ios")))]
static APP_LOCK: Mutex<Option<HashMap<PathBuf, File>>> = Mutex::new(None);
#[cfg(not(any(target_os = "android", target_os = "ios")))]
static CHECK_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn lock_updates() -> std::sync::MutexGuard<'static, Option<UpdateState>> {
    let mut guard = STATE.lock().unwrap_or_else(PoisonError::into_inner);
    if guard.is_none() {
        *guard = Some(UpdateState::new());
    }
    guard
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn lock_app_files() -> std::sync::MutexGuard<'static, Option<HashMap<PathBuf, File>>> {
    let mut guard = APP_LOCK.lock().unwrap_or_else(PoisonError::into_inner);
    if guard.is_none() {
        *guard = Some(HashMap::new());
    }
    guard
}

#[cfg(test)]
#[frb(ignore)]
pub(crate) fn reset_for_test() {
    let mut guard = STATE.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(mut state) = guard.take() {
        if let Some(task) = state.task.take() {
            task.abort();
        }
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let mut files = lock_app_files();
        if let Some(files) = files.as_mut() {
            files.clear();
        }
    }
}

#[cfg(test)]
#[frb(ignore)]
pub(crate) fn drain_events() -> Vec<UpdateEvent> {
    let mut guard = lock_updates();
    let state = guard.as_mut().unwrap();
    state.backlog.drain(..).collect()
}

/// Registers the event stream sink for update notifications.
pub fn update_events(sink: StreamSink<UpdateEvent>) -> Result<(), String> {
    let mut guard = lock_updates();
    let state = guard.as_mut().unwrap();
    while let Some(event) = state.backlog.pop_front() {
        if sink.add(event).is_err() {
            return Ok(());
        }
    }
    state.sink = Some(sink);
    Ok(())
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
const BACKLOG_CAPACITY: usize = 10;

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn deliver(event: UpdateEvent, expected: &Arc<Updater>) {
    let mut guard = lock_updates();
    let state = guard.as_mut().unwrap();
    if !state
        .updater
        .as_ref()
        .is_some_and(|u| Arc::ptr_eq(u, expected))
    {
        tracing::info!("discarding update event for superseded updater");
        return;
    }
    if let Some(sink) = &state.sink {
        if sink.add(event.clone()).is_err() {
            state.sink = None;
            push_backlog(state, event);
        }
    } else {
        push_backlog(state, event);
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn push_backlog(state: &mut UpdateState, event: UpdateEvent) {
    if state.backlog.len() >= BACKLOG_CAPACITY {
        state.backlog.pop_front();
    }
    state.backlog.push_back(event);
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn deliver_outcome(outcome: Result<CheckOutcome, UpdateError>, expected: &Arc<Updater>) {
    match outcome {
        Ok(CheckOutcome::Downloaded {
            version,
            download_json,
        }) => {
            deliver(
                UpdateEvent::Downloaded {
                    version: version.to_string(),
                    download_json: download_json.to_string_lossy().into_owned(),
                },
                expected,
            );
        }
        Ok(CheckOutcome::AlreadyWaiting(version)) => {
            deliver(
                UpdateEvent::AlreadyWaiting {
                    version: version.to_string(),
                },
                expected,
            );
        }
        Ok(CheckOutcome::UpToDate) => {}
        Err(error) => {
            deliver(
                UpdateEvent::Failed {
                    kind: error.kind().to_string(),
                    message: error.to_string(),
                },
                expected,
            );
        }
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn prepare_running(layout: &Layout) -> bool {
    let path = layout.prepare_lock();
    if !path.exists() {
        return false;
    }
    let Ok(file) = OpenOptions::new().read(true).write(true).open(&path) else {
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

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn now_ms() -> u64 {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO);
    u64::try_from(since_epoch.as_millis()).unwrap_or(u64::MAX)
}

/// Starts periodic update checking.
#[frb(sync)]
pub fn start_updates(root: String, track: String, installed: String) -> Result<(), String> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let track = Track::parse(&track).ok_or_else(|| format!("unknown track: {track}"))?;
        let installed = Version::parse(&installed).map_err(|e| format!("invalid version: {e}"))?;
        let platform = opencord_update::release::PLATFORM
            .ok_or_else(|| "updates not supported on this platform".to_string())?;
        let _ = rustls::crypto::ring::default_provider().install_default();
        let http = reqwest::Client::builder()
            .build()
            .map_err(|e| e.to_string())?;

        let handle = tokio::runtime::Handle::try_current().or_else(|_| {
            crate::api::client::runtime()
                .map(|r| r.handle().clone())
                .map_err(|e| e.to_string())
        })?;

        let throttle = {
            let _guard = handle.enter();
            Arc::new(VoiceThrottle::new(crate::updates::voice_readings()))
        };

        let updater = Arc::new(Updater::new(
            PathBuf::from(root),
            track,
            installed,
            platform,
            http,
            throttle,
        ));

        start_updates_with_updater(updater)
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = (root, track, installed);
        Err("updates not supported on mobile".to_string())
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[frb(ignore)]
pub(crate) fn start_updates_with_updater(updater: Arc<Updater>) -> Result<(), String> {
    let handle = tokio::runtime::Handle::try_current().or_else(|_| {
        crate::api::client::runtime()
            .map(|r| r.handle().clone())
            .map_err(|e| e.to_string())
    })?;

    let mut guard = lock_updates();
    let state = guard.as_mut().unwrap();
    if let Some(task) = state.task.take() {
        task.abort();
    }
    state.updater = Some(updater.clone());

    let task = handle.spawn(async move {
        tokio::time::sleep(FIRST_CHECK_DELAY).await;
        loop {
            let _lock = CHECK_LOCK.lock().await;
            if !prepare_running(updater.layout()) {
                let outcome = updater.check_once().await;
                deliver_outcome(outcome, &updater);
            } else {
                tracing::info!("prepare is running, skipping update check");
            }
            drop(_lock);
            tokio::time::sleep(CHECK_INTERVAL).await;
        }
    });
    state.task = Some(task);
    Ok(())
}

/// Stops periodic update checking.
#[frb(sync)]
pub fn stop_updates() {
    let mut guard = lock_updates();
    let state = guard.as_mut().unwrap();
    if let Some(task) = state.task.take() {
        task.abort();
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        state.updater = None;
    }
}

/// Manually triggers an update check immediately.
pub async fn check_updates_now() -> Result<(), String> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let updater = {
            let mut guard = lock_updates();
            let state = guard.as_mut().unwrap();
            state.updater.clone().ok_or("updates not started")?
        };
        let _lock = CHECK_LOCK.lock().await;
        if prepare_running(updater.layout()) {
            return Err("prepare is running".to_string());
        }
        let outcome = updater.check_once().await;
        let res = match &outcome {
            Ok(_) => Ok(()),
            Err(e) => Err(e.to_string()),
        };
        deliver_outcome(outcome, &updater);
        res
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        Err("updates not supported on mobile".to_string())
    }
}

/// Writes `started.json` and takes `app.lock`.
#[frb(sync)]
pub fn mark_started(root: String, version: String) -> Result<(), String> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let ver = Version::parse(&version).map_err(|e| e.to_string())?;
        let layout = Layout::new(PathBuf::from(root));
        fs::create_dir_all(layout.root()).map_err(|e| e.to_string())?;

        let lock_path = layout.app_lock();
        let mut files_guard = lock_app_files();
        let files = files_guard.as_mut().unwrap();

        if !files.contains_key(&lock_path) {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(&lock_path)
                .map_err(|e| e.to_string())?;
            match file.try_lock() {
                Ok(()) => {}
                Err(TryLockError::WouldBlock) => {
                    return Err("another Opencord holds the install".to_string());
                }
                Err(TryLockError::Error(e)) => return Err(e.to_string()),
            }
            files.insert(lock_path, file);
        }

        let mark = Mark {
            version: ver,
            at: now_ms(),
        };
        layout.write_started(&mark).map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = (root, version);
        Err("updates not supported on mobile".to_string())
    }
}

/// Reads the waiting update version from `pending.json`, if any.
#[frb(sync)]
pub fn waiting_version(root: String) -> Option<String> {
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    {
        let layout = Layout::new(PathBuf::from(root));
        layout.read_pending().map(|p| p.version.to_string())
    }
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        let _ = root;
        None
    }
}

#[cfg(all(test, not(any(target_os = "android", target_os = "ios"))))]
mod tests {
    use super::*;
    use opencord_update::pending::Pending;
    use tempfile::tempdir;

    static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    fn test_updater(root: PathBuf, base_url: String) -> Arc<Updater> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let http = reqwest::Client::builder().build().unwrap();
        let throttle = Arc::new(crate::updates::Unlimited);
        Arc::new(Updater::new_with_base_url(
            root,
            Track::Beta,
            Version::parse("0.0.1").unwrap(),
            "client-linux-x64",
            http,
            throttle,
            base_url,
        ))
    }

    async fn start_test_server() -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let app = axum::Router::new().fallback(|| async { axum::http::StatusCode::NOT_FOUND });
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        base
    }

    async fn start_pausing_test_server(
        notify_req: Arc<tokio::sync::Notify>,
        notify_wait: Arc<tokio::sync::Notify>,
    ) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let repo = opencord_update::REPOSITORY;
        let listing_path = format!("/repos/{repo}/releases");
        let manifest_path = format!("/{repo}/releases/download/v0.0.2-beta.2/release.json");

        let app = axum::Router::new()
            .route(
                &listing_path,
                axum::routing::get(move || {
                    let notify_req = notify_req.clone();
                    let notify_wait = notify_wait.clone();
                    async move {
                        notify_req.notify_one();
                        notify_wait.notified().await;
                        axum::Json(serde_json::json!([
                            {"tag_name": "v0.0.2-beta.2"}
                        ]))
                    }
                }),
            )
            .route(
                &manifest_path,
                axum::routing::get(|| async {
                    axum::Json(serde_json::json!({
                        "schema": 1,
                        "version": "0.0.2-beta.2",
                        "tag": "v0.0.2-beta.2",
                        "platforms": {
                            "client-linux-x64": {
                                "file": "opencord.tar.gz",
                                "url": "http://127.0.0.1/assets/opencord.tar.gz",
                                "size": 100,
                                "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
                            }
                        }
                    }))
                }),
            );
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        base
    }

    #[tokio::test]
    async fn start_updates_checks_after_15_s() {
        // Fails if the first check runs before 15 s (e.g. FIRST_CHECK_DELAY changed from 15 s to 0 s)
        let _test_lock = TEST_LOCK.lock().await;
        reset_for_test();

        let base = start_test_server().await;
        tokio::time::pause();

        let dir = tempdir().unwrap();
        let updater = test_updater(dir.path().to_path_buf(), base);

        start_updates_with_updater(updater).unwrap();
        tokio::task::yield_now().await;

        tokio::time::advance(Duration::from_secs(14)).await;
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(drain_events().is_empty(), "no check should run before 15 s");

        tokio::time::advance(Duration::from_secs(2)).await;
        let mut events = Vec::new();
        for _ in 0..50 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            events = drain_events();
            if !events.is_empty() {
                break;
            }
        }
        assert_eq!(events.len(), 1, "expected 1 event after 15s check delay");
    }

    #[tokio::test]
    async fn checks_repeat_every_4_h() {
        // Fails if periodic loop does not repeat every 4 hours (e.g. break after first check)
        let _test_lock = TEST_LOCK.lock().await;
        reset_for_test();

        let base = start_test_server().await;
        tokio::time::pause();

        let dir = tempdir().unwrap();
        let updater = test_updater(dir.path().to_path_buf(), base);

        start_updates_with_updater(updater).unwrap();
        tokio::task::yield_now().await;

        // Advance 16 s: first check runs
        tokio::time::advance(Duration::from_secs(16)).await;
        let mut events = Vec::new();
        for _ in 0..50 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            events = drain_events();
            if !events.is_empty() {
                break;
            }
        }
        assert_eq!(events.len(), 1, "expected first event at 15 s");

        // Advance 4 h - 1 min: no second check yet
        tokio::time::advance(CHECK_INTERVAL - Duration::from_secs(60)).await;
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(
            drain_events().is_empty(),
            "no check should run before 4 h interval"
        );

        // Advance another 2 min: second check runs
        tokio::time::advance(Duration::from_secs(120)).await;
        let mut second_events = Vec::new();
        for _ in 0..50 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            second_events = drain_events();
            if !second_events.is_empty() {
                break;
            }
        }
        assert_eq!(
            second_events.len(),
            1,
            "expected second event after 4 h interval"
        );
    }

    #[tokio::test]
    async fn a_second_start_replaces_the_first() {
        // Fails if a second start_updates does not abort the first task (e.g. omitting task.abort())
        let _test_lock = TEST_LOCK.lock().await;
        reset_for_test();

        let base = start_test_server().await;
        tokio::time::pause();

        let dir = tempdir().unwrap();
        let updater = test_updater(dir.path().to_path_buf(), base);

        start_updates_with_updater(updater.clone()).unwrap();
        start_updates_with_updater(updater).unwrap();
        tokio::task::yield_now().await;

        tokio::time::advance(Duration::from_secs(16)).await;
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let events = drain_events();
        assert_eq!(
            events.len(),
            1,
            "expected exactly 1 event from replaced task"
        );
    }

    #[tokio::test]
    async fn stop_updates_ends_the_task() {
        // Fails if stop_updates does not abort the background task
        let _test_lock = TEST_LOCK.lock().await;
        reset_for_test();

        let base = start_test_server().await;
        tokio::time::pause();

        let dir = tempdir().unwrap();
        let updater = test_updater(dir.path().to_path_buf(), base);

        start_updates_with_updater(updater).unwrap();
        tokio::task::yield_now().await;

        tokio::time::advance(Duration::from_secs(16)).await;
        let mut events = Vec::new();
        for _ in 0..50 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            events = drain_events();
            if !events.is_empty() {
                break;
            }
        }
        assert_eq!(events.len(), 1);

        stop_updates();

        tokio::time::advance(CHECK_INTERVAL).await;
        for _ in 0..20 {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(
            drain_events().is_empty(),
            "no check should run after stop_updates"
        );
    }

    #[tokio::test]
    async fn a_failure_is_sent_as_failed() {
        // Fails if check failure is not delivered as UpdateEvent::Failed
        let _test_lock = TEST_LOCK.lock().await;
        reset_for_test();

        let dir = tempdir().unwrap();
        let updater = test_updater(dir.path().to_path_buf(), "http://127.0.0.1:1".to_string());

        start_updates_with_updater(updater).unwrap();
        let res = check_updates_now().await;
        assert!(res.is_err());

        let events = drain_events();
        assert_eq!(events.len(), 1);
        match &events[0] {
            UpdateEvent::Failed { kind, message } => {
                assert_eq!(kind, "network");
                assert!(!message.is_empty());
            }
            other => panic!("expected UpdateEvent::Failed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_check_from_before_a_track_switch_sends_nothing() {
        // Fails if check_updates_now delivers outcome using current updater rather than the one that ran the check
        let _test_lock = TEST_LOCK.lock().await;
        reset_for_test();

        let notify_req = Arc::new(tokio::sync::Notify::new());
        let notify_wait = Arc::new(tokio::sync::Notify::new());
        let base = start_pausing_test_server(notify_req.clone(), notify_wait.clone()).await;

        let dir = tempdir().unwrap();
        let layout = Layout::new(dir.path().to_path_buf());
        let version = Version::parse("0.0.2-beta.2").unwrap();
        let exe = opencord_update::layout::app_executable(&layout.version_dir(&version));
        fs::create_dir_all(exe.parent().unwrap()).unwrap();
        fs::write(exe, b"fake_app").unwrap();
        layout
            .write_pending(&Pending {
                version,
                track: "beta".to_string(),
                prepared_at: 100,
                confirmed_at: Some(100),
            })
            .unwrap();

        let old_updater = test_updater(dir.path().to_path_buf(), base.clone());
        let new_dir = tempdir().unwrap();
        let new_updater = test_updater(new_dir.path().to_path_buf(), base);

        start_updates_with_updater(old_updater).unwrap();

        let check_task = tokio::spawn(async { check_updates_now().await });
        notify_req.notified().await;

        // Switch to new updater while old check is in flight
        start_updates_with_updater(new_updater).unwrap();
        notify_wait.notify_one();

        let res = check_task.await.unwrap();
        assert!(res.is_ok(), "check_updates_now should succeed: {res:?}");
        assert!(
            drain_events().is_empty(),
            "outcome from old updater must be discarded after track switch"
        );
    }

    #[tokio::test]
    async fn no_check_runs_while_a_prepare_holds_the_lock() {
        // Fails if check_updates_now checks prepare_running before taking CHECK_LOCK
        let _test_lock = TEST_LOCK.lock().await;
        reset_for_test();

        let base = start_test_server().await;
        let dir = tempdir().unwrap();
        let updater = test_updater(dir.path().to_path_buf(), base);
        start_updates_with_updater(updater.clone()).unwrap();

        let layout = updater.layout();
        fs::create_dir_all(layout.root()).unwrap();

        let check_guard = CHECK_LOCK.lock().await;

        let check_task = tokio::spawn(async { check_updates_now().await });
        tokio::task::yield_now().await;

        let prepare_lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(layout.prepare_lock())
            .unwrap();
        prepare_lock_file.lock().unwrap();

        drop(check_guard);

        let res = check_task.await.unwrap();
        assert_eq!(res.unwrap_err(), "prepare is running");
        assert!(
            drain_events().is_empty(),
            "no event should arrive when prepare is running"
        );
    }

    #[tokio::test]
    async fn background_loop_skips_check_while_prepare_is_running() {
        // Fails if background loop runs check while prepare is running (e.g. if true || !prepare_running)
        let _test_lock = TEST_LOCK.lock().await;
        reset_for_test();

        let base = start_test_server().await;
        tokio::time::pause();

        let dir = tempdir().unwrap();
        let updater = test_updater(dir.path().to_path_buf(), base);
        let layout = updater.layout();
        fs::create_dir_all(layout.root()).unwrap();

        let prepare_lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(layout.prepare_lock())
            .unwrap();
        prepare_lock_file.lock().unwrap();

        start_updates_with_updater(updater).unwrap();
        tokio::task::yield_now().await;

        tokio::time::advance(Duration::from_secs(16)).await;
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(
            drain_events().is_empty(),
            "no event while prepare holds the lock"
        );

        drop(prepare_lock_file);
        tokio::time::advance(Duration::from_secs(4 * 60 * 60 + 1)).await;
        let mut events = Vec::new();
        for _ in 0..50 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            events = drain_events();
            if !events.is_empty() {
                break;
            }
        }
        assert_eq!(events.len(), 1, "expected 1 event after lock released");
    }

    #[tokio::test]
    async fn start_updates_succeeds_and_sets_updater_and_task() {
        // Fails if start_updates does not call start_updates_with_updater (e.g. returns Ok(()) without starting)
        let _test_lock = TEST_LOCK.lock().await;
        reset_for_test();

        tokio::time::pause();
        let dir = tempdir().unwrap();
        let res = start_updates(
            dir.path().to_str().unwrap().to_string(),
            "beta".to_string(),
            "0.0.1".to_string(),
        );
        assert!(res.is_ok(), "start_updates should succeed: {res:?}");

        {
            let guard = lock_updates();
            let state = guard.as_ref().unwrap();
            assert!(state.updater.is_some(), "state.updater must be set");
            assert!(state.task.is_some(), "state.task must be set");
        }

        stop_updates();
    }

    #[test]
    fn start_updates_refuses_an_unknown_track_or_version() {
        // Fails if start_updates does not validate track or version
        let _test_lock = TEST_LOCK.blocking_lock();
        reset_for_test();

        let res_track = start_updates(
            "/tmp".to_string(),
            "invalid_track".to_string(),
            "0.0.1".to_string(),
        );
        assert!(res_track.is_err(), "unknown track must be refused");
        assert!(res_track.unwrap_err().contains("unknown track"));

        let res_ver = start_updates(
            "/tmp".to_string(),
            "beta".to_string(),
            "not_a_version".to_string(),
        );
        assert!(res_ver.is_err(), "invalid version must be refused");
        assert!(res_ver.unwrap_err().contains("invalid version"));
    }

    #[test]
    fn mark_started_writes_started_json() {
        // Fails if mark_started does not write started.json
        let _test_lock = TEST_LOCK.blocking_lock();
        reset_for_test();

        let dir = tempdir().unwrap();
        let root = dir.path().to_str().unwrap().to_string();
        let layout = Layout::new(dir.path().to_path_buf());

        mark_started(root, "0.0.2-beta.2".to_string()).unwrap();

        let started = layout.read_started().expect("started.json must exist");
        assert_eq!(started.version, Version::parse("0.0.2-beta.2").unwrap());
    }

    #[test]
    fn mark_started_holds_the_app_lock() {
        // Fails if mark_started does not hold app.lock or allow lock detection
        let _test_lock = TEST_LOCK.blocking_lock();
        reset_for_test();

        let dir = tempdir().unwrap();
        let root = dir.path().to_str().unwrap().to_string();
        let layout = Layout::new(dir.path().to_path_buf());

        mark_started(root.clone(), "0.0.2-beta.2".to_string()).unwrap();

        assert!(
            layout.app_running(),
            "app_running must be true when lock held"
        );

        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(layout.app_lock())
            .unwrap();
        assert!(
            lock_file.try_lock().is_err(),
            "second lock acquisition must fail"
        );

        // Also verify that another mark_started call against an externally locked install fails
        let dir2 = tempdir().unwrap();
        let layout2 = Layout::new(dir2.path().to_path_buf());
        fs::create_dir_all(layout2.root()).unwrap();
        let held = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(layout2.app_lock())
            .unwrap();
        held.lock().unwrap();

        let res = mark_started(
            dir2.path().to_str().unwrap().to_string(),
            "0.0.2-beta.2".to_string(),
        );
        assert_eq!(res.unwrap_err(), "another Opencord holds the install");
    }

    #[test]
    fn waiting_version_reads_pending() {
        // Fails if waiting_version does not return the version from pending.json
        let _test_lock = TEST_LOCK.blocking_lock();
        reset_for_test();

        let dir = tempdir().unwrap();
        let root = dir.path().to_str().unwrap().to_string();
        let layout = Layout::new(dir.path().to_path_buf());

        assert_eq!(waiting_version(root.clone()), None);

        layout
            .write_pending(&Pending {
                version: Version::parse("0.0.3-beta.1").unwrap(),
                track: "beta".to_string(),
                prepared_at: 1000,
                confirmed_at: None,
            })
            .unwrap();

        assert_eq!(waiting_version(root), Some("0.0.3-beta.1".to_string()));
    }
}
