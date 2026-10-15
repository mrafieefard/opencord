//! The audio bridge (Phase 3 plan §11, §13): a stand-in for Discord's voice
//! servers, so an unmodified Lavalink plays into Opencord. Lavalink signs
//! in over a WebSocket on the main TLS port, speaking Discord's voice
//! protocol (v8), and sends Opus over UDP; the bridge decrypts it and hands
//! the frames on, unchanged, as a channel's music stream.
//!
//! The main server announces each Lavalink player before Lavalink connects
//! ([`MusicBridge::expect`]): its channel, a single-use token and session
//! id. Nothing else gets in.

use std::collections::{HashMap, VecDeque};
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::sfu::MusicPacket;
use tokio::sync::{mpsc, watch};

mod gateway;
pub mod rtp;
mod udp;

use rtp::Opener;

/// How long an announced player waits for Lavalink to connect.
pub const EXPECT_FOR: Duration = Duration::from_secs(60);
/// How long a connection whose WebSocket dropped can resume.
pub const RESUME_FOR: Duration = Duration::from_secs(60);
/// What Lavalink is told to heartbeat at (Discord's value is similar).
pub const HEARTBEAT: Duration = Duration::from_millis(13_750);
/// Connection attempts allowed from one address per [`ATTEMPT_PERIOD`].
const ATTEMPTS: u32 = 20;
const ATTEMPT_PERIOD: Duration = Duration::from_secs(10);
/// Frames waiting for the SFU; more are dropped, as a late frame would be.
const FRAME_QUEUE: usize = 1024;

#[derive(Debug, Clone)]
pub struct BridgeConfig {
    /// UDP port Lavalink sends audio to; 0 picks a free one.
    pub udp_port: u16,
    /// Where Lavalink nodes outside this machine and its network send audio:
    /// the server's public address.
    pub public_ip: Option<IpAddr>,
    pub heartbeat: Duration,
}

/// A Lavalink player the main server set up and the bridge should let in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedPlayer {
    pub channel_id: i64,
    /// The internal Lavalink user id (plan §12.3).
    pub user_id: i64,
    pub token: String,
    pub session_id: String,
    /// With the node's strict address check, the only addresses it may
    /// connect and send from.
    pub allowed: Option<Vec<IpAddr>>,
}

/// One Opus frame of a channel's music, for its voice node.
#[derive(Debug, Clone)]
pub struct MusicFrame {
    pub channel_id: i64,
    pub packet: MusicPacket,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BridgeEvent {
    /// Lavalink finished the handshake for a channel's player.
    Connected { channel_id: i64, connection: u64 },
    /// Its connection ended for good: refused, closed or not resumed.
    Closed { channel_id: i64, code: u16 },
}

/// What the bridge counts (plan §13.3, metrics).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BridgeStats {
    pub connections: u32,
    pub packets: u64,
    pub decrypt_failures: u64,
    pub dropped: u64,
}

/// Discord's voice close codes, as Lavalink reports them.
pub mod close {
    pub const NOT_AUTHENTICATED: u16 = 4003;
    pub const AUTHENTICATION_FAILED: u16 = 4004;
    pub const ALREADY_AUTHENTICATED: u16 = 4005;
    pub const SESSION_NO_LONGER_VALID: u16 = 4006;
    pub const SESSION_TIMEOUT: u16 = 4009;
    pub const UNKNOWN_PROTOCOL: u16 = 4012;
    pub const DISCONNECTED: u16 = 4014;
    pub const UNKNOWN_ENCRYPTION_MODE: u16 = 4016;
    pub const BAD_REQUEST: u16 = 4020;
    pub const RATE_LIMITED: u16 = 4021;
}

/// The running bridge. Cheap to clone.
#[derive(Clone)]
pub struct MusicBridge {
    shared: Arc<Mutex<Shared>>,
    config: Arc<Settings>,
}

impl std::fmt::Debug for MusicBridge {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MusicBridge")
            .field("udp_port", &self.config.udp_port)
            .finish_non_exhaustive()
    }
}

struct Settings {
    udp_port: u16,
    public_ip: Mutex<Option<IpAddr>>,
    heartbeat: Duration,
    frames: mpsc::Sender<MusicFrame>,
    events: mpsc::UnboundedSender<BridgeEvent>,
    stop: watch::Sender<bool>,
}

#[derive(Default)]
struct Shared {
    expected: HashMap<i64, (ExpectedPlayer, Instant)>,
    live: HashMap<i64, Live>,
    by_ssrc: HashMap<u32, i64>,
    attempts: HashMap<IpAddr, (Instant, u32)>,
    next_connection: u64,
    stats: BridgeStats,
}

/// A Lavalink connection that finished Identify.
struct Live {
    connection: u64,
    player: ExpectedPlayer,
    ssrc: u32,
    /// The key Select Protocol made.
    media: Option<Arc<Opener>>,
    /// Where IP discovery came from: audio must come from this address.
    discovered: Option<IpAddr>,
    /// Whether audio came yet, to log where from once.
    heard: bool,
    /// The WebSocket now attached; `None` while it is away.
    socket: Option<Attached>,
    detached_at: Option<Instant>,
    /// Sequence numbers of what was sent, for v8's buffered resume.
    sent: VecDeque<(u32, String)>,
    seq: u32,
}

#[derive(Clone)]
struct Attached {
    id: u64,
    outbound: mpsc::Sender<gateway::Outbound>,
}

/// Messages kept for a resume; Lavalink acknowledges within a heartbeat.
const RESUME_BUFFER: usize = 64;

impl MusicBridge {
    /// Binds the audio port and starts the bridge. Frames and events come
    /// out of the receivers.
    pub async fn start(
        config: BridgeConfig,
    ) -> std::io::Result<(
        Self,
        mpsc::Receiver<MusicFrame>,
        mpsc::UnboundedReceiver<BridgeEvent>,
    )> {
        let sockets = udp::Sockets::bind(config.udp_port).await?;
        let (frames_tx, frames) = mpsc::channel(FRAME_QUEUE);
        let (events_tx, events) = mpsc::unbounded_channel();
        let bridge = Self {
            shared: Arc::default(),
            config: Arc::new(Settings {
                udp_port: sockets.port,
                public_ip: Mutex::new(config.public_ip),
                heartbeat: config.heartbeat,
                frames: frames_tx,
                events: events_tx,
                stop: watch::channel(false).0,
            }),
        };
        tokio::spawn(udp::run(bridge.clone(), sockets));
        Ok((bridge, frames, events))
    }

    pub fn udp_port(&self) -> u16 {
        self.config.udp_port
    }

    /// Lets a player's Lavalink in, once, within [`EXPECT_FOR`]. A
    /// connection already playing for the channel is ended: its token is
    /// revoked, and late audio from it is refused.
    pub fn expect(&self, player: ExpectedPlayer) {
        let channel_id = player.channel_id;
        let mut shared = self.lock();
        let ended = shared.remove_live(channel_id);
        shared
            .expected
            .insert(channel_id, (player, Instant::now() + EXPECT_FOR));
        drop(shared);
        self.ended(channel_id, ended, close::SESSION_NO_LONGER_VALID);
    }

    /// Forgets a channel's player: nothing more is accepted for it.
    pub fn cancel(&self, channel_id: i64) {
        let mut shared = self.lock();
        shared.expected.remove(&channel_id);
        let ended = shared.remove_live(channel_id);
        drop(shared);
        self.ended(channel_id, ended, close::DISCONNECTED);
    }

    /// Where Lavalink nodes outside this machine and its network send
    /// audio, as the server's public address changes.
    pub fn set_public_ip(&self, ip: Option<IpAddr>) {
        *self
            .config
            .public_ip
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = ip;
    }

    pub fn public_ip(&self) -> Option<IpAddr> {
        *self
            .config
            .public_ip
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Stops listening and ends every connection.
    pub fn shutdown(&self) {
        let _ = self.config.stop.send(true);
        let channels: Vec<i64> = self.lock().live.keys().copied().collect();
        for channel_id in channels {
            self.cancel(channel_id);
        }
    }

    /// Closes a channel's WebSocket as a missed heartbeat would, leaving
    /// the connection to resume. For tests of resuming.
    #[doc(hidden)]
    pub fn drop_socket(&self, channel_id: i64) {
        let socket = self
            .lock()
            .live
            .get(&channel_id)
            .and_then(|live| live.socket.clone());
        if let Some(socket) = socket {
            let _ = socket
                .outbound
                .try_send(gateway::Outbound::Close(close::SESSION_TIMEOUT));
        }
    }

    pub fn stats(&self) -> BridgeStats {
        let shared = self.lock();
        BridgeStats {
            connections: u32::try_from(shared.live.len()).unwrap_or(u32::MAX),
            ..shared.stats
        }
    }

    /// Serves one WebSocket from Lavalink until it closes. `peer` is where
    /// it came from (a trusted proxy's `X-Forwarded-For`, when behind one).
    pub async fn serve(&self, socket: axum::extract::ws::WebSocket, peer: SocketAddr) {
        gateway::serve(self.clone(), socket, peer).await;
    }

    fn lock(&self) -> MutexGuard<'_, Shared> {
        self.shared
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Closes the WebSocket of a connection taken away, and reports it.
    fn ended(&self, channel_id: i64, ended: Option<Live>, code: u16) {
        let Some(live) = ended else {
            return;
        };
        if let Some(socket) = live.socket {
            let _ = socket.outbound.try_send(gateway::Outbound::Close(code));
        }
        let _ = self
            .config
            .events
            .send(BridgeEvent::Closed { channel_id, code });
    }

    /// Ends connections whose WebSocket did not come back in time, and
    /// players never connected.
    fn expire(&self, now: Instant) {
        let mut shared = self.lock();
        shared.expected.retain(|_, (_, until)| *until > now);
        let gone: Vec<i64> = shared
            .live
            .iter()
            .filter(|(_, live)| live.detached_at.is_some_and(|at| now - at > RESUME_FOR))
            .map(|(channel, _)| *channel)
            .collect();
        let ended: Vec<(i64, Option<Live>)> = gone
            .into_iter()
            .map(|channel| (channel, shared.remove_live(channel)))
            .collect();
        shared
            .attempts
            .retain(|_, (since, _)| now - *since < ATTEMPT_PERIOD);
        drop(shared);
        for (channel, live) in ended {
            self.ended(channel, live, close::SESSION_TIMEOUT);
        }
    }

    /// Whether `ip` may try again: a few attempts per period each.
    fn attempt(&self, ip: IpAddr) -> bool {
        let now = Instant::now();
        let mut shared = self.lock();
        let entry = shared.attempts.entry(ip).or_insert((now, 0));
        if now - entry.0 >= ATTEMPT_PERIOD {
            *entry = (now, 0);
        }
        entry.1 += 1;
        entry.1 <= ATTEMPTS
    }
}

impl Shared {
    fn remove_live(&mut self, channel_id: i64) -> Option<Live> {
        let live = self.live.remove(&channel_id)?;
        self.by_ssrc.remove(&live.ssrc);
        Some(live)
    }
}

/// Whether `ip` is on this machine or a private network, where the
/// server's public address may not be reachable (no hairpin NAT).
fn is_local(ip: IpAddr) -> bool {
    match ip.to_canonical() {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                // Shared address space (RFC 6598), as CGNAT and Tailscale use.
                || (v4.octets()[0] == 100 && v4.octets()[1] & 0xc0 == 64)
        }
        IpAddr::V6(v6) => v6.is_loopback() || v6.is_unique_local() || v6.is_unicast_link_local(),
    }
}

/// This machine's address on the route to `peer`, as the kernel picks it.
fn local_address_towards(peer: IpAddr) -> Option<IpAddr> {
    let unspecified: IpAddr = match peer {
        IpAddr::V4(_) => std::net::Ipv4Addr::UNSPECIFIED.into(),
        IpAddr::V6(_) => std::net::Ipv6Addr::UNSPECIFIED.into(),
    };
    let socket = std::net::UdpSocket::bind((unspecified, 0)).ok()?;
    socket.connect((peer, 9)).ok()?;
    Some(socket.local_addr().ok()?.ip())
}

#[cfg(test)]
mod tests;
