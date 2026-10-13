//! Screen share sound against the session's real PipeWire (Phase 2 plan
//! §10.4): tone players on a null sink of the test's own, some started
//! outside this process's tree (other apps), some as its children (as
//! Opencord's own sound would be). Each tone is measured in what the
//! capture hands over. Needs a PipeWire session with pipewire-pulse
//! (`pactl`) and `pw-play`; run with `--ignored`.

#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use opencord_media::share_audio::pipewire::{
    Capture, Captured, SAMPLE_RATE, Source, apps_with_audio,
};

/// A null sink, unloaded when dropped: nothing reaches a speaker.
struct NullSink {
    name: String,
    module: String,
}

impl NullSink {
    fn new(tag: &str) -> Self {
        let name = format!("opencord_test_{tag}_{}", std::process::id());
        let output = Command::new("pactl")
            .args([
                "load-module",
                "module-null-sink",
                &format!("sink_name={name}"),
                "channels=2",
                "rate=48000",
            ])
            .output()
            .expect("pactl runs");
        assert!(output.status.success(), "{output:?}");
        let module = String::from_utf8(output.stdout).unwrap().trim().to_owned();
        Self { name, module }
    }
}

impl Drop for NullSink {
    fn drop(&mut self) {
        let _ = Command::new("pactl")
            .args(["unload-module", &self.module])
            .status();
    }
}

/// A WAV of a tone in each channel (`None` silent), 30 s long.
fn tone_file(dir: &Path, name: &str, left: f32, right: Option<f32>) -> PathBuf {
    let channels: u16 = if right.is_some() { 2 } else { 1 };
    let rate = 48_000u32;
    let mut data = Vec::new();
    for n in 0..rate * 30 {
        let t = n as f32 / rate as f32;
        for frequency in std::iter::once(left).chain(right) {
            let value = (0.25 * (std::f32::consts::TAU * frequency * t).sin() * 32_767.0) as i16;
            data.extend_from_slice(&value.to_le_bytes());
        }
    }
    let mut file = b"RIFF".to_vec();
    file.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    file.extend_from_slice(b"WAVEfmt ");
    file.extend_from_slice(&16u32.to_le_bytes());
    file.extend_from_slice(&1u16.to_le_bytes());
    file.extend_from_slice(&channels.to_le_bytes());
    file.extend_from_slice(&rate.to_le_bytes());
    file.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
    file.extend_from_slice(&(channels * 2).to_le_bytes());
    file.extend_from_slice(&16u16.to_le_bytes());
    file.extend_from_slice(b"data");
    file.extend_from_slice(&(data.len() as u32).to_le_bytes());
    file.extend_from_slice(&data);
    let path = dir.join(format!("{name}.wav"));
    std::fs::write(&path, file).unwrap();
    path
}

fn play_args(sink: &NullSink, app: &str, file: &Path) -> Vec<String> {
    vec![
        "--target".to_owned(),
        sink.name.clone(),
        "-P".to_owned(),
        format!("{{ application.name = \"{app}\" }}"),
        file.to_string_lossy().into_owned(),
    ]
}

/// Another app playing: started through `setsid --fork`, so it is not a
/// descendant of this process.
struct Stranger {
    pid: u32,
}

impl Stranger {
    fn play(sink: &NullSink, app: &str, file: &Path) -> Self {
        let status = Command::new("setsid")
            .arg("--fork")
            .arg("pw-play")
            .args(play_args(sink, app, file))
            .status()
            .expect("setsid runs");
        assert!(status.success());
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let found = Command::new("pgrep")
                .args(["-f", &file.to_string_lossy()])
                .output()
                .unwrap();
            if let Some(pid) = String::from_utf8_lossy(&found.stdout)
                .lines()
                .next()
                .and_then(|line| line.trim().parse().ok())
            {
                return Self { pid };
            }
            assert!(Instant::now() < deadline, "pw-play did not start");
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for Stranger {
    fn drop(&mut self) {
        let _ = Command::new("kill").arg(self.pid.to_string()).status();
    }
}

/// Opencord's own sound: a child of this process.
struct Own(Child);

impl Own {
    fn play(sink: &NullSink, file: &Path) -> Self {
        Self(
            Command::new("pw-play")
                .args(play_args(sink, "OpencordTestOwn", file))
                .spawn()
                .unwrap(),
        )
    }
}

impl Drop for Own {
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

/// The level of `frequency` in one channel of interleaved stereo, in dBFS
/// (a full-scale sine is 0).
fn level(samples: &[f32], channel: usize, frequency: f32) -> f32 {
    let channel: Vec<f32> = samples.iter().skip(channel).step_by(2).copied().collect();
    if channel.is_empty() {
        return -200.0;
    }
    let coefficient = 2.0 * (std::f32::consts::TAU * frequency / SAMPLE_RATE as f32).cos();
    let (mut s1, mut s2) = (0f32, 0f32);
    for sample in &channel {
        (s1, s2) = (sample + coefficient * s1 - s2, s1);
    }
    let power = (s1 * s1 + s2 * s2 - coefficient * s1 * s2).max(0.0);
    let amplitude = 2.0 * power.sqrt() / channel.len() as f32;
    20.0 * amplitude.max(1e-10).log10()
}

const HEARD: f32 = -25.0;
const ABSENT: f32 = -50.0;

#[test]
#[ignore = "needs a PipeWire session, pactl and pw-play"]
fn a_screen_shares_other_apps_but_not_opencords_own_sound() {
    let dir = tempfile::tempdir().unwrap();
    let sink = NullSink::new("screen");
    let _other = Stranger::play(
        &sink,
        "OcOther",
        &tone_file(dir.path(), "other", 1000.0, None),
    );
    let _own = Own::play(&sink, &tone_file(dir.path(), "own", 3170.0, None));
    std::thread::sleep(Duration::from_millis(300));
    let (frames, received) = mpsc::sync_channel(64);

    let capture = Capture::start(Source::Everything, frames).unwrap();
    let samples = record(
        &received,
        Duration::from_millis(700),
        Duration::from_secs(2),
    );

    let (other, own) = (level(&samples, 0, 1000.0), level(&samples, 0, 3170.0));
    println!("other app {other:.1} dBFS, Opencord's own {own:.1} dBFS");
    assert!(other > HEARD, "the other app: {other}");
    assert!(own < ABSENT, "Opencord's own: {own}");
    // A mono stream goes to both sides.
    assert!(level(&samples, 1, 1000.0) > HEARD);
    assert!(capture.linked_apps().contains(&"OcOther".to_owned()));
    assert!(
        !capture
            .linked_apps()
            .contains(&"OpencordTestOwn".to_owned())
    );
}

#[test]
#[ignore = "needs a PipeWire session, pactl and pw-play"]
fn a_window_shares_only_its_apps_sound_and_can_switch() {
    let dir = tempfile::tempdir().unwrap();
    let sink = NullSink::new("window");
    let _first = Stranger::play(
        &sink,
        "OcFirst",
        &tone_file(dir.path(), "first", 1000.0, None),
    );
    let second = Stranger::play(
        &sink,
        "OcSecond",
        &tone_file(dir.path(), "second", 2400.0, None),
    );
    std::thread::sleep(Duration::from_millis(300));
    let (frames, received) = mpsc::sync_channel(64);

    let capture = Capture::start(Source::App("OcFirst".to_owned()), frames).unwrap();
    let by_name = record(
        &received,
        Duration::from_millis(700),
        Duration::from_secs(1),
    );
    capture.share(Source::Process(second.pid));
    let by_process = record(
        &received,
        Duration::from_millis(700),
        Duration::from_secs(1),
    );
    capture.share(Source::Nothing);
    let nothing = record(
        &received,
        Duration::from_millis(500),
        Duration::from_secs(1),
    );

    let levels = |samples: &[f32]| (level(samples, 0, 1000.0), level(samples, 0, 2400.0));
    println!(
        "by name {:?}, by process {:?}, nothing {:?}",
        levels(&by_name),
        levels(&by_process),
        levels(&nothing)
    );
    assert!(levels(&by_name).0 > HEARD && levels(&by_name).1 < ABSENT);
    assert!(levels(&by_process).0 < ABSENT && levels(&by_process).1 > HEARD);
    assert!(levels(&nothing).0 < ABSENT && levels(&nothing).1 < ABSENT);
}

#[test]
#[ignore = "needs a PipeWire session, pactl and pw-play"]
fn an_app_that_starts_playing_while_sharing_is_linked_and_stereo_stays_apart() {
    let dir = tempfile::tempdir().unwrap();
    let sink = NullSink::new("later");
    let (frames, received) = mpsc::sync_channel(64);
    let capture = Capture::start(Source::App("OcLater".to_owned()), frames).unwrap();
    let before = record(&received, Duration::ZERO, Duration::from_millis(500));

    let _later = Stranger::play(
        &sink,
        "OcLater",
        &tone_file(dir.path(), "later", 1000.0, Some(2400.0)),
    );
    let after = record(
        &received,
        Duration::from_millis(700),
        Duration::from_secs(1),
    );

    println!(
        "before {:.1}; left {:.1}/{:.1}, right {:.1}/{:.1}",
        level(&before, 0, 1000.0),
        level(&after, 0, 1000.0),
        level(&after, 0, 2400.0),
        level(&after, 1, 1000.0),
        level(&after, 1, 2400.0),
    );
    assert!(level(&before, 0, 1000.0) < ABSENT);
    assert!(level(&after, 0, 1000.0) > HEARD && level(&after, 0, 2400.0) < ABSENT);
    assert!(level(&after, 1, 2400.0) > HEARD && level(&after, 1, 1000.0) < ABSENT);
    assert_eq!(capture.linked_apps(), ["OcLater"]);
}

#[test]
#[ignore = "needs a PipeWire session, pactl and pw-play"]
fn apps_playing_are_listed_but_not_opencords_own() {
    let dir = tempfile::tempdir().unwrap();
    let sink = NullSink::new("list");
    let _other = Stranger::play(
        &sink,
        "OcListed",
        &tone_file(dir.path(), "listed", 1000.0, None),
    );
    let _own = Own::play(&sink, &tone_file(dir.path(), "unlisted", 2400.0, None));
    std::thread::sleep(Duration::from_millis(500));

    let apps = apps_with_audio().unwrap();

    let names: Vec<&str> = apps.iter().map(|app| app.name.as_str()).collect();
    println!("{names:?}");
    assert!(names.contains(&"OcListed"));
    assert!(!names.contains(&"OpencordTestOwn"));
}
