use std::path::Path;
use std::sync::Arc;

use super::{AudioOut, MediaPlayer, MediaSource, PlayerError, PlayerEvent, Probe, VideoOut};

pub fn supported() -> bool {
    false
}

impl MediaPlayer {
    pub fn open(
        _source: Box<dyn MediaSource>,
        _video: Option<Arc<dyn VideoOut>>,
        _audio: Box<dyn AudioOut>,
        _events: impl FnMut(PlayerEvent) + Send + 'static,
    ) -> Result<Self, PlayerError> {
        Err(PlayerError::Unsupported)
    }

    pub fn play(&self) {}
    pub fn pause(&self) {}
    pub fn seek(&self, _position_ms: u32) {}
    pub fn set_volume(&self, _volume: f32, _muted: bool) {}
    pub fn close(self) {}
}

pub fn probe_and_poster(
    _path: &Path,
    _max_side: u32,
) -> Result<(Probe, Option<Vec<u8>>), PlayerError> {
    Err(PlayerError::Unsupported)
}
