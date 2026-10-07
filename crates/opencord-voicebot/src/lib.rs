//! A headless voice client (Phase 2 plan §15): joins a server and a voice
//! channel through the same core as the app, sends a tone as Opus, and
//! reports what it hears from whom. Used by the integration tests, load
//! tests, and self-hosters checking their voice setup.

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
use opencord_media::transport::{AudioFrame, VoiceConnection, VoiceEvent, VoiceTarget};
use tokio::sync::{broadcast, mpsc};

pub const SAMPLE_RATE: u32 = 48_000;
/// 20 ms at 48 kHz.
pub const FRAME_SAMPLES: usize = 960;
const FRAME: Duration = Duration::from_millis(20);
const WAIT: Duration = Duration::from_secs(15);

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
}

/// A bot in a voice channel.
pub struct VoiceSession {
    shared: Arc<Mutex<Shared>>,
}

impl VoiceSession {
    async fn start(
        server: VoiceServer,
        voice_servers: broadcast::Receiver<VoiceServer>,
    ) -> anyhow::Result<Self> {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let events = connect(&shared, &server).await?;
        tokio::spawn(follow(Arc::clone(&shared), server, events, voice_servers));
        let session = Self { shared };
        session.wait_until(|shared| shared.media_connected).await?;
        Ok(session)
    }

    /// Sends a sine tone for `duration`, a 20 ms frame at a time.
    pub fn play_tone(&self, frequency: f32, duration: Duration) -> tokio::task::JoinHandle<()> {
        let shared = Arc::clone(&self.shared);
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
                        samples: FRAME_SAMPLES as u32,
                        audio_level: level_dbov(&pcm),
                        voice_activity: true,
                    });
                }
            }
        })
    }

    /// What has been heard so far, by user id.
    pub fn heard(&self) -> HashMap<i64, Heard> {
        lock(&self.shared).heard.clone()
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
    pub closed: Option<Option<u16>>,
}

async fn connect(
    shared: &Arc<Mutex<Shared>>,
    server: &VoiceServer,
) -> anyhow::Result<mpsc::UnboundedReceiver<VoiceEvent>> {
    let fingerprint: Option<[u8; 32]> = server.certificate_fingerprint.as_slice().try_into().ok();
    let media_host = server
        .server_key
        .rsplit_once(':')
        .map_or(server.server_key.as_str(), |(host, _)| host)
        .trim_matches(['[', ']'])
        .to_owned();
    let (connection, events) = VoiceConnection::connect(VoiceTarget {
        gateway_url: server.gateway_url(),
        certificate_fingerprint: fingerprint,
        media_host,
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
    Ok(events)
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
                Some(VoiceEvent::MediaConnected) => lock(&shared).media_connected = true,
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
                Some(_) => {}
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
