//! This device's screen share (Phase 2 plan §9): a source picked through
//! the ScreenCast portal (or, for checks with nobody to pick, a PipeWire
//! node named in `OPENCORD_SCREEN_NODE`), captured, published as a screen
//! track and sent in its layers. The streamer's own tile shows a small,
//! slow preview (plan §9.4). Going live on the main server is the
//! client's part; this is the media.

use std::os::fd::OwnedFd;
use std::sync::mpsc::{Receiver, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use opencord_common::video::ScreenPreset;
use opencord_media::share_audio::pipewire::{Capture, Source};
use opencord_media::share_audio::{self, sender::Encoded};
use opencord_media::transport::{EncodedScreenAudio, TrackError, TrackKind, TrackRequest};
use opencord_media::video::camera::RawFrame;
use opencord_media::video::camera::pipewire::Remote;
use opencord_media::video::layers::{ScreenShape, screen_layers};
use opencord_media::video::screen::capture::{ScreenCapture, Target};
use opencord_media::video::screen::portal::{self, Picked};
use opencord_media::video::screen::{ScreenError, SourceKind};
use opencord_media::video::sender::VideoSender;

use super::{Video, lock};
use crate::api::types::{
    CoreError, ScreenProblem, ScreenShareRequest, ScreenShareResolution, ScreenShareStarted,
    StreamSourceKind,
};

/// Names a PipeWire node to share in place of the portal's picker.
const NODE_VARIABLE: &str = "OPENCORD_SCREEN_NODE";
/// The streamer's own preview is drawn this often at most.
const PREVIEW_EVERY: Duration = Duration::from_millis(200);
/// And this large until its tile says.
const PREVIEW: (u32, u32) = (480, 270);

pub(super) struct Share {
    pub server_key: String,
    pub stream_key: String,
    pub request: TrackRequest,
    pub sender: VideoSender,
    pub kind: StreamSourceKind,
    pub size: (u32, u32),
    pub preview_size: Arc<Mutex<(u32, u32)>>,
    /// The voice node stopped its track.
    pub stopped: bool,
    capture: ScreenCapture,
    _picked: Option<Picked>,
    /// What sound is shared (plan §10.1): a screen's everything but
    /// Opencord, a window's chosen app.
    audio_source: Source,
    audio: Option<ScreenAudio>,
}

/// The shared sound: captured and sent while held.
struct ScreenAudio {
    capture: Capture,
    _sender: share_audio::sender::Sender,
}

/// A source chosen and capturing, not live yet.
pub(crate) struct Pending {
    capture: ScreenCapture,
    frames: Receiver<RawFrame>,
    picked: Option<Picked>,
    pub kind: StreamSourceKind,
    /// For sharing the same source again without the picker.
    pub restore_token: Option<String>,
}

pub(crate) fn screen_error(problem: ScreenProblem, message: impl Into<String>) -> CoreError {
    CoreError::Screen {
        problem,
        message: message.into(),
    }
}

fn problem(error: ScreenError) -> CoreError {
    let problem = match error {
        ScreenError::NotSupported => ScreenProblem::NotSupported,
        ScreenError::Cancelled => ScreenProblem::Cancelled,
        ScreenError::Denied => ScreenProblem::Denied,
        ScreenError::Failed(_) => ScreenProblem::Failed,
    };
    screen_error(problem, error.to_string())
}

/// What a preset and frame rate allow.
pub(crate) fn shape(request: &ScreenShareRequest) -> ScreenShape {
    let preset = match request.resolution {
        ScreenShareResolution::P480 => ScreenPreset::P480,
        ScreenShareResolution::P720 => ScreenPreset::P720,
        ScreenShareResolution::P1080 => ScreenPreset::P1080,
        ScreenShareResolution::P1440 => ScreenPreset::P1440,
        ScreenShareResolution::Source => ScreenPreset::Source,
    };
    ScreenShape {
        max_pixels: preset.max_pixels(),
        fps: request.fps,
    }
}

/// Picks the source (the portal's picker, unless `restore_token` still
/// names one) and starts capturing it.
pub(crate) async fn pick(fps: u32, restore_token: Option<String>) -> Result<Pending, CoreError> {
    let (frames, received) = sync_channel(4);
    let (remote, target, picked) = match std::env::var(NODE_VARIABLE) {
        Ok(name) => (Remote::Session, Target::Named(name), None),
        Err(_) => {
            let picked = portal::pick(restore_token.as_deref())
                .await
                .map_err(problem)?;
            let remote: OwnedFd = picked
                .remote
                .try_clone()
                .map_err(|error| screen_error(ScreenProblem::Failed, error.to_string()))?;
            (
                Remote::Portal(remote),
                Target::Id(picked.node_id),
                Some(picked),
            )
        }
    };
    let capture =
        tokio::task::spawn_blocking(move || ScreenCapture::start(remote, target, fps, frames))
            .await
            .map_err(|error| screen_error(ScreenProblem::Failed, error.to_string()))?
            .map_err(problem)?;
    let kind = match picked.as_ref().map(|picked| picked.kind) {
        Some(SourceKind::Window) => StreamSourceKind::Window,
        _ => StreamSourceKind::Screen,
    };
    let restore_token = picked
        .as_ref()
        .and_then(|picked| picked.restore_token.clone());
    Ok(Pending {
        capture,
        frames: received,
        picked,
        kind,
        restore_token,
    })
}

/// Publishes a screen track for `pending` and sends it; the share replaces
/// any other.
pub(super) async fn publish(
    video: &Video,
    pending: Pending,
    server_key: String,
    stream_key: String,
    request: ScreenShareRequest,
) -> Result<ScreenShareStarted, CoreError> {
    let Some(connection) = lock(&video.outlet).clone() else {
        drop_later(pending);
        return Err(screen_error(
            ScreenProblem::NotInVoice,
            "not in a voice channel",
        ));
    };
    let shape = shape(&request);
    let size = pending.capture.size;
    let layers = screen_layers(size.0, size.1, shape);
    let track_id = format!("screen-{:016x}", rand_id());
    let track = TrackRequest {
        track_id: track_id.clone(),
        kind: TrackKind::Screen,
        layers: layers.clone(),
    };
    let preview = lock(&video.hub)
        .textures
        .as_ref()
        .and_then(|textures| textures.create());
    let texture_id = preview.as_ref().map(|preview| preview.id());
    let preview_size = Arc::new(Mutex::new(PREVIEW));
    let outlet = Arc::clone(&video.outlet);
    let sender = VideoSender::screen(
        track_id.clone(),
        layers,
        shape,
        pending.frames,
        move |frame| {
            if let Some(connection) = lock(&outlet).as_ref() {
                connection.send_video(frame);
            }
        },
        slowly(super::camera::preview_drawer(
            preview,
            Arc::clone(&preview_size),
        )),
    );

    // The portal says nothing of which app a window is: its sound waits
    // until one is chosen (plan §10.4, "Sound from").
    let audio_source = match pending.kind {
        StreamSourceKind::Screen => Source::Everything,
        StreamSourceKind::Window => Source::Nothing,
    };
    let audio = request
        .has_audio
        .then(|| start_audio(audio_source.clone(), &video.outlet, &video.engine))
        .flatten();

    let share = Share {
        server_key,
        stream_key: stream_key.clone(),
        request: track.clone(),
        sender,
        kind: pending.kind,
        size,
        preview_size,
        stopped: false,
        capture: pending.capture,
        _picked: pending.picked,
        audio_source,
        audio,
    };
    if let Err(error) = connection.publish_track(track).await {
        drop_later(share);
        return Err(match error {
            TrackError::Refused { reason, message } => CoreError::Server {
                code: crate::convert::error_code(reason),
                message,
                retry_after_ms: None,
            },
            other => screen_error(ScreenProblem::Failed, other.to_string()),
        });
    }
    if let Some(previous) = lock(&video.hub).screen.replace(share) {
        connection.unpublish_track(&previous.request.track_id);
        drop_later(previous);
    }
    Ok(ScreenShareStarted {
        stream_key,
        track_id,
        texture_id,
        width: size.0,
        height: size.1,
        source_kind: pending.kind,
    })
}

/// Captures the shared sound from the session's PipeWire (the portal's
/// connection shows only the screen) and sends it on its own stream; the
/// call's echo canceller hears it too (plan §10.6). A share goes on
/// without its sound when this fails.
fn start_audio(
    source: Source,
    outlet: &super::Outlet,
    engine: &super::Engine,
) -> Option<ScreenAudio> {
    let (captured, frames) = sync_channel(8);
    let capture = match Capture::start(source, captured) {
        Ok(capture) => capture,
        Err(error) => {
            tracing::warn!(%error, "screen audio capture could not start");
            return None;
        }
    };
    let outlet = Arc::clone(outlet);
    let engine = Arc::clone(engine);
    let sender = share_audio::sender::Sender::start(
        frames,
        move |stereo: &[f32]| {
            if let Some(engine) = lock(&engine).as_ref() {
                engine.shared_sound(stereo.to_vec());
            }
        },
        move |encoded: Encoded| {
            if let Some(connection) = lock(&outlet).as_ref() {
                connection.send_screen_audio(EncodedScreenAudio {
                    payload: encoded.payload,
                    position: encoded.position,
                    captured: encoded.captured,
                });
            }
        },
    );
    Some(ScreenAudio {
        capture,
        _sender: sender,
    })
}

/// A window share's sound comes from `app` from now on (none: silence).
pub(super) fn share_audio_from(share: &mut Share, app: Option<String>) {
    let source = app.map_or(Source::Nothing, Source::App);
    if let Some(audio) = &share.audio {
        audio.capture.share(source.clone());
    }
    share.audio_source = source;
}

/// Starts or stops a share's sound, as `has_audio` now says.
fn follow_audio(video: &Video, has_audio: bool) {
    let mut hub = lock(&video.hub);
    let Some(share) = hub.screen.as_mut() else {
        return;
    };
    match (has_audio, share.audio.is_some()) {
        (true, false) => {
            share.audio = start_audio(share.audio_source.clone(), &video.outlet, &video.engine);
        }
        (false, true) => {
            if let Some(audio) = share.audio.take() {
                drop_later(audio);
            }
        }
        _ => {}
    }
}

/// A new quality: a new track in the new layers, then the old one goes.
/// The sound follows `has_audio`.
pub(super) async fn reshape(video: &Video, request: ScreenShareRequest) -> Result<(), CoreError> {
    follow_audio(video, request.has_audio);
    let not_live = || screen_error(ScreenProblem::Failed, "not sharing a screen");
    let connection = lock(&video.outlet).clone().ok_or_else(not_live)?;
    let size = lock(&video.hub)
        .screen
        .as_ref()
        .map(|share| share.size)
        .ok_or_else(not_live)?;
    let shape = shape(&request);
    let layers = screen_layers(size.0, size.1, shape);
    let unchanged = lock(&video.hub)
        .screen
        .as_ref()
        .is_some_and(|share| share.request.layers == layers);
    if unchanged {
        // Only the sound changed.
        return Ok(());
    }
    let track = TrackRequest {
        track_id: format!("screen-{:016x}", rand_id()),
        kind: TrackKind::Screen,
        layers: layers.clone(),
    };
    connection
        .publish_track(track.clone())
        .await
        .map_err(|error| screen_error(ScreenProblem::Failed, error.to_string()))?;
    let mut hub = lock(&video.hub);
    let Some(share) = hub.screen.as_mut() else {
        connection.unpublish_track(&track.track_id);
        return Err(not_live());
    };
    share.sender.reshape(track.track_id.clone(), layers, shape);
    let old = std::mem::replace(&mut share.request, track);
    connection.unpublish_track(&old.track_id);
    Ok(())
}

/// Draws only every [`PREVIEW_EVERY`]: the streamer's own preview is
/// small and slow, so it does not mirror itself (plan §9.4).
fn slowly(
    mut draw: impl FnMut(&opencord_media::video::picture::Picture) + Send + 'static,
) -> impl FnMut(&opencord_media::video::picture::Picture) + Send + 'static {
    let mut drawn: Option<Instant> = None;
    move |picture| {
        let now = Instant::now();
        if drawn.is_some_and(|at| now.saturating_duration_since(at) < PREVIEW_EVERY) {
            return;
        }
        drawn = Some(now);
        draw(picture);
    }
}

/// Whether a share has ended by itself: its source went, or the node
/// stopped its track. Says why.
pub(super) fn ended(share: &Share) -> Option<&'static str> {
    if share.stopped {
        return Some("The screen share was stopped");
    }
    if !share.capture.finished() && !share.sender.finished() {
        return None;
    }
    Some(match share.kind {
        StreamSourceKind::Window => "The shared window was closed",
        StreamSourceKind::Screen => "The shared screen is no longer available",
    })
}

/// Joins a share's threads off the async runtime.
pub(super) fn drop_later<T: Send + 'static>(value: T) {
    std::thread::spawn(move || drop(value));
}

fn rand_id() -> u64 {
    let mut bytes = [0u8; 8];
    let _ = getrandom::fill(&mut bytes);
    u64::from_ne_bytes(bytes)
}
