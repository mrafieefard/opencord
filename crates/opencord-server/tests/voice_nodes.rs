//! External voice nodes on the main server (Phase 2 plan §3.4): the control
//! channel at `/internal/voice`, which node a channel goes to, and failover.
//! The nodes here are fakes that speak the control protocol.

mod common;

use std::path::PathBuf;
use std::time::Duration;

use common::{TestClient, TestServer, channel_id, owner, self_id};
use futures_util::{SinkExt, StreamExt};
use internal::control_envelope::Payload as Control;
use opencord_common::voice::close;
use opencord_proto::internal::v1 as internal;
use opencord_proto::v1 as proto;
use opencord_server::config::{ExternalNode, VoiceMode};
use opencord_voice::control;
use prost::Message as _;
use proto::event::Kind as Event;
use proto::request::Kind as Request;
use proto::response::Result as Response;
use sha2::{Digest, Sha256};
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;

const QUIET: Duration = Duration::from_millis(300);
const A: &str = "wss://a.example:7712";
const B: &str = "wss://b.example:7712";
const SECRET_A: &[u8] = b"a-secret-that-is-long-enough-000";
const SECRET_B: &[u8] = b"b-secret-that-is-long-enough-111";

/// A main server in external mode with nodes A and B configured.
struct Cluster {
    server: TestServer,
    owner: TestClient,
    owner_id: i64,
    general: i64,
    _secrets: tempfile::TempDir,
}

async fn cluster() -> Cluster {
    let secrets = tempfile::tempdir().unwrap();
    let secret_a = secrets.path().join("a.secret");
    let secret_b = secrets.path().join("b.secret");
    std::fs::write(&secret_a, [SECRET_A, b"\n"].concat()).unwrap();
    std::fs::write(&secret_b, SECRET_B).unwrap();
    let server = TestServer::start_with(|config| {
        config.voice.mode = VoiceMode::External;
        config.voice.external_nodes = vec![node_config(A, secret_a), node_config(B, secret_b)];
    })
    .await;
    let (owner, ready) = owner(&server).await;
    Cluster {
        owner_id: self_id(&ready),
        general: channel_id(&ready, "General"),
        server,
        owner,
        _secrets: secrets,
    }
}

fn node_config(endpoint: &str, secret_file: PathBuf) -> ExternalNode {
    ExternalNode {
        endpoint: endpoint.to_owned(),
        secret_file,
    }
}

type Socket = WebSocketStream<TlsStream<TcpStream>>;

/// A registered node, as far as the main server can tell.
struct FakeNode {
    ws: Socket,
    fingerprint: Vec<u8>,
    registered: internal::Registered,
}

impl FakeNode {
    /// Answers the challenge with `secret`; `Err` holds the close code of a
    /// refusal.
    async fn register(
        server: &TestServer,
        endpoint: &str,
        secret: &[u8],
    ) -> Result<Self, Option<u16>> {
        let mut ws = common::websocket(server, "/internal/voice").await.unwrap();
        let challenge = match recv(&mut ws).await {
            Ok(Control::Challenge(challenge)) => challenge,
            other => panic!("expected a challenge, got {other:?}"),
        };
        let fingerprint = Sha256::digest(endpoint.as_bytes()).to_vec();
        send(
            &mut ws,
            Control::Register(internal::Register {
                proof: control::proof(secret, &challenge.nonce),
                endpoint: endpoint.to_owned(),
                certificate_fingerprint: fingerprint.clone(),
                public_address: "203.0.113.9".to_owned(),
                udp_port: 7711,
            }),
        )
        .await;
        loop {
            match recv(&mut ws).await {
                Ok(Control::Registered(registered)) => {
                    return Ok(Self {
                        ws,
                        fingerprint,
                        registered,
                    });
                }
                Ok(Control::Error(_)) => {}
                Ok(other) => panic!("expected Registered, got {other:?}"),
                Err(code) => return Err(code),
            }
        }
    }

    async fn send(&mut self, payload: Control) {
        send(&mut self.ws, payload).await;
    }

    /// The next command about `user_id`, skipping others.
    async fn command_for(&mut self, user_id: i64) -> Control {
        loop {
            let payload = recv(&mut self.ws)
                .await
                .expect("the control channel closed");
            let about = match &payload {
                Control::ParticipantUpdate(update) => update.user_id,
                Control::DisconnectParticipant(gone) => gone.user_id,
                _ => continue,
            };
            if about == user_id {
                return payload;
            }
        }
    }
}

async fn send(ws: &mut Socket, payload: Control) {
    let envelope = internal::ControlEnvelope {
        payload: Some(payload),
    };
    ws.send(Message::Binary(envelope.encode_to_vec().into()))
        .await
        .unwrap();
}

/// The next payload, or the close code once the channel closes.
async fn recv(ws: &mut Socket) -> Result<Control, Option<u16>> {
    loop {
        let message = tokio::time::timeout(common::TIMEOUT, ws.next())
            .await
            .expect("timed out waiting for the main server");
        match message {
            Some(Ok(Message::Binary(bytes))) => {
                let envelope = internal::ControlEnvelope::decode(bytes).unwrap();
                return Ok(envelope.payload.expect("a payload"));
            }
            Some(Ok(Message::Close(frame))) => return Err(frame.map(|f| u16::from(f.code))),
            Some(Ok(_)) => {}
            Some(Err(_)) | None => return Err(None),
        }
    }
}

fn join_voice(channel_id: i64) -> Request {
    Request::UpdateVoiceState(proto::UpdateVoiceState {
        channel_id: Some(channel_id),
        ..Default::default()
    })
}

async fn join(cluster: &mut Cluster) -> proto::VoiceState {
    match cluster.owner.ok(join_voice(cluster.general)).await {
        Response::VoiceState(state) => state,
        other => panic!("expected a voice state, got {other:?}"),
    }
}

async fn server_update(client: &mut TestClient) -> proto::VoiceServerUpdate {
    client
        .wait_for(|event| match event {
            Event::VoiceServerUpdate(update) => Some(update.clone()),
            _ => None,
        })
        .await
}

fn is_voice_event(event: &Event) -> bool {
    matches!(
        event,
        Event::VoiceStateUpdate(_) | Event::VoiceServerUpdate(_)
    )
}

#[tokio::test]
async fn a_node_that_proves_its_secret_registers_and_gets_the_voice_key() {
    let cluster = cluster().await;

    let node = FakeNode::register(&cluster.server, A, SECRET_A)
        .await
        .unwrap();

    assert_eq!(node.registered.node_id, A);
    assert_eq!(
        node.registered.voice_signing_public_key,
        cluster.server.handle.voice_key.to_bytes().to_vec()
    );
    assert!(node.registered.settings.is_some());
}

#[tokio::test]
async fn a_wrong_secret_or_an_unknown_endpoint_is_refused() {
    let cluster = cluster().await;

    let wrong_secret = FakeNode::register(&cluster.server, A, SECRET_B).await.err();
    let unknown = FakeNode::register(&cluster.server, "wss://c.example:7712", SECRET_A)
        .await
        .err();

    assert_eq!(wrong_secret, Some(Some(close::AUTHENTICATION_FAILED)));
    assert_eq!(unknown, Some(Some(close::AUTHENTICATION_FAILED)));
}

#[tokio::test]
async fn without_external_nodes_there_is_no_control_endpoint() {
    let server = TestServer::start().await;

    let refused = common::websocket(&server, "/internal/voice").await.err();

    assert!(
        matches!(
            &refused,
            Some(tokio_tungstenite::tungstenite::Error::Http(response))
                if response.status() == 404
        ),
        "{refused:?}"
    );
}

#[tokio::test]
async fn people_are_sent_to_a_registered_node() {
    let mut cluster = cluster().await;
    let node = FakeNode::register(&cluster.server, A, SECRET_A)
        .await
        .unwrap();

    join(&mut cluster).await;
    let update = server_update(&mut cluster.owner).await;

    assert_eq!(update.endpoint, A);
    assert_eq!(update.certificate_fingerprint, node.fingerprint);
    assert_eq!(update.channel_id, cluster.general);
}

#[tokio::test]
async fn without_a_node_nobody_can_join() {
    let mut cluster = cluster().await;

    let refused = cluster.owner.error(join_voice(cluster.general)).await;

    assert_eq!(refused.code, proto::ErrorCode::Conflict as i32);
}

#[tokio::test]
async fn a_node_reporting_more_people_gets_new_channels_last() {
    let mut cluster = cluster().await;
    let mut a = FakeNode::register(&cluster.server, A, SECRET_A)
        .await
        .unwrap();
    let _b = FakeNode::register(&cluster.server, B, SECRET_B)
        .await
        .unwrap();

    a.send(Control::LoadReport(internal::LoadReport {
        participants: 5,
        channels: 1,
        ..Default::default()
    }))
    .await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    join(&mut cluster).await;
    let update = server_update(&mut cluster.owner).await;

    assert_eq!(update.endpoint, B);
}

#[tokio::test]
async fn when_its_node_goes_away_a_channel_moves_to_another() {
    let mut cluster = cluster().await;
    let a = FakeNode::register(&cluster.server, A, SECRET_A)
        .await
        .unwrap();
    let _b = FakeNode::register(&cluster.server, B, SECRET_B)
        .await
        .unwrap();
    join(&mut cluster).await;
    let first = server_update(&mut cluster.owner).await;

    drop(a);
    let moved = server_update(&mut cluster.owner).await;

    assert_eq!(first.endpoint, A);
    assert_eq!(moved.endpoint, B);
    assert_eq!(moved.channel_id, cluster.general);
    assert_ne!(moved.token, first.token);
}

#[tokio::test]
async fn with_no_node_left_people_stay_and_wait_for_one() {
    let mut cluster = cluster().await;
    let a = FakeNode::register(&cluster.server, A, SECRET_A)
        .await
        .unwrap();
    join(&mut cluster).await;
    server_update(&mut cluster.owner).await;

    drop(a);
    cluster.owner.assert_no_event(QUIET, is_voice_event).await;
    let _a = FakeNode::register(&cluster.server, A, SECRET_A)
        .await
        .unwrap();
    let back = server_update(&mut cluster.owner).await;

    assert_eq!(back.endpoint, A);
    assert_eq!(back.channel_id, cluster.general);
}

#[tokio::test]
async fn a_node_is_told_about_the_people_in_its_channels() {
    let mut cluster = cluster().await;
    let mut a = FakeNode::register(&cluster.server, A, SECRET_A)
        .await
        .unwrap();
    join(&mut cluster).await;
    let joined = a.command_for(cluster.owner_id).await;

    cluster
        .owner
        .ok(Request::UpdateVoiceState(proto::UpdateVoiceState {
            channel_id: Some(cluster.general),
            self_mute: true,
            ..Default::default()
        }))
        .await;
    let muted = a.command_for(cluster.owner_id).await;
    cluster
        .owner
        .ok(Request::UpdateVoiceState(proto::UpdateVoiceState::default()))
        .await;
    let left = a.command_for(cluster.owner_id).await;

    assert!(matches!(joined, Control::ParticipantUpdate(update) if !update.self_mute));
    assert!(matches!(muted, Control::ParticipantUpdate(update) if update.self_mute));
    assert!(
        matches!(left, Control::DisconnectParticipant(gone) if gone.channel_id == cluster.general)
    );
}

#[tokio::test]
async fn a_node_can_end_voice_states_only_in_its_own_channels() {
    let mut cluster = cluster().await;
    let mut a = FakeNode::register(&cluster.server, A, SECRET_A)
        .await
        .unwrap();
    let mut b = FakeNode::register(&cluster.server, B, SECRET_B)
        .await
        .unwrap();
    let joined = join(&mut cluster).await;
    assert_eq!(server_update(&mut cluster.owner).await.endpoint, A);
    let gone = Control::ParticipantDisconnected(internal::ParticipantDisconnected {
        user_id: cluster.owner_id,
        channel_id: cluster.general,
        session_id: joined.session_id.clone(),
    });

    b.send(gone.clone()).await;
    cluster.owner.assert_no_event(QUIET, is_voice_event).await;
    a.send(gone).await;
    let left = cluster
        .owner
        .wait_for(|event| match event {
            Event::VoiceStateUpdate(proto::VoiceStateUpdate {
                voice_state: Some(state),
            }) => Some(state.clone()),
            _ => None,
        })
        .await;

    assert_eq!(left.user_id, cluster.owner_id);
    assert_eq!(left.channel_id, None);
}
