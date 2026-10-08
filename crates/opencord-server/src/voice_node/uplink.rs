//! The node's end of the control channel (Phase 2 plan §3.4): register
//! with the shared secret, apply the main server's commands, report ended
//! voice connections and the load every 5 seconds. Whenever the channel
//! drops, every call on the node ends (the main server sends people
//! elsewhere) and the node registers again.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, anyhow, bail};
use ed25519_dalek::VerifyingKey;
use futures_util::{SinkExt, StreamExt};
use internal::control_envelope::Payload;
use opencord_common::address::{Fingerprint, ServerAddress};
use opencord_proto::internal::v1 as internal;
use opencord_voice::control::{self, CONTROL_PATH, LOAD_EVERY, SILENT_FOR};
use opencord_voice::node::{NodeEvent, NodeLoad, VoiceNode};
use prost::Message as _;
use rustls::pki_types::ServerName;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;
use tokio_rustls::TlsConnector;
use tokio_rustls::client::TlsStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

use crate::tls;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const RECONNECT_MIN: Duration = Duration::from_millis(500);
const RECONNECT_MAX: Duration = Duration::from_secs(30);
const FRAME_BYTES: usize = 64 * 1024;

type Socket = WebSocketStream<TlsStream<TcpStream>>;

/// Who this node is to the main server, and how to reach it.
#[derive(Debug, Clone)]
pub struct Uplink {
    pub main_server: ServerAddress,
    /// `None` trusts the public certificate authorities.
    pub main_fingerprint: Option<Fingerprint>,
    pub secret: Vec<u8>,
    pub endpoint: String,
    pub certificate_fingerprint: Fingerprint,
    /// Empty for "the endpoint's host".
    pub public_address: String,
    pub udp_port: u16,
}

/// Keeps the node registered with the main server, for as long as it runs.
pub async fn run(
    uplink: Uplink,
    node: VoiceNode,
    mut events: mpsc::UnboundedReceiver<NodeEvent>,
    registered: watch::Sender<bool>,
) {
    let mut pause = RECONNECT_MIN;
    loop {
        let ended = session(&uplink, &node, &mut events, &registered).await;
        let was_registered = registered.send_replace(false);
        if events.is_closed() {
            tracing::error!("the voice node stopped; leaving the main server");
            return;
        }
        node.suspend();
        // Reports from before the main server forgot this node are stale.
        while events.try_recv().is_ok() {}
        if was_registered {
            tracing::warn!("lost the main server, registering again: {ended:#}");
            pause = RECONNECT_MIN;
        } else {
            tracing::warn!(
                "could not register with the main server, trying again in {pause:?}: {ended:#}"
            );
        }
        tokio::time::sleep(pause).await;
        if !was_registered {
            pause = (pause * 2).min(RECONNECT_MAX);
        }
    }
}

/// One control channel, from connecting to its end; returns why it ended.
async fn session(
    uplink: &Uplink,
    node: &VoiceNode,
    events: &mut mpsc::UnboundedReceiver<NodeEvent>,
    registered: &watch::Sender<bool>,
) -> anyhow::Error {
    let mut socket = match connect(uplink).await {
        Ok(socket) => socket,
        Err(error) => return error,
    };
    let key = match register(uplink, &mut socket).await {
        Ok(key) => key,
        Err(error) => return error,
    };
    node.set_verifying_key(key);
    registered.send_replace(true);
    tracing::info!(main_server = %uplink.main_server, "registered with the main server");
    serve(&mut socket, node, events).await
}

async fn connect(uplink: &Uplink) -> anyhow::Result<Socket> {
    let address = &uplink.main_server;
    let tcp = tokio::time::timeout(
        CONNECT_TIMEOUT,
        TcpStream::connect((address.host.as_str(), address.port)),
    )
    .await
    .context("timed out connecting")?
    .with_context(|| format!("could not connect to {address}"))?;
    let _ = tcp.set_nodelay(true);
    let config = tls::client_config(uplink.main_fingerprint)?;
    let name = ServerName::try_from(address.host.clone())
        .map_err(|_| anyhow!("{} is not a valid host name", address.host))?;
    let stream = tokio::time::timeout(
        CONNECT_TIMEOUT,
        TlsConnector::from(Arc::new(config)).connect(name, tcp),
    )
    .await
    .context("timed out during the TLS handshake")?
    .context("the TLS handshake failed")?;
    let url = format!("wss://{address}{CONTROL_PATH}");
    let config = WebSocketConfig::default()
        .max_message_size(Some(FRAME_BYTES))
        .max_frame_size(Some(FRAME_BYTES));
    let (socket, _) = tokio::time::timeout(
        CONNECT_TIMEOUT,
        tokio_tungstenite::client_async_with_config(url, stream, Some(config)),
    )
    .await
    .context("timed out opening the control channel")?
    .context("could not open the control channel")?;
    Ok(socket)
}

/// Answers the challenge and registers; returns the voice-signing key.
async fn register(uplink: &Uplink, socket: &mut Socket) -> anyhow::Result<VerifyingKey> {
    let challenge = match handshake_message(socket).await? {
        Payload::Challenge(challenge) => challenge,
        other => bail!("expected a challenge, got {other:?}"),
    };
    let register = internal::Register {
        proof: control::proof(&uplink.secret, &challenge.nonce),
        endpoint: uplink.endpoint.clone(),
        certificate_fingerprint: uplink.certificate_fingerprint.to_vec(),
        public_address: uplink.public_address.clone(),
        udp_port: u32::from(uplink.udp_port),
    };
    send(socket, Payload::Register(register)).await?;
    let registered = loop {
        match handshake_message(socket).await? {
            Payload::Registered(registered) => break registered,
            Payload::Error(error) => tracing::error!(
                "the main server refused this node: {} (check node.endpoint and the shared secret)",
                error.message
            ),
            other => bail!("expected Registered, got {other:?}"),
        }
    };
    let key: [u8; 32] = registered
        .voice_signing_public_key
        .as_slice()
        .try_into()
        .context("the main server sent a voice key of the wrong size")?;
    VerifyingKey::from_bytes(&key).context("the main server sent an invalid voice key")
}

async fn handshake_message(socket: &mut Socket) -> anyhow::Result<Payload> {
    tokio::time::timeout(CONNECT_TIMEOUT, recv(socket))
        .await
        .context("the main server did not answer in time")?
}

/// Next message; an error once the channel is closed.
async fn recv(socket: &mut Socket) -> anyhow::Result<Payload> {
    loop {
        match socket.next().await {
            Some(Ok(Message::Binary(bytes))) => {
                return internal::ControlEnvelope::decode(bytes)
                    .context("the main server sent an unreadable message")?
                    .payload
                    .context("the main server sent an empty message");
            }
            Some(Ok(Message::Close(frame))) => {
                let code = frame.map(|frame| u16::from(frame.code));
                bail!("the main server closed the control channel (close {code:?})")
            }
            Some(Ok(_)) => {}
            Some(Err(error)) => return Err(error.into()),
            None => bail!("the control channel closed"),
        }
    }
}

/// Takes commands and sends reports until the channel ends.
async fn serve(
    socket: &mut Socket,
    node: &VoiceNode,
    events: &mut mpsc::UnboundedReceiver<NodeEvent>,
) -> anyhow::Error {
    let mut reports = tokio::time::interval(LOAD_EVERY);
    let mut last = (Instant::now(), node.load());
    let mut heard_at = Instant::now();
    loop {
        let result = tokio::select! {
            event = events.recv() => match event {
                Some(event) => send_envelope(socket, &control::envelope_for_event(&event)).await,
                None => Err(anyhow!("the voice node stopped")),
            },
            _ = reports.tick() => {
                if heard_at.elapsed() > SILENT_FOR {
                    return anyhow!("the main server went quiet");
                }
                let now = (Instant::now(), node.load());
                let report = load_report(&last, &now);
                last = now;
                // The ping's pong tells this node the main server is there.
                match send(socket, Payload::LoadReport(report)).await {
                    Ok(()) => write(socket, Message::Ping(Vec::new().into())).await,
                    Err(error) => Err(error),
                }
            }
            message = socket.next() => {
                heard_at = Instant::now();
                match message {
                    Some(Ok(Message::Binary(bytes))) => apply(node, &bytes),
                    Some(Ok(Message::Close(frame))) => {
                        let code = frame.map(|frame| u16::from(frame.code));
                        Err(anyhow!("the main server closed the control channel (close {code:?})"))
                    }
                    Some(Ok(_)) => Ok(()),
                    Some(Err(error)) => Err(error.into()),
                    None => Err(anyhow!("the control channel closed")),
                }
            }
        };
        if let Err(error) = result {
            return error;
        }
    }
}

/// A command from the main server.
fn apply(node: &VoiceNode, bytes: &[u8]) -> anyhow::Result<()> {
    let envelope = internal::ControlEnvelope::decode(bytes)
        .context("the main server sent an unreadable message")?;
    match &envelope.payload {
        // Limits arrive per channel; nothing else in the settings matters here yet.
        Some(Payload::SettingsUpdate(_)) => {}
        Some(Payload::Error(error)) => {
            tracing::warn!(message = %error.message, "the main server reported an error");
        }
        _ => {
            let command = control::command_from(envelope)
                .context("the main server sent a message nodes do not take")?;
            node.send(command);
        }
    }
    Ok(())
}

/// The load between two readings of the node's counters.
fn load_report(
    (then, before): &(Instant, NodeLoad),
    (now, after): &(Instant, NodeLoad),
) -> internal::LoadReport {
    let seconds = now
        .saturating_duration_since(*then)
        .as_secs_f64()
        .max(0.001);
    let per_second = |delta: u64| (delta as f64 / seconds).round() as u64;
    let packets = (after.packets_in + after.packets_out)
        .saturating_sub(before.packets_in + before.packets_out);
    internal::LoadReport {
        // Measured with the load tests (V10).
        cpu_percent: 0.0,
        channels: after.channels,
        participants: after.participants,
        packets_per_second: per_second(packets),
        bitrate_in: per_second(after.bytes_in.saturating_sub(before.bytes_in) * 8),
        bitrate_out: per_second(after.bytes_out.saturating_sub(before.bytes_out) * 8),
    }
}

async fn send(socket: &mut Socket, payload: Payload) -> anyhow::Result<()> {
    let envelope = internal::ControlEnvelope {
        payload: Some(payload),
    };
    send_envelope(socket, &envelope).await
}

async fn send_envelope(
    socket: &mut Socket,
    envelope: &internal::ControlEnvelope,
) -> anyhow::Result<()> {
    write(socket, Message::Binary(envelope.encode_to_vec().into())).await
}

/// Writes a frame; a main server that stops reading counts as gone.
async fn write(socket: &mut Socket, message: Message) -> anyhow::Result<()> {
    tokio::time::timeout(SILENT_FOR, socket.send(message))
        .await
        .context("the main server stopped reading")?
        .context("could not write to the control channel")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_reports_turn_counters_into_rates() {
        let start = Instant::now();
        let before = NodeLoad {
            packets_in: 100,
            packets_out: 200,
            bytes_in: 10_000,
            bytes_out: 20_000,
            ..NodeLoad::default()
        };
        let after = NodeLoad {
            channels: 2,
            participants: 5,
            packets_in: 600,
            packets_out: 1_200,
            bytes_in: 60_000,
            bytes_out: 120_000,
        };

        let report = load_report(&(start, before), &(start + Duration::from_secs(5), after));

        assert_eq!(report.channels, 2);
        assert_eq!(report.participants, 5);
        assert_eq!(report.packets_per_second, 300);
        assert_eq!(report.bitrate_in, 80_000);
        assert_eq!(report.bitrate_out, 160_000);
    }
}
