//! The player's clock: the sound the listener has heard, or the wall clock
//! when there is no sound to follow, in the media's own time at its speed.

use std::time::{Duration, Instant};

#[derive(Debug, PartialEq, Eq)]
pub enum FrameAction {
    Present,
    Wait,
    Drop,
}

/// What the position follows.
enum Source {
    /// `base_ms` plus the 48 kHz frames written since, less those queued.
    Audio { frames_written: usize },
    /// `base_ms` plus the time played since `since`.
    Wall { since: Option<Instant> },
}

pub struct MediaClock {
    source: Source,
    base_ms: u32,
    playing: bool,
    /// Media time per time heard (D87).
    speed: f32,
}

impl MediaClock {
    pub fn new(has_audio: bool) -> Self {
        Self {
            source: Self::start(has_audio, false),
            base_ms: 0,
            playing: false,
            speed: 1.0,
        }
    }

    fn start(audio: bool, playing: bool) -> Source {
        if audio {
            Source::Audio { frames_written: 0 }
        } else {
            Source::Wall {
                since: playing.then(Instant::now),
            }
        }
    }

    pub fn play(&mut self) {
        self.playing = true;
        if let Source::Wall { since } = &mut self.source {
            *since = Some(Instant::now());
        }
    }

    pub fn pause(&mut self) {
        self.playing = false;
        if let Source::Wall { since } = &mut self.source
            && let Some(start) = since.take()
        {
            self.base_ms += self.scaled(millis(start.elapsed()));
        }
    }

    /// From now on, media time runs at `speed`. What is written and not yet
    /// heard (`queued_frames`) counts from here.
    pub fn set_speed(&mut self, speed: f32, queued_frames: usize) {
        self.base_ms = self.now_ms(queued_frames);
        match &mut self.source {
            Source::Audio { frames_written } => *frames_written = queued_frames,
            Source::Wall { since } => *since = self.playing.then(Instant::now),
        }
        self.speed = speed;
    }

    fn scaled(&self, ms: u32) -> u32 {
        (f64::from(ms) * f64::from(self.speed)) as u32
    }

    /// Restarts at `target_ms`, following the sound again when there is some.
    pub fn seek(&mut self, target_ms: u32, has_audio: bool) {
        self.base_ms = target_ms;
        self.source = Self::start(has_audio, self.playing);
    }

    pub fn on_audio_written(&mut self, frames: usize) {
        if let Source::Audio { frames_written } = &mut self.source {
            *frames_written += frames;
        }
    }

    /// The sound ran out before the pictures: time goes on by the wall
    /// clock from what was last heard.
    pub fn audio_ended(&mut self) {
        if matches!(self.source, Source::Audio { .. }) {
            self.base_ms = self.now_ms(0);
            self.source = Self::start(false, self.playing);
        }
    }

    pub fn now_ms(&self, queued_frames: usize) -> u32 {
        match &self.source {
            Source::Audio { frames_written } => {
                let heard = frames_written.saturating_sub(queued_frames);
                self.base_ms + self.scaled((heard as u64 * 1000 / 48_000) as u32)
            }
            Source::Wall { since } => {
                self.base_ms + self.scaled(since.map_or(0, |s| millis(s.elapsed())))
            }
        }
    }

    /// Present a frame once `pts ≤ clock + 10 ms`; drop it when it is more
    /// than 80 ms late.
    pub fn check_video_frame(&self, pts_ms: u32, queued_frames: usize) -> FrameAction {
        let clock = self.now_ms(queued_frames);
        if pts_ms > clock + 10 {
            FrameAction::Wait
        } else if pts_ms + 80 < clock {
            FrameAction::Drop
        } else {
            FrameAction::Present
        }
    }
}

fn millis(duration: Duration) -> u32 {
    duration.as_millis().min(u128::from(u32::MAX)) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_audio_clock_counts_what_was_heard_from_the_seek() {
        let mut clock = MediaClock::new(true);
        clock.seek(1_000, true);
        clock.on_audio_written(4_800);
        assert_eq!(clock.now_ms(2_400), 1_050);
    }

    #[test]
    fn when_the_sound_ends_time_goes_on_from_its_last_moment() {
        let mut clock = MediaClock::new(true);
        clock.play();
        clock.on_audio_written(48_000);
        clock.audio_ended();
        let now = clock.now_ms(0);
        assert!((1_000..1_050).contains(&now), "{now}");
    }

    #[test]
    fn at_twice_the_speed_heard_sound_counts_double() {
        let mut clock = MediaClock::new(true);
        clock.set_speed(2.0, 0);
        clock.seek(1_000, true);
        clock.on_audio_written(4_800);
        assert_eq!(clock.now_ms(2_400), 1_100);
    }

    #[test]
    fn the_wall_clock_runs_at_the_speed() {
        let mut clock = MediaClock::new(false);
        clock.set_speed(2.0, 0);
        // A busy machine sleeps for longer than it was asked to, so the limits
        // come from how long this really took: the clock ran for at least the
        // sleep and at most the whole test.
        let test = Instant::now();
        clock.play();
        let sleep = Instant::now();
        std::thread::sleep(Duration::from_millis(100));
        let slept = millis(sleep.elapsed());
        let now = clock.now_ms(0);
        let total = millis(test.elapsed());
        assert!(
            (2 * slept..=2 * total).contains(&now),
            "{now}, slept {slept}, total {total}"
        );
    }

    #[test]
    fn a_speed_change_keeps_the_position() {
        let mut clock = MediaClock::new(true);
        clock.play();
        clock.on_audio_written(48_000);
        assert_eq!(clock.now_ms(0), 1_000);
        clock.set_speed(2.0, 0);
        assert_eq!(clock.now_ms(0), 1_000);
        clock.on_audio_written(4_800);
        assert_eq!(clock.now_ms(0), 1_200);
    }

    #[test]
    fn sound_queued_at_a_change_is_still_to_be_heard() {
        let mut clock = MediaClock::new(true);
        clock.play();
        clock.on_audio_written(48_000);
        // 2,400 frames written but not heard yet.
        clock.set_speed(2.0, 2_400);
        assert_eq!(clock.now_ms(2_400), 950);
    }
}
