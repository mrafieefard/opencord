//! Lowering other apps against the session's real PipeWire (Phase 2 plan
//! §12): a tone player on a null sink of the test's own, its stream's
//! channel volumes read back with `pw-dump`. Needs a PipeWire session with
//! pipewire-pulse (`pactl`), `pw-play` and `pw-dump`; run with `--ignored`.

#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use opencord_media::attenuation::Attenuator;

/// A null sink, unloaded when dropped: nothing reaches a speaker.
struct NullSink {
    name: String,
    module: String,
}

impl NullSink {
    fn new() -> Self {
        let name = format!("opencord_test_attenuation_{}", std::process::id());
        let output = Command::new("pactl")
            .args([
                "load-module",
                "module-null-sink",
                &format!("sink_name={name}"),
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

/// Ten seconds of silence as a WAV: the stream is what matters.
fn quiet_file(dir: &Path) -> PathBuf {
    let rate = 48_000u32;
    let data = vec![0u8; (rate * 10 * 2) as usize];
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
    let path = dir.join("quiet.wav");
    std::fs::write(&path, file).unwrap();
    path
}

/// Another app playing: through `setsid --fork`, so not a child of this
/// process (a child counts as Opencord's own and is left alone).
struct Stranger {
    pid: u32,
}

impl Stranger {
    fn play(sink: &NullSink, app: &str, file: &Path) -> Self {
        let status = Command::new("setsid")
            .args(["--fork", "pw-play", "--target", &sink.name, "-P"])
            .arg(format!("{{ application.name = \"{app}\" }}"))
            .arg(file)
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

/// The channel volumes of `app`'s playback stream, as PipeWire has them.
fn volumes_of(app: &str) -> Option<Vec<f64>> {
    let output = Command::new("pw-dump").output().ok()?;
    let objects: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    objects.as_array()?.iter().find_map(|object| {
        let info = object.get("info")?;
        let props = info.get("props")?;
        if props.get("application.name")?.as_str()? != app
            || props.get("media.class")?.as_str()? != "Stream/Output/Audio"
        {
            return None;
        }
        let volumes = info
            .get("params")?
            .get("Props")?
            .get(0)?
            .get("channelVolumes")?;
        volumes
            .as_array()?
            .iter()
            .map(serde_json::Value::as_f64)
            .collect()
    })
}

/// Waits up to 3 s for `app`'s volumes to be `expected`.
fn settles_at(app: &str, expected: f64) -> Vec<f64> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let volumes = volumes_of(app).unwrap_or_default();
        let there = !volumes.is_empty() && volumes.iter().all(|v| (v - expected).abs() < 0.01);
        if there || Instant::now() > deadline {
            return volumes;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
#[ignore = "needs a PipeWire session, pactl, pw-play and pw-dump"]
fn other_apps_are_lowered_while_asked_and_put_back_as_they_were() {
    let sink = NullSink::new();
    let dir = tempfile::tempdir().unwrap();
    let file = quiet_file(dir.path());
    let app = format!("OpencordAttenuationTest{}", std::process::id());
    let _player = Stranger::play(&sink, &app, &file);
    let before = settles_at(&app, 1.0);
    assert!(!before.is_empty(), "the player's stream never showed");
    let before = before[0];

    let attenuator = Attenuator::start().expect("PipeWire is there");
    attenuator.set(Some(0.3));
    let lowered = settles_at(&app, before * 0.3);
    println!("before {before}, lowered {lowered:?}");
    assert!(
        lowered.iter().all(|v| (v - before * 0.3).abs() < 0.01),
        "{lowered:?}"
    );

    attenuator.set(None);
    let back = settles_at(&app, before);
    assert!(back.iter().all(|v| (v - before).abs() < 0.01), "{back:?}");

    // Dropped while lowered: it still puts them back.
    attenuator.set(Some(0.5));
    settles_at(&app, before * 0.5);
    drop(attenuator);
    let after = settles_at(&app, before);
    assert!(after.iter().all(|v| (v - before).abs() < 0.01), "{after:?}");
}
