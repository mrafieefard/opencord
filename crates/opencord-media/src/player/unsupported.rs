//! Systems without a media player yet (macOS until P2): opening fails, so
//! no player ever exists.

use std::convert::Infallible;
use std::path::Path;
use std::sync::Arc;

use super::{AudioOut, MediaSource, PlayerError, PlayerEvent, Probe, VideoOut};

pub fn supported() -> bool {
    false
}

pub struct MediaPlayer(Infallible);

impl MediaPlayer {
    pub fn open(
        _source: Box<dyn MediaSource>,
        _video: Option<Arc<dyn VideoOut>>,
        _audio: Box<dyn AudioOut>,
        _events: impl FnMut(PlayerEvent) + Send + 'static,
    ) -> Result<Self, PlayerError> {
        Err(PlayerError::Unsupported)
    }

    pub fn play(&self) {
        match self.0 {}
    }

    pub fn pause(&self) {
        match self.0 {}
    }

    pub fn seek(&self, _position_ms: u32) {
        match self.0 {}
    }

    pub fn set_volume(&self, _volume: f32, _muted: bool) {
        match self.0 {}
    }

    pub fn close(self) {
        match self.0 {}
    }
}

pub fn probe_and_poster(
    _path: &Path,
    _max_side: u32,
) -> Result<(Probe, Option<Vec<u8>>), PlayerError> {
    Err(PlayerError::Unsupported)
}
