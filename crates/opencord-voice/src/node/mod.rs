//! A voice node: the voice gateway and the SFU on one UDP port (Phase 2
//! plan §3.4, §6). The main server embeds one, or talks to external ones
//! over the control channel; either way it drives the node with
//! [`NodeCommand`]s and hears back through [`NodeEvent`]s.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::WebSocket;
use ed25519_dalek::VerifyingKey;
use opencord_proto::voice::v1 as voice;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;

use crate::sfu::{PeerState, Sfu, SfuError};
use crate::token::UsedTokens;
use runtime::{Command, Runtime};

mod gateway;
mod runtime;
mod session;

pub use runtime::RESUME_WINDOW;

/// What str0m is told packets arrived at, per family (see
/// [`crate::sfu::Sfu`]). Documentation addresses, never routed.
const LABEL_V4: IpAddr = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1));
const LABEL_V6: IpAddr = IpAddr::V6(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1));

#[derive(Debug, Clone)]
pub struct NodeConfig {
    /// UDP port for media; 0 picks a free one.
    pub udp_port: u16,
    /// Host or IP clients send media to; `None` means the host they reached
    /// the main server with.
    pub public_address: Option<String>,
    /// The main server's voice-signing key.
    pub verifying_key: VerifyingKey,
    pub heartbeat_interval: Duration,
}

/// From the main server.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeCommand {
    /// Moderation, self flags or permissions changed.
    Update {
        user_id: i64,
        channel_id: i64,
        state: PeerState,
        permissions: u64,
    },
    /// Left, moved or disconnected.
    Disconnect { user_id: i64, channel_id: i64 },
    Limits {
        channel_id: i64,
        limits: voice::Limits,
    },
}

/// To the main server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeEvent {
    /// Media came up.
    Connected {
        user_id: i64,
        channel_id: i64,
        session_id: String,
    },
    /// The voice connection ended for good.
    Disconnected {
        user_id: i64,
        channel_id: i64,
        session_id: String,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum NodeError {
    #[error("could not use UDP port {port}: {source}")]
    Udp { port: u16, source: std::io::Error },
    #[error(transparent)]
    Sfu(#[from] SfuError),
}

/// A running voice node.
#[derive(Clone)]
pub struct VoiceNode {
    inner: Arc<Inner>,
}

struct Inner {
    commands: mpsc::UnboundedSender<Command>,
    udp_port: u16,
    heartbeat_interval: Duration,
}

impl VoiceNode {
    /// Binds the media port and starts the node.
    pub async fn start(
        config: NodeConfig,
    ) -> Result<(Self, mpsc::UnboundedReceiver<NodeEvent>), NodeError> {
        let v4 = UdpSocket::bind(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            config.udp_port,
        ))
        .await
        .map_err(|source| NodeError::Udp {
            port: config.udp_port,
            source,
        })?;
        let udp_port = v4.local_addr().map_err(|source| NodeError::Udp {
            port: config.udp_port,
            source,
        })?;
        let udp_port = udp_port.port();
        // IPv6 is a bonus: hosts without it still serve IPv4.
        let v6 = UdpSocket::bind(SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), udp_port))
            .await
            .ok();
        let sfu = Sfu::new(
            SocketAddr::new(LABEL_V4, udp_port),
            SocketAddr::new(LABEL_V6, udp_port),
        )?;
        let (events_tx, events) = mpsc::unbounded_channel();
        let (commands, receiver) = mpsc::unbounded_channel();
        let runtime = Runtime {
            sfu,
            v4,
            v6,
            verifying_key: config.verifying_key,
            public_ip: config.public_address.unwrap_or_default(),
            udp_port,
            events: events_tx,
            used_tokens: UsedTokens::default(),
            sessions: HashMap::new(),
            pending: HashMap::new(),
            limits: HashMap::new(),
        };
        tokio::spawn(runtime.run(receiver));
        Ok((
            Self {
                inner: Arc::new(Inner {
                    commands,
                    udp_port,
                    heartbeat_interval: config.heartbeat_interval,
                }),
            },
            events,
        ))
    }

    pub fn udp_port(&self) -> u16 {
        self.inner.udp_port
    }

    pub fn heartbeat_interval(&self) -> Duration {
        self.inner.heartbeat_interval
    }

    /// Applies a command from the main server.
    pub fn send(&self, command: NodeCommand) {
        self.command(Command::Node(command));
    }

    /// Serves one voice gateway WebSocket until it closes.
    pub async fn serve(&self, socket: WebSocket) {
        gateway::serve(self.clone(), socket).await;
    }

    /// Ends every voice session (clients wait for the main server to send
    /// them elsewhere) and stops the node.
    pub fn shutdown(&self) {
        self.command(Command::Shutdown);
    }

    fn command(&self, command: Command) {
        let _ = self.inner.commands.send(command);
    }
}
