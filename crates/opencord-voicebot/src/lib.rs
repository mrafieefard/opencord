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
    AddServerOutcome, ChannelKind, CoreEvent, CoreEventPayload, ReadySnapshot,
};
use opencord_core::client::Client;
use opencord_core::identity::Identity;
use opencord_core::voice::VoiceServer;
use opencord_media::transport::{
    AudioFrame, Layer, RemoteTrack, SinkWant, TrackKind, VoiceConnection, VoiceEvent, VoiceTarget,
};
use opencord_media::video::pattern::{TestPattern, check};
use tokio::sync::{broadcast, mpsc};

pub const SAMPLE_RATE: u32 = 48_000;
/// 20 ms at 48 kHz.
pub const FRAME_SAMPLES: usize = 960;
const FRAME: Duration = Duration::from_millis(20);
const WAIT: Duration = Duration::from_secs(15);
/// How often an idle test pattern looks again for layers to send.
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
        let server_key = match client.add_server(server, claim_token.clone()).await? {
            AddServerOutcome::Added(added) => added.key,
            AddServerOutcome::NeedsTrust {
                address,
                fingerprint,
            } => {
                client.trust_fingerprint(&address, &fingerprint)?;
                match client.add_server(server, claim_token).await? {
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
        self.client.voice_join(&self.server_key, channel_id).await?;
        let server = next_voice_server(&mut voice_servers, &self.server_key).await?;
        VoiceSession::start(server, voice_servers).await
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
    republish: bool,
    /// Others' tracks, and the tile height to watch them at.
    tracks: Vec<(i64, RemoteTrack)>,
    watch_height: Option<u32>,
}

impl Shared {
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

    /// Watches everyone's tracks in tiles `height` pixels tall.
    pub fn watch(&self, height: u32) {
        let mut shared = lock(&self.shared);
        shared.watch_height = Some(height);
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
    shared.republish = shared.pattern.is_some();
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
    let (request, connection) = {
        let mut shared = lock(shared);
        if !std::mem::take(&mut shared.republish) {
            return;
        }
        let request = shared
            .pattern
            .as_ref()
            .map(|pattern| pattern.request(TrackKind::Camera));
        (request, shared.connection.clone())
    };
    if let (Some(request), Some(connection)) = (request, connection) {
        tokio::spawn(async move {
            if let Err(error) = connection.publish_track(request).await {
                eprintln!("voicebot: could not publish the camera again: {error}");
            }
        });
    }
}

/// Counts a picture someone sent.
fn record_video(shared: &Mutex<Shared>, video: &opencord_media::transport::ReceivedVideo) {
    let intact = check(&video.nal_units).is_some();
    let bytes: usize = video.nal_units.iter().map(Vec::len).sum();
    let mut shared = lock(shared);
    let seen = shared.seen.entry(video.user_id).or_default();
    seen.frames += 1;
    seen.intact += u64::from(intact);
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
            layers,
            fps_scale,
            size_scale,
            ..
        } => {
            if let Some(pattern) = shared.pattern.as_mut() {
                pattern.encode(&layers, fps_scale, size_scale);
            }
        }
        VoiceEvent::KeyframeRequested { layer, .. } => {
            if let Some(pattern) = shared.pattern.as_mut() {
                pattern.keyframe(layer);
            }
        }
        VoiceEvent::TrackStopped { message, .. } => {
            eprintln!("voicebot: the node stopped the camera: {message}");
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
