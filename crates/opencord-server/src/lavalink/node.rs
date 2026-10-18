//! One node's connection (Phase 3 plan §12.2, §12.5): `GET /v4/info`, the
//! WebSocket with its session (resumed after drops and restarts), a ping
//! every 5 s, `GET /v4/stats` every 15 s, and reconnecting with backoff
//! from 1 s to 30 s until the node is disabled or deleted.

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

use super::client::{self, Failure, Op, Rest};
use super::{Node, lock};
use crate::db;
use crate::state::{AppState, now_ms};

const PING_EVERY: Duration = Duration::from_secs(5);
/// No pong for this long: the connection hangs.
const PONG_WITHIN: Duration = Duration::from_secs(12);
const STATS_EVERY: Duration = Duration::from_secs(15);
const RESOLVE_EVERY: Duration = Duration::from_secs(5 * 60);
const BACKOFF_FIRST: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(30);

pub(super) fn spawn(state: &Arc<AppState>, node: &Arc<Node>) {
    let state = Arc::clone(state);
    let node = Arc::clone(node);
    tokio::spawn(async move { run(&state, &node).await });
}

async fn run(state: &Arc<AppState>, node: &Arc<Node>) {
    let mut backoff = BACKOFF_FIRST;
    loop {
        resolve(node).await;
        match connection(state, node).await {
            Ok(()) => backoff = BACKOFF_FIRST,
            Err(failure) => failed(state, node, &failure),
        }
        if node.stop.is_cancelled() {
            disconnected(state, node, None);
            return;
        }
        tokio::select! {
            () = tokio::time::sleep(backoff) => {}
            () = node.reconnect.notified() => backoff = BACKOFF_FIRST,
            () = node.stop.cancelled() => {
                disconnected(state, node, None);
                return;
            }
        }
        backoff = (backoff * 2).min(BACKOFF_MAX);
    }
}

/// The node's addresses, for the bridge's strict check.
async fn resolve(node: &Node) {
    let settings = node.settings();
    if let Ok(found) = tokio::net::lookup_host((settings.host.as_str(), settings.port)).await {
        let mut addresses: Vec<_> = found.map(|address| address.ip().to_canonical()).collect();
        addresses.sort();
        addresses.dedup();
        lock(&node.status).addresses = addresses;
    }
}

fn failed(state: &AppState, node: &Node, failure: &Failure) {
    {
        let mut status = lock(&node.status);
        status.observed.first_attempt = false;
        status.observed.password_rejected = *failure == Failure::PasswordRejected;
        status.observed.last_error = Some(failure.text());
    }
    disconnected(state, node, Some(failure));
}

/// The WebSocket is gone (or never came).
fn disconnected(state: &AppState, node: &Node, failure: Option<&Failure>) {
    {
        let mut status = lock(&node.status);
        if status.observed.connected {
            status.observed.connected = false;
            status.observed.disconnected_at = Some(Instant::now());
            status.connected_since_ms = None;
        }
        status.observed.first_attempt = false;
        if let Some(failure) = failure {
            status.observed.last_error = Some(failure.text());
        }
    }
    state.lavalink.judge(state, node, Instant::now());
}

/// One connection, from `GET /v4/info` until the WebSocket ends. `Ok`
/// when it was connected and ended; the failure when it never got there
/// or ended in one.
async fn connection(state: &Arc<AppState>, node: &Arc<Node>) -> Result<(), Failure> {
    let endpoint = node.endpoint(state.lavalink.user_id());
    let rest = Rest::new(state.network.http().clone(), endpoint.clone());
    let info = rest.info().await?;
    let resume = lock(&node.status).session_id.clone();
    let connected = client::connect(&endpoint, resume.as_deref()).await?;
    rest.enable_resuming(&connected.session_id).await?;
    if resume.as_deref() != Some(connected.session_id.as_str())
        && let Ok(mut conn) = state.db.acquire().await
        && let Err(error) =
            db::lavalink::set_resume_session(&mut conn, node.id, Some(&connected.session_id)).await
    {
        tracing::warn!(node_id = node.id, %error, "the Lavalink session could not be saved");
    }
    tracing::info!(
        node_id = node.id,
        version = info.version,
        resumed = connected.resumed,
        "connected to Lavalink"
    );
    {
        let mut status = lock(&node.status);
        status.info = Some(info);
        status.session_id = Some(connected.session_id.clone());
        status.connected_since_ms = Some(now_ms());
        let observed = &mut status.observed;
        observed.connected = true;
        observed.disconnected_at = None;
        observed.first_attempt = false;
        observed.password_rejected = false;
        observed.rest_failures = 0;
    }
    state.lavalink.judge(state, node, Instant::now());
    let ended = session(state, node, &rest, connected.socket).await;
    disconnected(state, node, ended.as_ref().err());
    ended
}

/// Reads the WebSocket and keeps watch until it ends.
async fn session(
    state: &Arc<AppState>,
    node: &Arc<Node>,
    rest: &Rest,
    socket: client::Socket,
) -> Result<(), Failure> {
    let (mut sink, mut stream) = socket.split();
    let mut ping = tokio::time::interval(PING_EVERY);
    let mut stats = tokio::time::interval(STATS_EVERY);
    let mut resolve_at = Instant::now() + RESOLVE_EVERY;
    let mut pong_at = Instant::now();
    let lost = |text: &str| Failure::Other(text.to_owned());
    loop {
        tokio::select! {
            message = stream.next() => match message {
                Some(Ok(Message::Text(text))) => {
                    match client::parse_op(text.as_str()) {
                        Op::Stats(load) => {
                            lock(&node.status).observed.load = Some(load);
                            state.lavalink.node_changed(state, node);
                        }
                        Op::Player { guild_id, value } => {
                            crate::music::lavalink_event(state, node.id, guild_id, value);
                        }
                        Op::Other => {}
                    }
                }
                Some(Ok(Message::Pong(_))) => pong_at = Instant::now(),
                Some(Ok(Message::Close(_))) | None => {
                    return Err(lost("Lavalink closed the WebSocket"));
                }
                Some(Ok(_)) => {}
                Some(Err(error)) => return Err(Failure::Other(format!("the WebSocket failed: {error}"))),
            },
            _ = ping.tick() => {
                if pong_at.elapsed() > PONG_WITHIN {
                    return Err(lost("Lavalink stopped answering pings"));
                }
                if !client::ping(&mut sink).await {
                    return Err(lost("the WebSocket could not be written to"));
                }
            }
            _ = stats.tick() => {
                poll_stats(state, node, rest).await;
                if Instant::now() >= resolve_at {
                    resolve(node).await;
                    resolve_at = Instant::now() + RESOLVE_EVERY;
                }
            }
            () = node.reconnect.notified() => {
                let _ = sink.send(Message::Close(None)).await;
                return Ok(());
            }
            () = node.stop.cancelled() => {
                // With resuming on, Lavalink keeps the players for a minute.
                let _ = sink.send(Message::Close(None)).await;
                return Ok(());
            }
        }
    }
}

/// `GET /v4/stats`: the load and the REST round trip, the node's ping.
async fn poll_stats(state: &AppState, node: &Node, rest: &Rest) {
    let result = rest.stats().await;
    {
        let mut status = lock(&node.status);
        let observed = &mut status.observed;
        match result {
            Ok((load, took)) => {
                // Frame statistics come only with the WebSocket's stats.
                let frames = observed.load.and_then(|before| before.frames);
                observed.load = Some(client_load(load, frames));
                observed.ping_ms = Some(u32::try_from(took.as_millis()).unwrap_or(u32::MAX));
                observed.rest_failures = 0;
            }
            Err(failure) => {
                observed.rest_failures += 1;
                observed.password_rejected = failure == Failure::PasswordRejected;
                observed.last_error = Some(failure.text());
            }
        }
    }
    state.lavalink.judge(state, node, Instant::now());
    state.lavalink.node_changed(state, node);
}

fn client_load(
    load: super::health::Load,
    frames: Option<super::health::FrameStats>,
) -> super::health::Load {
    super::health::Load {
        frames: load.frames.or(frames),
        ..load
    }
}
