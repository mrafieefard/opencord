//! The connection test (Phase 3 plan §12.9): REST and the password, the
//! WebSocket's `ready`, then a temporary player whose voice state points at
//! the audio bridge, which must see Lavalink sign in and do UDP IP
//! discovery within 10 s. Each step is sent to the session that asked, as
//! it happens; a failure says how to fix it.

use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use opencord_proto::v1 as proto;
use opencord_voice::bridge::{BridgeEvent, ExpectedPlayer};
use proto::event::Kind as Event;
use proto::{LavalinkTestStepKind as Kind, LavalinkTestStepState as State};
use serde_json::json;
use tokio_tungstenite::tungstenite::Message;

use super::bridge::{self, BridgeAddress};
use super::client::{self, Op, Rest};
use super::{Node, lock};
use crate::error::ApiError;
use crate::state::AppState;

/// How long the bridge waits for Lavalink.
const BRIDGE_WAIT: Duration = Duration::from_secs(10);

struct Progress {
    state: Arc<AppState>,
    session_id: String,
    node_id: i64,
    steps: Vec<proto::LavalinkTestStep>,
}

impl Progress {
    fn set(&mut self, kind: Kind, state: State, detail: String) {
        if let Some(step) = self.steps.iter_mut().find(|step| step.kind == kind as i32) {
            step.state = state as i32;
            step.detail = detail;
        }
        self.send(false);
    }

    fn running(&mut self, kind: Kind) {
        self.set(kind, State::Running, String::new());
    }

    /// Fails `kind`; the steps after it are skipped.
    fn fail(&mut self, kind: Kind, detail: String) {
        let mut after = false;
        for step in &mut self.steps {
            if after {
                step.state = State::Skipped as i32;
            }
            after |= step.kind == kind as i32;
        }
        self.set(kind, State::Failed, detail);
    }

    fn send(&self, done: bool) {
        let Some(session) = self.state.sessions.get(&self.session_id) else {
            return;
        };
        let event = proto::Event {
            kind: Some(Event::LavalinkTestProgress(proto::LavalinkTestProgress {
                node_id: self.node_id,
                steps: self.steps.clone(),
                done,
            })),
        };
        session.push_event(&event, Instant::now());
    }
}

/// Starts the test of `node_id` for `session_id`.
pub fn start(state: &Arc<AppState>, node_id: i64, session_id: &str) -> Result<(), ApiError> {
    let node = state
        .lavalink
        .node(node_id)
        .ok_or_else(|| ApiError::not_found("Lavalink node"))?;
    let step = |kind: Kind| proto::LavalinkTestStep {
        kind: kind as i32,
        state: State::Waiting as i32,
        detail: String::new(),
    };
    let mut progress = Progress {
        state: Arc::clone(state),
        session_id: session_id.to_owned(),
        node_id,
        steps: vec![step(Kind::Rest), step(Kind::Websocket), step(Kind::Bridge)],
    };
    tokio::spawn(async move {
        run(&mut progress, &node).await;
        progress.send(true);
    });
    Ok(())
}

async fn run(progress: &mut Progress, node: &Arc<Node>) {
    let state = Arc::clone(&progress.state);
    let endpoint = node.endpoint(state.lavalink.user_id());
    let rest = Rest::new(state.network.http().clone(), endpoint.clone());

    progress.running(Kind::Rest);
    match rest.info().await {
        Ok(info) => {
            let sources = if info.sources.is_empty() {
                "no sources enabled".to_owned()
            } else {
                format!("sources: {}", info.sources.join(", "))
            };
            progress.set(
                Kind::Rest,
                State::Passed,
                format!("Lavalink {} · {sources}", info.version),
            );
        }
        Err(failure) => return progress.fail(Kind::Rest, failure.text()),
    }

    progress.running(Kind::Websocket);
    let connected = match client::connect(&endpoint, None).await {
        Ok(connected) => connected,
        Err(failure) => return progress.fail(Kind::Websocket, failure.text()),
    };
    progress.set(
        Kind::Websocket,
        State::Passed,
        "Lavalink is ready for players".to_owned(),
    );

    progress.running(Kind::Bridge);
    let outcome = bridge_step(&state, node, &rest, connected).await;
    match outcome {
        Ok(detail) => progress.set(Kind::Bridge, State::Passed, detail),
        Err(detail) => progress.fail(Kind::Bridge, detail),
    }
}

/// What came back while the bridge waited.
#[derive(Debug, Default)]
struct Seen {
    signed_in: bool,
    discovered_from: Option<IpAddr>,
    refused_from: Option<IpAddr>,
    /// Lavalink's `WebSocketClosedEvent`: code and reason.
    closed: Option<(i64, String)>,
}

impl Seen {
    fn finished(&self) -> bool {
        self.discovered_from.is_some() || self.refused_from.is_some() || self.closed.is_some()
    }
}

/// A temporary player, pointed at the bridge; destroyed again.
async fn bridge_step(
    state: &Arc<AppState>,
    node: &Arc<Node>,
    rest: &Rest,
    connected: client::Connected,
) -> Result<String, String> {
    let Some(music_bridge) = state.music_bridge.clone() else {
        return Err(
            "Music is off on this server: it needs [music] enabled in opencord.toml \
                    and voice running on this server"
                .to_owned(),
        );
    };
    let address = bridge::address(state);
    let channel_id = state.ids.next_id();
    let player = ExpectedPlayer {
        channel_id,
        user_id: state.lavalink.user_id(),
        token: crate::random::code(32),
        session_id: crate::random::code(32),
        allowed: node.allowed_addresses(),
    };
    let mut events = state.lavalink.wait_for_bridge(channel_id);
    music_bridge.expect(player.clone());
    let started = Instant::now();
    let body = json!({
        "voice": {
            "token": player.token,
            "endpoint": address.endpoint(),
            "sessionId": player.session_id,
            "channelId": channel_id.to_string(),
        },
    });
    let (mut sink, mut socket) = connected.socket.split();
    let session = connected.session_id;
    let mut seen = Seen::default();
    let created = rest.update_player(&session, channel_id, &body).await;
    if let Err(failure) = &created {
        seen.closed = Some((0, failure.text()));
    }
    let deadline = tokio::time::Instant::now() + BRIDGE_WAIT;
    while !seen.finished() {
        tokio::select! {
            () = tokio::time::sleep_until(deadline) => break,
            event = events.recv() => match event {
                Some(BridgeEvent::Connected { .. }) => seen.signed_in = true,
                Some(BridgeEvent::Discovered { from, .. }) => seen.discovered_from = Some(from),
                Some(BridgeEvent::Refused { peer, .. }) => seen.refused_from = Some(peer),
                Some(BridgeEvent::Closed { .. }) | None => {}
            },
            message = socket.next() => match message {
                Some(Ok(Message::Text(text))) => {
                    if let Op::Player { guild_id, value } = client::parse_op(text.as_str())
                        && guild_id == channel_id
                        && value["type"] == "WebSocketClosedEvent"
                    {
                        let reason = value["reason"].as_str().unwrap_or_default().to_owned();
                        seen.closed = Some((value["code"].as_i64().unwrap_or(0), reason));
                    }
                }
                Some(Ok(_)) => {}
                Some(Err(_)) | None => break,
            },
        }
    }
    state.lavalink.stop_waiting_for_bridge(channel_id);
    music_bridge.cancel(channel_id);
    if created.is_ok() {
        let _ = rest.destroy_player(&session, channel_id).await;
    }
    let _ = sink.send(Message::Close(None)).await;
    let refusals = state.lavalink.refusals().since(started);
    verdict(
        state,
        node,
        &address,
        &seen,
        refusals.first().map(|refusal| refusal.ip),
    )
}

/// The step's result, from what was seen.
fn verdict(
    state: &AppState,
    node: &Node,
    address: &BridgeAddress,
    seen: &Seen,
    tls_refused_by: Option<IpAddr>,
) -> Result<String, String> {
    let url = address.url();
    let audio = bridge::audio_address(state)
        .map(|(ip, port)| format!("UDP {ip}:{port}"))
        .unwrap_or_else(|| "the music UDP port".to_owned());
    if let Some(from) = seen.discovered_from {
        return Ok(format!(
            "Lavalink signed in at {url} and reached the audio port from {from}"
        ));
    }
    if let Some(peer) = seen.refused_from {
        let addresses = lock(&node.status)
            .addresses
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "Lavalink connected from {peer}, which is not one of {}'s addresses ({addresses}). \
             Turn off Strict address check if this node's address changes",
            node.settings().host
        ));
    }
    if seen.signed_in {
        return Err(format!(
            "Lavalink signed in, but didn't send audio to {audio}. Open the port for this node"
        ));
    }
    let untrusted = || {
        if address.trusted {
            format!(
                "Lavalink couldn't verify the certificate at {url}. Check that the domain \
                 serves its certificate"
            )
        } else {
            "Lavalink couldn't verify Opencord's certificate. Activate a domain with a \
             certificate in Domains & network, or import Opencord's certificate into this \
             Lavalink's trust store"
                .to_owned()
        }
    };
    if tls_refused_by.is_some() {
        return Err(untrusted());
    }
    if let Some((code, reason)) = &seen.closed {
        let lower = reason.to_ascii_lowercase();
        if lower.contains("pkix") || lower.contains("certificate") || lower.contains("ssl") {
            return Err(untrusted());
        }
        if lower.contains("unresolved") || lower.contains("unknownhost") {
            return Err(format!(
                "Lavalink couldn't find {}. It needs to resolve that name to this server",
                address.host
            ));
        }
        if *code == 0 {
            return Err(format!("Lavalink refused the test player: {reason}"));
        }
        return Err(format!(
            "Lavalink couldn't connect to {url}: {reason} ({code}). Check that the node can \
             reach that address"
        ));
    }
    Err(format!(
        "Lavalink didn't connect to {url} within {} s. Check that the node can reach that \
         address{}",
        BRIDGE_WAIT.as_secs(),
        if address.trusted {
            ""
        } else {
            " and trusts its certificate"
        }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seen_discovery_or_failure_ends_the_wait() {
        let mut seen = Seen::default();
        assert!(!seen.finished());
        seen.signed_in = true;
        assert!(!seen.finished(), "sign-in alone waits for discovery");
        seen.discovered_from = Some("127.0.0.1".parse().unwrap());
        assert!(seen.finished());
        let closed = Seen {
            closed: Some((4006, "x".to_owned())),
            ..Seen::default()
        };
        assert!(closed.finished());
    }
}
