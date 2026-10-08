//! A voice node on localhost with real clients (opencord-media's
//! transport), for the integration tests. Each test binary uses part of it.
#![allow(dead_code)]

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use axum::Router;
use axum::extract::{State, WebSocketUpgrade};
use axum::response::Response;
use axum::routing::get;
use ed25519_dalek::SigningKey;
use opencord_media::transport::{ConnectOptions, VoiceConnection, VoiceEvent, VoiceTarget};
use opencord_proto::internal::v1::VoiceTokenClaims;
use opencord_proto::voice::v1::Limits;
use opencord_voice::node::{NodeConfig, NodeEvent, VoiceNode};
use opencord_voice::token;
use tokio::sync::mpsc::UnboundedReceiver;

pub const CHANNEL: i64 = 5;
pub const WAIT: Duration = Duration::from_secs(10);
/// `CONNECT`.
pub const CONNECT: u64 = 1 << 16;

pub struct Node {
    pub node: VoiceNode,
    pub events: UnboundedReceiver<NodeEvent>,
    pub key: SigningKey,
    pub gateway: String,
}

pub async fn node() -> Node {
    start_node(true).await
}

/// A node, told the main server's key or not.
pub async fn start_node(knows_key: bool) -> Node {
    let key = SigningKey::from_bytes(&[9; 32]);
    let (node, events) = VoiceNode::start(NodeConfig {
        udp_port: 0,
        public_address: Some("127.0.0.1".to_owned()),
        verifying_key: knows_key.then(|| key.verifying_key()),
        heartbeat_interval: Duration::from_secs(1),
    })
    .await
    .unwrap();
    let app = Router::new()
        .route("/voice", get(upgrade))
        .with_state(node.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gateway = format!("ws://{}/voice", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await });
    Node {
        node,
        events,
        key,
        gateway,
    }
}

async fn upgrade(State(node): State<VoiceNode>, upgrade: WebSocketUpgrade) -> Response {
    upgrade.on_upgrade(move |socket| async move { node.serve(socket).await })
}

static TOKEN_IDS: AtomicU32 = AtomicU32::new(1);

pub fn token_for(node: &Node, user_id: i64) -> Vec<u8> {
    token_with(node, user_id, CONNECT, None)
}

/// A token with these permissions and limits.
pub fn token_with(node: &Node, user_id: i64, permissions: u64, limits: Option<Limits>) -> Vec<u8> {
    let mut token_id = vec![0u8; 16];
    token_id[..4].copy_from_slice(&TOKEN_IDS.fetch_add(1, Ordering::Relaxed).to_be_bytes());
    token::issue(
        &node.key,
        &VoiceTokenClaims {
            token_id,
            user_id,
            channel_id: CHANNEL,
            session_id: format!("session-{user_id}"),
            permissions,
            limits,
            expires_at_ms: now_ms() + 60_000,
            ..Default::default()
        },
    )
}

pub fn target(node: &Node, user_id: i64, token: Vec<u8>) -> VoiceTarget {
    VoiceTarget {
        gateway_url: node.gateway.clone(),
        certificate_fingerprint: None,
        token,
        user_id,
        session_id: format!("session-{user_id}"),
        channel_id: CHANNEL,
    }
}

pub async fn join(node: &Node, user_id: i64) -> (VoiceConnection, UnboundedReceiver<VoiceEvent>) {
    join_with(
        node,
        user_id,
        token_for(node, user_id),
        ConnectOptions::default(),
    )
    .await
}

/// Joins with a given token and options, and waits for media.
pub async fn join_with(
    node: &Node,
    user_id: i64,
    token: Vec<u8>,
    options: ConnectOptions,
) -> (VoiceConnection, UnboundedReceiver<VoiceEvent>) {
    let (connection, mut events) =
        VoiceConnection::connect_with(target(node, user_id, token), options)
            .await
            .unwrap();
    wait_for(&mut events, |event| {
        matches!(event, VoiceEvent::MediaConnected)
    })
    .await;
    (connection, events)
}

pub async fn wait_for(
    events: &mut UnboundedReceiver<VoiceEvent>,
    mut pick: impl FnMut(&VoiceEvent) -> bool,
) -> VoiceEvent {
    tokio::time::timeout(WAIT, async {
        loop {
            let event = events.recv().await.expect("the connection ended");
            if pick(&event) {
                return event;
            }
        }
    })
    .await
    .expect("timed out waiting for a voice event")
}

pub fn now_ms() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap()
}
