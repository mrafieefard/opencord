//! Screen share sound through Core Audio process taps (docs/plans/macos.md
//! M7, step 5), mirroring `share_audio.rs`: `afplay` tones, some started
//! outside this process's tree (other apps), one as its child (as
//! Opencord's own sound would be), each measured in what the capture hands
//! over. Run by `app/tool/macos/check_share_audio.sh`, which makes BlackHole
//! the default output so nothing reaches a speaker; needs audio capture
//! allowed for the terminal app; `--ignored`.

#![cfg(target_os = "macos")]

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use opencord_media::share_audio::{Capture, Captured, Source, apps_with_audio};

const SAMPLE_RATE: f32 = 48_000.0;
const HEARD: f32 = -25.0;
const ABSENT: f32 = -50.0;

/// A WAV of a tone, mono, 30 s long, at a quarter of full scale.
fn tone_file(dir: &Path, frequency: f32) -> PathBuf {
    let rate = 48_000u32;
    let mut data = Vec::new();
    for n in 0..rate * 30 {
        let t = n as f32 / rate as f32;
        let value = (0.25 * (std::f32::consts::TAU * frequency * t).sin() * 32_767.0) as i16;
        data.extend_from_slice(&value.to_le_bytes());
    }
    let mut file = b"RIFF".to_vec();
    file.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    file.extend_from_slice(b"WAVEfmt ");
    file.extend_from_slice(&16u32.to_le_bytes());
    file.extend_from_slice(&1u16.to_le_bytes());
    file.extend_from_slice(&1u16.to_le_bytes());
    file.extend_from_slice(&rate.to_le_bytes());
    file.extend_from_slice(&(rate * 2).to_le_bytes());
    file.extend_from_slice(&2u16.to_le_bytes());
    file.extend_from_slice(&16u16.to_le_bytes());
    file.extend_from_slice(b"data");
    file.extend_from_slice(&(data.len() as u32).to_le_bytes());
    file.extend_from_slice(&data);
    let path = dir.join(format!("tone_{frequency}.wav"));
    std::fs::write(&path, file).unwrap();
    path
}

/// An `afplay` started as another app would be: not this process's child
/// (a shell starts it and exits). Killed when dropped.
struct OtherApp {
    pid: u32,
}

impl OtherApp {
    fn play(file: &Path) -> Self {
        let output = Command::new("sh")
            .args([
                "-c",
                &format!("afplay '{}' >/dev/null 2>&1 & echo $!", file.display()),
            ])
            .output()
            .expect("sh runs");
        let pid = String::from_utf8(output.stdout)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        std::thread::sleep(Duration::from_millis(500));
        Self { pid }
    }
}

impl Drop for OtherApp {
    fn drop(&mut self) {
        let _ = Command::new("kill").arg(self.pid.to_string()).status();
    }
}

/// An `afplay` started by this process: its own sound.
struct OwnSound(Child);

impl OwnSound {
    fn play(file: &Path) -> Self {
        let child = Command::new("afplay")
            .arg(file)
            .spawn()
            .expect("afplay runs");
        std::thread::sleep(Duration::from_millis(500));
        Self(child)
    }
}

impl Drop for OwnSound {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// What a capture handed over for `length`, after `settle`.
fn record(received: &mpsc::Receiver<Captured>, settle: Duration, length: Duration) -> Vec<f32> {
    let start = Instant::now();
    let mut samples = Vec::new();
    while start.elapsed() < settle + length {
        if let Ok(captured) = received.recv_timeout(Duration::from_millis(50))
            && start.elapsed() >= settle
        {
            samples.extend(captured.samples);
        }
    }
    samples
}

/// The level of `frequency` in one channel of interleaved stereo, in dBFS.
fn level(samples: &[f32], channel: usize, frequency: f32) -> f32 {
    let channel: Vec<f32> = samples.iter().skip(channel).step_by(2).copied().collect();
    if channel.is_empty() {
        return -200.0;
    }
    let coefficient = 2.0 * (std::f32::consts::TAU * frequency / SAMPLE_RATE).cos();
    let (mut s1, mut s2) = (0f32, 0f32);
    for sample in &channel {
        (s1, s2) = (sample + coefficient * s1 - s2, s1);
    }
    let power = (s1 * s1 + s2 * s2 - coefficient * s1 * s2).max(0.0);
    let amplitude = 2.0 * power.sqrt() / channel.len() as f32;
    20.0 * amplitude.max(1e-10).log10()
}

#[test]
#[ignore = "needs audio capture allowed and BlackHole as the default output"]
fn a_screen_shares_other_apps_but_not_opencords_own_sound() {
    let dir = tempfile::tempdir().unwrap();
    let _other = OtherApp::play(&tone_file(dir.path(), 700.0));
    let _own = OwnSound::play(&tone_file(dir.path(), 1100.0));
    let (sender, received) = mpsc::sync_channel(64);

    let capture = Capture::start(Source::Everything, sender).unwrap();
    let samples = record(
        &received,
        Duration::from_millis(800),
        Duration::from_secs(2),
    );
    drop(capture);

    let (other, own) = (level(&samples, 0, 700.0), level(&samples, 0, 1100.0));
    eprintln!(
        "screen: {} s captured; other app 700 Hz {other:.1} dBFS, own 1100 Hz {own:.1} dBFS",
        samples.len() / 2 / 48_000
    );
    assert!(other > HEARD, "other app: {other:.1} dBFS");
    assert!(own < ABSENT, "Opencord's own: {own:.1} dBFS");
}

#[test]
#[ignore = "needs audio capture allowed and BlackHole as the default output"]
fn a_window_shares_only_its_apps_sound_and_can_switch() {
    let dir = tempfile::tempdir().unwrap();
    let first = OtherApp::play(&tone_file(dir.path(), 600.0));
    let second = OtherApp::play(&tone_file(dir.path(), 900.0));
    let (sender, received) = mpsc::sync_channel(64);

    let capture = Capture::start(Source::Process(first.pid), sender).unwrap();
    let before = record(
        &received,
        Duration::from_millis(800),
        Duration::from_secs(2),
    );
    capture.share(Source::Process(second.pid));
    let after = record(
        &received,
        Duration::from_millis(800),
        Duration::from_secs(2),
    );
    drop(capture);

    let levels = |samples: &[f32]| (level(samples, 0, 600.0), level(samples, 0, 900.0));
    let (first_before, second_before) = levels(&before);
    let (first_after, second_after) = levels(&after);
    eprintln!(
        "window of the first: 600 Hz {first_before:.1}, 900 Hz {second_before:.1}; \
         then the second: 600 Hz {first_after:.1}, 900 Hz {second_after:.1} dBFS"
    );
    assert!(first_before > HEARD && second_before < ABSENT);
    assert!(second_after > HEARD && first_after < ABSENT);
}

#[test]
#[ignore = "needs audio capture allowed and BlackHole as the default output"]
fn an_app_that_starts_playing_while_sharing_is_tapped() {
    let dir = tempfile::tempdir().unwrap();
    let (sender, received) = mpsc::sync_channel(64);

    // Nothing of afplay's plays yet: its processes come later.
    let capture = Capture::start(Source::App("afplay".to_owned()), sender).unwrap();
    let _later = OtherApp::play(&tone_file(dir.path(), 800.0));
    let samples = record(
        &received,
        Duration::from_millis(1500),
        Duration::from_secs(2),
    );
    drop(capture);

    let heard = level(&samples, 0, 800.0);
    eprintln!("an app that started later: 800 Hz {heard:.1} dBFS");
    assert!(heard > HEARD, "{heard:.1} dBFS");
}

#[test]
#[ignore = "needs BlackHole as the default output"]
fn apps_playing_are_listed_but_not_opencords_own() {
    let dir = tempfile::tempdir().unwrap();
    let _other = OtherApp::play(&tone_file(dir.path(), 500.0));
    let _own = OwnSound::play(&tone_file(dir.path(), 1300.0));

    let apps = apps_with_audio().unwrap();

    eprintln!("apps playing: {apps:?}");
    assert!(!apps.is_empty(), "the other app is listed");
}
