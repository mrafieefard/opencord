//! One WebSocket connection: Hello, handshake, then heartbeats and requests.

use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use axum::extract::ws::{CloseFrame, Message, WebSocket};
use bytes::Bytes;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use opencord_common::auth::Nonce;
use opencord_proto::v1 as proto;
use prost::Message as _;
use proto::envelope::Payload;
use tokio::sync::mpsc;
use tokio::time::Instant;

use super::session::{CloseCode, CloseHandle, Connection, OUTBOUND_CAPACITY, Session};
use super::{HANDSHAKE_TIMEOUT, frames, identify};
use crate::error::ApiError;
use crate::handlers;
use crate::random;
use crate::state::AppState;

static NEXT_CONNECTION_ID: AtomicU64 = AtomicU64::new(1);

pub async fn run(state: Arc<AppState>, socket: WebSocket, ip: IpAddr) {
    let (sink, stream) = socket.split();
    let (outbound, frames) = mpsc::channel(OUTBOUND_CAPACITY);
    let closer = CloseHandle::new(state.shutdown.child_token());
    let connection = Connection {
        id: NEXT_CONNECTION_ID.fetch_add(1, Ordering::Relaxed),
        outbound,
        closer: closer.clone(),
    };
    let writer = tokio::spawn(write_frames(sink, frames, closer.clone()));

    if let Some(session) = drive(&state, stream, &connection, ip).await {
        session.detach(connection.id, std::time::Instant::now());
    }
    closer.close(CloseCode::GOING_AWAY);
    drop(connection);
    let _ = writer.await;
}

/// Sends queued frames until the connection is closed, then flushes what is
/// left and sends the close frame.
async fn write_frames(
    mut sink: SplitSink<WebSocket, Message>,
    mut frames: mpsc::Receiver<Bytes>,
    closer: CloseHandle,
) {
    loop {
        tokio::select! {
            biased;
            () = closer.closed() => break,
            frame = frames.recv() => match frame {
                Some(frame) => {
                    if sink.send(Message::Binary(frame)).await.is_err() {
                        return;
                    }
                }
                None => break,
            },
        }
    }
    while let Ok(frame) = frames.try_recv() {
        if sink.send(Message::Binary(frame)).await.is_err() {
            return;
        }
    }
    let close = CloseFrame {
        code: closer.code().0,
        reason: "".into(),
    };
    let _ = sink.send(Message::Close(Some(close))).await;
    let _ = sink.close().await;
}

async fn drive(
    state: &Arc<AppState>,
    mut stream: SplitStream<WebSocket>,
    connection: &Connection,
    ip: IpAddr,
) -> Option<Arc<Session>> {
    let nonce: Nonce = random::bytes();
    let hello = {
        let guild = state.guild();
        proto::Hello {
            server_id: guild.meta.server_id.to_vec(),
            server_name: guild.meta.name.clone(),
            nonce: nonce.to_vec(),
            heartbeat_interval_ms: state.config.gateway.heartbeat_interval_ms,
            protocol_version: opencord_common::PROTOCOL_VERSION,
        }
    };
    send(connection, frames::encode(Payload::Hello(hello)));
    let session = handshake(state, &mut stream, connection, &nonce, ip).await?;
    serve(state, &mut stream, connection, &session).await;
    Some(session)
}

async fn handshake(
    state: &Arc<AppState>,
    stream: &mut SplitStream<WebSocket>,
    connection: &Connection,
    nonce: &Nonce,
    ip: IpAddr,
) -> Option<Arc<Session>> {
    let deadline = Instant::now() + HANDSHAKE_TIMEOUT;
    loop {
        let next = tokio::select! {
            () = connection.closer.closed() => return None,
            next = tokio::time::timeout_at(deadline, next_envelope(stream)) => next,
        };
        let envelope = match next {
            Err(_) => return close(connection, CloseCode::HANDSHAKE_TIMEOUT),
            Ok(Next::Closed) => return None,
            Ok(Next::Invalid) => return close(connection, CloseCode::INVALID_FRAME),
            Ok(Next::Envelope(envelope)) => envelope,
        };
        match envelope.payload {
            Some(Payload::Identify(message)) => {
                match identify::identify(state, message, nonce, ip, connection).await {
                    Ok(session) => return Some(session),
                    Err(rejection) => {
                        send_error(connection, &rejection.error);
                        return close(connection, rejection.close);
                    }
                }
            }
            Some(Payload::Resume(resume)) => match identify::resume(state, &resume, connection) {
                Ok(session) => return Some(session),
                Err(error) => send_error(connection, &error),
            },
            Some(Payload::Heartbeat(_)) => send_heartbeat_ack(connection),
            Some(Payload::Request(_)) => return close(connection, CloseCode::NOT_AUTHENTICATED),
            _ => return close(connection, CloseCode::INVALID_FRAME),
        }
    }
}

async fn serve(
    state: &Arc<AppState>,
    stream: &mut SplitStream<WebSocket>,
    connection: &Connection,
    session: &Arc<Session>,
) {
    let allowed_silence =
        Duration::from_millis(u64::from(state.config.gateway.heartbeat_interval_ms)) * 2;
    let mut deadline = Instant::now() + allowed_silence;
    loop {
        let next = tokio::select! {
            () = connection.closer.closed() => return,
            () = tokio::time::sleep_until(deadline) => {
                connection.closer.close(CloseCode::HEARTBEAT_TIMEOUT);
                return;
            }
            next = next_envelope(stream) => next,
        };
        let envelope = match next {
            Next::Closed => return,
            Next::Invalid => {
                connection.closer.close(CloseCode::INVALID_FRAME);
                return;
            }
            Next::Envelope(envelope) => envelope,
        };
        match envelope.payload {
            Some(Payload::Heartbeat(_)) => {
                deadline = Instant::now() + allowed_silence;
                send_heartbeat_ack(connection);
            }
            Some(Payload::Request(request)) => {
                let result = handlers::handle(state, session, request).await;
                let response = Payload::Response(proto::Response {
                    result: Some(result),
                });
                send(
                    connection,
                    frames::envelope(0, envelope.request_id, response),
                );
            }
            _ => {
                connection.closer.close(CloseCode::INVALID_FRAME);
                return;
            }
        }
    }
}

enum Next {
    Envelope(proto::Envelope),
    Invalid,
    Closed,
}

async fn next_envelope(stream: &mut SplitStream<WebSocket>) -> Next {
    loop {
        match stream.next().await {
            Some(Ok(Message::Binary(bytes))) => {
                return match proto::Envelope::decode(bytes) {
                    Ok(envelope) => Next::Envelope(envelope),
                    Err(_) => Next::Invalid,
                };
            }
            Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
            Some(Ok(Message::Text(_))) => return Next::Invalid,
            Some(Ok(Message::Close(_)) | Err(_)) | None => return Next::Closed,
        }
    }
}

fn close(connection: &Connection, code: CloseCode) -> Option<Arc<Session>> {
    connection.closer.close(code);
    None
}

fn send(connection: &Connection, frame: Bytes) {
    if connection.outbound.try_send(frame).is_err() {
        connection.closer.close(CloseCode::TOO_SLOW);
    }
}

fn send_error(connection: &Connection, error: &ApiError) {
    send(connection, frames::encode(Payload::Error(error.to_proto())));
}

fn send_heartbeat_ack(connection: &Connection) {
    send(
        connection,
        frames::encode(Payload::HeartbeatAck(proto::HeartbeatAck {})),
    );
}
