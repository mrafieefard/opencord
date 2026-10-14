//! Video in the core (Phase 2 plan §7.8-7.11, §8): this device's camera,
//! and others' tracks drawn into the app's textures at the size of their
//! tiles. Cameras, encoders and decoders exist on Linux only for now.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use opencord_media::transport::{RemoteTrack, SinkWant, TrackKind, VoiceConnection, VoiceEvent};
use tokio::runtime::Handle;
use tokio::sync::mpsc;

use crate::api::types::{CameraProblem, CoreError, MediaEvent, VideoTrackKind, VideoWant};

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub mod flutter;
pub(crate) mod screen;

/// Where a track's pictures are drawn: one of the app's textures.
pub trait FrameSink: Send + Sync {
    fn id(&self) -> i64;
    /// Shows these RGBA rows from the next frame on.
    fn present(&self, width: u32, height: u32, rgba: Vec<u8>);
}

/// Makes sinks for the app's engine.
pub trait Textures: Send + Sync {
    fn create(&self) -> Option<Arc<dyn FrameSink>>;
}

/// The voice connection video goes out on, while there is one.
pub(super) type Outlet = Arc<Mutex<Option<Arc<VoiceConnection>>>>;

pub struct Video {
    runtime: Handle,
    events: mpsc::UnboundedSender<MediaEvent>,
    outlet: Outlet,
    /// The call's audio engine: the shared screen sound goes to its echo
    /// canceller (plan §10.6).
    engine: Engine,
    hub: Mutex<Hub>,
}

pub(super) type Engine = Arc<Mutex<Option<Arc<opencord_media::audio::engine::AudioEngine>>>>;

#[derive(Default)]
struct Hub {
    textures: Option<Arc<dyn Textures>>,
    /// How long each watched stream's sound waits to play, by streamer.
    stream_delays: HashMap<i64, std::time::Duration>,
    /// The server and channel of the live connection.
    place: Option<(String, i64)>,
    tracks: HashMap<String, Remote>,
    /// Tile sizes the app asked for, by track.
    wants: HashMap<String, (u32, u32)>,
    camera: Option<camera::Camera>,
    screen: Option<screen::Share>,
}

impl Hub {
    /// The sender of this device's camera or screen share with `track_id`.
    fn sender_of(&self, track_id: &str) -> Option<&opencord_media::video::sender::VideoSender> {
        let camera = self
            .camera
            .as_ref()
            .filter(|camera| camera.request.track_id == track_id)
            .map(|camera| &camera.sender);
        let screen = self
            .screen
            .as_ref()
            .filter(|share| share.request.track_id == track_id)
            .map(|share| &share.sender);
        camera.or(screen)
    }
}

struct Remote {
    user_id: i64,
    /// A screen share: its pictures wait for its sound.
    screen: bool,
    texture: Option<Arc<dyn FrameSink>>,
    receiver: Option<opencord_media::video::receiver::VideoReceiver>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn video_kind(kind: TrackKind) -> VideoTrackKind {
    match kind {
        TrackKind::Camera => VideoTrackKind::Camera,
        TrackKind::Screen | TrackKind::ScreenAudio => VideoTrackKind::Screen,
    }
}

impl Video {
    pub fn new(runtime: Handle, events: mpsc::UnboundedSender<MediaEvent>) -> Self {
        Self {
            runtime,
            events,
            outlet: Arc::new(Mutex::new(None)),
            engine: Arc::new(Mutex::new(None)),
            hub: Mutex::new(Hub::default()),
        }
    }

    /// `user_id`'s screen share sound plays `delay` after arriving: its
    /// pictures wait as long (plan §10.5).
    pub fn set_stream_delay(&self, user_id: i64, delay: std::time::Duration) {
        let mut hub = lock(&self.hub);
        hub.stream_delays.insert(user_id, delay);
        for remote in hub.tracks.values() {
            if remote.screen
                && remote.user_id == user_id
                && let Some(receiver) = &remote.receiver
            {
                receiver.set_hold(delay);
            }
        }
    }

    /// The call's audio engine, while there is a call.
    pub fn set_audio_engine(
        &self,
        engine: Option<Arc<opencord_media::audio::engine::AudioEngine>>,
    ) {
        *lock(&self.engine) = engine;
    }

    /// The call that had `engine` ended (a newer one may have its own).
    pub fn release_audio_engine(&self, engine: &Arc<opencord_media::audio::engine::AudioEngine>) {
        let mut current = lock(&self.engine);
        if current
            .as_ref()
            .is_some_and(|held| Arc::ptr_eq(held, engine))
        {
            *current = None;
        }
    }

    /// Where pictures are drawn from now on (the app's engine).
    pub fn set_textures(&self, textures: Arc<dyn Textures>) {
        lock(&self.hub).textures = Some(textures);
    }

    /// A voice connection is live, in this server's channel. The camera, if
    /// on, is published on it.
    pub fn connected(&self, server_key: &str, channel_id: i64, connection: Arc<VoiceConnection>) {
        *lock(&self.outlet) = Some(Arc::clone(&connection));
        let mut hub = lock(&self.hub);
        hub.place = Some((server_key.to_owned(), channel_id));
        let requests = hub
            .camera
            .iter()
            .map(|camera| camera.request.clone())
            .chain(hub.screen.iter().map(|share| share.request.clone()));
        for request in requests {
            let connection = Arc::clone(&connection);
            self.runtime.spawn(async move {
                if let Err(error) = connection.publish_track(request).await {
                    tracing::warn!(%error, "a track could not be published again");
                }
            });
        }
    }

    /// The connection is gone; others' tracks with it (a new one announces
    /// them again). The camera keeps capturing.
    pub fn disconnected(&self) {
        *lock(&self.outlet) = None;
        let mut hub = lock(&self.hub);
        let removed: Vec<(String, Remote)> = hub.tracks.drain().collect();
        let place = hub.place.take();
        drop(hub);
        if let Some((server_key, channel_id)) = place {
            for (track_id, remote) in removed {
                let _ = self.events.send(MediaEvent::VideoTrackRemoved {
                    server_key: server_key.clone(),
                    channel_id,
                    user_id: remote.user_id,
                    track_id,
                });
            }
        }
    }

    /// Handles the transport's video news; `false` for everything else.
    pub fn on_event(&self, event: &VoiceEvent) -> bool {
        match event {
            VoiceEvent::Track { user_id, track } => self.track(*user_id, track),
            VoiceEvent::TrackRemoved { user_id, track_id } => {
                self.track_removed(*user_id, track_id)
            }
            VoiceEvent::Video(video) => {
                if let Some(receiver) = lock(&self.hub)
                    .tracks
                    .get(&video.track_id)
                    .and_then(|remote| remote.receiver.as_ref())
                {
                    receiver.push(video.clone());
                }
            }
            VoiceEvent::Encode {
                track_id,
                layers,
                bitrates,
                fps_scale,
                size_scale,
            } => {
                // The camera's or the screen share's: only the layers
                // someone watches, within the uplink.
                if let Some(sender) = lock(&self.hub).sender_of(track_id) {
                    sender.encode(layers, bitrates, *fps_scale, *size_scale);
                }
            }
            // A viewer that starts watching asks for a keyframe: without
            // one the node sends it nothing.
            VoiceEvent::KeyframeRequested { track_id, layer } => {
                if let Some(sender) = lock(&self.hub).sender_of(track_id) {
                    sender.keyframe(*layer);
                }
            }
            VoiceEvent::TrackStopped {
                track_id, message, ..
            } => {
                if let Some(share) = lock(&self.hub)
                    .screen
                    .as_mut()
                    .filter(|share| share.request.track_id == *track_id)
                {
                    // The periodic check ends it, on the server too.
                    share.stopped = true;
                    return true;
                }
                let _ = track_id;
                if self.stop_camera() {
                    let _ = self.events.send(MediaEvent::CameraStopped {
                        message: message.clone(),
                    });
                }
            }
            _ => return false,
        }
        true
    }

    fn track(&self, user_id: i64, track: &RemoteTrack) {
        let mut hub = lock(&self.hub);
        if hub.tracks.contains_key(&track.track_id) {
            return;
        }
        let texture = hub.textures.as_ref().and_then(|textures| textures.create());
        let texture_id = texture.as_ref().map(|texture| texture.id());
        hub.tracks.insert(
            track.track_id.clone(),
            Remote {
                user_id,
                screen: track.kind == TrackKind::Screen,
                texture,
                receiver: None,
            },
        );
        if let Some((server_key, channel_id)) = hub.place.clone() {
            let (width, height) = track
                .layers
                .iter()
                .max_by_key(|layer| layer.height)
                .map_or((16, 9), |layer| (layer.width, layer.height));
            let _ = self.events.send(MediaEvent::VideoTrackAdded {
                server_key,
                channel_id,
                user_id,
                track_id: track.track_id.clone(),
                kind: video_kind(track.kind),
                texture_id,
                width,
                height,
            });
        }
        self.apply_wants(&mut hub);
    }

    fn track_removed(&self, user_id: i64, track_id: &str) {
        let mut hub = lock(&self.hub);
        if hub.tracks.remove(track_id).is_none() {
            return;
        }
        if let Some((server_key, channel_id)) = hub.place.clone() {
            let _ = self.events.send(MediaEvent::VideoTrackRemoved {
                server_key,
                channel_id,
                user_id,
                track_id: track_id.to_owned(),
            });
        }
        self.apply_wants(&mut hub);
    }

    /// The tiles the app shows, and their sizes.
    pub fn set_wants(&self, wants: Vec<VideoWant>) {
        let mut hub = lock(&self.hub);
        hub.wants = wants
            .into_iter()
            .map(|want| (want.track_id, (want.width, want.height)))
            .collect();
        if let Some(camera) = &hub.camera {
            let size = hub.wants.get(&camera.request.track_id).copied();
            *lock(&camera.preview_size) = size.unwrap_or_default();
        }
        if let Some(share) = &hub.screen {
            let size = hub.wants.get(&share.request.track_id).copied();
            *lock(&share.preview_size) = size.unwrap_or_default();
        }
        self.apply_wants(&mut hub);
    }

    /// Decodes what is shown, at its size; tells the node what to send.
    fn apply_wants(&self, hub: &mut Hub) {
        let Hub {
            tracks,
            wants,
            stream_delays,
            ..
        } = hub;
        let mut sink_wants = Vec::new();
        for (track_id, remote) in tracks.iter_mut() {
            let size = wants
                .get(track_id)
                .copied()
                .filter(|&(w, h)| w > 0 && h > 0);
            if let Some((_, height)) = size {
                sink_wants.push(SinkWant {
                    track_id: track_id.clone(),
                    max_height: height,
                });
            }
            match (size, remote.texture.clone()) {
                (Some((width, height)), Some(texture)) => {
                    let receiver = remote.receiver.get_or_insert_with(|| {
                        let outlet = Arc::clone(&self.outlet);
                        let track_id = track_id.clone();
                        opencord_media::video::receiver::VideoReceiver::start(
                            move |rgba| texture.present(rgba.width, rgba.height, rgba.data),
                            move || {
                                if let Some(connection) = lock(&outlet).as_ref() {
                                    connection.request_keyframe(&track_id);
                                }
                            },
                        )
                    });
                    receiver.set_size(width, height);
                    if remote.screen
                        && let Some(delay) = stream_delays.get(&remote.user_id)
                    {
                        receiver.set_hold(*delay);
                    }
                }
                _ => remote.receiver = None,
            }
        }
        if let Some(connection) = lock(&self.outlet).as_ref() {
            connection.set_sink_wants(sink_wants);
        }
    }

    /// Turns the camera off; whether it was on.
    pub fn stop_camera(&self) -> bool {
        let camera = lock(&self.hub).camera.take();
        let Some(camera) = camera else {
            return false;
        };
        if let Some(connection) = lock(&self.outlet).as_ref() {
            connection.unpublish_track(&camera.request.track_id);
        }
        // Joins its threads: off the async runtime.
        std::thread::spawn(move || drop(camera));
        true
    }

    /// Publishes a picked screen and sends it, replacing any other share.
    pub(crate) async fn publish_screen(
        &self,
        pending: screen::Pending,
        server_key: String,
        stream_key: String,
        request: crate::api::types::ScreenShareRequest,
    ) -> Result<crate::api::types::ScreenShareStarted, CoreError> {
        screen::publish(self, pending, server_key, stream_key, request).await
    }

    /// A new quality for the screen share.
    pub(crate) async fn reshape_screen(
        &self,
        request: crate::api::types::ScreenShareRequest,
    ) -> Result<(), CoreError> {
        screen::reshape(self, request).await
    }

    /// Whose sound a window share sends: an app by name, or none.
    pub fn share_audio_from(&self, app: Option<String>) -> Result<(), CoreError> {
        let mut hub = lock(&self.hub);
        let share = hub.screen.as_mut().ok_or_else(|| CoreError::InvalidInput {
            message: "not sharing a screen".to_owned(),
        })?;
        screen::share_audio_from(share, app);
        Ok(())
    }

    /// Ends the screen share here; its server and stream key, if it was on.
    pub fn stop_screen(&self) -> Option<(String, String)> {
        let share = lock(&self.hub).screen.take()?;
        if let Some(connection) = lock(&self.outlet).as_ref() {
            connection.unpublish_track(&share.request.track_id);
        }
        let keys = (share.server_key.clone(), share.stream_key.clone());
        screen::drop_later(share);
        Some(keys)
    }

    /// The live screen share's server and stream key.
    pub fn screen_share(&self) -> Option<(String, String)> {
        lock(&self.hub)
            .screen
            .as_ref()
            .map(|share| (share.server_key.clone(), share.stream_key.clone()))
    }

    /// A screen share that ended by itself (its window closed, its screen
    /// went, the node stopped it) is stopped here and reported; returns its
    /// server and stream key, for the server.
    pub fn screen_ended(&self) -> Option<(String, String)> {
        let message = lock(&self.hub).screen.as_ref().and_then(screen::ended)?;
        let keys = self.stop_screen()?;
        let _ = self.events.send(MediaEvent::ScreenShareStopped {
            message: message.to_owned(),
        });
        Some(keys)
    }

    /// The server ended this device's stream (a permission or the server's
    /// maximum changed): stop sharing here too, and say so.
    pub fn screen_ended_by_server(&self, stream_key: &str) {
        let ours = self
            .screen_share()
            .is_some_and(|(_, key)| key == stream_key);
        if ours && self.stop_screen().is_some() {
            let _ = self.events.send(MediaEvent::ScreenShareStopped {
                message: "The server ended the screen share".to_owned(),
            });
        }
    }

    pub fn camera_on(&self) -> bool {
        lock(&self.hub).camera.is_some()
    }

    /// The encoders of this device's camera and screen share, while on.
    pub fn encoders(&self) -> (Option<String>, Option<String>) {
        let hub = lock(&self.hub);
        (
            hub.camera
                .as_ref()
                .and_then(|camera| camera.sender.encoder()),
            hub.screen.as_ref().and_then(|share| share.sender.encoder()),
        )
    }

    /// Whether the camera is on but has stopped by itself (unplugged).
    pub fn camera_ended(&self) -> bool {
        lock(&self.hub)
            .camera
            .as_ref()
            .is_some_and(|camera| camera.sender.finished())
    }

    /// Turns the camera on and publishes it.
    pub async fn start_camera(
        &self,
        device_id: Option<String>,
    ) -> Result<crate::api::types::CameraStarted, CoreError> {
        camera::start(self, device_id).await
    }
}

pub(crate) fn camera_error(problem: CameraProblem, message: impl Into<String>) -> CoreError {
    CoreError::Camera {
        problem,
        message: message.into(),
    }
}

/// The cameras there are.
pub async fn cameras() -> Result<Vec<crate::api::types::CameraDevice>, CoreError> {
    opencord_media::video::camera::cameras()
        .await
        .map(|cameras| {
            cameras
                .into_iter()
                .map(|camera| crate::api::types::CameraDevice {
                    id: camera.id,
                    name: camera.name,
                })
                .collect()
        })
        .map_err(camera::problem)
}

mod camera {
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use opencord_media::transport::{TrackError, TrackKind, TrackRequest};
    use opencord_media::video::camera::{CameraError, Capture, RawFrame};
    use opencord_media::video::codec::scale::Scaler;
    use opencord_media::video::layers::camera_layers;
    use opencord_media::video::sender::VideoSender;

    use super::{FrameSink, Video, camera_error, lock};
    use crate::api::types::{CameraProblem, CameraStarted, CoreError};

    /// How large the preview is drawn until the app says.
    const PREVIEW: (u32, u32) = (640, 360);

    pub(super) struct Camera {
        pub request: TrackRequest,
        pub sender: VideoSender,
        pub preview_size: Arc<Mutex<(u32, u32)>>,
        _capture: Capture,
    }

    pub(super) fn problem(error: CameraError) -> CoreError {
        let problem = match error {
            CameraError::NotSupported => CameraProblem::NotSupported,
            CameraError::NoCamera => CameraProblem::NoCamera,
            CameraError::Denied => CameraProblem::Denied,
            CameraError::NoUsableMode => CameraProblem::NoUsableMode,
            CameraError::Failed(_) => CameraProblem::Failed,
        };
        camera_error(problem, error.to_string())
    }

    /// A camera switched from may still be closing on its own thread for
    /// a moment (V4L2 and Media Foundation then say it is busy), so a
    /// switch tries again for up to this long.
    const SWITCH_WAIT: Duration = Duration::from_secs(1);
    const SWITCH_RETRY: Duration = Duration::from_millis(100);

    async fn open(
        device_id: Option<String>,
        frames: std::sync::mpsc::SyncSender<RawFrame>,
        switching: bool,
    ) -> Result<Capture, CoreError> {
        let wait = if switching {
            SWITCH_WAIT
        } else {
            Duration::ZERO
        };
        let give_up = Instant::now() + wait;
        loop {
            match Capture::start(device_id.clone(), frames.clone()).await {
                Ok(capture) => return Ok(capture),
                Err(CameraError::Failed(_)) if Instant::now() < give_up => {
                    tokio::time::sleep(SWITCH_RETRY).await;
                }
                Err(error) => return Err(problem(error)),
            }
        }
    }

    pub(super) async fn start(
        video: &Video,
        device_id: Option<String>,
    ) -> Result<CameraStarted, CoreError> {
        let Some(connection) = lock(&video.outlet).clone() else {
            return Err(camera_error(
                CameraProblem::NotInVoice,
                "not in a voice channel",
            ));
        };
        let switching = video.stop_camera();
        let (frames, captured) = std::sync::mpsc::sync_channel(4);
        let capture = open(device_id, frames, switching).await?;
        let mode = capture.mode();
        let layers = camera_layers(mode.width, mode.height);
        let track_id = format!("camera-{:016x}", rand_id());
        let request = TrackRequest {
            track_id: track_id.clone(),
            kind: TrackKind::Camera,
            layers: layers.clone(),
        };
        let preview = lock(&video.hub)
            .textures
            .as_ref()
            .and_then(|textures| textures.create());
        let texture_id = preview.as_ref().map(|preview| preview.id());
        let preview_size = Arc::new(Mutex::new(PREVIEW));
        let outlet = Arc::clone(&video.outlet);
        let sender = VideoSender::camera(
            track_id.clone(),
            layers,
            captured,
            move |frame| {
                if let Some(connection) = lock(&outlet).as_ref() {
                    connection.send_video(frame);
                }
            },
            preview_drawer(preview, Arc::clone(&preview_size)),
        );
        let camera = Camera {
            request: request.clone(),
            sender,
            preview_size,
            _capture: capture,
        };
        if let Err(error) = connection.publish_track(request).await {
            std::thread::spawn(move || drop(camera));
            return Err(match error {
                TrackError::Refused { reason, message } => CoreError::Server {
                    code: crate::convert::error_code(reason),
                    message,
                    retry_after_ms: None,
                },
                other => camera_error(CameraProblem::Failed, other.to_string()),
            });
        }
        lock(&video.hub).camera = Some(camera);
        Ok(CameraStarted {
            track_id,
            texture_id,
            width: mode.width,
            height: mode.height,
        })
    }

    /// Draws each camera picture into the preview at the size its tile
    /// asked for.
    pub(super) fn preview_drawer(
        preview: Option<Arc<dyn FrameSink>>,
        size: Arc<Mutex<(u32, u32)>>,
    ) -> impl FnMut(&opencord_media::video::picture::Picture) + Send + 'static {
        let mut scaler = Scaler::new();
        move |picture| {
            let Some(preview) = &preview else {
                return;
            };
            let (tile_width, tile_height) = *lock(&size);
            if tile_width == 0 || tile_height == 0 {
                return;
            }
            let scale = (f64::from(tile_width) / f64::from(picture.width))
                .min(f64::from(tile_height) / f64::from(picture.height))
                .min(1.0);
            let even = |value: f64| ((value.round() as u32) & !1).max(2);
            let (width, height) = (
                even(f64::from(picture.width) * scale),
                even(f64::from(picture.height) * scale),
            );
            if let Ok(rgba) = scaler.rgba(picture, width, height) {
                preview.present(width, height, rgba);
            }
        }
    }

    fn rand_id() -> u64 {
        let mut bytes = [0u8; 8];
        let _ = getrandom::fill(&mut bytes);
        u64::from_ne_bytes(bytes)
    }
}
