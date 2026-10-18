//! The client core against a real server.

use std::time::Duration;

use opencord_common::address::format_fingerprint;
use opencord_common::permissions::Permissions;
use opencord_core::api::types::{
    AddServerOutcome, ChannelKind, ConnectionState, CoreError, CoreEvent, CoreEventPayload,
    FailureReason, HotkeyAction, HotkeyBinding, MediaEvent, Message, ReadySnapshot, RoleChanges,
    SoundEffect, SoundSelection, SoundboardSound, VoiceConnectionState, VoiceState,
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
        let mut attempts = 0;
        let handle = loop {
            match server::start(Self::config(&self._dir, port)).await {
                Ok(handle) => break handle,
                Err(_err) if attempts < 50 => {
                    attempts += 1;
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                Err(err) => panic!("could not restart test server: {err:?}"),
            }
        };
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
        config.music.audio_udp_port = 0;
        config.network.ip_lookup_v4 = String::new();
        config.network.ip_lookup_v6 = String::new();
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
        .create_invite(&server.address(), None, None, None)
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
    let invite = owner
        .client
        .create_invite(&key, None, None, None)
        .await
        .unwrap();

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

#[tokio::test]
async fn hotkeys_refuse_keys_they_cannot_use_and_say_where_they_work() {
    let client = TestClient::new("Someone");

    let refused = client
        .client
        .set_hotkeys(&[HotkeyBinding {
            action: HotkeyAction::PushToTalk,
            accelerator: "CTRL+Hyper_L".to_owned(),
        }])
        .await;
    let none = client.client.set_hotkeys(&[]).await;

    assert!(
        matches!(refused, Err(CoreError::InvalidInput { .. })),
        "{refused:?}"
    );
    assert!(none.is_ok(), "{none:?}");
}

/// A 16-bit mono WAV of a 440 Hz tone, in the client's data directory.
fn tone_file(client: &TestClient, seconds: u32) -> String {
    let rate = 44_100u32;
    let mut data = Vec::new();
    for n in 0..rate * seconds {
        let t = n as f32 / rate as f32;
        let value = (0.3 * (std::f32::consts::TAU * 440.0 * t).sin() * 32_767.0) as i16;
        data.extend_from_slice(&value.to_le_bytes());
    }
    let mut file = b"RIFF".to_vec();
    file.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    file.extend_from_slice(b"WAVEfmt ");
    for field in [16u32, 1 | (1 << 16), rate, rate * 2, 2 | (16 << 16)] {
        file.extend_from_slice(&field.to_le_bytes());
    }
    file.extend_from_slice(b"data");
    file.extend_from_slice(&(data.len() as u32).to_le_bytes());
    file.extend_from_slice(&data);
    let path = client.dir.path().join("tone.wav");
    std::fs::write(&path, file).unwrap();
    path.to_string_lossy().into_owned()
}

/// Uploads two seconds of a tone as a new sound.
async fn upload_tone(client: &TestClient, server_key: &str, name: &str) -> SoundboardSound {
    let draft = client
        .client
        .soundboard_open_draft(tone_file(client, 3))
        .await
        .unwrap();
    assert_eq!(draft.duration_ms, 3_000);
    assert_eq!(draft.waveform.len(), 600);
    let selection = SoundSelection {
        draft_id: draft.draft_id,
        start_ms: 500,
        length_ms: 2_000,
        normalize: true,
        volume: 70,
    };
    client
        .client
        .soundboard_upload(
            server_key,
            selection,
            name.to_owned(),
            Some("🔔".to_owned()),
        )
        .await
        .unwrap()
}

fn sound_effect(payload: &CoreEventPayload) -> Option<SoundEffect> {
    match payload {
        CoreEventPayload::VoiceChannelEffect(effect) => Some(effect.clone()),
        _ => None,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_sound_made_from_a_file_is_fetched_by_everyone_and_played_in_the_call() {
    let server = TestServer::start().await;
    let (mut owner, owner_ready, mut member, member_ready) = owner_and_member(&server).await;
    let key = server.address();
    let voice = voice_channel(&owner_ready);

    let sound = upload_tone(&owner, &key, "Beep").await;

    assert_eq!((sound.name.as_str(), sound.volume), ("Beep", 70));
    assert_eq!(
        (sound.emoji.as_deref(), sound.duration_ms),
        (Some("🔔"), 2_000)
    );
    let announced = member
        .wait_for(|payload| match payload {
            CoreEventPayload::SoundboardSoundCreate(sound) => Some(sound.clone()),
            _ => None,
        })
        .await;
    assert_eq!(announced, sound);
    // Fetched in the background before anyone plays it (plan §11.4).
    let cached = member
        .dir
        .path()
        .join(format!("sounds/{}.opus", sound.sha256));
    tokio::time::timeout(WAIT, async {
        while !cached.is_file() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the sound was not fetched");

    owner.client.voice_join(&key, voice).await.unwrap();
    member.client.voice_join(&key, voice).await.unwrap();
    let sound_ref = format!("server:{}", sound.sha256);
    member
        .client
        .soundboard_play(&key, voice, &sound_ref, None)
        .await
        .unwrap();
    let heard = owner.wait_for(sound_effect).await;
    let again = member
        .client
        .soundboard_play(&key, voice, &sound_ref, None)
        .await;

    assert_eq!(heard.user_id, member_ready.self_user.id);
    assert_eq!((heard.sound_ref, heard.duration_ms), (sound_ref, 2_000));
    assert!(matches!(
        again,
        Err(CoreError::Server {
            code: opencord_core::api::types::ErrorCode::SoundCooldown,
            ..
        })
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_sound_from_another_server_is_uploaded_there_the_first_time_it_is_played() {
    let here = TestServer::start().await;
    let elsewhere = TestServer::start().await;
    let (mut owner, owner_ready, mut member, _) = owner_and_member(&here).await;
    owner
        .client
        .trust_fingerprint(&elsewhere.address(), &elsewhere.fingerprint())
        .unwrap();
    owner
        .client
        .add_server(&elsewhere.address(), elsewhere.handle.claim_token.clone())
        .await
        .unwrap();
    owner.ready().await;
    let sound = upload_tone(&owner, &elsewhere.address(), "Far away").await;
    let (key, voice) = (here.address(), voice_channel(&owner_ready));
    owner.client.voice_join(&key, voice).await.unwrap();
    member.client.voice_join(&key, voice).await.unwrap();

    let sound_ref = format!("external:{}", sound.sha256);
    owner
        .client
        .soundboard_play(
            &key,
            voice,
            &sound_ref,
            Some((elsewhere.address(), sound.clone())),
        )
        .await
        .unwrap();
    let heard = member.wait_for(sound_effect).await;

    assert_eq!(heard.sound_ref, sound_ref);
    assert_eq!((heard.name.as_str(), heard.volume), ("Far away", 70));
    assert_eq!(heard.duration_ms, 2_000);
}

/// A TCP relay to the server that can stop moving bytes without closing
/// anything, as a sleeping computer's connections do, and on waking drops
/// the connections it had, as a router that forgot them would.
struct Relay {
    address: String,
    frozen: std::sync::Arc<std::sync::atomic::AtomicBool>,
    generation: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl Relay {
    async fn start(target: std::net::SocketAddr) -> Self {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let frozen = Arc::new(AtomicBool::new(false));
        let generation = Arc::new(AtomicU64::new(0));
        let (frozen_in, generation_in) = (Arc::clone(&frozen), Arc::clone(&generation));
        tokio::spawn(async move {
            while let Ok((client, _)) = listener.accept().await {
                let Ok(server) = tokio::net::TcpStream::connect(target).await else {
                    continue;
                };
                let born = generation_in.load(Ordering::SeqCst);
                let (client_read, client_write) = client.into_split();
                let (server_read, server_write) = server.into_split();
                for (mut from, mut to) in [
                    (
                        Box::new(client_read) as Box<dyn tokio::io::AsyncRead + Send + Unpin>,
                        Box::new(server_write) as Box<dyn tokio::io::AsyncWrite + Send + Unpin>,
                    ),
                    (Box::new(server_read), Box::new(client_write)),
                ] {
                    let (frozen, generation) = (Arc::clone(&frozen_in), Arc::clone(&generation_in));
                    tokio::spawn(async move {
                        use tokio::io::{AsyncReadExt, AsyncWriteExt};
                        let mut buffer = vec![0; 16 * 1024];
                        loop {
                            let Ok(read) = from.read(&mut buffer).await else {
                                return;
                            };
                            while frozen.load(Ordering::SeqCst) {
                                tokio::time::sleep(Duration::from_millis(20)).await;
                            }
                            if read == 0 || generation.load(Ordering::SeqCst) != born {
                                return;
                            }
                            if to.write_all(&buffer[..read]).await.is_err() {
                                return;
                            }
                        }
                    });
                }
            }
        });
        Self {
            address,
            frozen,
            generation,
        }
    }

    fn sleep(&self) {
        self.frozen.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    fn wake(&self) {
        self.generation
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.frozen
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

#[tokio::test]
async fn after_the_computer_sleeps_the_call_comes_back_within_seconds() {
    let server = TestServer::start().await;
    let relay = Relay::start(server.handle.local_addr).await;
    let mut owner = TestClient::new("Owner");
    owner
        .client
        .trust_fingerprint(&relay.address, &server.fingerprint())
        .unwrap();
    owner
        .client
        .add_server(&relay.address, server.handle.claim_token.clone())
        .await
        .unwrap();
    let ready = owner.ready().await;
    let voice = voice_channel(&ready);
    owner
        .client
        .voice_join(&relay.address, voice)
        .await
        .unwrap();
    owner.wait_for(voice_of(ready.self_user.id)).await;

    relay.sleep();
    tokio::time::sleep(Duration::from_secs(2)).await;
    relay.wake();
    let woke = std::time::Instant::now();
    // What the wake watcher does when the wall clock jumps.
    owner.client.check_connections().await;

    // Back well before the next heartbeat (30 s) would have noticed.
    owner
        .wait_for(|payload| match payload {
            CoreEventPayload::ConnectionState(ConnectionState::Reconnecting { .. }) => Some(()),
            _ => None,
        })
        .await;
    owner
        .wait_for(|payload| match payload {
            CoreEventPayload::ConnectionState(ConnectionState::Connected) => Some(()),
            _ => None,
        })
        .await;
    assert!(
        woke.elapsed() < Duration::from_secs(8),
        "{:?}",
        woke.elapsed()
    );
    let mut probe = TestClient::new("Probe");
    probe
        .client
        .trust_fingerprint(&server.address(), &server.fingerprint())
        .unwrap();
    let invite = owner
        .client
        .create_invite(&relay.address, None, None, None)
        .await
        .unwrap();
    let link = invite.link.replace(&relay.address, &server.address());
    probe.client.add_server(&link, None).await.unwrap();
    let seen = probe.ready().await;
    let held = seen
        .voice_states
        .iter()
        .find(|state| state.user_id == ready.self_user.id)
        .copied();
    assert_eq!(held.and_then(|state| state.channel_id), Some(voice));
}

/// Joining the channel this device is already in leaves its call alone:
/// no new token, no reconnection.
#[tokio::test]
async fn joining_the_channel_this_device_is_in_keeps_the_call() {
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
    assert_eq!(
        states_until_connected(&mut media).await,
        connecting_to(voice)
    );

    member
        .client
        .voice_join(&server.address(), voice)
        .await
        .unwrap();

    let after = tokio::time::timeout(Duration::from_secs(1), media.recv()).await;
    assert!(
        !matches!(after, Ok(Some(MediaEvent::ConnectionState { .. }))),
        "the call reconnected: {after:?}"
    );
}

/// A client on its own runtime, so it can die the way a closed app does:
/// every task and socket at once.
fn client_on(
    runtime: &tokio::runtime::Runtime,
    identity: Identity,
    dir: tempfile::TempDir,
) -> TestClient {
    let (client, events) = Client::new(dir.path(), runtime.handle().clone()).unwrap();
    client.set_identity(identity.clone(), "Member".to_owned());
    TestClient {
        client,
        events,
        identity,
        dir,
    }
}

fn own_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
}

/// The app closed while in voice: the server and the node still hold the
/// old session for their grace periods, and the next launch joins at once.
#[tokio::test(flavor = "multi_thread")]
async fn after_the_app_dies_in_voice_the_next_launch_gets_voice_back() {
    let server = TestServer::start().await;
    let (_owner, owner_ready, member, _) = owner_and_member(&server).await;
    let voice = voice_channel(&owner_ready);
    let (identity, dir) = (member.identity.clone(), member.dir);
    drop(member.client);

    let first = own_runtime();
    let mut app = client_on(&first, identity.clone(), dir);
    app.client.connect_saved_servers();
    app.ready().await;
    let mut media = app
        .client
        .enable_media(MediaOptions {
            open_devices: false,
        })
        .unwrap();
    app.client
        .voice_join(&server.address(), voice)
        .await
        .unwrap();
    assert_eq!(
        states_until_connected(&mut media).await,
        connecting_to(voice)
    );

    // The app is closed while in voice: everything stops at once.
    let dir = app.dir;
    drop(media);
    first.shutdown_background();
    tokio::time::sleep(Duration::from_millis(300)).await;

    let second = own_runtime();
    let mut app = client_on(&second, identity, dir);
    app.client.connect_saved_servers();
    app.ready().await;
    let mut media = app
        .client
        .enable_media(MediaOptions {
            open_devices: false,
        })
        .unwrap();
    app.client
        .voice_join(&server.address(), voice)
        .await
        .unwrap();
    assert_eq!(
        states_until_connected(&mut media).await,
        connecting_to(voice)
    );
    drop(media);
    second.shutdown_background();
}
