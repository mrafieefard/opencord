#![cfg(windows)]
//! The camera on this PC (docs/plans/windows.md W5, step 2). Ignored,
//! because it needs a real camera — the built-in one, or OBS Virtual
//! Camera — and turns it on, which lights its lamp:
//!
//!     cargo test -p opencord-media --test camera_windows -- --ignored --nocapture
//!
//! `OPENCORD_TEST_CAMERA` picks one by a piece of its name, as
//! `devices_windows.rs` does for microphones; without it the first is used.

use std::sync::mpsc::{RecvTimeoutError, sync_channel};
use std::time::{Duration, Instant};

use opencord_media::video::camera::{CameraError, Capture, cameras, supported};

/// How long the first frame may take. The plan's acceptance is under a
/// second from the app's point of view; this is the camera's own part of
/// that, so it is given the whole second.
const FIRST_FRAME: Duration = Duration::from_secs(1);
/// How long to watch before stopping.
const WATCH: Duration = Duration::from_secs(3);

fn chosen() -> Option<String> {
    std::env::var("OPENCORD_TEST_CAMERA")
        .ok()
        .filter(|name| !name.is_empty())
}

#[tokio::test]
async fn windows_lists_its_cameras() {
    assert!(supported(), "Windows has a camera backend");
    let found = match cameras().await {
        Ok(found) => found,
        // Said plainly, because this is the one error a person can fix.
        Err(CameraError::Denied) => panic!(
            "the camera was refused. Allow camera access for desktop apps in \
             Settings → Privacy & security → Camera"
        ),
        Err(error) => panic!("the cameras could not be listed: {error}"),
    };
    for camera in &found {
        eprintln!("{} — {}", camera.name, camera.id);
    }
    // A machine with no camera is not a failure; CI has none.
    if found.is_empty() {
        eprintln!("no camera on this machine");
        return;
    }
    for camera in &found {
        assert!(!camera.id.is_empty(), "every camera has an id");
        assert!(!camera.name.is_empty(), "every camera has a name");
        assert!(
            camera.id.starts_with("mf:"),
            "ids say where they came from: {}",
            camera.id
        );
    }
    let mut ids: Vec<&str> = found.iter().map(|camera| camera.id.as_str()).collect();
    ids.sort_unstable();
    let count = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), count, "no two cameras share an id");
}

/// Needs a camera; run with `--ignored` where there is one.
#[tokio::test]
#[ignore = "turns a real camera on"]
async fn the_camera_starts_delivers_frames_and_stops() {
    let found = cameras().await.expect("the cameras");
    let camera = match chosen() {
        Some(name) => found
            .iter()
            .find(|camera| camera.name.to_lowercase().contains(&name.to_lowercase()))
            .unwrap_or_else(|| panic!("no camera's name holds {name:?}; there are {found:?}")),
        None => found.first().expect("a camera is connected"),
    };
    eprintln!("camera: {} — {}", camera.name, camera.id);

    // Four frames deep, as the engine runs it: a slow consumer drops rather
    // than holds the camera up.
    let (frames, arriving) = sync_channel(4);
    let opening = Instant::now();
    let capture = Capture::start(Some(camera.id.clone()), frames)
        .await
        .expect("the camera opens");
    let opened = opening.elapsed();
    let mode = capture.mode();
    eprintln!(
        "opened in {:.0} ms as {mode:?}",
        opened.as_secs_f64() * 1000.0
    );

    let first = arriving
        .recv_timeout(FIRST_FRAME)
        .expect("a frame within a second");
    let waited = opening.elapsed();
    eprintln!(
        "first frame {}×{} {:?}, {} bytes, {:.0} ms after asking",
        first.width,
        first.height,
        first.format,
        first.data.len(),
        waited.as_secs_f64() * 1000.0
    );
    assert_eq!((first.width, first.height), (mode.width, mode.height));
    assert_eq!(first.format, mode.format);
    assert!(!first.data.is_empty(), "a frame with no bytes is no frame");

    // It keeps going, at about the rate it promised.
    let watching = Instant::now();
    let mut count = 1;
    while watching.elapsed() < WATCH {
        match arriving.recv_timeout(Duration::from_secs(1)) {
            Ok(_) => count += 1,
            Err(RecvTimeoutError::Timeout) => panic!("the camera stopped sending"),
            Err(RecvTimeoutError::Disconnected) => panic!("the camera went away"),
        }
    }
    let rate = f64::from(count) / watching.elapsed().as_secs_f64();
    eprintln!(
        "{count} frames in {:.1} s — {rate:.1} a second",
        watching.elapsed().as_secs_f64()
    );
    assert!(rate > 5.0, "only {rate:.1} frames a second");

    // Stopping is dropping, and it has to be quick: the app's camera button
    // waits for it.
    let stopping = Instant::now();
    drop(capture);
    let stopped = stopping.elapsed();
    eprintln!("stopped in {:.0} ms", stopped.as_secs_f64() * 1000.0);
    assert!(
        stopped < Duration::from_secs(1),
        "stopping took {stopped:?}, over the second §8 allows"
    );

    // And nothing comes after. Whatever was already in the queue is fair,
    // so the queue is drained first.
    while arriving.try_recv().is_ok() {}
    assert!(
        matches!(
            arriving.recv_timeout(Duration::from_millis(300)),
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected)
        ),
        "a frame arrived after the camera was stopped"
    );
}

/// Needs a camera; run with `--ignored` where there is one. Opening the
/// same camera twice is what another app holding it looks like from here,
/// which is the error W5 asks to have recorded.
#[tokio::test]
#[ignore = "turns a real camera on"]
async fn a_camera_another_app_is_using_says_so() {
    let found = cameras().await.expect("the cameras");
    let Some(camera) = found.first() else {
        eprintln!("no camera on this machine");
        return;
    };
    let (frames, _first_queue) = sync_channel(4);
    let held = Capture::start(Some(camera.id.clone()), frames)
        .await
        .expect("the camera opens");

    let (frames, _second_queue) = sync_channel(4);
    let second = Capture::start(Some(camera.id.clone()), frames).await;

    match &second {
        // Some cameras — OBS's virtual one, and a few drivers — really do
        // allow two readers. That is worth saying, not failing.
        Ok(_) => eprintln!("this camera allows two readers at once"),
        Err(error) => eprintln!("the second open said: {error}"),
    }
    drop(held);
}
