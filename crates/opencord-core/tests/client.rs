//! The client core against a real server.

use std::time::Duration;

use opencord_common::address::format_fingerprint;
use opencord_common::permissions::Permissions;
use opencord_core::api::types::{
    AddServerOutcome, ConnectionState, CoreError, CoreEvent, CoreEventPayload, FailureReason,
    Message, ReadySnapshot, RoleChanges,
};
use opencord_core::client::Client;
use opencord_core::identity::Identity;
use opencord_server::config::Config;
use opencord_server::server::{self, ServerHandle};
use tokio::sync::mpsc::UnboundedReceiver;

const WAIT: Duration = Duration::from_secs(10);

struct TestServer {
    handle: ServerHandle,
    _dir: tempfile::TempDir,
}

impl TestServer {
    async fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.server.bind = "127.0.0.1:0".parse().unwrap();
        config.server.public_host = "127.0.0.1".to_owned();
        config.server.data_dir = dir.path().join("data");
        config.server.name = "Test Server".to_owned();
        let handle = server::start(config).await.unwrap();
        Self { handle, _dir: dir }
    }

    fn address(&self) -> String {
        format!("127.0.0.1:{}", self.handle.local_addr.port())
    }

    fn fingerprint(&self) -> String {
        format_fingerprint(&self.handle.fingerprint)
    }
}

struct TestClient {
    client: Client,
    events: UnboundedReceiver<CoreEvent>,
    identity: Identity,
    dir: tempfile::TempDir,
}

impl TestClient {
    fn new(name: &str) -> Self {
        Self::with(Identity::generate(), tempfile::tempdir().unwrap(), name)
    }

    fn with(identity: Identity, dir: tempfile::TempDir, name: &str) -> Self {
        let (client, events) = Client::new(dir.path(), tokio::runtime::Handle::current()).unwrap();
        client.set_identity(identity.clone(), name.to_owned());
        Self {
            client,
            events,
            identity,
            dir,
        }
    }

    /// Skips events until `pick` returns something.
    async fn wait_for<T>(&mut self, mut pick: impl FnMut(&CoreEventPayload) -> Option<T>) -> T {
        tokio::time::timeout(WAIT, async {
            loop {
                let event = self.events.recv().await.expect("the event stream ended");
                if let Some(found) = pick(&event.payload) {
                    return found;
                }
            }
        })
        .await
        .expect("timed out waiting for an event")
    }

    async fn ready(&mut self) -> ReadySnapshot {
        self.wait_for(|payload| match payload {
            CoreEventPayload::Ready(ready) => Some(ready.clone()),
            _ => None,
        })
        .await
    }

    async fn message(&mut self) -> Message {
        self.wait_for(|payload| match payload {
            CoreEventPayload::MessageCreate(message) => Some(message.clone()),
            _ => None,
        })
        .await
    }

    /// Collects events for `wait`.
    async fn drain(&mut self, wait: Duration) -> Vec<CoreEventPayload> {
        let mut seen = Vec::new();
        while let Ok(Some(event)) = tokio::time::timeout(wait, self.events.recv()).await {
            seen.push(event.payload);
        }
        seen
    }
}

/// The owner (trusting the certificate and claiming the server) and a member
/// who joined through an invite link.
async fn owner_and_member(
    server: &TestServer,
) -> (TestClient, ReadySnapshot, TestClient, ReadySnapshot) {
    let mut owner = TestClient::new("Owner");
    let claim = server.handle.claim_token.clone();
    owner
        .client
        .trust_fingerprint(&server.address(), &server.fingerprint())
        .unwrap();
    owner
        .client
        .add_server(&server.address(), claim)
        .await
        .unwrap();
    let owner_ready = owner.ready().await;
    let invite = owner
        .client
        .create_invite(&server.address(), None, None)
        .await
        .unwrap();
    let mut member = TestClient::new("Member");
    member.client.add_server(&invite.link, None).await.unwrap();
    let member_ready = member.ready().await;
    (owner, owner_ready, member, member_ready)
}

/// Permission bits as the API carries them.
fn bits(permissions: Permissions) -> i64 {
    i64::from_ne_bytes(permissions.bits().to_ne_bytes())
}

fn permissions(bits: i64) -> Permissions {
    Permissions::from_bits_truncate(u64::from_ne_bytes(bits.to_ne_bytes()))
}

fn general(ready: &ReadySnapshot) -> i64 {
    ready
        .channels
        .iter()
        .find(|channel| channel.name == "general")
        .unwrap()
        .id
}

#[tokio::test(flavor = "multi_thread")]
async fn unknown_certificates_need_trust_before_claiming() {
    let server = TestServer::start().await;
    let mut owner = TestClient::new("Owner");
    let claim = server.handle.claim_token.clone();

    let first = owner
        .client
        .add_server(&server.address(), claim.clone())
        .await
        .unwrap();
    assert_eq!(
        first,
        AddServerOutcome::NeedsTrust {
            address: server.address(),
            fingerprint: server.fingerprint()
        }
    );
    assert!(owner.client.servers().is_empty());

    owner
        .client
        .trust_fingerprint(&server.address(), &server.fingerprint())
        .unwrap();
    let second = owner
        .client
        .add_server(&server.address(), claim)
        .await
        .unwrap();
    let ready = owner.ready().await;

    let AddServerOutcome::Added(added) = second else {
        panic!("expected the server to be added, got {second:?}");
    };
    assert_eq!(added.name, "Test Server");
    assert_eq!(added.fingerprint, Some(server.fingerprint()));
    assert_eq!(ready.server.owner_id, Some(ready.self_user.id));
    assert_eq!(ready.self_user.fingerprint, owner.identity.fingerprint());
    assert_eq!(owner.client.servers(), vec![added]);
}

#[tokio::test(flavor = "multi_thread")]
async fn members_join_by_invite_link_and_chat() {
    let server = TestServer::start().await;
    let (mut owner, owner_ready, mut member, member_ready) = owner_and_member(&server).await;
    let channel = general(&owner_ready);
    let key = server.address();

    let sent = member
        .client
        .send_message(
            &key,
            channel,
            "hello from the core".to_owned(),
            "n1".to_owned(),
        )
        .await
        .unwrap();
    let seen_by_owner = owner.message().await;
    owner
        .client
        .send_message(&key, channel, "welcome".to_owned(), "n2".to_owned())
        .await
        .unwrap();
    let reply = member
        .wait_for(|payload| match payload {
            CoreEventPayload::MessageCreate(message) if message.content == "welcome" => {
                Some(message.clone())
            }
            _ => None,
        })
        .await;
    let history = member
        .client
        .fetch_messages(&key, channel, None, 10)
        .await
        .unwrap();

    assert_eq!(sent.nonce.as_deref(), Some("n1"));
    assert_eq!(seen_by_owner.id, sent.id);
    assert_eq!(seen_by_owner.author_id, member_ready.self_user.id);
    assert_eq!(reply.author_id, owner_ready.self_user.id);
    assert_eq!(
        history.iter().map(|message| message.id).collect::<Vec<_>>(),
        [reply.id, sent.id]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dropped_connection_is_resumed_without_losing_messages() {
    let server = TestServer::start().await;
    let (owner, owner_ready, mut member, member_ready) = owner_and_member(&server).await;
    let key = server.address();
    member.drain(Duration::from_millis(200)).await;

    server
        .handle
        .drop_user_connections(member_ready.self_user.id);
    member
        .wait_for(|payload| match payload {
            CoreEventPayload::ConnectionState(ConnectionState::Reconnecting { .. }) => Some(()),
            _ => None,
        })
        .await;
    let missed = owner
        .client
        .send_message(
            &key,
            general(&owner_ready),
            "while you were away".to_owned(),
            "n".to_owned(),
        )
        .await
        .unwrap();
    let mut after_reconnect = Vec::new();
    let received = member
        .wait_for(|payload| {
            after_reconnect.push(payload.clone());
            match payload {
                CoreEventPayload::MessageCreate(message) => Some(message.clone()),
                _ => None,
            }
        })
        .await;

    assert_eq!(received.id, missed.id);
    assert!(after_reconnect.contains(&CoreEventPayload::ConnectionState(
        ConnectionState::Connected
    )));
    assert!(
        !after_reconnect
            .iter()
            .any(|payload| matches!(payload, CoreEventPayload::Ready(_))),
        "a resume must not start a fresh session"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_changed_certificate_is_refused() {
    let server = TestServer::start().await;
    let client = TestClient::new("Owner");
    client
        .client
        .trust_fingerprint(&server.address(), &"00".repeat(32))
        .unwrap();

    let result = client
        .client
        .add_server(&server.address(), server.handle.claim_token.clone())
        .await;

    assert!(
        matches!(result, Err(CoreError::FingerprintMismatch { .. })),
        "{result:?}"
    );
    assert!(client.client.servers().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn saved_servers_reconnect_after_a_restart() {
    let server = TestServer::start().await;
    let (owner, _, _member, _) = owner_and_member(&server).await;
    let TestClient {
        client,
        identity,
        dir,
        ..
    } = owner;
    client.shutdown().await;

    let mut restarted = TestClient::with(identity, dir, "Owner");
    restarted.client.connect_saved_servers();
    let ready = restarted.ready().await;

    assert_eq!(restarted.client.servers().len(), 1);
    assert_eq!(ready.server.owner_id, Some(ready.self_user.id));
}

#[tokio::test(flavor = "multi_thread")]
async fn kicked_members_stop_reconnecting() {
    let server = TestServer::start().await;
    let (owner, _, mut member, member_ready) = owner_and_member(&server).await;
    let key = server.address();

    owner
        .client
        .kick_member(&key, member_ready.self_user.id, None)
        .await
        .unwrap();
    let reason = member
        .wait_for(|payload| match payload {
            CoreEventPayload::ConnectionState(ConnectionState::Failed { reason, .. }) => {
                Some(*reason)
            }
            _ => None,
        })
        .await;
    let request = member
        .client
        .send_message(&key, 1, "still here?".to_owned(), "n".to_owned())
        .await;

    assert_eq!(reason, FailureReason::Kicked);
    assert_eq!(request, Err(CoreError::NotConnected));
}

#[tokio::test(flavor = "multi_thread")]
async fn permission_changes_are_reported() {
    let server = TestServer::start().await;
    let (owner, _, mut member, member_ready) = owner_and_member(&server).await;
    let key = server.address();

    let role = owner
        .client
        .create_role(
            &key,
            "mods".to_owned(),
            0,
            bits(Permissions::MANAGE_MESSAGES),
            false,
            false,
        )
        .await
        .unwrap();
    owner
        .client
        .add_member_role(&key, member_ready.self_user.id, role.id)
        .await
        .unwrap();
    let server_permissions = member
        .wait_for(|payload| match payload {
            CoreEventPayload::PermissionsUpdate {
                server_permissions, ..
            } => Some(*server_permissions),
            _ => None,
        })
        .await;
    owner
        .client
        .update_role(
            &key,
            role.id,
            RoleChanges {
                permissions: Some(bits(Permissions::KICK_MEMBERS)),
                ..RoleChanges::default()
            },
        )
        .await
        .unwrap();
    let after_update = member
        .wait_for(|payload| match payload {
            CoreEventPayload::PermissionsUpdate {
                server_permissions, ..
            } => Some(*server_permissions),
            _ => None,
        })
        .await;

    assert!(permissions(server_permissions).contains(Permissions::MANAGE_MESSAGES));
    assert!(permissions(after_update).contains(Permissions::KICK_MEMBERS));
    assert!(!permissions(after_update).contains(Permissions::MANAGE_MESSAGES));
}

#[tokio::test(flavor = "multi_thread")]
async fn server_errors_keep_their_code() {
    let server = TestServer::start().await;
    let (_owner, owner_ready, member, _) = owner_and_member(&server).await;

    let result = member
        .client
        .delete_channel(&server.address(), general(&owner_ready))
        .await;

    assert!(
        matches!(
            result,
            Err(CoreError::Server {
                code: opencord_core::api::types::ErrorCode::Forbidden,
                ..
            })
        ),
        "{result:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_kicked_member_can_rejoin_with_a_new_invite() {
    let server = TestServer::start().await;
    let (owner, _, mut member, member_ready) = owner_and_member(&server).await;
    let key = server.address();
    owner
        .client
        .kick_member(&key, member_ready.self_user.id, None)
        .await
        .unwrap();
    member
        .wait_for(|payload| match payload {
            CoreEventPayload::ConnectionState(ConnectionState::Failed { .. }) => Some(()),
            _ => None,
        })
        .await;
    let invite = owner.client.create_invite(&key, None, None).await.unwrap();

    let client = member.client.clone();
    let outcome = tokio::time::timeout(
        WAIT,
        tokio::spawn(async move { client.add_server(&invite.link, None).await }),
    )
    .await
    .expect("adding a saved server again must not hang")
    .unwrap()
    .unwrap();
    let ready = member.ready().await;

    assert!(matches!(outcome, AddServerOutcome::Added(_)), "{outcome:?}");
    assert_eq!(ready.self_user.id, member_ready.self_user.id);
    assert_eq!(member.client.servers().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn adding_a_connected_server_again_reconnects_it() {
    let server = TestServer::start().await;
    let (mut owner, owner_ready, _member, _) = owner_and_member(&server).await;

    let client = owner.client.clone();
    let address = server.address();
    let outcome = tokio::time::timeout(
        WAIT,
        tokio::spawn(async move { client.add_server(&address, None).await }),
    )
    .await
    .expect("adding a connected server again must not hang")
    .unwrap()
    .unwrap();
    let ready = owner.ready().await;

    assert!(matches!(outcome, AddServerOutcome::Added(_)), "{outcome:?}");
    assert_eq!(ready.self_user.id, owner_ready.self_user.id);
    assert_eq!(owner.client.servers().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn retry_now_skips_the_wait_before_reconnecting() {
    let server = TestServer::start().await;
    let (_owner, _, mut member, member_ready) = owner_and_member(&server).await;
    let key = server.address();
    member.drain(Duration::from_millis(200)).await;

    server
        .handle
        .drop_user_connections(member_ready.self_user.id);
    let wait = member
        .wait_for(|payload| match payload {
            CoreEventPayload::ConnectionState(ConnectionState::Reconnecting {
                retry_in_ms,
                ..
            }) => Some(Duration::from_millis(u64::from(*retry_in_ms))),
            _ => None,
        })
        .await;
    let asked = tokio::time::Instant::now();
    member.client.retry_now(&key).await.unwrap();
    member
        .wait_for(|payload| match payload {
            CoreEventPayload::ConnectionState(ConnectionState::Connected) => Some(()),
            _ => None,
        })
        .await;

    assert!(
        asked.elapsed() < wait / 2,
        "reconnected after {:?}; the wait was {wait:?}",
        asked.elapsed()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn retry_now_tries_again_after_a_failure() {
    let server = TestServer::start().await;
    let (owner, _, mut member, member_ready) = owner_and_member(&server).await;
    let key = server.address();
    owner
        .client
        .kick_member(&key, member_ready.self_user.id, None)
        .await
        .unwrap();
    member
        .wait_for(|payload| match payload {
            CoreEventPayload::ConnectionState(ConnectionState::Failed { .. }) => Some(()),
            _ => None,
        })
        .await;

    member.client.retry_now(&key).await.unwrap();
    let states = member
        .wait_for({
            let mut seen = Vec::new();
            move |payload| {
                if let CoreEventPayload::ConnectionState(state) = payload {
                    seen.push(state.clone());
                    if matches!(state, ConnectionState::Failed { .. }) {
                        return Some(seen.clone());
                    }
                }
                None
            }
        })
        .await;

    assert_eq!(states.first(), Some(&ConnectionState::Connecting));
    assert!(matches!(
        states.last(),
        Some(ConnectionState::Failed {
            reason: FailureReason::Rejected,
            ..
        })
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_certificate_change_on_reconnect_reports_both_fingerprints() {
    let server = TestServer::start().await;
    let (_owner, _, mut member, member_ready) = owner_and_member(&server).await;
    let key = server.address();
    let pinned = "00".repeat(32);
    member.client.trust_fingerprint(&key, &pinned).unwrap();

    server
        .handle
        .drop_user_connections(member_ready.self_user.id);
    let failed = member
        .wait_for(|payload| match payload {
            CoreEventPayload::ConnectionState(state @ ConnectionState::Failed { .. }) => {
                Some(state.clone())
            }
            _ => None,
        })
        .await;

    assert_eq!(
        failed,
        ConnectionState::Failed {
            reason: FailureReason::FingerprintChanged,
            message: "the server's certificate no longer matches the trusted one".to_owned(),
            expected_fingerprint: Some(pinned),
            presented_fingerprint: Some(server.fingerprint()),
        }
    );
}
