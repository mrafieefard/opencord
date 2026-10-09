//! A headless voice client (Phase 2 plan §15): joins a server and a voice
//! channel through the same core as the app, sends a tone as Opus and a
//! synthetic three-layer camera, and reports what it hears and sees from
//! whom. Used by the integration tests, load tests, and self-hosters
//! checking their voice setup.

use std::collections::HashMap;
use std::f32::consts::TAU;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use anyhow::{Context as _, anyhow, bail};
use opencord_core::api::types::{
    AddServerOutcome, ChannelKind, CoreError, CoreEvent, CoreEventPayload, ErrorCode, ReadySnapshot,
};
use opencord_core::client::Client;
use opencord_core::identity::Identity;
use opencord_core::voice::VoiceServer;
use opencord_media::transport::{
    AudioFrame, Layer, RemoteTrack, SinkWant, TrackKind, TrackRequest, VoiceConnection, VoiceEvent,
    VoiceTarget,
};
#[cfg(target_os = "linux")]
use opencord_media::video::codec::decoder::Decoder;
use opencord_media::video::pattern::{TestPattern, check};
use tokio::sync::{broadcast, mpsc};

pub const SAMPLE_RATE: u32 = 48_000;
/// 20 ms at 48 kHz.
pub const FRAME_SAMPLES: usize = 960;
const FRAME: Duration = Duration::from_millis(20);
const WAIT: Duration = Duration::from_secs(15);
/// How often an idle test pattern looks again for layers to send.
/// How long the bot keeps trying when the server says to slow down or
/// cannot be reached: bots started together from one address meet its
/// per-address limits (5 identifies a minute).
const RETRY_FOR: Duration = Duration::from_secs(120);
const RETRY_EVERY: Duration = Duration::from_secs(3);
const PATTERN_IDLE: Duration = Duration::from_millis(50);

/// The bot's camera: 180p at 15 fps, 360p and 720p at 30, each at its
/// layer's usual ceiling.
pub fn camera_layers() -> Vec<Layer> {
    let layer = |rid: &str, width, height, fps, max_bitrate| Layer {
        rid: rid.to_owned(),
        width,
        height,
        fps,
        max_bitrate,
    };
    vec![
        layer("l", 320, 180, 15, 150_000),
        layer("m", 640, 360, 30, 500_000),
        layer("h", 1280, 720, 30, 1_500_000),
    ]
}

/// A bot connected to one server.
pub struct Voicebot {
    pub client: Client,
    events: mpsc::UnboundedReceiver<CoreEvent>,
    pub server_key: String,
    pub ready: ReadySnapshot,
    _data: tempfile::TempDir,
}

/// `attempt`'s result, trying again while it fails for a while only:
/// trouble connecting, or the server asking to slow down.
async fn retrying<T, F>(mut attempt: impl FnMut() -> F) -> Result<T, CoreError>
where
    F: Future<Output = Result<T, CoreError>>,
{
    let started = Instant::now();
    loop {
        let error = match attempt().await {
            Err(error) => error,
            done => return done,
        };
        let wait = match &error {
            CoreError::Connection { .. } => RETRY_EVERY,
            CoreError::Server {
                code: ErrorCode::RateLimited,
                retry_after_ms,
                ..
            } => retry_after_ms.map_or(RETRY_EVERY, |ms| Duration::from_millis(u64::from(ms))),
            _ => return Err(error),
        };
        if started.elapsed() + wait > RETRY_FOR {
            return Err(error);
        }
        tokio::time::sleep(wait).await;
    }
}

impl Voicebot {
    /// Adds the server (an invite link or `host:port`), trusting its
    /// certificate on first use, and waits for Ready.
    pub async fn connect(
        server: &str,
        claim_token: Option<String>,
        name: &str,
    ) -> anyhow::Result<Self> {
        let data = tempfile::tempdir()?;
        let (client, mut events) = Client::new(data.path(), tokio::runtime::Handle::current())?;
        client.set_identity(Identity::generate(), name.to_owned());
        let add = || retrying(|| client.add_server(server, claim_token.clone()));
        let server_key = match add().await? {
            AddServerOutcome::Added(added) => added.key,
            AddServerOutcome::NeedsTrust {
                address,
                fingerprint,
            } => {
                client.trust_fingerprint(&address, &fingerprint)?;
                match add().await? {
                    AddServerOutcome::Added(added) => added.key,
                    AddServerOutcome::NeedsTrust { .. } => {
                        bail!("the certificate is still not trusted")
                    }
                }
            }
        };
        let ready = wait_for(&mut events, |payload| match payload {
            CoreEventPayload::Ready(ready) => Some(ready.clone()),
            _ => None,
        })
        .await?;
        Ok(Self {
            client,
            events,
            server_key,
            ready,
            _data: data,
        })
    }

    /// The id of the voice channel called `name`.
    pub fn voice_channel(&self, name: &str) -> anyhow::Result<i64> {
        self.ready
            .channels
            .iter()
            .find(|channel| channel.kind == ChannelKind::Voice && channel.name == name)
            .map(|channel| channel.id)
            .ok_or_else(|| anyhow!("no voice channel named {name}"))
    }

    pub fn user_id(&self) -> i64 {
        self.ready.self_user.id
    }

    /// Joins the voice channel and connects media; follows the server when
    /// it sends this bot to another node.
    pub async fn join(&self, channel_id: i64) -> anyhow::Result<VoiceSession> {
        let mut voice_servers = self.client.voice_servers();
        retrying(|| self.client.voice_join(&self.server_key, channel_id)).await?;
        let server = next_voice_server(&mut voice_servers, &self.server_key).await?;
        VoiceSession::start(server, voice_servers).await
    }

    /// The next core event, without a time limit; `None` once the core
    /// stopped.
    pub async fn next_event(&mut self) -> Option<CoreEventPayload> {
        self.events.recv().await.map(|event| event.payload)
    }

    /// Skips core events until `pick` returns something.
    pub async fn wait_for<T>(
        &mut self,
        pick: impl FnMut(&CoreEventPayload) -> Option<T>,
    ) -> anyhow::Result<T> {
        wait_for(&mut self.events, pick).await
    }
}

async fn wait_for<T>(
    events: &mut mpsc::UnboundedReceiver<CoreEvent>,
    mut pick: impl FnMut(&CoreEventPayload) -> Option<T>,
) -> anyhow::Result<T> {
    tokio::time::timeout(WAIT, async {
        loop {
            let event = events.recv().await.context("the core stopped")?;
            if let Some(found) = pick(&event.payload) {
                return Ok(found);
            }
        }
    })
    .await
    .context("timed out waiting for the server")?
}

async fn next_voice_server(
    voice_servers: &mut broadcast::Receiver<VoiceServer>,
    server_key: &str,
) -> anyhow::Result<VoiceServer> {
    tokio::time::timeout(WAIT, async {
        loop {
            match voice_servers.recv().await {
                Ok(server) if server.server_key == server_key => return Ok(server),
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => bail!("the core stopped"),
            }
        }
    })
    .await
    .context("timed out waiting for a voice server")?
}

/// What a bot saw of one person's video.
#[derive(Debug, Clone, Default)]
pub struct Seen {
    pub frames: u64,
    /// Pictures whose test-pattern checksum held.
    pub intact: u64,
    /// Pictures that decoded, when watching with decoding.
    pub decoded: u64,
    pub keyframes: u64,
    pub bytes: u64,
    /// Pictures by layer, lowest first.
    pub layers: [u64; 3],
}

/// What a bot heard from one person.
#[derive(Debug, Clone, Default)]
pub struct Heard {
    pub packets: u64,
    /// Frames Opus could decode.
    pub decoded: u64,
    /// Loudest decoded frame, in dBFS.
    pub peak_dbfs: f32,
    pub first: Option<Instant>,
    pub last: Option<Instant>,
    /// Arrival time of every packet.
    pub arrivals: Vec<Instant>,
}

#[derive(Default)]
struct Shared {
    heard: HashMap<i64, Heard>,
    connection: Option<Arc<VoiceConnection>>,
    media_connected: bool,
    connections: u32,
    closed: Option<Option<u16>>,
    /// The voice gateway of the current connection.
    gateway_url: String,
    /// The capture clock: where the next frame would start, in samples.
    position: u64,
    seen: HashMap<i64, Seen>,
    /// The camera this bot publishes, if any, and whether a new connection
    /// still has to be told of it.
    pattern: Option<TestPattern>,
    #[cfg(target_os = "linux")]
    encoded: Option<camera::EncodedVideo>,
    /// The screen this bot shares, if any.
    #[cfg(target_os = "linux")]
    screen: Option<camera::EncodedVideo>,
    republish: bool,
    /// Others' tracks, and the tile height to watch them at.
    tracks: Vec<(i64, RemoteTrack)>,
    watch_height: Option<u32>,
    /// Decoders for what is watched, by track, when decoding.
    #[cfg(target_os = "linux")]
    decoders: Option<HashMap<String, Decoder>>,
}

impl Shared {
    /// What to publish this bot's camera with, if it has one.
    fn camera_request(&self) -> Option<TrackRequest> {
        #[cfg(target_os = "linux")]
        if let Some(camera) = &self.encoded {
            return Some(camera.request.clone());
        }
        self.pattern
            .as_ref()
            .map(|pattern| pattern.request(TrackKind::Camera))
    }

    /// Every track this bot publishes: its camera and its screen.
    fn requests(&self) -> Vec<TrackRequest> {
        let mut requests: Vec<TrackRequest> = self.camera_request().into_iter().collect();
        #[cfg(target_os = "linux")]
        if let Some(screen) = &self.screen {
            requests.push(screen.request.clone());
        }
        requests
    }

    /// This bot's encoded track with this id.
    #[cfg(target_os = "linux")]
    fn encoded(&self, track_id: &str) -> Option<&camera::EncodedVideo> {
        [self.encoded.as_ref(), self.screen.as_ref()]
            .into_iter()
            .flatten()
            .find(|video| video.request.track_id == track_id)
    }

    /// Tells the node which tracks this bot watches.
    fn send_wants(&self) {
        let (Some(height), Some(connection)) = (self.watch_height, self.connection.as_ref()) else {
            return;
        };
        connection.set_sink_wants(
            self.tracks
                .iter()
                .map(|(_, track)| SinkWant {
                    track_id: track.track_id.clone(),
                    max_height: height,
                })
                .collect(),
        );
    }
}

/// A bot in a voice channel.
pub struct VoiceSession {
    shared: Arc<Mutex<Shared>>,
    /// Where the capture clock starts.
    started: Instant,
}

impl VoiceSession {
    async fn start(
        server: VoiceServer,
        voice_servers: broadcast::Receiver<VoiceServer>,
    ) -> anyhow::Result<Self> {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let events = connect(&shared, &server).await?;
        tokio::spawn(follow(Arc::clone(&shared), server, events, voice_servers));
        let session = Self {
            shared,
            started: Instant::now(),
        };
        session.wait_until(|shared| shared.media_connected).await?;
        Ok(session)
    }

    /// Sends a sine tone for `duration`, a 20 ms frame at a time, as one
    /// talk spurt.
    pub fn play_tone(&self, frequency: f32, duration: Duration) -> tokio::task::JoinHandle<()> {
        let shared = Arc::clone(&self.shared);
        let started = self.started;
        tokio::spawn(async move {
            let Ok(mut encoder) =
                opus::Encoder::new(SAMPLE_RATE, opus::Channels::Mono, opus::Application::Voip)
            else {
                return;
            };
            let frames = (duration.as_millis() / FRAME.as_millis()).max(1);
            let mut ticker = tokio::time::interval(FRAME);
            let mut phase = 0.0f32;
            let step = TAU * frequency / SAMPLE_RATE as f32;
            let mut pcm = vec![0i16; FRAME_SAMPLES];
            // The capture clock runs while the bot is silent too.
            let mut position = {
                let mut shared = lock(&shared);
                let now = started.elapsed().as_micros() as u64 * u64::from(SAMPLE_RATE) / 1_000_000;
                let position = now.max(shared.position);
                shared.position = position + frames as u64 * FRAME_SAMPLES as u64;
                position
            };
            let mut marker = true;
            for _ in 0..frames {
                ticker.tick().await;
                for sample in &mut pcm {
                    *sample = (phase.sin() * 8_000.0) as i16;
                    phase = (phase + step) % TAU;
                }
                let Ok(payload) = encoder.encode_vec(&pcm, 1_500) else {
                    continue;
                };
                let connection = lock(&shared).connection.clone();
                if let Some(connection) = connection {
                    connection.send_audio(AudioFrame {
                        payload,
                        position,
                        marker: std::mem::take(&mut marker),
                        audio_level: level_dbov(&pcm),
                        voice_activity: true,
                    });
                }
                position += FRAME_SAMPLES as u64;
            }
        })
    }

    /// What has been heard so far, by user id.
    pub fn heard(&self) -> HashMap<i64, Heard> {
        lock(&self.shared).heard.clone()
    }

    /// Publishes a synthetic three-layer camera (the test pattern): the
    /// layers the node wants, as the uplink allows, with keyframes on
    /// request. Published again whenever the call moves to another node.
    pub async fn publish_camera(&self, track_id: &str) -> anyhow::Result<()> {
        let pattern = TestPattern::new(track_id, camera_layers(), Instant::now());
        let request = pattern.request(TrackKind::Camera);
        let connection = lock(&self.shared)
            .connection
            .clone()
            .context("not connected")?;
        connection.publish_track(request).await?;
        lock(&self.shared).pattern = Some(pattern);
        tokio::spawn(send_pattern(Arc::clone(&self.shared)));
        Ok(())
    }

    /// Publishes a camera of real H.264: the moving test scene at 1280×720
    /// and 30 fps through the same encoders as the app's camera, its layers
    /// as the node and the uplink allow. Published again whenever the call
    /// moves to another node.
    #[cfg(target_os = "linux")]
    pub async fn publish_encoded_camera(&self, track_id: &str) -> anyhow::Result<()> {
        let connection = lock(&self.shared)
            .connection
            .clone()
            .context("not connected")?;
        let camera = camera::EncodedVideo::camera(track_id, Arc::clone(&self.shared))?;
        connection.publish_track(camera.request.clone()).await?;
        lock(&self.shared).encoded = Some(camera);
        Ok(())
    }

    /// Publishes a screen share of real H.264: the moving scene as a
    /// 1920×1080 screen, in the layers `shape` gives (plan §9.2). Going live
    /// on the main server is the caller's part.
    #[cfg(target_os = "linux")]
    pub async fn publish_encoded_screen(
        &self,
        track_id: &str,
        shape: opencord_media::video::layers::ScreenShape,
    ) -> anyhow::Result<()> {
        let connection = lock(&self.shared)
            .connection
            .clone()
            .context("not connected")?;
        let screen = camera::EncodedVideo::screen(track_id, shape, Arc::clone(&self.shared))?;
        connection.publish_track(screen.request.clone()).await?;
        lock(&self.shared).screen = Some(screen);
        Ok(())
    }

    /// Watches everyone's tracks in tiles `height` pixels tall, decoding
    /// what comes if `decode` (Linux only).
    pub fn watch(&self, height: u32, decode: bool) {
        let mut shared = lock(&self.shared);
        shared.watch_height = Some(height);
        #[cfg(target_os = "linux")]
        if decode && shared.decoders.is_none() {
            shared.decoders = Some(HashMap::new());
        }
        #[cfg(not(target_os = "linux"))]
        let _ = decode;
        shared.send_wants();
    }

    /// What has been seen so far, by user id.
    pub fn seen(&self) -> HashMap<i64, Seen> {
        lock(&self.shared).seen.clone()
    }

    /// Forgets what was heard so far.
    pub fn clear_heard(&self) {
        lock(&self.shared).heard.clear();
    }

    /// How many voice connections this session has had: 1, plus one per
    /// time the server sent it elsewhere.
    pub fn connections(&self) -> u32 {
        lock(&self.shared).connections
    }

    /// The voice gateway this session is connected to now.
    pub fn gateway_url(&self) -> String {
        lock(&self.shared).gateway_url.clone()
    }

    /// Drops the voice gateway as a network failure would.
    pub fn drop_gateway(&self) {
        if let Some(connection) = lock(&self.shared).connection.clone() {
            connection.drop_gateway();
        }
    }

    /// The close code, once the voice connection ended for good.
    pub fn closed(&self) -> Option<Option<u16>> {
        lock(&self.shared).closed
    }

    /// Waits until `check` holds.
    pub async fn wait_until(&self, check: impl Fn(&SessionView<'_>) -> bool) -> anyhow::Result<()> {
        let end = Instant::now() + WAIT;
        loop {
            {
                let shared = lock(&self.shared);
                let view = SessionView {
                    media_connected: shared.media_connected,
                    connections: shared.connections,
                    heard: &shared.heard,
                    seen: &shared.seen,
                    closed: shared.closed,
                };
                if check(&view) {
                    return Ok(());
                }
            }
            if Instant::now() >= end {
                bail!("timed out waiting for the voice session");
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

/// A look at a session's state, for [`VoiceSession::wait_until`].
pub struct SessionView<'a> {
    pub media_connected: bool,
    pub connections: u32,
    pub heard: &'a HashMap<i64, Heard>,
    pub seen: &'a HashMap<i64, Seen>,
    pub closed: Option<Option<u16>>,
}

async fn connect(
    shared: &Arc<Mutex<Shared>>,
    server: &VoiceServer,
) -> anyhow::Result<mpsc::UnboundedReceiver<VoiceEvent>> {
    let fingerprint: Option<[u8; 32]> = server.certificate_fingerprint.as_slice().try_into().ok();
    let gateway_url = server.gateway_url();
    let (connection, events) = VoiceConnection::connect(VoiceTarget {
        gateway_url: gateway_url.clone(),
        certificate_fingerprint: fingerprint,
        token: server.token.clone(),
        user_id: server.user_id,
        session_id: server.session_id.clone(),
        channel_id: server.channel_id,
    })
    .await?;
    let mut shared = lock(shared);
    shared.connection = Some(Arc::new(connection));
    shared.media_connected = false;
    shared.connections += 1;
    shared.closed = None;
    shared.gateway_url = gateway_url;
    // The new node announces its tracks again; a camera is published
    // again once media is up.
    shared.tracks.clear();
    shared.republish = !shared.requests().is_empty();
    Ok(events)
}

/// Sends the test pattern's pictures as they fall due, on whichever
/// connection is current, until the camera stops.
async fn send_pattern(shared: Arc<Mutex<Shared>>) {
    loop {
        let due = match lock(&shared).pattern.as_ref() {
            Some(pattern) => pattern.next_due(),
            None => return,
        };
        let wake = due.unwrap_or_else(|| Instant::now() + PATTERN_IDLE);
        tokio::time::sleep_until(wake.into()).await;
        let (frames, connection) = {
            let mut shared = lock(&shared);
            let connection = shared.connection.clone();
            match shared.pattern.as_mut() {
                Some(pattern) => (pattern.frames(Instant::now()), connection),
                None => return,
            }
        };
        if let Some(connection) = connection {
            for frame in frames {
                connection.send_video(frame);
            }
        }
    }
}

/// Publishes the camera on a connection that just came up.
fn republish(shared: &Arc<Mutex<Shared>>) {
    let (requests, connection) = {
        let mut shared = lock(shared);
        if !std::mem::take(&mut shared.republish) {
            return;
        }
        (shared.requests(), shared.connection.clone())
    };
    let Some(connection) = connection else {
        return;
    };
    for request in requests {
        let connection = Arc::clone(&connection);
        tokio::spawn(async move {
            if let Err(error) = connection.publish_track(request).await {
                eprintln!("voicebot: could not publish a track again: {error}");
            }
        });
    }
}

/// Counts a picture someone sent.
fn record_video(shared: &Mutex<Shared>, video: &opencord_media::transport::ReceivedVideo) {
    let intact = check(&video.nal_units).is_some();
    let bytes: usize = video.nal_units.iter().map(Vec::len).sum();
    let mut shared = lock(shared);
    #[cfg(target_os = "linux")]
    let decoded = shared.decoders.as_mut().map(|decoders| {
        let decoder = decoders
            .entry(video.track_id.clone())
            .or_insert_with(|| Decoder::open(false).expect("FFmpeg's decoder opens"));
        decoder
            .decode(&video.nal_units, video.arrived)
            .map_or(0, |pictures| pictures.len() as u64)
    });
    #[cfg(not(target_os = "linux"))]
    let decoded: Option<u64> = None;
    let seen = shared.seen.entry(video.user_id).or_default();
    seen.frames += 1;
    seen.intact += u64::from(intact);
    seen.decoded += decoded.unwrap_or(0);
    seen.keyframes += u64::from(video.keyframe);
    seen.bytes += bytes as u64;
    if let Some(count) = seen.layers.get_mut(usize::from(video.layer)) {
        *count += 1;
    }
}

/// Follows what the transport says about tracks: others' tracks to watch,
/// and what to encode of this bot's camera.
fn on_video_event(shared: &Mutex<Shared>, event: VoiceEvent) {
    let mut shared = lock(shared);
    match event {
        VoiceEvent::Track { user_id, track } => {
            shared
                .tracks
                .retain(|(_, known)| known.track_id != track.track_id);
            shared.tracks.push((user_id, track));
            shared.send_wants();
        }
        VoiceEvent::TrackRemoved { track_id, .. } => {
            shared
                .tracks
                .retain(|(_, known)| known.track_id != track_id);
            shared.send_wants();
        }
        VoiceEvent::Encode {
            track_id,
            layers,
            bitrates,
            fps_scale,
            size_scale,
        } => {
            if let Some(pattern) = shared.pattern.as_mut() {
                pattern.encode(&layers, fps_scale, size_scale);
            }
            #[cfg(target_os = "linux")]
            if let Some(video) = shared.encoded(&track_id) {
                video
                    .sender
                    .encode(&layers, &bitrates, fps_scale, size_scale);
            }
            #[cfg(not(target_os = "linux"))]
            let _ = (track_id, bitrates);
        }
        VoiceEvent::KeyframeRequested { track_id, layer } => {
            if let Some(pattern) = shared.pattern.as_mut() {
                pattern.keyframe(layer);
            }
            #[cfg(target_os = "linux")]
            if let Some(video) = shared.encoded(&track_id) {
                video.sender.keyframe(layer);
            }
            #[cfg(not(target_os = "linux"))]
            let _ = track_id;
        }
        VoiceEvent::TrackStopped {
            track_id, message, ..
        } => {
            eprintln!("voicebot: the node stopped {track_id}: {message}");
            #[cfg(target_os = "linux")]
            {
                let screen = shared
                    .screen
                    .as_ref()
                    .is_some_and(|screen| screen.request.track_id == track_id);
                if screen {
                    shared.screen = None;
                    return;
                }
                shared.encoded = None;
            }
            shared.pattern = None;
        }
        _ => {}
    }
}

/// Keeps the session going: records what arrives, and reconnects when the
/// server names another voice node.
async fn follow(
    shared: Arc<Mutex<Shared>>,
    mut server: VoiceServer,
    mut events: mpsc::UnboundedReceiver<VoiceEvent>,
    mut voice_servers: broadcast::Receiver<VoiceServer>,
) {
    let mut decoders: HashMap<i64, opus::Decoder> = HashMap::new();
    let mut pcm = vec![0i16; FRAME_SAMPLES * 6];
    loop {
        tokio::select! {
            event = events.recv() => match event {
                Some(VoiceEvent::MediaConnected) => {
                    lock(&shared).media_connected = true;
                    republish(&shared);
                }
                Some(VoiceEvent::MediaDisconnected) => lock(&shared).media_connected = false,
                Some(VoiceEvent::Audio(audio)) => {
                    let decoder = decoders.entry(audio.user_id).or_insert_with(|| {
                        opus::Decoder::new(SAMPLE_RATE, opus::Channels::Mono)
                            .expect("Opus decoders can be made")
                    });
                    let decoded = decoder.decode(&audio.payload, &mut pcm, false).ok();
                    let mut shared = lock(&shared);
                    let heard = shared.heard.entry(audio.user_id).or_insert_with(|| Heard {
                        peak_dbfs: -120.0,
                        ..Heard::default()
                    });
                    heard.packets += 1;
                    heard.first.get_or_insert(audio.arrived);
                    heard.last = Some(audio.arrived);
                    heard.arrivals.push(audio.arrived);
                    if let Some(samples) = decoded {
                        heard.decoded += 1;
                        heard.peak_dbfs = heard.peak_dbfs.max(dbfs(&pcm[..samples]));
                    }
                }
                Some(VoiceEvent::Closed { code }) => lock(&shared).closed = Some(code),
                Some(VoiceEvent::Video(video)) => record_video(&shared, &video),
                Some(other) => on_video_event(&shared, other),
                None => {
                    // This connection is over; a new voice server may follow.
                    events = mpsc::unbounded_channel().1;
                }
            },
            next = voice_servers.recv() => match next {
                Ok(next) if next.server_key == server.server_key => {
                    if let Some(old) = lock(&shared).connection.take() {
                        old.close();
                    }
                    match connect(&shared, &next).await {
                        Ok(new_events) => {
                            events = new_events;
                            server = next;
                        }
                        Err(error) => eprintln!("voicebot: could not reconnect: {error:#}"),
                    }
                }
                Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => return,
            },
        }
    }
}

fn lock(shared: &Mutex<Shared>) -> MutexGuard<'_, Shared> {
    shared.lock().unwrap_or_else(PoisonError::into_inner)
}

/// RMS level in dBFS.
pub fn dbfs(pcm: &[i16]) -> f32 {
    if pcm.is_empty() {
        return -120.0;
    }
    let sum: f64 = pcm.iter().map(|s| f64::from(*s).powi(2)).sum();
    let rms = (sum / pcm.len() as f64).sqrt() / f64::from(i16::MAX);
    if rms <= 0.0 {
        -120.0
    } else {
        (20.0 * rms.log10()) as f32
    }
}

/// The RFC 6464 audio level: -dBov from 0 (loudest) to -127.
fn level_dbov(pcm: &[i16]) -> i8 {
    dbfs(pcm).clamp(-127.0, 0.0) as i8
}

/// The voicebot's camera of real H.264.
#[cfg(target_os = "linux")]
mod camera {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};
    use std::sync::{Arc, Mutex};
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};

    use opencord_media::transport::{TrackKind, TrackRequest};
    use opencord_media::video::camera::{RawFormat, RawFrame};
    use opencord_media::video::layers::{ScreenShape, camera_layers, screen_layers};
    use opencord_media::video::pattern::scene;
    use opencord_media::video::sender::VideoSender;

    use super::{Shared, lock};

    /// A screen share's source: a 1080p screen.
    const SCREEN: (u32, u32) = (1920, 1080);

    /// Real H.264 of the moving scene, sent as a camera (1280×720 at
    /// 30 fps) or a screen (1920×1080 at the share's frame rate).
    pub(super) struct EncodedVideo {
        pub request: TrackRequest,
        pub sender: VideoSender,
        stop: Arc<AtomicBool>,
        feeder: Option<JoinHandle<()>>,
    }

    impl EncodedVideo {
        pub fn camera(track_id: &str, shared: Arc<Mutex<Shared>>) -> anyhow::Result<Self> {
            let (width, height, fps) = (1280, 720, 30);
            let layers = camera_layers(width, height);
            let (frames, captured) = sync_channel(4);
            let sender = VideoSender::camera(
                track_id.to_owned(),
                layers.clone(),
                captured,
                sending(shared),
                |_| {},
            );
            Self::start(
                track_id,
                TrackKind::Camera,
                layers,
                sender,
                frames,
                (width, height, fps),
            )
        }

        pub fn screen(
            track_id: &str,
            shape: ScreenShape,
            shared: Arc<Mutex<Shared>>,
        ) -> anyhow::Result<Self> {
            let (width, height) = SCREEN;
            let layers = screen_layers(width, height, shape);
            let (frames, captured) = sync_channel(4);
            let sender = VideoSender::screen(
                track_id.to_owned(),
                layers.clone(),
                shape,
                captured,
                sending(shared),
                |_| {},
            );
            Self::start(
                track_id,
                TrackKind::Screen,
                layers,
                sender,
                frames,
                (width, height, shape.fps),
            )
        }

        fn start(
            track_id: &str,
            kind: TrackKind,
            layers: Vec<opencord_media::transport::Layer>,
            sender: VideoSender,
            frames: SyncSender<RawFrame>,
            size: (u32, u32, u32),
        ) -> anyhow::Result<Self> {
            let stop = Arc::new(AtomicBool::new(false));
            let feeder = std::thread::Builder::new()
                .name("voicebot-video".to_owned())
                .spawn({
                    let stop = Arc::clone(&stop);
                    move || feed(&frames, &stop, size)
                })?;
            Ok(Self {
                request: TrackRequest {
                    track_id: track_id.to_owned(),
                    kind,
                    layers,
                },
                sender,
                stop,
                feeder: Some(feeder),
            })
        }
    }

    impl Drop for EncodedVideo {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(feeder) = self.feeder.take() {
                let _ = feeder.join();
            }
        }
    }

    fn sending(
        shared: Arc<Mutex<Shared>>,
    ) -> impl FnMut(opencord_media::transport::VideoFrame) + Send + 'static {
        move |frame| {
            let connection = lock(&shared).connection.clone();
            if let Some(connection) = connection {
                connection.send_video(frame);
            }
        }
    }

    /// The moving scene at its size and frame rate, as a camera or a
    /// screen would deliver it.
    fn feed(
        frames: &SyncSender<RawFrame>,
        stop: &AtomicBool,
        (width, height, fps): (u32, u32, u32),
    ) {
        let frame_time = Duration::from_secs(1) / fps.max(1);
        let start = Instant::now();
        let mut number = 0u64;
        while !stop.load(Ordering::Relaxed) {
            let picture = scene(width, height, number, Instant::now());
            let frame = RawFrame {
                format: RawFormat::Nv12,
                width,
                height,
                data: picture.data,
                captured: picture.captured,
            };
            if let Err(TrySendError::Disconnected(_)) = frames.try_send(frame) {
                return;
            }
            number += 1;
            let due = start + frame_time * u32::try_from(number).unwrap_or(u32::MAX);
            std::thread::sleep(due.saturating_duration_since(Instant::now()));
        }
    }
}
