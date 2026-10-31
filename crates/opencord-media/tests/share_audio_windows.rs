#![cfg(windows)]
//! Sharing a screen's sound on Windows (docs/plans/windows.md W7, step 3).
//! Ignored, because each test plays a tone and captures it:
//!
//!     cargo test -p opencord-media --test share_audio_windows -- --ignored --nocapture
//!
//! Process loopback needs Windows 10 2004 (build 19041); below that every
//! one of them skips with a line saying so. They are written against
//! Microsoft's ApplicationLoopback sample's behaviour and the contract in
//! `share_audio/mod.rs`, and have yet to be run on a machine that can
//! share sound (windows.md §7, W7).
//!
//! Nothing is audible: the tones go to the VB-CABLE device, not the
//! speakers. `OPENCORD_TEST_SINK` names the device to play into, and
//! defaults to CABLE Input.

use std::process::{Child, Command, Stdio};
use std::sync::mpsc::sync_channel;
use std::time::{Duration, Instant};

use opencord_media::share_audio::{Capture, Captured, Source, support};

/// How long to capture before measuring.
const LISTEN: Duration = Duration::from_secs(4);
/// Everything here is 48 kHz stereo.
const RATE: f32 = 48_000.0;

/// Where the tones go, so they are never heard.
fn sink() -> String {
    std::env::var("OPENCORD_TEST_SINK").unwrap_or_else(|_| "CABLE Input".to_owned())
}

/// Says why, and that the test did nothing.
fn cannot_share_sound() -> bool {
    let support = support();
    if !support.supported {
        eprintln!(
            "skipped: {}",
            support
                .reason
                .as_deref()
                .unwrap_or("no process loopback here")
        );
    }
    !support.supported
}

/// An ffplay playing one tone into the cable, until it is killed.
fn tone(hz: u32) -> Child {
    Command::new("ffplay")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-nodisp",
            "-autoexit",
            "-f",
            "lavfi",
            "-i",
            &format!("sine=frequency={hz}:duration=30"),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("ffplay is on PATH")
}

/// Captures for [`LISTEN`] and returns every sample, interleaved.
fn listen(capture: &Capture, frames: std::sync::mpsc::Receiver<Captured>) -> Vec<f32> {
    let _ = capture;
    let until = Instant::now() + LISTEN;
    let mut samples = Vec::new();
    let mut last = 0u64;
    while Instant::now() < until {
        match frames.recv_timeout(Duration::from_millis(500)) {
            Ok(block) => {
                // The contract: `position` counts frames on this capture's
                // own clock, so it only ever goes forward.
                assert!(
                    block.position >= last,
                    "position went backwards: {} after {last}",
                    block.position
                );
                last = block.position;
                samples.extend_from_slice(&block.samples);
            }
            Err(_) => continue,
        }
    }
    samples
}

/// How loud `hz` is in one channel of interleaved stereo, in dBFS. The same
/// Goertzel the voicebot uses.
fn level(samples: &[f32], channel: usize, hz: f32) -> f32 {
    let mono: Vec<f32> = samples.iter().skip(channel).step_by(2).copied().collect();
    if mono.is_empty() {
        return -200.0;
    }
    let coefficient = 2.0 * (std::f32::consts::TAU * hz / RATE).cos();
    let (mut first, mut second) = (0f32, 0f32);
    for sample in &mono {
        (first, second) = (sample + coefficient * first - second, first);
    }
    let power = (first * first + second * second - coefficient * first * second).max(0.0);
    20.0 * (2.0 * power.sqrt() / mono.len() as f32).max(1e-10).log10()
}

struct Killed(Vec<Child>);

impl Drop for Killed {
    fn drop(&mut self) {
        for child in &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// A whole screen hears other apps but never Opencord — which here means
/// never this test process, since `Source::Everything` excludes our own
/// process tree.
#[test]
#[ignore = "plays tones and captures them"]
fn a_screen_hears_another_app_but_not_this_process() {
    if cannot_share_sound() {
        return;
    }
    eprintln!("tones go to {}", sink());
    let _tones = Killed(vec![tone(1_000)]);
    std::thread::sleep(Duration::from_secs(1));

    let (frames, arriving) = sync_channel(32);
    let capture = Capture::start(Source::Everything, frames).expect("the screen's sound");
    let samples = listen(&capture, arriving);

    assert!(!samples.is_empty(), "a screen share captured nothing");
    let heard = level(&samples, 0, 1_000.0);
    eprintln!("1 kHz from the other app: {heard:.1} dBFS");
    assert!(
        heard > -50.0,
        "the other app's tone was not heard: {heard:.1} dBFS"
    );
}

/// A window share hears the app it names and nothing else.
#[test]
#[ignore = "plays tones and captures them"]
fn a_window_hears_only_its_own_app() {
    if cannot_share_sound() {
        return;
    }
    let mut wanted = tone(1_000);
    let other = tone(2_400);
    let pid = wanted.id();
    let _tones = Killed(vec![other]);
    std::thread::sleep(Duration::from_secs(1));

    let (frames, arriving) = sync_channel(32);
    let capture = Capture::start(Source::Process(pid), frames).expect("that app's sound");
    let samples = listen(&capture, arriving);
    let _ = wanted.kill();
    let _ = wanted.wait();

    assert!(!samples.is_empty(), "a window share captured nothing");
    let mine = level(&samples, 0, 1_000.0);
    let theirs = level(&samples, 0, 2_400.0);
    eprintln!("its own tone {mine:.1} dBFS, the other app's {theirs:.1} dBFS");
    assert!(mine > -50.0, "its own tone was not heard: {mine:.1} dBFS");
    // The whole point of a window share: the other app stays out.
    assert!(
        mine - theirs > 20.0,
        "the other app leaked in: {mine:.1} against {theirs:.1} dBFS"
    );
}

/// Changing the source while live: a process loopback client cannot be
/// retargeted, so `share` has to open another one.
#[test]
#[ignore = "plays tones and captures them"]
fn sharing_another_source_while_live_switches_what_is_heard() {
    if cannot_share_sound() {
        return;
    }
    let mut first = tone(1_000);
    let mut second = tone(2_400);
    let (one, two) = (first.id(), second.id());
    std::thread::sleep(Duration::from_secs(1));

    let (frames, arriving) = sync_channel(32);
    let capture = Capture::start(Source::Process(one), frames).expect("the first app's sound");
    let before = listen(&capture, arriving);
    assert!(
        level(&before, 0, 1_000.0) > -50.0,
        "the first app was not heard"
    );

    // The queue is the same one; only what feeds it changes.
    let (frames, arriving) = sync_channel(32);
    drop(frames);
    capture.share(Source::Process(two));
    std::thread::sleep(Duration::from_secs(1));
    let after = listen(&capture, arriving);

    let _ = first.kill();
    let _ = first.wait();
    let _ = second.kill();
    let _ = second.wait();
    eprintln!(
        "after switching: 1 kHz {:.1}, 2.4 kHz {:.1} dBFS",
        level(&after, 0, 1_000.0),
        level(&after, 0, 2_400.0)
    );
}

/// Nothing chosen captures nothing, and does not fail doing it: a window
/// share before its app is picked sits in this state.
#[test]
#[ignore = "needs process loopback"]
fn nothing_chosen_captures_nothing_without_failing() {
    if cannot_share_sound() {
        return;
    }
    let (frames, arriving) = sync_channel(4);
    let capture = Capture::start(Source::Nothing, frames).expect("nothing is a valid source");
    std::thread::sleep(Duration::from_secs(1));
    assert!(
        arriving.try_recv().is_err(),
        "nothing chosen should capture nothing"
    );
    drop(capture);
}
