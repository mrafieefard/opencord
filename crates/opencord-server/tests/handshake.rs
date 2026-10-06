mod common;

use std::collections::BTreeSet;
use std::time::Duration;

use common::{TestClient, TestServer, key, owner};
use opencord_common::permissions::Permissions;
use opencord_proto::v1 as proto;
use proto::envelope::Payload;

#[tokio::test]
async fn hello_describes_the_server() {
    let server = TestServer::start().await;

    let client = TestClient::connect(&server, key(1)).await;

    assert_eq!(client.hello.server_name, "Test Server");
    assert_eq!(client.hello.server_id.len(), 16);
    assert_eq!(client.hello.nonce.len(), 32);
    assert_eq!(client.hello.heartbeat_interval_ms, 30_000);
    assert_eq!(
        client.hello.protocol_version,
        opencord_common::PROTOCOL_VERSION
    );
}

#[tokio::test]
async fn nonces_are_fresh_per_connection() {
    let server = TestServer::start().await;

    let first = TestClient::connect(&server, key(1)).await;
    let second = TestClient::connect(&server, key(1)).await;

    assert_ne!(first.hello.nonce, second.hello.nonce);
}

#[tokio::test]
async fn claiming_ownership_gives_a_ready_with_the_defaults() {
    let server = TestServer::start().await;

    let (_client, ready) = owner(&server).await;

    let me = ready.self_user.unwrap();
    let server_info = ready.server.unwrap();
    assert_eq!(me.display_name, "Owner");
    assert_eq!(me.public_key, key(1).verifying_key().to_bytes());
    assert_eq!(server_info.owner_id, Some(me.id));
    assert_eq!(server_info.name, "Test Server");
    let channels: BTreeSet<(i32, String)> = ready
        .channels
        .iter()
        .map(|channel| (channel.kind, channel.name.clone()))
        .collect();
    assert_eq!(
        channels,
        BTreeSet::from([
            (proto::ChannelKind::Text as i32, "general".to_owned()),
            (proto::ChannelKind::Voice as i32, "General".to_owned()),
        ])
    );
    assert_eq!(ready.roles.len(), 1);
    assert_eq!(ready.roles[0].id, server_info.everyone_role_id);
    assert_eq!(ready.roles[0].position, 0);
    assert_eq!(
        ready.roles[0].permissions,
        Permissions::DEFAULT_EVERYONE.bits()
    );
    assert_eq!(ready.members.len(), 1);
    assert_eq!(ready.members[0].user.as_ref().unwrap().id, me.id);
    assert_eq!(ready.server_permissions, Permissions::all().bits());
    assert_eq!(ready.channel_permissions.len(), 2);
    assert!(
        ready
            .channel_permissions
            .values()
            .all(|permissions| *permissions == Permissions::all().bits())
    );
    assert!(
        ready
            .presences
            .iter()
            .any(|presence| presence.user_id == me.id
                && presence.status == proto::PresenceStatus::Online as i32)
    );
    assert!(!ready.session_id.is_empty());
    assert_eq!(ready.resume_token.len(), 32);
}

#[tokio::test]
async fn the_claim_token_works_only_once() {
    let server = TestServer::start().await;
    let token = server.claim_token();
    owner(&server).await;

    let mut second = TestClient::connect(&server, key(2)).await;
    let error = second
        .identify("Thief", None, Some(&token))
        .await
        .unwrap_err();

    assert_eq!(error.code, proto::ErrorCode::Unauthorized as i32);
    assert_eq!(second.expect_close().await, Some(4003));
}

#[tokio::test]
async fn strangers_need_an_invite() {
    let server = TestServer::start().await;
    owner(&server).await;

    let mut stranger = TestClient::connect(&server, key(2)).await;
    let error = stranger.identify("Stranger", None, None).await.unwrap_err();

    assert_eq!(error.code, proto::ErrorCode::Unauthorized as i32);
    assert_eq!(stranger.expect_close().await, Some(4003));
}

#[tokio::test]
async fn bad_signatures_are_rejected() {
    let server = TestServer::start().await;
    let mut client = TestClient::connect(&server, key(1)).await;
    let mut identify = client.identify_message("Owner", None, Some(&server.claim_token()));
    identify.signature[0] ^= 0xff;

    client.send(Payload::Identify(identify)).await;

    match client.recv_payload().await {
        Payload::Error(error) => assert_eq!(error.code, proto::ErrorCode::Unauthorized as i32),
        other => panic!("expected an error, got {other:?}"),
    }
    assert_eq!(client.expect_close().await, Some(4003));
}

#[tokio::test]
async fn requests_before_identify_close_the_connection() {
    let server = TestServer::start().await;
    let mut client = TestClient::connect(&server, key(1)).await;

    client
        .send(Payload::Request(proto::Request {
            kind: Some(proto::request::Kind::FetchBans(proto::FetchBans {})),
        }))
        .await;

    assert_eq!(client.expect_close().await, Some(4002));
}

#[tokio::test]
async fn heartbeats_are_acknowledged() {
    let server = TestServer::start().await;
    let (mut client, _ready) = owner(&server).await;

    client
        .send(Payload::Heartbeat(proto::Heartbeat { last_seq: 0 }))
        .await;

    assert!(matches!(
        client.recv_payload().await,
        Payload::HeartbeatAck(_)
    ));
}

#[tokio::test]
async fn missing_heartbeats_close_the_connection() {
    let server = TestServer::start_with(|config| config.gateway.heartbeat_interval_ms = 100).await;
    let (mut client, _ready) = owner(&server).await;

    let code = tokio::time::timeout(Duration::from_secs(2), client.expect_close())
        .await
        .unwrap();

    assert_eq!(code, Some(4005));
}

#[tokio::test]
async fn health_and_info_respond() {
    let server = TestServer::start().await;
    owner(&server).await;

    let (health_status, health) = server.get("/health").await;
    let (info_status, info) = server.get("/info").await;

    assert_eq!(health_status, 200);
    assert!(health.contains("\"ok\""), "{health}");
    assert_eq!(info_status, 200);
    assert!(info.contains("\"name\":\"Test Server\""), "{info}");
    assert!(info.contains("\"protocol_version\":1"), "{info}");
    assert!(info.contains("\"member_count\":1"), "{info}");
}
