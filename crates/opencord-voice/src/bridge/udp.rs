//! The bridge's UDP port (Phase 3 plan §13.3–13.4): IP discovery for
//! connections that finished Identify, then their audio. A packet counts
//! only from the address its connection discovered from, under its own
//! SSRC, and only if its key opens it. The port may differ from the one
//! discovery came from: Lavalink sends audio from a socket of its own.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

use tokio::net::UdpSocket;

use super::rtp::{self, MAX_DATAGRAM};
use super::{BridgeEvent, MusicBridge, MusicFrame, MusicPacket};

/// Housekeeping: expired players and connections not resumed.
const TICK: Duration = Duration::from_secs(1);

pub(super) struct Sockets {
    v4: UdpSocket,
    v6: Option<UdpSocket>,
    pub port: u16,
}

impl Sockets {
    pub async fn bind(port: u16) -> std::io::Result<Self> {
        let v4 = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port)).await?;
        let port = v4.local_addr()?.port();
        // IPv6 is a bonus: hosts without it still take audio over IPv4.
        let v6 = crate::bind_v6_only(port).ok();
        Ok(Self { v4, v6, port })
    }
}

pub(super) async fn run(bridge: MusicBridge, sockets: Sockets) {
    let mut stop = bridge.config.stop.subscribe();
    let mut v4_buffer = [0u8; MAX_DATAGRAM + 1];
    let mut v6_buffer = [0u8; MAX_DATAGRAM + 1];
    let mut tick = tokio::time::interval(TICK);
    loop {
        tokio::select! {
            received = sockets.v4.recv_from(&mut v4_buffer) => {
                if let Ok((size, from)) = received {
                    bridge.receive(&sockets.v4, &v4_buffer[..size], from);
                }
            }
            received = recv_maybe(sockets.v6.as_ref(), &mut v6_buffer) => {
                if let (Ok((size, from)), Some(v6)) = (received, sockets.v6.as_ref()) {
                    bridge.receive(v6, &v6_buffer[..size], from);
                }
            }
            _ = tick.tick() => bridge.expire(Instant::now()),
            _ = stop.changed() => return,
        }
    }
}

async fn recv_maybe(
    socket: Option<&UdpSocket>,
    buffer: &mut [u8],
) -> std::io::Result<(usize, SocketAddr)> {
    match socket {
        Some(socket) => socket.recv_from(buffer).await,
        None => std::future::pending().await,
    }
}

impl MusicBridge {
    fn receive(&self, socket: &UdpSocket, data: &[u8], from: SocketAddr) {
        let at = Instant::now();
        if data.len() > MAX_DATAGRAM {
            self.lock().stats.dropped += 1;
            return;
        }
        let ip = from.ip().to_canonical();
        if let Some(ssrc) = rtp::discovery_request(data) {
            if self.discovered(ssrc, ip) {
                let _ = socket.try_send_to(&rtp::discovery_response(ssrc, from), from);
            }
            return;
        }
        // RTP: version 2, and an SSRC to look up.
        if data.len() < 12 || data[0] >> 6 != 2 {
            return;
        }
        let ssrc = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
        let found = {
            let mut shared = self.lock();
            let found = shared
                .by_ssrc
                .get(&ssrc)
                .and_then(|channel| shared.live.get(channel))
                .filter(|live| live.discovered == Some(ip))
                .and_then(|live| {
                    let opener = std::sync::Arc::clone(live.media.as_ref()?);
                    Some((live.player.channel_id, live.connection, opener))
                });
            if found.is_none() {
                shared.stats.dropped += 1;
            }
            found
        };
        let Some((channel_id, connection, opener)) = found else {
            return;
        };
        let Some(opened) = opener.open(data).filter(|opened| opened.ssrc == ssrc) else {
            self.lock().stats.decrypt_failures += 1;
            return;
        };
        let frame = MusicFrame {
            channel_id,
            packet: MusicPacket {
                connection,
                sequence: opened.sequence,
                timestamp: opened.timestamp,
                opus: opened.opus.into(),
                received_at: at,
            },
        };
        let timestamp = frame.packet.timestamp;
        let sent = self.config.frames.try_send(frame).is_ok();
        let mut shared = self.lock();
        let mut started = false;
        if let Some(live) = shared.live.get_mut(&channel_id) {
            if !live.heard {
                live.heard = true;
                tracing::debug!(channel_id, %from, "music audio from Lavalink");
            }
            started = !live.flowing;
            live.flowing = true;
            live.last_audio = Some(at);
            live.jitter_after(at, timestamp, started);
        }
        if sent {
            shared.stats.packets += 1;
        } else {
            shared.stats.dropped += 1;
        }
        drop(shared);
        if started {
            let _ = self.config.events.send(BridgeEvent::Audio { channel_id });
        }
    }

    /// IP discovery for `ssrc` from `ip`: allowed for a connection that
    /// finished Identify, from an address it may use. It is where its
    /// audio must then come from.
    fn discovered(&self, ssrc: u32, ip: IpAddr) -> bool {
        let mut shared = self.lock();
        let Some(channel) = shared.by_ssrc.get(&ssrc).copied() else {
            return false;
        };
        let Some(live) = shared.live.get_mut(&channel) else {
            return false;
        };
        let allowed = live
            .player
            .allowed
            .as_ref()
            .is_none_or(|allowed| allowed.contains(&ip));
        let first = allowed && live.discovered.is_none();
        if allowed {
            live.discovered = Some(ip);
        }
        drop(shared);
        if first {
            let _ = self.config.events.send(BridgeEvent::Discovered {
                channel_id: channel,
                from: ip,
            });
        }
        allowed
    }
}
