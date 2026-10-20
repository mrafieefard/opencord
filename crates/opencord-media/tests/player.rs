use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use opencord_media::player::{
    AudioOut, MediaPlayer, PlayerEvent, VideoOut, probe_and_poster, supported,
};

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

type FrameRecord = (u32, u32, Vec<u8>);

struct TestVideoOut {
    frames: Arc<Mutex<Vec<FrameRecord>>>,
}

impl VideoOut for TestVideoOut {
    fn present(&self, width: u32, height: u32, rgba: Vec<u8>) {
        self.frames.lock().unwrap().push((width, height, rgba));
    }
}

/// An AudioOut sink that consumes frames at real time (48 kHz) with a fixed buffer capacity.
struct RealTimeAudioOut {
    start: Option<Instant>,
    frames_written: usize,
    capacity_frames: usize,
}

impl RealTimeAudioOut {
    fn new(capacity_frames: usize) -> Self {
        Self {
            start: None,
            frames_written: 0,
            capacity_frames,
        }
    }
}

impl AudioOut for RealTimeAudioOut {
    fn write(&mut self, stereo_48k: &[f32]) -> usize {
        if self.start.is_none() {
            self.start = Some(Instant::now());
        }
        let queued = self.queued_frames();
        if queued >= self.capacity_frames {
            return 0;
        }
        let room_frames = self.capacity_frames - queued;
        let frames_in_slice = stereo_48k.len() / 2;
        let to_write_frames = frames_in_slice.min(room_frames);
        self.frames_written += to_write_frames;
        to_write_frames * 2
    }

    fn queued_frames(&self) -> usize {
        let Some(start) = self.start else {
            return 0;
        };
        let elapsed_secs = start.elapsed().as_secs_f64();
        let elapsed_frames = (elapsed_secs * 48000.0) as usize;
        let consumed = elapsed_frames.min(self.frames_written);
        self.frames_written.saturating_sub(consumed)
    }
}

#[test]
fn opens_and_reports_duration_and_size() {
    if !supported() {
        return;
    }
    let file = std::fs::File::open(fixture_path("h264_aac.mp4")).expect("open fixture");
    let (event_tx, event_rx) = std::sync::mpsc::channel();
    let audio = Box::new(RealTimeAudioOut::new(4800));

    let _player = MediaPlayer::open(Box::new(file), None, audio, move |event| {
        let _ = event_tx.send(event);
    })
    .expect("media player open");

    let opened = event_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("opened event");
    match opened {
        PlayerEvent::Opened {
            duration_ms,
            width,
            height,
            has_video,
            has_audio,
        } => {
            assert!(
                (1900..=2100).contains(&duration_ms),
                "duration {duration_ms} ms"
            );
            assert_eq!(width, 320);
            assert_eq!(height, 240);
            assert!(has_video);
            assert!(has_audio);
        }
        other => panic!("expected Opened event, got {other:?}"),
    }
}

#[test]
fn plays_to_the_end_in_real_time() {
    if !supported() {
        return;
    }
    let file = std::fs::File::open(fixture_path("h264_aac.mp4")).expect("open fixture");
    let (event_tx, event_rx) = std::sync::mpsc::channel();
    // 60 ms pacing capacity = 2880 frames
    let audio = Box::new(RealTimeAudioOut::new(2880));

    let player = MediaPlayer::open(Box::new(file), None, audio, move |event| {
        let _ = event_tx.send(event);
    })
    .expect("media player open");

    let start = Instant::now();
    player.play();

    loop {
        match event_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("event")
        {
            PlayerEvent::Ended => break,
            PlayerEvent::Failed(reason) => panic!("playback failed: {reason}"),
            _ => {}
        }
    }

    let elapsed = start.elapsed();
    let elapsed_secs = elapsed.as_secs_f64();
    // 2.0s ± 15% -> 1.7s to 2.3s
    assert!(
        (1.7..=2.3).contains(&elapsed_secs),
        "playback took {elapsed_secs:.2}s, expected ~2.0s"
    );
}

#[test]
fn seeks_accurately() {
    if !supported() {
        return;
    }
    let file = std::fs::File::open(fixture_path("h264_aac.mp4")).expect("open fixture");
    let (event_tx, event_rx) = std::sync::mpsc::channel();
    let audio = Box::new(RealTimeAudioOut::new(4800));

    let player = MediaPlayer::open(Box::new(file), None, audio, move |event| {
        let _ = event_tx.send(event);
    })
    .expect("media player open");

    player.play();
    player.seek(1000);

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut found_position = false;
    while Instant::now() < deadline {
        if let Ok(PlayerEvent::Position { position_ms, .. }) =
            event_rx.recv_timeout(Duration::from_millis(500))
            && (950..=1050).contains(&position_ms)
        {
            found_position = true;
            break;
        }
    }

    assert!(found_position, "seek did not land within 50 ms of 1000 ms");
}

#[test]
fn rotation_is_applied() {
    if !supported() {
        return;
    }
    let file = std::fs::File::open(fixture_path("rotated_90.mp4")).expect("open fixture");
    let frames = Arc::new(Mutex::new(Vec::new()));
    let video = Arc::new(TestVideoOut {
        frames: frames.clone(),
    });
    let audio = Box::new(RealTimeAudioOut::new(4800));
    let (event_tx, _event_rx) = std::sync::mpsc::channel();

    let player = MediaPlayer::open(Box::new(file), Some(video), audio, move |event| {
        let _ = event_tx.send(event);
    })
    .expect("media player open");

    player.play();

    // Wait until at least one frame is presented
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if !frames.lock().unwrap().is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    let guard = frames.lock().unwrap();
    assert!(!guard.is_empty(), "no frames presented");
    let (w, h, rgba) = &guard[0];
    assert_eq!(*w, 240, "portrait width");
    assert_eq!(*h, 320, "portrait height");
    assert_eq!(rgba.len(), 240 * 320 * 4);
}

#[test]
fn probe_and_poster_makes_a_jpeg() {
    if !supported() {
        return;
    }
    let path = fixture_path("h264_aac.mp4");
    let (probe, poster) = probe_and_poster(&path, 800).expect("probe and poster");

    assert!(probe.has_video);
    assert_eq!(probe.width, 320);
    assert_eq!(probe.height, 240);
    assert!((1900..=2100).contains(&probe.duration_ms));

    let poster = poster.expect("poster exists for video");
    assert!(
        poster.starts_with(&[0xFF, 0xD8, 0xFF]),
        "must be JPEG magic bytes"
    );
    assert!(poster.len() <= 2 * 1024 * 1024, "poster <= 2 MiB");
}
