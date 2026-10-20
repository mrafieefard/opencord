use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
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

/// An AudioOut that plays at real time (48 kHz) from a queue of fixed
/// capacity. Like a device, it plays silence when the queue runs dry, and
/// the time that passes then gives no room later.
struct RealTimeAudioOut {
    /// Frames queued as of `checked`.
    queue: std::cell::Cell<(usize, Instant)>,
    capacity_frames: usize,
    /// Every frame ever accepted, for the test to read.
    total: Arc<AtomicUsize>,
}

impl RealTimeAudioOut {
    fn new(capacity_frames: usize) -> Self {
        Self {
            queue: std::cell::Cell::new((0, Instant::now())),
            capacity_frames,
            total: Arc::default(),
        }
    }
}

impl AudioOut for RealTimeAudioOut {
    fn write(&mut self, stereo_48k: &[f32]) -> usize {
        let queued = self.queued_frames();
        let frames = (stereo_48k.len() / 2).min(self.capacity_frames - queued);
        self.queue.set((queued + frames, self.queue.get().1));
        self.total.fetch_add(frames, Ordering::Relaxed);
        frames * 2
    }

    fn queued_frames(&self) -> usize {
        let (queued, checked) = self.queue.get();
        let played = (checked.elapsed().as_secs_f64() * 48_000.0) as usize;
        let now = if played > 0 { Instant::now() } else { checked };
        let queued = queued.saturating_sub(played);
        self.queue.set((queued, now));
        queued
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

    // Play after the end starts again from the beginning.
    player.play();
    let restarted = loop {
        match event_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("event")
        {
            PlayerEvent::Position { position_ms, .. } => break position_ms,
            PlayerEvent::Failed(reason) => panic!("playback failed: {reason}"),
            _ => {}
        }
    };
    assert!(restarted < 500, "replay started at {restarted} ms");
}

/// Pausing holds the place; playing again goes on from it to the end.
#[test]
fn a_pause_holds_the_place_and_play_goes_on() {
    if !supported() {
        return;
    }
    let file = std::fs::File::open(fixture_path("h264_aac.mp4")).expect("open fixture");
    let (event_tx, event_rx) = std::sync::mpsc::channel();
    let audio = RealTimeAudioOut::new(2880);
    let written = audio.total.clone();
    let player = MediaPlayer::open(Box::new(file), None, Box::new(audio), move |event| {
        let _ = event_tx.send(event);
    })
    .expect("media player open");

    let start = Instant::now();
    player.play();
    std::thread::sleep(Duration::from_millis(700));
    player.pause();
    std::thread::sleep(Duration::from_millis(100));
    let at_pause = written.load(Ordering::Relaxed);
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(
        written.load(Ordering::Relaxed),
        at_pause,
        "sound went on while paused"
    );
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
    // 2 s of media and 0.6 s paused, nothing replayed or skipped.
    let elapsed = start.elapsed().as_secs_f64();
    assert!(
        (2.3..=2.95).contains(&elapsed),
        "playback took {elapsed:.2}s, expected ~2.6s"
    );
    let heard_ms = written.load(Ordering::Relaxed) / 48;
    assert!(
        (1_900..=2_100).contains(&heard_ms),
        "{heard_ms} ms of sound for 2 s of media"
    );
}

#[test]
fn seeks_accurately() {
    if !supported() {
        return;
    }
    let file = std::fs::File::open(fixture_path("h264_aac.mp4")).expect("open fixture");
    let (event_tx, event_rx) = std::sync::mpsc::channel();
    let audio = RealTimeAudioOut::new(2880);
    let total = audio.total.clone();

    let player = MediaPlayer::open(Box::new(file), None, Box::new(audio), move |event| {
        let _ = event_tx.send(event);
    })
    .expect("media player open");

    // The fixture's only keyframe is at 0, so the demuxer lands a second
    // early: what decodes before 1000 ms must be skipped, sound included.
    player.seek(1000);
    player.play();
    let start = Instant::now();
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
    let heard = total.load(Ordering::Relaxed) as f64 / 48_000.0;
    let took = start.elapsed().as_secs_f64();
    assert!(
        (0.85..=1.15).contains(&heard),
        "{heard:.2} s of sound after the seek"
    );
    assert!(
        (0.85..=1.15).contains(&took),
        "the last second took {took:.2} s"
    );
}

#[test]
fn a_webm_with_opus_opens() {
    if !supported() {
        return;
    }
    let file = std::fs::File::open(fixture_path("vp9_opus.webm")).expect("open fixture");
    let (event_tx, event_rx) = std::sync::mpsc::channel();
    let _player = MediaPlayer::open(
        Box::new(file),
        None,
        Box::new(RealTimeAudioOut::new(2880)),
        move |event| {
            let _ = event_tx.send(event);
        },
    )
    .expect("media player open");
    match event_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("event")
    {
        PlayerEvent::Opened {
            width,
            height,
            has_video,
            has_audio,
            ..
        } => assert_eq!(
            (width, height, has_video, has_audio),
            (320, 240, true, true)
        ),
        other => panic!("expected Opened, got {other:?}"),
    }
}

/// The fixture's bytes, failing every read past `limit`.
struct Breaks {
    file: std::fs::File,
    limit: u64,
}

impl Read for Breaks {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let left = self.limit.saturating_sub(self.file.stream_position()?);
        if left == 0 {
            return Err(std::io::Error::other("the network went away"));
        }
        let len = buf.len().min(left as usize);
        self.file.read(&mut buf[..len])
    }
}

impl Seek for Breaks {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.file.seek(pos)
    }
}

#[test]
fn a_read_that_fails_is_a_failure_not_the_end() {
    if !supported() {
        return;
    }
    let file = std::fs::File::open(fixture_path("h264_aac.mp4")).expect("open fixture");
    let source = Breaks {
        file,
        limit: 12_000,
    };
    let (event_tx, event_rx) = std::sync::mpsc::channel();
    let player = MediaPlayer::open(
        Box::new(source),
        None,
        Box::new(RealTimeAudioOut::new(2880)),
        move |event| {
            let _ = event_tx.send(event);
        },
    )
    .expect("media player open");
    player.play();
    loop {
        match event_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("event")
        {
            PlayerEvent::Failed(_) => break,
            PlayerEvent::Ended => panic!("a broken read ended playback as if it were done"),
            _ => {}
        }
    }
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

/// What Task 32 builds FFmpeg with: CI runs this against that build.
#[cfg(any(target_os = "linux", target_os = "windows"))]
#[test]
fn the_planned_decoders_are_there() {
    if !supported() {
        return;
    }
    use ffmpeg_next::codec::Id;
    ffmpeg_next::init().unwrap();
    let missing: Vec<String> = [
        Id::H264,
        Id::HEVC,
        Id::VP8,
        Id::VP9,
        Id::MPEG4,
        Id::MJPEG,
        Id::AAC,
        Id::MP3,
        Id::OPUS,
        Id::VORBIS,
        Id::FLAC,
        Id::PCM_S16LE,
        Id::PCM_S24LE,
        Id::PCM_S32LE,
        Id::PCM_F32LE,
        Id::PCM_U8,
    ]
    .into_iter()
    .filter(|id| ffmpeg_next::decoder::find(*id).is_none())
    .map(|id| format!("{id:?}"))
    .chain(
        ffmpeg_next::decoder::find_by_name("libdav1d")
            .is_none()
            .then(|| "libdav1d".to_owned()),
    )
    .collect();
    assert!(missing.is_empty(), "no decoder for {missing:?}");
}
