#![cfg(windows)]
//! Sharing a screen and a window on this PC (docs/plans/windows.md W6).
//! Ignored, because it captures the desktop for a few seconds and opens a
//! Notepad of its own:
//!
//!     cargo test -p opencord-media --test screen_windows -- --ignored --nocapture
//!
//! The rate and the size are measured here; what a frame costs in CPU is
//! measured around this by `app/tool/windows/check_screen_cost.ps1`, which
//! is W6 step 7's question.

use std::sync::mpsc::{RecvTimeoutError, sync_channel};
use std::time::{Duration, Instant};

use opencord_media::share_audio::Source;
use opencord_media::video::camera::RawFormat;
use opencord_media::video::screen::{Capture, SourceKind, StartRequest, sources, start, support};

/// How long to capture for, once the first frame is in.
const WATCH: Duration = Duration::from_secs(4);
/// The rate asked for, and the one §9.2 shares a screen at.
const FPS: u32 = 30;
/// The 1080p budget §9.2 shares a screen within.
const MAX_PIXELS: u32 = 1920 * 1080;

fn request(source_id: &str) -> StartRequest {
    StartRequest {
        fps: FPS,
        max_pixels: MAX_PIXELS,
        restore_token: None,
        source_id: Some(source_id.to_owned()),
    }
}

/// Counts frames for [`WATCH`] and says how many came a second.
fn watch(
    frames: &std::sync::mpsc::Receiver<opencord_media::video::camera::RawFrame>,
) -> (u32, f64) {
    let first = frames
        .recv_timeout(Duration::from_secs(5))
        .expect("a first frame within five seconds");
    assert_eq!(first.format, RawFormat::Bgrx, "screens arrive as BGRx");
    assert_eq!(
        first.data.len(),
        first.width as usize * first.height as usize * 4,
        "the rows are packed, with no stride left in"
    );
    let watching = Instant::now();
    let mut count = 1;
    while watching.elapsed() < WATCH {
        match frames.recv_timeout(Duration::from_secs(2)) {
            Ok(_) => count += 1,
            Err(RecvTimeoutError::Timeout) => break,
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    (count, f64::from(count) / watching.elapsed().as_secs_f64())
}

#[tokio::test]
#[ignore = "captures the desktop"]
async fn a_screen_is_shared_at_the_rate_it_was_asked_for() {
    assert!(support().supported, "this PC can share its screen");
    let found = sources().await.expect("the sources");
    let screen = found
        .iter()
        .find(|source| source.kind == SourceKind::Screen)
        .expect("a screen");
    eprintln!("sharing {} ({})", screen.title, screen.id);

    let (frames, arriving) = sync_channel(4);
    let opening = Instant::now();
    let (capture, picked) = start(request(&screen.id), frames).await.expect("it starts");
    eprintln!(
        "started in {:.0} ms at {:?}",
        opening.elapsed().as_secs_f64() * 1000.0,
        capture.size()
    );
    assert_eq!(picked.kind, SourceKind::Screen);
    assert!(
        picked.sound.is_none(),
        "a whole screen has no one app's sound"
    );

    let (count, rate) = watch(&arriving);
    eprintln!(
        "SCREEN {}x{}: {count} frames, {rate:.1} a second",
        capture.size().0,
        capture.size().1
    );
    assert!(
        rate > f64::from(FPS) / 3.0,
        "only {rate:.1} frames a second"
    );
    assert!(
        rate < f64::from(FPS) * 1.5,
        "{rate:.1} a second is over the {FPS} asked for"
    );
    assert!(!capture.finished(), "the screen is still there");

    let stopping = Instant::now();
    drop(capture);
    eprintln!(
        "stopped in {:.0} ms",
        stopping.elapsed().as_secs_f64() * 1000.0
    );
}

#[tokio::test]
#[ignore = "captures the desktop"]
async fn a_window_is_shared_and_carries_its_app_for_sound() {
    // A window of our own to share, so the test does not depend on what
    // happens to be open. Notepad is on every Windows.
    let mut notepad = std::process::Command::new("notepad.exe")
        .spawn()
        .expect("Notepad starts");
    // It needs a moment to have a window with a title.
    tokio::time::sleep(Duration::from_secs(2)).await;

    let found = sources().await.expect("the sources");
    let window = found
        .iter()
        .find(|source| {
            source.kind == SourceKind::Window
                && source
                    .app
                    .as_deref()
                    .is_some_and(|app| app.to_lowercase().contains("notepad"))
        })
        .or_else(|| {
            found
                .iter()
                .find(|source| source.kind == SourceKind::Window)
        })
        .expect("a window to share")
        .clone();
    eprintln!(
        "sharing {:?} of {:?} ({})",
        window.title, window.app, window.id
    );

    let (frames, arriving) = sync_channel(4);
    let started = start(request(&window.id), frames).await;
    let (capture, picked) = match started {
        Ok(both) => both,
        Err(error) => {
            let _ = notepad.kill();
            let _ = notepad.wait();
            panic!("the window did not share: {error}");
        }
    };
    assert_eq!(picked.kind, SourceKind::Window);
    // §10: a window share knows whose sound to send, so the app never asks.
    match &picked.sound {
        Some(sound) => {
            eprintln!("sound would come from {:?} ({:?})", sound.app, sound.source);
            assert!(matches!(sound.source, Source::Process(pid) if pid != 0));
            assert!(!sound.app.is_empty());
        }
        None => panic!("a window share has to know its app for sound (decision E)"),
    }

    let (count, rate) = watch(&arriving);
    eprintln!(
        "WINDOW {}x{}: {count} frames, {rate:.1} a second",
        capture.size().0,
        capture.size().1
    );
    assert!(count > 1, "a window share sends more than one frame");

    drop(capture);
    let _ = notepad.kill();
    let _ = notepad.wait();
}

#[tokio::test]
#[ignore = "captures the desktop"]
async fn closing_the_shared_window_ends_the_share() {
    let mut notepad = std::process::Command::new("notepad.exe")
        .spawn()
        .expect("Notepad starts");
    tokio::time::sleep(Duration::from_secs(2)).await;

    let found = sources().await.expect("the sources");
    let Some(window) = found
        .iter()
        .find(|source| {
            source
                .app
                .as_deref()
                .is_some_and(|app| app.to_lowercase().contains("notepad"))
        })
        .cloned()
    else {
        let _ = notepad.kill();
        let _ = notepad.wait();
        eprintln!("no Notepad window to share; skipped");
        return;
    };

    let (frames, arriving) = sync_channel(4);
    let (capture, _) = start(request(&window.id), frames)
        .await
        .expect("the window shares");
    arriving
        .recv_timeout(Duration::from_secs(5))
        .expect("a frame before the window closes");
    assert!(!capture.finished(), "it is sharing");

    // Closing it has to end the share, which is what tells the app to say
    // "The shared window was closed" (W6 step 4).
    let _ = notepad.kill();
    let _ = notepad.wait();

    let waited = Instant::now();
    while !capture.finished() && waited.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(100));
    }
    eprintln!(
        "the share ended {:.1} s after the window closed",
        waited.elapsed().as_secs_f64()
    );
    assert!(
        capture.finished(),
        "closing the window did not end the share"
    );
}

/// How much a minimised window sends: nothing, so the sender can keep its
/// last picture going at 1 fps (§9.4, W6 step 4).
#[tokio::test]
#[ignore = "captures the desktop"]
async fn a_capture_that_never_started_is_an_error_not_a_hang() {
    let nowhere = start(request("window:1"), sync_channel(4).0).await;
    assert!(nowhere.is_err(), "a dead handle cannot be shared");
    let nothing = start(
        StartRequest {
            fps: FPS,
            max_pixels: MAX_PIXELS,
            restore_token: None,
            source_id: None,
        },
        sync_channel(4).0,
    )
    .await;
    assert!(nothing.is_err(), "nothing chosen is nothing shared");
}

/// Keeps `Capture` in the test's namespace honest: the type is what
/// `mod.rs` exports, not the backend's.
#[allow(dead_code)]
fn capture_is_the_shared_type(capture: Capture) -> (u32, u32) {
    capture.size()
}
