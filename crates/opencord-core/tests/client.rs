//! The client core against a real server.

use std::time::Duration;

use opencord_common::address::format_fingerprint;
use opencord_common::permissions::Permissions;
use opencord_core::api::types::{
    AddServerOutcome, ChannelKind, ConnectionState, CoreError, CoreEvent, CoreEventPayload,
    FailureReason, MediaEvent, Message, ReadySnapshot, RoleChanges, VoiceConnectionState,
    VoiceState,
};
use opencord_core::client::Client;
use opencord_core::identity::Identity;
use opencord_core::media::MediaOptions;
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
        let handle = server::start(Self::config(&dir, 0)).await.unwrap();
        Self { handle, _dir: dir }
    }

    /// Stops the server and starts it again on the same port and data.
    async fn restart(self) -> Self {
        let port = self.handle.local_addr.port();
        self.handle.shutdown().await;
        let handle = server::start(Self::config(&self._dir, port)).await.unwrap();
        Self {
            handle,
            _dir: self._dir,
        }
    }

    fn config(dir: &tempfile::TempDir, port: u16) -> Config {
        let mut config = Config::default();
        config.server.bind = format!("127.0.0.1:{port}").parse().unwrap();
        config.server.public_host = "127.0.0.1".to_owned();
        config.server.data_dir = dir.path().join("data");
        config.server.name = "Test Server".to_owned();
        config.voice.udp_port = 0;
        config
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
async fn retry_now_says_at_once_that_it_is_retrying() {
    let server = TestServer::start().await;
    let (_owner, _, mut member, member_ready) = owner_and_member(&server).await;
    let key = server.address();
    member.drain(Duration::from_millis(200)).await;
    server
        .handle
        .drop_user_connections(member_ready.self_user.id);
    member
        .wait_for(|payload| {
            matches!(
                payload,
                CoreEventPayload::ConnectionState(ConnectionState::Reconnecting { .. })
            )
            .then_some(())
        })
        .await;

    member.client.retry_now(&key).await.unwrap();
    let next = member
        .wait_for(|payload| match payload {
            CoreEventPayload::ConnectionState(state) => Some(state.clone()),
            _ => None,
        })
        .await;

    // The app shows "Retrying…" instead of the old countdown.
    assert!(
        matches!(next, ConnectionState::Reconnecting { retry_in_ms: 0, .. }),
        "got {next:?}"
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

fn voice_channel(ready: &ReadySnapshot) -> i64 {
    ready
        .channels
        .iter()
        .find(|channel| channel.kind == opencord_core::api::types::ChannelKind::Voice)
        .unwrap()
        .id
}

/// The next voice state for `user_id`.
fn voice_of(user_id: i64) -> impl FnMut(&CoreEventPayload) -> Option<VoiceState> {
    move |payload| match payload {
        CoreEventPayload::VoiceStateUpdate(state) if state.user_id == user_id => Some(*state),
        _ => None,
    }
}

#[tokio::test]
async fn voice_states_reach_members_and_say_whose_device_holds_them() {
    let server = TestServer::start().await;
    let (mut owner, owner_ready, mut member, member_ready) = owner_and_member(&server).await;
    let voice = voice_channel(&owner_ready);
    let member_id = member_ready.self_user.id;

    let joined = member
        .client
        .voice_join(&server.address(), voice)
        .await
        .unwrap();

    assert!(joined.this_device);
    let seen = owner.wait_for(voice_of(member_id)).await;
    assert_eq!((seen.channel_id, seen.this_device), (Some(voice), false));
    let own = member.wait_for(voice_of(member_id)).await;
    assert_eq!((own.channel_id, own.this_device), (Some(voice), true));

    member.client.voice_leave().await.unwrap();

    assert_eq!(owner.wait_for(voice_of(member_id)).await.channel_id, None);
}

#[tokio::test]
async fn self_mute_waits_for_the_next_join_and_follows_into_voice() {
    let server = TestServer::start().await;
    let (mut owner, owner_ready, member, member_ready) = owner_and_member(&server).await;
    let voice = voice_channel(&owner_ready);
    let member_id = member_ready.self_user.id;
    member.client.set_voice_self(Some(true), None);

    let joined = member
        .client
        .voice_join(&server.address(), voice)
        .await
        .unwrap();
    member.client.set_voice_self(Some(false), Some(true));

    assert!(joined.self_mute);
    assert!(owner.wait_for(voice_of(member_id)).await.self_mute);
    let changed = owner.wait_for(voice_of(member_id)).await;
    assert_eq!((changed.self_mute, changed.self_deaf), (false, true));
}

#[tokio::test]
async fn joining_voice_on_another_server_leaves_the_first() {
    let first = TestServer::start().await;
    let second = TestServer::start().await;
    let (mut owner, first_ready, mut member, member_ready) = owner_and_member(&first).await;
    owner
        .client
        .trust_fingerprint(&second.address(), &second.fingerprint())
        .unwrap();
    owner
        .client
        .add_server(&second.address(), second.handle.claim_token.clone())
        .await
        .unwrap();
    let second_ready = owner.ready().await;
    let owner_id = first_ready.self_user.id;
    owner
        .client
        .voice_join(&first.address(), voice_channel(&first_ready))
        .await
        .unwrap();
    assert!(
        member
            .wait_for(voice_of(owner_id))
            .await
            .channel_id
            .is_some()
    );

    owner
        .client
        .voice_join(&second.address(), voice_channel(&second_ready))
        .await
        .unwrap();

    assert_eq!(member.wait_for(voice_of(owner_id)).await.channel_id, None);
    assert_ne!(member_ready.self_user.id, owner_id);
}

#[tokio::test]
async fn voice_comes_back_after_the_server_restarts() {
    let server = TestServer::start().await;
    let (mut owner, owner_ready, _member, _) = owner_and_member(&server).await;
    let voice = voice_channel(&owner_ready);
    let owner_id = owner_ready.self_user.id;
    owner
        .client
        .voice_join(&server.address(), voice)
        .await
        .unwrap();
    owner.wait_for(voice_of(owner_id)).await;

    let server = server.restart().await;
    owner.client.retry_now(&server.address()).await.ok();
    let ready = owner.ready().await;

    assert!(ready.voice_states.is_empty(), "the restart forgot voice");
    let back = owner.wait_for(voice_of(owner_id)).await;
    assert_eq!((back.channel_id, back.this_device), (Some(voice), true));
}

#[tokio::test]
async fn a_refused_rejoin_tells_the_app_this_device_left() {
    let server = TestServer::start().await;
    let (owner, owner_ready, mut member, member_ready) = owner_and_member(&server).await;
    let voice = voice_channel(&owner_ready);
    let member_id = member_ready.self_user.id;
    member
        .client
        .voice_join(&server.address(), voice)
        .await
        .unwrap();
    member.wait_for(voice_of(member_id)).await;
    // Losing Connect keeps people in the channel until they leave.
    owner
        .client
        .set_channel_overwrite(
            &server.address(),
            voice,
            opencord_core::api::types::PermissionOverwrite {
                target_kind: opencord_core::api::types::OverwriteTargetKind::Member,
                target_id: member_id,
                allow: 0,
                deny: bits(Permissions::CONNECT),
            },
        )
        .await
        .unwrap();

    let server = server.restart().await;
    member.client.retry_now(&server.address()).await.ok();
    member.ready().await;

    let left = member.wait_for(voice_of(member_id)).await;
    assert_eq!((left.channel_id, left.this_device), (None, true));
    drop(owner);
}

#[tokio::test]
async fn joining_voice_tells_the_media_engine_where_to_connect() {
    let server = TestServer::start().await;
    let (_owner, owner_ready, member, member_ready) = owner_and_member(&server).await;
    let mut voice_servers = member.client.voice_servers();
    let voice = voice_channel(&owner_ready);

    let joined = member
        .client
        .voice_join(&server.address(), voice)
        .await
        .unwrap();
    let update = tokio::time::timeout(WAIT, voice_servers.recv())
        .await
        .unwrap()
        .unwrap();

    assert!(joined.this_device);
    assert_eq!(update.server_key, server.address());
    assert_eq!(update.channel_id, voice);
    assert_eq!(update.user_id, member_ready.self_user.id);
    assert!(!update.session_id.is_empty());
    assert_eq!(
        update.gateway_url(),
        format!("wss://{}/voice", server.address())
    );
    assert_eq!(
        update.certificate_fingerprint,
        server.handle.fingerprint.to_vec()
    );
    assert!(!update.token.is_empty());
}

/// Voice connection states from voice media, through the next `Connected`.
async fn states_until_connected(
    media: &mut UnboundedReceiver<MediaEvent>,
) -> Vec<(i64, VoiceConnectionState)> {
    tokio::time::timeout(WAIT, async {
        let mut states = Vec::new();
        loop {
            let event = media.recv().await.expect("voice media stopped");
            if let MediaEvent::ConnectionState {
                channel_id, state, ..
            } = event
            {
                let connected = state == VoiceConnectionState::Connected;
                states.push((channel_id, state));
                if connected {
                    return states;
                }
            }
        }
    })
    .await
    .expect("timed out waiting for voice to connect")
}

fn connecting_to(channel_id: i64) -> Vec<(i64, VoiceConnectionState)> {
    vec![
        (channel_id, VoiceConnectionState::Authenticating),
        (channel_id, VoiceConnectionState::RtcConnecting),
        (channel_id, VoiceConnectionState::Connected),
    ]
}

#[tokio::test]
async fn joining_voice_connects_voice_media_to_the_node() {
    let server = TestServer::start().await;
    let (_owner, owner_ready, member, _) = owner_and_member(&server).await;
    let mut media = member
        .client
        .enable_media(MediaOptions {
            open_devices: false,
        })
        .unwrap();
    let voice = voice_channel(&owner_ready);

    member
        .client
        .voice_join(&server.address(), voice)
        .await
        .unwrap();
    let states = states_until_connected(&mut media).await;

    assert_eq!(states, connecting_to(voice));
}

#[tokio::test]
async fn voice_media_follows_a_move_to_another_channel() {
    let server = TestServer::start().await;
    let (owner, owner_ready, member, member_ready) = owner_and_member(&server).await;
    let mut media = member
        .client
        .enable_media(MediaOptions {
            open_devices: false,
        })
        .unwrap();
    let voice = voice_channel(&owner_ready);
    let lounge = owner
        .client
        .create_channel(
            &server.address(),
            ChannelKind::Voice,
            "Lounge".to_owned(),
            None,
            None,
        )
        .await
        .unwrap()
        .id;
    member
        .client
        .voice_join(&server.address(), voice)
        .await
        .unwrap();
    states_until_connected(&mut media).await;

    owner
        .client
        .move_member(&server.address(), member_ready.self_user.id, lounge)
        .await
        .unwrap();
    let states = states_until_connected(&mut media).await;

    assert_eq!(states, connecting_to(lounge));
}

#[tokio::test]
async fn media_can_be_turned_on_only_once() {
    let client = TestClient::new("Someone");
    let options = MediaOptions {
        open_devices: false,
    };

    let first = client.client.enable_media(options);
    let second = client.client.enable_media(options);

    assert!(first.is_some());
    assert!(second.is_none());
}
