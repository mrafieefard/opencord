//! Private messages in the client core, against real servers.

use std::time::Duration;

use opencord_common::address::format_fingerprint;
use opencord_core::api::types::{
    CoreEvent, CoreEventPayload, FriendStatus, PrivateEvent, ReadySnapshot,
};
use opencord_core::client::Client;
use opencord_core::identity::Identity;
use opencord_core::private::device::Device;
use opencord_core::private::store::PrivateStore;
use opencord_proto::v1 as proto;
use opencord_server::config::Config;
use opencord_server::private::PrivateCounts;
use opencord_server::server::{self, ServerHandle};
use tokio::sync::mpsc::UnboundedReceiver;

const WAIT: Duration = Duration::from_secs(10);

struct TestServer {
    handle: ServerHandle,
    _dir: tempfile::TempDir,
}

impl TestServer {
    async fn start() -> Self {
        Self::start_with(true).await
    }

    async fn start_with(private_messages: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.server.bind = "127.0.0.1:0".parse().unwrap();
        config.server.public_host = "127.0.0.1".to_owned();
        config.server.data_dir = dir.path().join("data");
        config.server.name = "Test Server".to_owned();
        config.voice.udp_port = 0;
        config.music.audio_udp_port = 0;
        config.network.ip_lookup_v4 = String::new();
        config.network.ip_lookup_v6 = String::new();
        config.private_messages.enabled = private_messages;
        let handle = server::start(config).await.unwrap();
        Self { handle, _dir: dir }
    }

    fn address(&self) -> String {
        format!("127.0.0.1:{}", self.handle.local_addr.port())
    }

    fn fingerprint(&self) -> String {
        format_fingerprint(&self.handle.fingerprint)
    }

    /// Waits until the member's counts satisfy `done`, and returns them.
    async fn counts_until(
        &self,
        user_id: i64,
        mut done: impl FnMut(&PrivateCounts) -> bool,
    ) -> PrivateCounts {
        let deadline = tokio::time::Instant::now() + WAIT;
        loop {
            let counts = self.handle.private_counts(user_id).await;
            if done(&counts) {
                return counts;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "timed out waiting on the private counts, last {counts:?}"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// Waits until the member sent privacy settings.
    async fn privacy_of(&self, user_id: i64) -> (bool, bool, Vec<i64>) {
        let deadline = tokio::time::Instant::now() + WAIT;
        loop {
            if let Some(privacy) = self.handle.private_privacy(user_id) {
                return privacy;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "timed out waiting on the privacy settings"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

struct TestClient {
    client: Client,
    events: UnboundedReceiver<CoreEvent>,
    private: Option<UnboundedReceiver<PrivateEvent>>,
    identity: Identity,
    dir: tempfile::TempDir,
    key: Option<u8>,
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
            private: None,
            identity,
            dir,
            key: None,
        }
    }

    /// Opens private messages with the master key `[key; 32]`.
    async fn open_private(&mut self, key: u8) {
        self.private = Some(self.client.private_events());
        self.client.private_open([key; 32]).await.unwrap();
        self.key = Some(key);
    }

    /// Skips private events until `pick` returns something.
    async fn private_event<T>(&mut self, mut pick: impl FnMut(&PrivateEvent) -> Option<T>) -> T {
        let events = self.private.as_mut().expect("private messages are open");
        tokio::time::timeout(WAIT, async {
            loop {
                let event = events.recv().await.expect("the private events ended");
                if let Some(found) = pick(&event) {
                    return found;
                }
            }
        })
        .await
        .expect("timed out waiting for a private event")
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

    /// Joins `server` as its owner.
    async fn join_as_owner(&mut self, server: &TestServer) -> ReadySnapshot {
        self.client
            .trust_fingerprint(&server.address(), &server.fingerprint())
            .unwrap();
        self.client
            .add_server(&server.address(), server.handle.claim_token.clone())
            .await
            .unwrap();
        self.ready().await
    }

    /// Collects events for `wait`.
    async fn drain(&mut self, wait: Duration) -> Vec<CoreEventPayload> {
        let mut seen = Vec::new();
        while let Ok(Some(event)) = tokio::time::timeout(wait, self.events.recv()).await {
            seen.push(event.payload);
        }
        seen
    }

    /// This device's id, read from its own database.
    async fn device_id(&self) -> [u8; 16] {
        let key = self.key.expect("private messages were opened");
        let opened =
            PrivateStore::open(self.dir.path(), &self.identity.public_key(), &[key; 32], 0)
                .await
                .unwrap();
        assert!(!opened.started_fresh, "the device's database opens again");
        let id = Device::load_or_create(&opened.store, 0).await.unwrap().id();
        opened.store.close().await;
        id
    }

    fn user(&self, ready: &ReadySnapshot, name: &str) -> proto::User {
        proto::User {
            id: ready.self_user.id,
            public_key: self.identity.public_key().to_vec(),
            display_name: name.to_owned(),
            avatar_hash: String::new(),
        }
    }
}

/// The owner and a member who joined through an invite link.
async fn owner_and_member(
    server: &TestServer,
) -> (TestClient, ReadySnapshot, TestClient, ReadySnapshot) {
    let mut owner = TestClient::new("Owner");
    let owner_ready = owner.join_as_owner(server).await;
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

fn registered(counts: &PrivateCounts) -> bool {
    counts.devices == 1 && counts.one_time_keys == 20
}

#[tokio::test(flavor = "multi_thread")]
async fn opening_registers_this_device_on_every_server_with_private_messages() {
    let with = TestServer::start().await;
    let without = TestServer::start_with(false).await;
    let mut client = TestClient::new("Owner");
    client.open_private(1).await;

    let with_ready = client.join_as_owner(&with).await;
    let without_ready = client.join_as_owner(&without).await;

    let counts = with.counts_until(with_ready.self_user.id, registered).await;
    assert_eq!((counts.devices, counts.one_time_keys), (1, 20));
    client.drain(Duration::from_millis(300)).await;
    let elsewhere = without
        .handle
        .private_counts(without_ready.self_user.id)
        .await;
    assert_eq!((elsewhere.devices, elsewhere.one_time_keys), (0, 0));
}

#[tokio::test(flavor = "multi_thread")]
async fn opening_after_ready_registers_too() {
    let server = TestServer::start().await;
    let mut client = TestClient::new("Owner");
    let ready = client.join_as_owner(&server).await;
    let before = server.handle.private_counts(ready.self_user.id).await;
    assert_eq!(before.devices, 0);

    client.open_private(1).await;

    server.counts_until(ready.self_user.id, registered).await;
    // Opening again for the same identity changes nothing.
    let again = client.client.private_open([1; 32]).await.unwrap();
    assert!(!again.started_fresh);
    client.drain(Duration::from_millis(300)).await;
    let after = server.handle.private_counts(ready.self_user.id).await;
    assert_eq!((after.devices, after.one_time_keys), (1, 20));
}

#[tokio::test(flavor = "multi_thread")]
async fn privacy_reaches_each_server_on_registration() {
    let first = TestServer::start().await;
    let second = TestServer::start().await;
    let mut client = TestClient::new("Owner");
    let first_ready = client.join_as_owner(&first).await;
    let second_ready = client.join_as_owner(&second).await;
    assert_eq!(first.handle.private_privacy(first_ready.self_user.id), None);

    client.open_private(1).await;

    assert_eq!(
        first.privacy_of(first_ready.self_user.id).await,
        (true, true, vec![])
    );
    assert_eq!(
        second.privacy_of(second_ready.self_user.id).await,
        (true, true, vec![])
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_queued_friend_request_becomes_a_pending_contact_and_is_acknowledged() {
    let server = TestServer::start().await;
    let (a, a_ready, mut b, b_ready) = owner_and_member(&server).await;
    b.open_private(2).await;
    server.counts_until(b_ready.self_user.id, registered).await;
    let device_id = b.device_id().await;
    let a_hex = hex::encode(a.identity.public_key());

    server
        .handle
        .queue_private_events(
            b_ready.self_user.id,
            device_id,
            vec![proto::pending_event::Kind::FriendRequestReceived(
                proto::FriendRequestReceived {
                    request_id: 7,
                    from: Some(a.user(&a_ready, "Owner")),
                    slip: a.identity.slip_for(&b.identity.public_key()).to_vec(),
                },
            )],
        )
        .await;

    let contact = b
        .private_event(|event| match event {
            PrivateEvent::ContactChanged(contact) if contact.peer == a_hex => Some(contact.clone()),
            _ => None,
        })
        .await;
    assert_eq!(contact.friend, FriendStatus::Incoming);
    assert_eq!(contact.name.as_deref(), Some("Owner"));
    assert!(!contact.request_spam);
    let request = b
        .private_event(|event| match event {
            PrivateEvent::FriendRequestReceived { peer, spam } if *peer == a_hex => Some(*spam),
            _ => None,
        })
        .await;
    assert!(!request, "a first request isn't spam");
    server
        .counts_until(b_ready.self_user.id, |counts| counts.pending == 0)
        .await;
    let contacts = b.client.private_contacts().await.unwrap();
    assert_eq!(contacts.len(), 1);
    assert_eq!(contacts[0].friend, FriendStatus::Incoming);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_device_back_after_many_events_acknowledges_them_all() {
    let server = TestServer::start().await;
    let (a, a_ready, mut b, b_ready) = owner_and_member(&server).await;
    b.open_private(2).await;
    server.counts_until(b_ready.self_user.id, registered).await;
    let device_id = b.device_id().await;
    let TestClient {
        client,
        identity,
        dir,
        ..
    } = b;
    client.shutdown().await;
    drop(client);

    let removed = proto::pending_event::Kind::FriendRemoved(proto::FriendRemoved {
        remover: Some(a.user(&a_ready, "Owner")),
        removed: Some(proto::User {
            id: b_ready.self_user.id,
            public_key: identity.public_key().to_vec(),
            display_name: "Member".to_owned(),
            avatar_hash: String::new(),
        }),
    });
    server
        .handle
        .queue_private_events(b_ready.self_user.id, device_id, vec![removed; 250])
        .await;
    assert_eq!(
        server
            .handle
            .private_counts(b_ready.self_user.id)
            .await
            .pending,
        250
    );

    let mut back = TestClient::with(identity, dir, "Member");
    back.client.connect_saved_servers();
    back.ready().await;
    back.open_private(2).await;

    server
        .counts_until(b_ready.self_user.id, |counts| counts.pending == 0)
        .await;
    let seen = back.drain(Duration::from_millis(500)).await;
    assert!(
        !seen.iter().any(|payload| matches!(
            payload,
            CoreEventPayload::Ready(_) | CoreEventPayload::ConnectionState(_)
        )),
        "the server never closed the connection: {seen:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn removing_a_server_unregisters_the_device_there() {
    let server = TestServer::start().await;
    let mut client = TestClient::new("Owner");
    let ready = client.join_as_owner(&server).await;
    client.open_private(1).await;
    server.counts_until(ready.self_user.id, registered).await;

    client
        .client
        .remove_server(&server.address())
        .await
        .unwrap();

    let counts = server.handle.private_counts(ready.self_user.id).await;
    assert_eq!((counts.devices, counts.one_time_keys), (0, 0));
}
