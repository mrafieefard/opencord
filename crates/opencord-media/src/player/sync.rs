use std::time::{Duration, Instant};

#[derive(Debug, PartialEq, Eq)]
pub enum FrameAction {
    Present,
    Wait(Duration),
    Drop,
}

/// Clock for media playback.
///
/// If audio is present: audio_clock_ms = samples_written - queued_frames at 48 kHz.
/// Without audio: a monotonic clock from play start.
pub struct MediaClock {
    has_audio: bool,
    frames_written: usize,
    start_instant: Option<Instant>,
    elapsed_before_pause: u32,
    is_playing: bool,
}

impl MediaClock {
    pub fn new(has_audio: bool) -> Self {
        Self {
            has_audio,
            frames_written: 0,
            start_instant: None,
            elapsed_before_pause: 0,
            is_playing: false,
        }
    }

    pub fn play(&mut self) {
        if !self.is_playing {
            self.is_playing = true;
            self.start_instant = Some(Instant::now());
        }
    }

    pub fn pause(&mut self) {
        if self.is_playing {
            self.is_playing = false;
            if let Some(start) = self.start_instant.take() {
                self.elapsed_before_pause += start.elapsed().as_millis() as u32;
            }
        }
    }

    pub fn seek(&mut self, target_ms: u32) {
        self.elapsed_before_pause = target_ms;
        self.start_instant = if self.is_playing {
            Some(Instant::now())
        } else {
            None
        };
        self.frames_written = (target_ms as f64 / 1000.0 * 48000.0) as usize;
    }

    pub fn on_audio_written(&mut self, stereo_samples: usize) {
        self.frames_written += stereo_samples / 2;
    }

    pub fn current_time_ms(&self, queued_audio_frames: usize) -> u32 {
        if self.has_audio {
            let clock_frames = self.frames_written.saturating_sub(queued_audio_frames);
            (clock_frames as f64 / 48000.0 * 1000.0) as u32
        } else {
            let mut elapsed = self.elapsed_before_pause;
            if self.is_playing
                && let Some(start) = self.start_instant
            {
                elapsed += start.elapsed().as_millis() as u32;
            }
            elapsed
        }
    }

    /// Present a video frame when `pts <= clock + 10 ms`; drop frames later than `clock - 80 ms`.
    pub fn check_video_frame(&self, pts_ms: u32, queued_audio_frames: usize) -> FrameAction {
        let clock = self.current_time_ms(queued_audio_frames);
        if pts_ms > clock + 10 {
            FrameAction::Wait(Duration::from_millis((pts_ms - (clock + 10)) as u64))
        } else if pts_ms + 80 < clock {
            FrameAction::Drop
        } else {
            FrameAction::Present
        }
    }
}
