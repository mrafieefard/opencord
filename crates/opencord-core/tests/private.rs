//! Private messages in the client core, against real servers.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use opencord_common::address::format_fingerprint;
use opencord_core::api::types::{
    ConnectionState, ConversationStatus, CoreError, CoreEvent, CoreEventPayload, ErrorCode,
    FriendStatus, PrivacySettingsData, PrivateEvent, PrivateMessage, PrivateMessageKind,
    PrivateSendState, ReadySnapshot,
};
use opencord_core::client::Client;
use opencord_core::identity::Identity;
use opencord_core::private::contacts::{self, Contact};
use opencord_core::private::device::Device;
use opencord_core::private::payload;
use opencord_core::private::sessions::{Decrypted, PeerSessions};
use opencord_core::private::store::PrivateStore;
use opencord_proto::v1 as proto;
use opencord_server::config::Config;
use opencord_server::private::PrivateCounts;
use opencord_server::server::{self, ServerHandle};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::task::AbortHandle;

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
        Self::start_in(tempfile::tempdir().unwrap(), 0, private_messages).await
    }

    async fn start_in(dir: tempfile::TempDir, port: u16, private_messages: bool) -> Self {
        let mut config = Config::default();
        config.server.bind = format!("127.0.0.1:{port}").parse().unwrap();
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

    /// Stops the server, then starts it again on the same port and data.
    async fn restart(self, between: impl FnOnce()) -> Self {
        let port = self.handle.local_addr.port();
        let Self { handle, _dir: dir } = self;
        handle.shutdown().await;
        between();
        Self::start_in(dir, port, true).await
    }

    fn address(&self) -> String {
        format!("127.0.0.1:{}", self.handle.local_addr.port())
    }

    /// The private message copies waiting for the device.
    async fn copies_for(&self, user_id: i64, device_id: [u8; 16]) -> Vec<proto::PrivateMessage> {
        self.handle
            .private_pending_for(user_id, device_id)
            .await
            .into_iter()
            .filter_map(|event| match event.kind {
                Some(proto::pending_event::Kind::PrivateMessage(message)) => Some(message),
                _ => None,
            })
            .collect()
    }

    /// Waits until the device has `count` copies waiting, and returns them.
    async fn copies_until(
        &self,
        user_id: i64,
        device_id: [u8; 16],
        count: usize,
    ) -> Vec<proto::PrivateMessage> {
        let deadline = tokio::time::Instant::now() + WAIT;
        loop {
            let copies = self.copies_for(user_id, device_id).await;
            if copies.len() == count {
                return copies;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "timed out waiting for {count} copies, last {}",
                copies.len()
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
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
    name: String,
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
            name: name.to_owned(),
        }
    }

    /// Closes this client and starts it again from its directory, as an app
    /// started again would: it reconnects, then opens private messages.
    async fn reopen(&mut self) {
        let key = self.key.expect("private messages were opened");
        self.client.shutdown().await;
        let dir = std::mem::replace(&mut self.dir, tempfile::tempdir().unwrap());
        let mut back = TestClient::with(self.identity.clone(), dir, &self.name);
        back.client.connect_saved_servers();
        back.ready().await;
        back.open_private(key).await;
        *self = back;
    }

    /// Skips private events until a message `pick` accepts is added.
    async fn message_added(&mut self, pick: impl Fn(&PrivateMessage) -> bool) -> PrivateMessage {
        self.private_event(|event| match event {
            PrivateEvent::MessageAdded(message) if pick(message) => Some(message.clone()),
            _ => None,
        })
        .await
    }

    /// Collects private events until none comes for `wait`.
    async fn drain_private(&mut self, wait: Duration) -> Vec<PrivateEvent> {
        let events = self.private.as_mut().expect("private messages are open");
        let mut seen = Vec::new();
        while let Ok(Some(event)) = tokio::time::timeout(wait, events.recv()).await {
            seen.push(event);
        }
        seen
    }

    /// This device's Curve25519 key, read from its own database.
    async fn curve25519(&self) -> [u8; 32] {
        let store = self.store().await;
        let key = Device::load_or_create(&store, 0)
            .await
            .unwrap()
            .curve25519();
        store.close().await;
        key
    }

    /// Encrypts `payload` for the peer device `to` on this device's session
    /// with it, straight from its database, as its client would.
    async fn encrypt_for(&self, to: [u8; 32], payload: &[u8]) -> (u32, Vec<u8>) {
        let store = self.store().await;
        let mut conn = store.pool().acquire().await.unwrap();
        let mut sessions = PeerSessions::load(&mut conn, store.pickle_key(), to)
            .await
            .unwrap();
        let encrypted = sessions.encrypt(payload).unwrap();
        sessions.save(&mut conn, store.pickle_key()).await.unwrap();
        drop(conn);
        store.close().await;
        encrypted
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

    /// This identity's database, opened beside the client's own.
    async fn store(&self) -> PrivateStore {
        let key = self.key.expect("private messages were opened");
        PrivateStore::open(self.dir.path(), &self.identity.public_key(), &[key; 32], 0)
            .await
            .unwrap()
            .store
    }

    /// Makes `server_id` (hex) the relay for the conversation with `peer`,
    /// as opening the conversation will.
    async fn set_relay(&self, peer: &TestClient, server_id: &str) {
        let store = self.store().await;
        let peer = peer.identity.public_key();
        let mut conn = store.pool().acquire().await.unwrap();
        let mut contact = contacts::load(&mut conn, store.sealer(), peer)
            .await
            .unwrap()
            .unwrap_or_else(|| Contact::new(peer));
        contact.relay_server_id = Some(server_id.to_owned());
        contacts::save(&mut conn, store.sealer(), &contact, 0)
            .await
            .unwrap();
        drop(conn);
        store.close().await;
    }

    /// Another device of this identity, joined to `server` at `address`.
    async fn another_device(&self, address: &str, fingerprint: &str, name: &str) -> TestClient {
        let mut device =
            TestClient::with(self.identity.clone(), tempfile::tempdir().unwrap(), name);
        device
            .client
            .trust_fingerprint(address, fingerprint)
            .unwrap();
        device.client.add_server(address, None).await.unwrap();
        device.ready().await;
        device
    }

    /// Waits for the message `id` to change to `state`.
    async fn message_state(&mut self, id: &str, state: PrivateSendState) -> PrivateMessage {
        self.private_event(|event| match event {
            PrivateEvent::MessageChanged(message) if message.id == id && message.state == state => {
                Some(message.clone())
            }
            _ => None,
        })
        .await
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
    owner_and_member_at(server, &server.address()).await
}

/// As `owner_and_member`, with both reaching the server at `address`.
async fn owner_and_member_at(
    server: &TestServer,
    address: &str,
) -> (TestClient, ReadySnapshot, TestClient, ReadySnapshot) {
    let mut owner = TestClient::new("Owner");
    owner
        .client
        .trust_fingerprint(address, &server.fingerprint())
        .unwrap();
    owner
        .client
        .add_server(address, server.handle.claim_token.clone())
        .await
        .unwrap();
    let owner_ready = owner.ready().await;
    let invite = owner
        .client
        .create_invite(address, None, None, None)
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

/// A TCP proxy in front of a server. It can drop what goes either way, as a
/// network losing everything would, and cut every connection.
struct Proxy {
    address: String,
    /// From the clients to the server.
    drop_up: Arc<AtomicBool>,
    /// From the server to the clients.
    drop_down: Arc<AtomicBool>,
    pumps: Arc<Mutex<Vec<AbortHandle>>>,
    accept: AbortHandle,
}

impl Proxy {
    async fn to(server: &TestServer) -> Self {
        let upstream = server.address();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let drop_up = Arc::new(AtomicBool::new(false));
        let drop_down = Arc::new(AtomicBool::new(false));
        let pumps: Arc<Mutex<Vec<AbortHandle>>> = Arc::default();
        let accept = tokio::spawn({
            let (drop_up, drop_down, pumps) = (drop_up.clone(), drop_down.clone(), pumps.clone());
            async move {
                while let Ok((inbound, _)) = listener.accept().await {
                    // A server that is down: the client sees its connection end.
                    let Ok(outbound) = TcpStream::connect(&upstream).await else {
                        continue;
                    };
                    let (in_read, in_write) = inbound.into_split();
                    let (out_read, out_write) = outbound.into_split();
                    let up = tokio::spawn(pump(in_read, out_write, drop_up.clone()));
                    let down = tokio::spawn(pump(out_read, in_write, drop_down.clone()));
                    pumps
                        .lock()
                        .unwrap()
                        .extend([up.abort_handle(), down.abort_handle()]);
                }
            }
        })
        .abort_handle();
        Self {
            address,
            drop_up,
            drop_down,
            pumps,
            accept,
        }
    }

    /// Ends every connection through the proxy at once.
    fn cut(&self) {
        for pump in self.pumps.lock().unwrap().drain(..) {
            pump.abort();
        }
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        self.accept.abort();
        self.cut();
    }
}

async fn pump(
    mut from: tokio::net::tcp::OwnedReadHalf,
    mut to: tokio::net::tcp::OwnedWriteHalf,
    dropping: Arc<AtomicBool>,
) {
    let mut buffer = vec![0; 16 * 1024];
    loop {
        match from.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(_) if dropping.load(Ordering::SeqCst) => {}
            Ok(read) => {
                if to.write_all(&buffer[..read]).await.is_err() {
                    break;
                }
            }
        }
    }
    // The other side may be gone already.
    let _ = to.shutdown().await;
}

/// A writes to B through a server they share, reached at `address`. Both
/// registered a device; B's client is closed, so what reaches B's device
/// waits on the server.
struct Pair {
    a: TestClient,
    a_ready: ReadySnapshot,
    b: TestClient,
    b_ready: ReadySnapshot,
    b_device: [u8; 16],
}

impl Pair {
    async fn on(server: &TestServer) -> Self {
        Self::at(server, &server.address()).await
    }

    async fn at(server: &TestServer, address: &str) -> Self {
        let (mut a, a_ready, mut b, b_ready) = owner_and_member_at(server, address).await;
        a.open_private(1).await;
        b.open_private(2).await;
        server.counts_until(a_ready.self_user.id, registered).await;
        server.counts_until(b_ready.self_user.id, registered).await;
        let b_device = b.device_id().await;
        b.client.shutdown().await;
        a.set_relay(&b, &a_ready.server.server_id_hex).await;
        Self {
            a,
            a_ready,
            b,
            b_ready,
            b_device,
        }
    }

    fn a_hex(&self) -> String {
        hex::encode(self.a.identity.public_key())
    }

    fn b_hex(&self) -> String {
        hex::encode(self.b.identity.public_key())
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_reaches_every_other_device_of_both_people() {
    let server = TestServer::start().await;
    let (mut a, a_ready, mut b, b_ready) = owner_and_member(&server).await;
    let mut a2 = a
        .another_device(&server.address(), &server.fingerprint(), "Owner")
        .await;
    a.open_private(1).await;
    a2.open_private(3).await;
    b.open_private(2).await;
    server
        .counts_until(a_ready.self_user.id, |counts| {
            counts.devices == 2 && counts.one_time_keys == 40
        })
        .await;
    server.counts_until(b_ready.self_user.id, registered).await;
    let (a1_device, a2_device, b_device) = (
        a.device_id().await,
        a2.device_id().await,
        b.device_id().await,
    );
    a2.client.shutdown().await;
    b.client.shutdown().await;
    a.set_relay(&b, &a_ready.server.server_id_hex).await;
    let b_hex = hex::encode(b.identity.public_key());

    let sent = a
        .client
        .private_send(&b_hex, "  hello there  ".to_owned())
        .await
        .unwrap();

    assert_eq!(sent.state, PrivateSendState::Sent);
    assert!(sent.outgoing);
    assert_eq!(sent.kind, PrivateMessageKind::Text);
    assert_eq!(sent.content, "hello there");
    assert_eq!(sent.peer, b_hex);
    let for_b = server.copies_for(b_ready.self_user.id, b_device).await;
    let for_a2 = server.copies_for(a_ready.self_user.id, a2_device).await;
    assert_eq!((for_b.len(), for_a2.len()), (1, 1));
    assert_eq!(for_b[0].sender_device_id, a1_device.to_vec());
    assert_eq!(
        sent.sent_at_ms, for_b[0].sent_at_ms,
        "the message carries the server's time"
    );
    assert!(
        server
            .copies_for(a_ready.self_user.id, a1_device)
            .await
            .is_empty(),
        "nothing goes to the sending device"
    );
    let stored = a.client.private_messages(&b_hex, None, 10).await.unwrap();
    assert_eq!(stored.len(), 2);
    assert_eq!(stored[0], sent);
    assert_eq!(stored[1].kind, PrivateMessageKind::Divider);
    let contact = a
        .private_event(|event| match event {
            PrivateEvent::ContactChanged(contact) if contact.peer == b_hex => Some(contact.clone()),
            _ => None,
        })
        .await;
    assert_eq!(contact.conversation, ConversationStatus::Accepted);
    assert_eq!(contact.preview.as_deref(), Some("hello there"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_device_added_meanwhile_gets_the_message_after_the_refusal() {
    let server = TestServer::start().await;
    let pair = Pair::on(&server).await;
    let b_hex = pair.b_hex();
    pair.a
        .client
        .private_send(&b_hex, "first".to_owned())
        .await
        .unwrap();
    let b_id = pair.b_ready.self_user.id;
    server.copies_until(b_id, pair.b_device, 1).await;

    // B opens a second device after A cached B's devices.
    let mut b2 = pair
        .b
        .another_device(&server.address(), &server.fingerprint(), "Member")
        .await;
    b2.open_private(4).await;
    // B1 is closed, with the key A claimed gone: 19 and B2's 20.
    server
        .counts_until(b_id, |counts| {
            counts.devices == 2 && counts.one_time_keys == 39
        })
        .await;
    let b2_device = b2.device_id().await;
    b2.client.shutdown().await;

    let second = pair
        .a
        .client
        .private_send(&b_hex, "second".to_owned())
        .await
        .unwrap();

    assert_eq!(second.state, PrivateSendState::Sent);
    assert_eq!(server.copies_for(b_id, b2_device).await.len(), 1);
    assert_eq!(server.copies_for(b_id, pair.b_device).await.len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unsent_message_goes_out_once_after_reconnecting() {
    let server = TestServer::start().await;
    let proxy = Proxy::to(&server).await;
    let mut pair = Pair::at(&server, &proxy.address).await;
    let b_hex = pair.b_hex();
    let b_id = pair.b_ready.self_user.id;
    // The devices and sessions are ready, so the next send asks nothing
    // before its request.
    pair.a
        .client
        .private_send(&b_hex, "first".to_owned())
        .await
        .unwrap();
    server.copies_until(b_id, pair.b_device, 1).await;

    // The request is lost, then the server stops.
    proxy.drop_up.store(true, Ordering::SeqCst);
    let client = pair.a.client.clone();
    let sending = tokio::spawn({
        let b_hex = b_hex.clone();
        async move { client.private_send(&b_hex, "second".to_owned()).await }
    });
    let id = pair
        .a
        .private_event(|event| match event {
            PrivateEvent::MessageAdded(message) if message.content == "second" => {
                Some(message.id.clone())
            }
            _ => None,
        })
        .await;
    let server = server
        .restart(|| proxy.drop_up.store(false, Ordering::SeqCst))
        .await;
    let unsent = sending.await.unwrap().unwrap();
    assert_eq!(unsent.id, id);
    assert_eq!(unsent.state, PrivateSendState::Sending);
    assert_eq!(server.copies_for(b_id, pair.b_device).await.len(), 1);

    // A reconnects to the server started again, and resends its request.
    let sent = pair.a.message_state(&id, PrivateSendState::Sent).await;

    assert_eq!(sent.content, "second");
    server.copies_until(b_id, pair.b_device, 2).await;
    pair.a.drain(Duration::from_millis(500)).await;
    assert_eq!(server.copies_for(b_id, pair.b_device).await.len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_send_retried_after_a_lost_answer_arrives_once() {
    let server = TestServer::start().await;
    let proxy = Proxy::to(&server).await;
    let mut pair = Pair::at(&server, &proxy.address).await;
    let b_hex = pair.b_hex();
    let b_id = pair.b_ready.self_user.id;
    // The devices and sessions are ready, so the next send asks nothing
    // before its request.
    pair.a
        .client
        .private_send(&b_hex, "first".to_owned())
        .await
        .unwrap();
    server.copies_until(b_id, pair.b_device, 1).await;

    // The server stores the message, but its answer is lost.
    proxy.drop_down.store(true, Ordering::SeqCst);
    let client = pair.a.client.clone();
    let sending = tokio::spawn({
        let b_hex = b_hex.clone();
        async move { client.private_send(&b_hex, "hello".to_owned()).await }
    });
    server.copies_until(b_id, pair.b_device, 2).await;
    proxy.cut();
    proxy.drop_down.store(false, Ordering::SeqCst);
    let unsent = sending.await.unwrap().unwrap();
    assert_eq!(unsent.state, PrivateSendState::Sending);
    pair.a
        .wait_for(|payload| match payload {
            CoreEventPayload::ConnectionState(ConnectionState::Reconnecting { .. }) => Some(()),
            _ => None,
        })
        .await;
    pair.a
        .wait_for(|payload| match payload {
            CoreEventPayload::ConnectionState(ConnectionState::Connected) => Some(()),
            _ => None,
        })
        .await;

    pair.a.client.private_retry(&unsent.id).await.unwrap();

    pair.a
        .message_state(&unsent.id, PrivateSendState::Sent)
        .await;
    pair.a.drain(Duration::from_millis(500)).await;
    assert_eq!(server.copies_for(b_id, pair.b_device).await.len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_device_the_identity_did_not_sign_is_never_used() {
    let server = TestServer::start().await;
    let pair = Pair::on(&server).await;
    let b_hex = pair.b_hex();
    let b_id = pair.b_ready.self_user.id;
    // A device with good keys of its own, vouched for by someone else.
    let forger = Identity::generate();
    let forged_dir = tempfile::tempdir().unwrap();
    let forged_store = PrivateStore::open(forged_dir.path(), &forger.public_key(), &[7; 32], 0)
        .await
        .unwrap()
        .store;
    let forged = Device::load_or_create(&forged_store, 0).await.unwrap();
    server
        .handle
        .insert_private_device_unchecked(b_id, forged.keys(&forger), forged.fallback_prekey())
        .await;

    let refused = pair
        .a
        .client
        .private_send(&b_hex, "hello".to_owned())
        .await
        .unwrap_err();

    assert!(
        matches!(
            refused,
            CoreError::Server {
                code: ErrorCode::DeviceListChanged,
                ..
            }
        ),
        "{refused:?}"
    );
    assert!(server.copies_for(b_id, forged.id()).await.is_empty());
    assert!(server.copies_for(b_id, pair.b_device).await.is_empty());
    let stored = pair
        .a
        .client
        .private_messages(&b_hex, None, 10)
        .await
        .unwrap();
    assert_eq!(stored.len(), 2);
    assert_eq!(stored[0].state, PrivateSendState::Failed);
    assert_eq!(stored[1].kind, PrivateMessageKind::Divider);
}

#[tokio::test(flavor = "multi_thread")]
async fn sessions_are_saved_before_sending() {
    let server = TestServer::start().await;
    let pair = Pair::on(&server).await;
    let b_hex = pair.b_hex();
    let b_id = pair.b_ready.self_user.id;
    for text in ["one", "two"] {
        pair.a
            .client
            .private_send(&b_hex, text.to_owned())
            .await
            .unwrap();
    }

    let copies = server.copies_until(b_id, pair.b_device, 2).await;

    assert_ne!(copies[0].body, copies[1].body);
    let a_curve25519 = {
        let store = pair.a.store().await;
        let curve25519 = Device::load_or_create(&store, 0)
            .await
            .unwrap()
            .curve25519();
        store.close().await;
        curve25519
    };
    let store = pair.b.store().await;
    let mut device = Device::load_or_create(&store, 0).await.unwrap();
    let mut conn = store.pool().acquire().await.unwrap();
    let mut sessions = PeerSessions::load(&mut conn, store.pickle_key(), a_curve25519)
        .await
        .unwrap();
    let mut texts = Vec::new();
    for copy in &copies {
        // A reused message key would make the second a duplicate.
        let Decrypted::Plaintext(plaintext) =
            sessions.decrypt(&mut device, copy.olm_type, &copy.body, 0)
        else {
            panic!("each copy decrypts, in order");
        };
        let parsed = payload::parse(&plaintext).unwrap();
        assert_eq!(parsed.recipient, pair.b.identity.public_key());
        assert_eq!(
            parsed.slip,
            Some(pair.a.identity.slip_for(&pair.b.identity.public_key()))
        );
        match parsed.body {
            proto::private_payload::Body::Text(text) => texts.push(text.content),
            other => panic!("a text, not {other:?}"),
        }
    }
    assert_eq!(texts, ["one", "two"]);
}

/// A with two devices and B with one, all open, sharing `server`, with A's
/// first device writing to B through it.
async fn three_devices(server: &TestServer) -> ThreeDevices {
    let (mut a, a_ready, mut b, b_ready) = owner_and_member(server).await;
    let mut a2 = a
        .another_device(&server.address(), &server.fingerprint(), "Owner")
        .await;
    a.open_private(1).await;
    a2.open_private(3).await;
    b.open_private(2).await;
    server
        .counts_until(a_ready.self_user.id, |counts| {
            counts.devices == 2 && counts.one_time_keys == 40
        })
        .await;
    server.counts_until(b_ready.self_user.id, registered).await;
    a.set_relay(&b, &a_ready.server.server_id_hex).await;
    ThreeDevices {
        a,
        a_id: a_ready.self_user.id,
        a2,
        b,
        b_ready,
    }
}

struct ThreeDevices {
    a: TestClient,
    a_id: i64,
    a2: TestClient,
    b: TestClient,
    b_ready: ReadySnapshot,
}

fn text(content: &str) -> proto::private_payload::Body {
    proto::private_payload::Body::Text(proto::PrivateText {
        content: content.to_owned(),
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_reaches_the_other_person_and_the_senders_other_device() {
    let server = TestServer::start().await;
    let ThreeDevices {
        a,
        mut a2,
        mut b,
        b_ready,
        ..
    } = three_devices(&server).await;
    let a_hex = hex::encode(a.identity.public_key());
    let b_hex = hex::encode(b.identity.public_key());

    let sent = a
        .client
        .private_send(&b_hex, "hello there".to_owned())
        .await
        .unwrap();

    let received = b
        .message_added(|message| message.peer == a_hex && message.kind == PrivateMessageKind::Text)
        .await;
    assert_eq!(received.id, sent.id);
    assert_eq!(received.content, "hello there");
    assert_eq!(received.kind, PrivateMessageKind::Text);
    assert!(!received.outgoing);
    assert_eq!(received.state, PrivateSendState::Received);
    assert_eq!(received.sent_at_ms, sent.sent_at_ms, "the server's time");
    let copy = a2
        .message_added(|message| message.peer == b_hex && message.kind == PrivateMessageKind::Text)
        .await;
    assert_eq!(copy.id, sent.id);
    assert_eq!(copy.content, "hello there");
    assert!(copy.outgoing);
    assert_eq!(copy.state, PrivateSendState::Sent);

    let b_msgs = b.client.private_messages(&a_hex, None, 10).await.unwrap();
    assert_eq!(b_msgs.len(), 2);
    assert_eq!(b_msgs[0], received);
    assert_eq!(b_msgs[1].kind, PrivateMessageKind::Divider);
    let b_contacts = b.client.private_contacts().await.unwrap();
    assert_eq!(b_contacts.len(), 1);
    assert_eq!(b_contacts[0].peer, a_hex);
    assert_eq!(b_contacts[0].conversation, ConversationStatus::Request);
    assert_eq!(b_contacts[0].unread, 1);
    let a2_contacts = a2.client.private_contacts().await.unwrap();
    assert_eq!(a2_contacts.len(), 1);
    assert_eq!(a2_contacts[0].peer, b_hex);
    assert_eq!(a2_contacts[0].conversation, ConversationStatus::Accepted);
    server
        .counts_until(b_ready.self_user.id, |counts| counts.pending == 0)
        .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn messages_wait_for_an_offline_device() {
    let server = TestServer::start().await;
    let mut pair = Pair::on(&server).await;
    let (a_hex, b_hex) = (pair.a_hex(), pair.b_hex());
    let b_id = pair.b_ready.self_user.id;
    for content in ["one", "two", "three"] {
        pair.a
            .client
            .private_send(&b_hex, content.to_owned())
            .await
            .unwrap();
    }
    server.copies_until(b_id, pair.b_device, 3).await;

    pair.b.reopen().await;

    let mut arrived = Vec::new();
    for _ in 0..3 {
        arrived.push(
            pair.b
                .message_added(|message| {
                    message.peer == a_hex && message.kind == PrivateMessageKind::Text
                })
                .await,
        );
    }
    let contents: Vec<&str> = arrived.iter().map(|m| m.content.as_str()).collect();
    assert_eq!(contents, ["one", "two", "three"]);
    assert!(
        arrived
            .windows(2)
            .all(|pair| pair[0].sent_at_ms <= pair[1].sent_at_ms)
    );
    let msgs = pair
        .b
        .client
        .private_messages(&a_hex, None, 10)
        .await
        .unwrap();
    let text_msgs: Vec<_> = msgs
        .into_iter()
        .filter(|m| m.kind == PrivateMessageKind::Text)
        .collect();
    assert_eq!(text_msgs.len(), 3);
    server
        .counts_until(b_id, |counts| counts.pending == 0)
        .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_delivered_twice_shows_once() {
    let server = TestServer::start().await;
    let mut pair = Pair::on(&server).await;
    let (a_hex, b_hex) = (pair.a_hex(), pair.b_hex());
    let b_id = pair.b_ready.self_user.id;
    pair.a
        .client
        .private_send(&b_hex, "once".to_owned())
        .await
        .unwrap();
    server.copies_until(b_id, pair.b_device, 1).await;
    let snapshot = server
        .handle
        .private_pending_for(b_id, pair.b_device)
        .await
        .remove(0);
    pair.b.reopen().await;
    pair.b
        .message_added(|message| message.peer == a_hex && message.kind == PrivateMessageKind::Text)
        .await;
    server
        .counts_until(b_id, |counts| counts.pending == 0)
        .await;

    // The same event again, as after a lost acknowledgement…
    server
        .handle
        .requeue_private_event(b_id, pair.b_device, snapshot.clone())
        .await;
    server
        .counts_until(b_id, |counts| counts.pending == 0)
        .await;
    // …and the same Olm message under another event's id, as a replay.
    let replay = proto::PendingEvent {
        id: snapshot.id + 1,
        ..snapshot
    };
    server
        .handle
        .requeue_private_event(b_id, pair.b_device, replay)
        .await;
    server
        .counts_until(b_id, |counts| counts.pending == 0)
        .await;

    let later = pair.b.drain_private(Duration::from_millis(500)).await;
    assert!(
        !later
            .iter()
            .any(|event| matches!(event, PrivateEvent::MessageAdded(_))),
        "{later:?}"
    );
    let stored = pair
        .b
        .client
        .private_messages(&a_hex, None, 10)
        .await
        .unwrap();
    let text_stored: Vec<_> = stored
        .into_iter()
        .filter(|m| m.kind == PrivateMessageKind::Text)
        .collect();
    assert_eq!(text_stored.len(), 1);
    assert_eq!(text_stored[0].kind, PrivateMessageKind::Text);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_crash_before_saving_decrypts_on_redelivery() {
    let server = TestServer::start().await;
    let mut pair = Pair::on(&server).await;
    let (a_hex, b_hex) = (pair.a_hex(), pair.b_hex());
    let b_id = pair.b_ready.self_user.id;
    pair.b.reopen().await;
    server.counts_until(b_id, registered).await;
    server.privacy_of(b_id).await;
    // Another connection holds the write lock, so B's save fails after it
    // decrypted.
    let store = pair.b.store().await;
    let mut holder = store.pool().acquire().await.unwrap();
    sqlx::query("BEGIN EXCLUSIVE")
        .execute(&mut *holder)
        .await
        .unwrap();

    let sent = pair
        .a
        .client
        .private_send(&b_hex, "after the crash".to_owned())
        .await
        .unwrap();

    // Longer than the database waits for its lock.
    let shown = pair.b.drain_private(Duration::from_secs(7)).await;
    assert!(
        !shown
            .iter()
            .any(|event| matches!(event, PrivateEvent::MessageAdded(_))),
        "{shown:?}"
    );
    assert_eq!(server.handle.private_counts(b_id).await.pending, 1);
    sqlx::query("ROLLBACK").execute(&mut *holder).await.unwrap();
    drop(holder);
    store.close().await;

    server.handle.drop_user_connections(b_id);

    let received = pair
        .b
        .message_added(|message| message.peer == a_hex && message.kind == PrivateMessageKind::Text)
        .await;
    assert_eq!(received.id, sent.id);
    assert_eq!(received.kind, PrivateMessageKind::Text);
    assert_eq!(received.content, "after the crash");
    server
        .counts_until(b_id, |counts| counts.pending == 0)
        .await;
    let later = pair.b.drain_private(Duration::from_millis(500)).await;
    assert!(
        !later
            .iter()
            .any(|event| matches!(event, PrivateEvent::MessageAdded(_))),
        "{later:?}"
    );
    let msgs = pair
        .b
        .client
        .private_messages(&a_hex, None, 10)
        .await
        .unwrap();
    let text_msgs: Vec<_> = msgs
        .into_iter()
        .filter(|m| m.kind == PrivateMessageKind::Text)
        .collect();
    assert_eq!(text_msgs, vec![received]);
}

#[tokio::test(flavor = "multi_thread")]
async fn edits_and_deletes_reach_every_device() {
    let server = TestServer::start().await;
    let ThreeDevices {
        mut a,
        a_id,
        mut a2,
        mut b,
        b_ready,
    } = three_devices(&server).await;
    let a_hex = hex::encode(a.identity.public_key());
    let b_hex = hex::encode(b.identity.public_key());
    let first = a
        .client
        .private_send(&b_hex, "first".to_owned())
        .await
        .unwrap();
    let second = a
        .client
        .private_send(&b_hex, "second".to_owned())
        .await
        .unwrap();
    for id in [&first.id, &second.id] {
        b.message_added(|message| &message.id == id).await;
        a2.message_added(|message| &message.id == id).await;
    }

    a.client
        .private_edit(&b_hex, &first.id, "first, edited".to_owned())
        .await
        .unwrap();
    a.client.private_delete(&b_hex, &second.id).await.unwrap();

    for (device, peer) in [(&mut b, &a_hex), (&mut a2, &b_hex)] {
        let edited = device
            .private_event(|event| match event {
                PrivateEvent::MessageChanged(message) if message.id == first.id => {
                    Some(message.clone())
                }
                _ => None,
            })
            .await;
        assert_eq!(edited.content, "first, edited");
        assert!(edited.edited_at_ms.is_some());
        let deleted = device
            .private_event(|event| match event {
                PrivateEvent::MessageDeleted { peer, message_id } => {
                    Some((peer.clone(), message_id.clone()))
                }
                _ => None,
            })
            .await;
        assert_eq!(deleted, (peer.clone(), second.id.clone()));
        let stored = device
            .client
            .private_messages(peer, None, 10)
            .await
            .unwrap();
        let text_msgs: Vec<_> = stored
            .into_iter()
            .filter(|m| m.kind == PrivateMessageKind::Text)
            .collect();
        assert_eq!(text_msgs.len(), 1);
        assert_eq!(text_msgs[0].content, "first, edited");
    }

    // B's device edits A's message: A's device leaves it as it is.
    b.client.shutdown().await;
    let b_device = b.device_id().await;
    let a1_device = a.device_id().await;
    let first_id: [u8; 16] = hex::decode(&first.id).unwrap().try_into().unwrap();
    let edit = payload::build(
        [9; 16],
        &a.identity.public_key(),
        None,
        proto::private_payload::Body::Edit(proto::PrivateEdit {
            message_id: first_id.to_vec(),
            content: "not yours".to_owned(),
        }),
        0,
    );
    let (olm_type, body) = b.encrypt_for(a.curve25519().await, &edit).await;
    a.drain_private(Duration::from_millis(300)).await;
    server
        .handle
        .queue_private_events(
            a_id,
            a1_device,
            vec![proto::pending_event::Kind::PrivateMessage(
                proto::PrivateMessage {
                    sender: Some(b.user(&b_ready, "Member")),
                    sender_device_id: b_device.to_vec(),
                    olm_type,
                    body,
                    sent_at_ms: 1,
                },
            )],
        )
        .await;
    server
        .counts_until(a_id, |counts| counts.pending == 0)
        .await;

    let later = a.drain_private(Duration::from_millis(500)).await;
    assert!(
        !later
            .iter()
            .any(|event| matches!(event, PrivateEvent::MessageChanged(_))),
        "{later:?}"
    );
    let stored = a.client.private_messages(&b_hex, None, 10).await.unwrap();
    let text_msgs: Vec<_> = stored
        .into_iter()
        .filter(|m| m.kind == PrivateMessageKind::Text)
        .collect();
    assert_eq!(text_msgs.len(), 1);
    assert_eq!(text_msgs[0].content, "first, edited");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stranger_starts_a_message_request() {
    let server = TestServer::start().await;
    let (mut c, c_ready, mut b, b_ready) = owner_and_member(&server).await;
    c.open_private(1).await;
    b.open_private(2).await;
    server.counts_until(c_ready.self_user.id, registered).await;
    server.counts_until(b_ready.self_user.id, registered).await;
    c.set_relay(&b, &c_ready.server.server_id_hex).await;
    let c_hex = hex::encode(c.identity.public_key());
    let b_hex = hex::encode(b.identity.public_key());

    c.client
        .private_send(&b_hex, "hi, we haven't met".to_owned())
        .await
        .unwrap();

    let request = b
        .message_added(|message| message.peer == c_hex && message.kind == PrivateMessageKind::Text)
        .await;
    assert_eq!(request.content, "hi, we haven't met");
    let contact = b
        .private_event(|event| match event {
            PrivateEvent::ContactChanged(contact) if contact.peer == c_hex => Some(contact.clone()),
            _ => None,
        })
        .await;
    assert_eq!(contact.conversation, ConversationStatus::Request);
    assert_eq!(contact.friend, FriendStatus::None);
    assert_eq!(contact.unread, 1);
    assert_eq!(contact.preview.as_deref(), Some("hi, we haven't met"));
    assert!(contact.last_activity_ms.is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_broken_session_recovers_with_a_reset() {
    let server = TestServer::start().await;
    let mut pair = Pair::on(&server).await;
    let (a_hex, b_hex) = (pair.a_hex(), pair.b_hex());
    let b_id = pair.b_ready.self_user.id;
    pair.a
        .client
        .private_send(&b_hex, "first".to_owned())
        .await
        .unwrap();
    pair.b.reopen().await;
    pair.b.message_added(|message| message.peer == a_hex).await;
    server
        .counts_until(b_id, |counts| counts.pending == 0)
        .await;
    pair.b.client.shutdown().await;
    // B loses its sessions with A's device.
    let a_curve25519 = pair.a.curve25519().await;
    let store = pair.b.store().await;
    sqlx::query("DELETE FROM sessions WHERE peer_curve25519 = $1")
        .bind(a_curve25519.as_slice())
        .execute(store.pool())
        .await
        .unwrap();
    store.close().await;

    pair.a
        .client
        .private_send(&b_hex, "second".to_owned())
        .await
        .unwrap();
    pair.b.reopen().await;

    let line = pair.b.message_added(|message| message.peer == a_hex).await;
    assert_eq!(line.kind, PrivateMessageKind::Undecryptable);
    assert!(!line.outgoing);
    assert!(line.content.is_empty());
    // A's device takes the reset: its session with B's device received.
    let a_store = pair.a.store().await;
    let deadline = tokio::time::Instant::now() + WAIT;
    loop {
        let received: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE last_received_at IS NOT NULL")
                .fetch_one(a_store.pool())
                .await
                .unwrap();
        if received > 0 {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "A's device never took the reset"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    a_store.close().await;

    pair.a
        .client
        .private_send(&b_hex, "third".to_owned())
        .await
        .unwrap();

    let third = pair.b.message_added(|message| message.peer == a_hex).await;
    assert_eq!(third.kind, PrivateMessageKind::Text);
    assert_eq!(third.content, "third");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_message_from_a_forged_device_is_not_believed() {
    let server = TestServer::start().await;
    let mut pair = Pair::on(&server).await;
    let a_hex = pair.a_hex();
    let b_id = pair.b_ready.self_user.id;
    let a_id = pair.a_ready.self_user.id;
    pair.b.reopen().await;
    server.counts_until(b_id, registered).await;
    // A device with good keys of its own, vouched for by someone else, put
    // under A's name by the server.
    let forger = Identity::generate();
    let forged_dir = tempfile::tempdir().unwrap();
    let forged_store = PrivateStore::open(forged_dir.path(), &forger.public_key(), &[7; 32], 0)
        .await
        .unwrap()
        .store;
    let forged = Device::load_or_create(&forged_store, 0).await.unwrap();
    server
        .handle
        .insert_private_device_unchecked(a_id, forged.keys(&forger), forged.fallback_prekey())
        .await;
    let b_fallback: [u8; 32] = {
        let store = pair.b.store().await;
        let device = Device::load_or_create(&store, 0).await.unwrap();
        store.close().await;
        device.fallback_prekey().key.try_into().unwrap()
    };
    let mut conn = forged_store.pool().acquire().await.unwrap();
    let mut sessions = PeerSessions::load(
        &mut conn,
        forged_store.pickle_key(),
        pair.b.curve25519().await,
    )
    .await
    .unwrap();
    sessions.add_outbound(&forged, b_fallback, 0).unwrap();
    let payload = payload::build(
        [5; 16],
        &pair.b.identity.public_key(),
        None,
        text("this is really A"),
        0,
    );
    let (olm_type, body) = sessions.encrypt(&payload).unwrap();

    server
        .handle
        .queue_private_events(
            b_id,
            pair.b_device,
            vec![proto::pending_event::Kind::PrivateMessage(
                proto::PrivateMessage {
                    sender: Some(pair.a.user(&pair.a_ready, "Owner")),
                    sender_device_id: forged.id().to_vec(),
                    olm_type,
                    body,
                    sent_at_ms: 1,
                },
            )],
        )
        .await;

    let line = pair.b.message_added(|message| message.peer == a_hex).await;
    assert_eq!(line.kind, PrivateMessageKind::Undecryptable);
    assert!(line.content.is_empty());
    server
        .counts_until(b_id, |counts| counts.pending == 0)
        .await;
    let stored = pair
        .b
        .client
        .private_messages(&a_hex, None, 10)
        .await
        .unwrap();
    assert!(
        stored
            .iter()
            .all(|message| message.kind == PrivateMessageKind::Undecryptable),
        "{stored:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn friends_are_made_through_a_shared_server() {
    let server = TestServer::start().await;
    let (mut a, a_ready, mut b, b_ready) = owner_and_member(&server).await;
    let mut a2 = a
        .another_device(&server.address(), &server.fingerprint(), "Owner")
        .await;
    a.open_private(1).await;
    a2.open_private(3).await;
    b.open_private(2).await;
    server
        .counts_until(a_ready.self_user.id, |c| {
            c.devices == 2 && c.one_time_keys == 40
        })
        .await;
    server.counts_until(b_ready.self_user.id, registered).await;

    let b_hex = hex::encode(b.identity.public_key());
    let a_hex = hex::encode(a.identity.public_key());
    let server_key = a.client.servers()[0].key.clone();

    // A sends friend request to B
    a.client
        .friend_request_send(&server_key, b_ready.self_user.id)
        .await
        .unwrap();

    // B gets incoming request
    b.private_event(|event| match event {
        PrivateEvent::ContactChanged(c)
            if c.peer == a_hex && c.friend == FriendStatus::Incoming =>
        {
            Some(())
        }
        _ => None,
    })
    .await;

    // B accepts
    b.client.friend_request_answer(&a_hex, true).await.unwrap();

    // B's contact for A is Friends
    let b_contacts = b.client.private_contacts().await.unwrap();
    let b_contact = b_contacts.iter().find(|c| c.peer == a_hex).unwrap();
    assert_eq!(b_contact.friend, FriendStatus::Friends);

    // A and A2 get ContactChanged to Friends
    a.private_event(|event| match event {
        PrivateEvent::ContactChanged(c) if c.peer == b_hex && c.friend == FriendStatus::Friends => {
            Some(())
        }
        _ => None,
    })
    .await;
    a2.private_event(|event| match event {
        PrivateEvent::ContactChanged(c) if c.peer == b_hex && c.friend == FriendStatus::Friends => {
            Some(())
        }
        _ => None,
    })
    .await;

    let a_contacts = a.client.private_contacts().await.unwrap();
    let a_contact = a_contacts.iter().find(|c| c.peer == b_hex).unwrap();
    assert_eq!(a_contact.friend, FriendStatus::Friends);

    let a2_contacts = a2.client.private_contacts().await.unwrap();
    let a2_contact = a2_contacts.iter().find(|c| c.peer == b_hex).unwrap();
    assert_eq!(a2_contact.friend, FriendStatus::Friends);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_declined_request_quietly_disappears() {
    let server = TestServer::start().await;
    let (mut a, a_ready, mut b, b_ready) = owner_and_member(&server).await;
    a.open_private(1).await;
    b.open_private(2).await;
    server.counts_until(a_ready.self_user.id, registered).await;
    server.counts_until(b_ready.self_user.id, registered).await;

    let a_hex = hex::encode(a.identity.public_key());
    let b_hex = hex::encode(b.identity.public_key());
    let server_key = a.client.servers()[0].key.clone();

    a.client
        .friend_request_send(&server_key, b_ready.self_user.id)
        .await
        .unwrap();

    b.private_event(|event| match event {
        PrivateEvent::ContactChanged(c)
            if c.peer == a_hex && c.friend == FriendStatus::Incoming =>
        {
            Some(())
        }
        _ => None,
    })
    .await;

    b.client.friend_request_answer(&a_hex, false).await.unwrap();

    // A gets ContactChanged back to None with no error
    a.private_event(|event| match event {
        PrivateEvent::ContactChanged(c) if c.peer == b_hex && c.friend == FriendStatus::None => {
            Some(())
        }
        _ => None,
    })
    .await;

    let a_contact = a
        .client
        .private_contacts()
        .await
        .unwrap()
        .into_iter()
        .find(|c| c.peer == b_hex)
        .unwrap();
    assert_eq!(a_contact.friend, FriendStatus::None);

    let b_contact = b
        .client
        .private_contacts()
        .await
        .unwrap()
        .into_iter()
        .find(|c| c.peer == a_hex)
        .unwrap();
    assert_eq!(b_contact.friend, FriendStatus::None);
    assert!(!b_contact.request_spam);
}

#[tokio::test(flavor = "multi_thread")]
async fn crossed_requests_become_a_friendship() {
    let server_x = TestServer::start().await;
    let server_y = TestServer::start().await;

    let mut a = TestClient::new("A");
    let a_x_ready = a.join_as_owner(&server_x).await;
    let a_y_ready = a.join_as_owner(&server_y).await;

    let invite_x = a
        .client
        .create_invite(&server_x.address(), None, None, None)
        .await
        .unwrap();
    let invite_y = a
        .client
        .create_invite(&server_y.address(), None, None, None)
        .await
        .unwrap();

    let mut b = TestClient::new("B");
    b.client.add_server(&invite_x.link, None).await.unwrap();
    let b_x_ready = b.ready().await;
    b.client.add_server(&invite_y.link, None).await.unwrap();
    let b_y_ready = b.ready().await;

    a.open_private(1).await;
    b.open_private(2).await;
    server_x
        .counts_until(a_x_ready.self_user.id, registered)
        .await;
    server_x
        .counts_until(b_x_ready.self_user.id, registered)
        .await;
    server_y
        .counts_until(a_y_ready.self_user.id, registered)
        .await;
    server_y
        .counts_until(b_y_ready.self_user.id, registered)
        .await;

    let a_hex = hex::encode(a.identity.public_key());
    let b_hex = hex::encode(b.identity.public_key());
    let a_x_key = a.client.servers()[0].key.clone();
    let b_y_key = b.client.servers()[1].key.clone();

    // A -> B on server X
    a.client
        .friend_request_send(&a_x_key, b_x_ready.self_user.id)
        .await
        .unwrap();
    // B -> A on server Y
    b.client
        .friend_request_send(&b_y_key, a_y_ready.self_user.id)
        .await
        .unwrap();

    // Wait until both end Friends
    a.private_event(|event| match event {
        PrivateEvent::ContactChanged(c) if c.peer == b_hex && c.friend == FriendStatus::Friends => {
            Some(())
        }
        _ => None,
    })
    .await;
    b.private_event(|event| match event {
        PrivateEvent::ContactChanged(c) if c.peer == a_hex && c.friend == FriendStatus::Friends => {
            Some(())
        }
        _ => None,
    })
    .await;

    let a_contact = a
        .client
        .private_contacts()
        .await
        .unwrap()
        .into_iter()
        .find(|c| c.peer == b_hex)
        .unwrap();
    assert_eq!(a_contact.friend, FriendStatus::Friends);

    let b_contact = b
        .client
        .private_contacts()
        .await
        .unwrap()
        .into_iter()
        .find(|c| c.peer == a_hex)
        .unwrap();
    assert_eq!(b_contact.friend, FriendStatus::Friends);

    // Both servers end with no requests
    let x_counts = server_x
        .counts_until(b_x_ready.self_user.id, |c| {
            c.friend_requests == 0 && c.pending == 0
        })
        .await;
    assert_eq!(x_counts.friend_requests, 0);
    let y_counts = server_y
        .counts_until(a_y_ready.self_user.id, |c| {
            c.friend_requests == 0 && c.pending == 0
        })
        .await;
    assert_eq!(y_counts.friend_requests, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_third_decline_sends_the_next_request_to_spam() {
    let server = TestServer::start().await;
    let (mut a, a_ready, mut b, b_ready) = owner_and_member(&server).await;
    a.open_private(1).await;
    b.open_private(2).await;
    server.counts_until(a_ready.self_user.id, registered).await;
    server.counts_until(b_ready.self_user.id, registered).await;

    let a_hex = hex::encode(a.identity.public_key());
    let server_key = a.client.servers()[0].key.clone();

    for _ in 0..3 {
        a.client
            .friend_request_send(&server_key, b_ready.self_user.id)
            .await
            .unwrap();
        b.private_event(|event| match event {
            PrivateEvent::ContactChanged(c)
                if c.peer == a_hex && c.friend == FriendStatus::Incoming =>
            {
                Some(())
            }
            _ => None,
        })
        .await;
        b.private_event(|event| match event {
            PrivateEvent::FriendRequestReceived { peer, spam } if *peer == a_hex => {
                assert!(!spam);
                Some(())
            }
            _ => None,
        })
        .await;
        b.client.friend_request_answer(&a_hex, false).await.unwrap();
        a.private_event(|event| match event {
            PrivateEvent::ContactChanged(c)
                if c.peer == hex::encode(b.identity.public_key())
                    && c.friend == FriendStatus::None =>
            {
                Some(())
            }
            _ => None,
        })
        .await;
    }

    // 4th request from A
    a.client
        .friend_request_send(&server_key, b_ready.self_user.id)
        .await
        .unwrap();

    let spam_received = b
        .private_event(|event| match event {
            PrivateEvent::FriendRequestReceived { peer, spam } if *peer == a_hex => Some(*spam),
            _ => None,
        })
        .await;
    assert!(spam_received);

    let b_contact = b
        .client
        .private_contacts()
        .await
        .unwrap()
        .into_iter()
        .find(|c| c.peer == a_hex)
        .unwrap();
    assert!(b_contact.request_spam);
}

#[tokio::test(flavor = "multi_thread")]
async fn closed_friend_requests_give_their_error() {
    let server = TestServer::start().await;
    let (mut a, a_ready, mut b, b_ready) = owner_and_member(&server).await;
    a.open_private(1).await;
    b.open_private(2).await;
    server.counts_until(a_ready.self_user.id, registered).await;
    server.counts_until(b_ready.self_user.id, registered).await;

    // B closes friend requests
    b.client
        .set_privacy_settings(PrivacySettingsData {
            allow_friend_requests: false,
            allow_message_requests: true,
        })
        .await
        .unwrap();

    let server_key = a.client.servers()[0].key.clone();
    let err = a
        .client
        .friend_request_send(&server_key, b_ready.self_user.id)
        .await
        .unwrap_err();

    assert!(
        matches!(
            err,
            CoreError::Server {
                code: ErrorCode::FriendRequestsClosed,
                ..
            }
        ),
        "expected FriendRequestsClosed, got {err:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn closed_message_requests_refuse_strangers_but_not_friends() {
    let server = TestServer::start().await;
    let (mut a, a_ready, mut b, b_ready) = owner_and_member(&server).await;
    let invite = a
        .client
        .create_invite(&server.address(), None, None, None)
        .await
        .unwrap();
    let mut c = TestClient::new("C");
    c.client.add_server(&invite.link, None).await.unwrap();
    let c_ready = c.ready().await;

    a.open_private(1).await;
    b.open_private(2).await;
    c.open_private(3).await;
    server.counts_until(a_ready.self_user.id, registered).await;
    server.counts_until(b_ready.self_user.id, registered).await;
    server.counts_until(c_ready.self_user.id, registered).await;

    // Make A and B friends
    let b_hex = hex::encode(b.identity.public_key());
    let a_hex = hex::encode(a.identity.public_key());
    let a_server_key = a.client.servers()[0].key.clone();

    a.client
        .friend_request_send(&a_server_key, b_ready.self_user.id)
        .await
        .unwrap();
    b.private_event(|event| match event {
        PrivateEvent::ContactChanged(c)
            if c.peer == a_hex && c.friend == FriendStatus::Incoming =>
        {
            Some(())
        }
        _ => None,
    })
    .await;
    b.client.friend_request_answer(&a_hex, true).await.unwrap();
    a.private_event(|event| match event {
        PrivateEvent::ContactChanged(c) if c.peer == b_hex && c.friend == FriendStatus::Friends => {
            Some(())
        }
        _ => None,
    })
    .await;

    // B closes message requests
    b.client
        .set_privacy_settings(PrivacySettingsData {
            allow_friend_requests: true,
            allow_message_requests: false,
        })
        .await
        .unwrap();

    // Stranger C tries to send message to B -> MessageRequestsClosed
    c.set_relay(&b, &c_ready.server.server_id_hex).await;
    let err = c
        .client
        .private_send(&b_hex, "hello stranger".to_owned())
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            CoreError::Server {
                code: ErrorCode::MessageRequestsClosed,
                ..
            }
        ),
        "expected MessageRequestsClosed, got {err:?}"
    );

    // Friend A sends message to B -> succeeds!
    a.set_relay(&b, &a_ready.server.server_id_hex).await;
    let sent = a
        .client
        .private_send(&b_hex, "hello friend".to_owned())
        .await
        .unwrap();
    assert_eq!(sent.state, PrivateSendState::Sent);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_accepted_message_request_lets_them_write_after_requests_close() {
    let server = TestServer::start().await;
    let (mut c, c_ready, mut b, b_ready) = owner_and_member(&server).await;
    c.open_private(1).await;
    b.open_private(2).await;
    server.counts_until(c_ready.self_user.id, registered).await;
    server.counts_until(b_ready.self_user.id, registered).await;

    let b_hex = hex::encode(b.identity.public_key());
    let c_hex = hex::encode(c.identity.public_key());
    c.set_relay(&b, &c_ready.server.server_id_hex).await;

    // C writes to B
    c.client
        .private_send(&b_hex, "first message".to_owned())
        .await
        .unwrap();

    // B gets message request
    b.private_event(|event| match event {
        PrivateEvent::MessageAdded(m) if m.peer == c_hex => Some(()),
        _ => None,
    })
    .await;
    let b_contact = b
        .client
        .private_contacts()
        .await
        .unwrap()
        .into_iter()
        .find(|contact| contact.peer == c_hex)
        .unwrap();
    assert_eq!(b_contact.conversation, ConversationStatus::Request);

    // Drain C's events from the first message
    let _ = c.drain_private(Duration::from_millis(100)).await;

    // B accepts message request
    b.client.message_request_accept(&c_hex).await.unwrap();

    // C receives RequestAccepted and updates contact with B's slip
    c.private_event(|event| match event {
        PrivateEvent::ContactChanged(c) if c.peer == b_hex => Some(()),
        _ => None,
    })
    .await;

    // B closes message requests
    b.client
        .set_privacy_settings(PrivacySettingsData {
            allow_friend_requests: true,
            allow_message_requests: false,
        })
        .await
        .unwrap();

    // C writes again -> succeeds because C has B's slip from RequestAccepted!
    let sent2 = c
        .client
        .private_send(&b_hex, "second message".to_owned())
        .await
        .unwrap();
    assert_eq!(sent2.state, PrivateSendState::Sent);

    let msg = b
        .private_event(|event| match event {
            PrivateEvent::MessageAdded(m) if m.peer == c_hex && m.content == "second message" => {
                Some(m.clone())
            }
            _ => None,
        })
        .await;
    assert_eq!(msg.content, "second message");
}

#[tokio::test(flavor = "multi_thread")]
async fn ignoring_a_message_request_deletes_it() {
    let server = TestServer::start().await;
    let (mut c, c_ready, mut b, b_ready) = owner_and_member(&server).await;
    c.open_private(1).await;
    b.open_private(2).await;
    server.counts_until(c_ready.self_user.id, registered).await;
    server.counts_until(b_ready.self_user.id, registered).await;

    let b_hex = hex::encode(b.identity.public_key());
    let c_hex = hex::encode(c.identity.public_key());
    c.set_relay(&b, &c_ready.server.server_id_hex).await;

    c.client
        .private_send(&b_hex, "message to ignore".to_owned())
        .await
        .unwrap();
    b.private_event(|event| match event {
        PrivateEvent::MessageAdded(m) if m.peer == c_hex => Some(()),
        _ => None,
    })
    .await;

    b.client.message_request_ignore(&c_hex).await.unwrap();

    let removed = b
        .private_event(|event| match event {
            PrivateEvent::ContactRemoved { peer } if peer == &c_hex => Some(()),
            _ => None,
        })
        .await;
    assert_eq!(removed, ());

    let messages = b.client.private_messages(&c_hex, None, 10).await.unwrap();
    assert!(messages.is_empty());
    let contacts = b.client.private_contacts().await.unwrap();
    assert!(!contacts.iter().any(|c| c.peer == c_hex));
}

#[tokio::test(flavor = "multi_thread")]
async fn blocking_ends_everything_and_refuses_them() {
    let server = TestServer::start().await;
    let (mut a, a_ready, mut b, b_ready) = owner_and_member(&server).await;
    a.open_private(1).await;
    b.open_private(2).await;
    server.counts_until(a_ready.self_user.id, registered).await;
    server.counts_until(b_ready.self_user.id, registered).await;

    let b_hex = hex::encode(b.identity.public_key());
    let a_hex = hex::encode(a.identity.public_key());
    let server_key = a.client.servers()[0].key.clone();

    // Make A and B friends
    a.client
        .friend_request_send(&server_key, b_ready.self_user.id)
        .await
        .unwrap();
    b.private_event(|event| match event {
        PrivateEvent::ContactChanged(c)
            if c.peer == a_hex && c.friend == FriendStatus::Incoming =>
        {
            Some(())
        }
        _ => None,
    })
    .await;
    b.client.friend_request_answer(&a_hex, true).await.unwrap();
    a.private_event(|event| match event {
        PrivateEvent::ContactChanged(c) if c.peer == b_hex && c.friend == FriendStatus::Friends => {
            Some(())
        }
        _ => None,
    })
    .await;

    // A blocks B
    a.client.private_block(&b_hex).await.unwrap();
    let _ = a.drain_private(Duration::from_millis(100)).await;

    // Both stop being friends
    let a_contacts = a.client.private_contacts().await.unwrap();
    let a_contact = a_contacts.iter().find(|c| c.peer == b_hex).unwrap();
    assert_eq!(a_contact.friend, FriendStatus::None);
    assert!(a_contact.blocked);

    b.private_event(|event| match event {
        PrivateEvent::ContactChanged(c) if c.peer == a_hex && c.friend == FriendStatus::None => {
            Some(())
        }
        _ => None,
    })
    .await;

    // B sends a message to A -> Undeliverable
    b.set_relay(&a, &b_ready.server.server_id_hex).await;
    let err = b
        .client
        .private_send(&a_hex, "are you there?".to_owned())
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            CoreError::Server {
                code: ErrorCode::Undeliverable,
                ..
            }
        ),
        "expected Undeliverable for message, got {err:?}"
    );

    // B sends a friend request to A -> Undeliverable
    let b_server_key = b.client.servers()[0].key.clone();
    let err2 = b
        .client
        .friend_request_send(&b_server_key, a_ready.self_user.id)
        .await
        .unwrap_err();
    assert!(
        matches!(
            err2,
            CoreError::Server {
                code: ErrorCode::Undeliverable,
                ..
            }
        ),
        "expected Undeliverable for friend request, got {err2:?}"
    );

    // A sees nothing
    let seen = a.drain_private(Duration::from_millis(300)).await;
    assert!(seen.is_empty(), "A should see nothing, got {seen:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn unblocking_lets_them_write_again() {
    let server = TestServer::start().await;
    let (mut a, a_ready, mut b, b_ready) = owner_and_member(&server).await;
    a.open_private(1).await;
    b.open_private(2).await;
    server.counts_until(a_ready.self_user.id, registered).await;
    server.counts_until(b_ready.self_user.id, registered).await;

    let b_hex = hex::encode(b.identity.public_key());
    let a_hex = hex::encode(a.identity.public_key());

    // A blocks B
    a.client.private_block(&b_hex).await.unwrap();

    // A unblocks B
    a.client.private_unblock(&b_hex).await.unwrap();

    // B sends a message to A -> arrives!
    b.set_relay(&a, &b_ready.server.server_id_hex).await;
    b.client
        .private_send(&a_hex, "can write again".to_owned())
        .await
        .unwrap();

    let msg = a
        .private_event(|event| match event {
            PrivateEvent::MessageAdded(m)
                if m.peer == b_hex && m.kind == PrivateMessageKind::Text =>
            {
                Some(m.clone())
            }
            _ => None,
        })
        .await;
    assert_eq!(msg.content, "can write again");
}

#[tokio::test(flavor = "multi_thread")]
async fn removing_a_friend_reaches_both_sides() {
    let server = TestServer::start().await;
    let (mut a, a_ready, mut b, b_ready) = owner_and_member(&server).await;
    a.open_private(1).await;
    b.open_private(2).await;
    server.counts_until(a_ready.self_user.id, registered).await;
    server.counts_until(b_ready.self_user.id, registered).await;

    let b_hex = hex::encode(b.identity.public_key());
    let a_hex = hex::encode(a.identity.public_key());
    let server_key = a.client.servers()[0].key.clone();

    // Make friends
    a.client
        .friend_request_send(&server_key, b_ready.self_user.id)
        .await
        .unwrap();
    b.private_event(|event| match event {
        PrivateEvent::ContactChanged(c)
            if c.peer == a_hex && c.friend == FriendStatus::Incoming =>
        {
            Some(())
        }
        _ => None,
    })
    .await;
    b.client.friend_request_answer(&a_hex, true).await.unwrap();
    a.private_event(|event| match event {
        PrivateEvent::ContactChanged(c) if c.peer == b_hex && c.friend == FriendStatus::Friends => {
            Some(())
        }
        _ => None,
    })
    .await;

    // A removes B as friend
    a.client.friend_remove(&b_hex).await.unwrap();

    // Both sides see friendship end
    let a_contact = a
        .client
        .private_contacts()
        .await
        .unwrap()
        .into_iter()
        .find(|c| c.peer == b_hex)
        .unwrap();
    assert_eq!(a_contact.friend, FriendStatus::None);

    b.private_event(|event| match event {
        PrivateEvent::ContactChanged(c) if c.peer == a_hex && c.friend == FriendStatus::None => {
            Some(())
        }
        _ => None,
    })
    .await;

    let b_contact = b
        .client
        .private_contacts()
        .await
        .unwrap()
        .into_iter()
        .find(|c| c.peer == a_hex)
        .unwrap();
    assert_eq!(b_contact.friend, FriendStatus::None);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_conversation_starts_on_the_profiles_server_with_a_divider() {
    let server = TestServer::start().await;
    let (mut a, a_ready, mut b, b_ready) = owner_and_member(&server).await;
    a.open_private(1).await;
    b.open_private(2).await;
    server.counts_until(a_ready.self_user.id, registered).await;
    server.counts_until(b_ready.self_user.id, registered).await;

    let b_hex = hex::encode(b.identity.public_key());
    let a_hex = hex::encode(a.identity.public_key());
    let server_key = a.client.servers()[0].key.clone();

    // A opens conversation on profile server
    let contact = a
        .client
        .private_open_conversation(&server_key, b_ready.self_user.id)
        .await
        .unwrap();
    assert_eq!(contact.peer, b_hex);
    assert_eq!(
        contact.relay_server_key.as_deref(),
        Some(server_key.as_str())
    );

    // A sends the first text message
    a.client
        .private_send(&b_hex, "first hello".to_owned())
        .await
        .unwrap();

    // A sees divider then text (newest first: text, then divider)
    let a_msgs = a.client.private_messages(&b_hex, None, 10).await.unwrap();
    assert_eq!(a_msgs.len(), 2);
    assert_eq!(a_msgs[0].kind, PrivateMessageKind::Text);
    assert_eq!(a_msgs[0].content, "first hello");
    assert_eq!(a_msgs[1].kind, PrivateMessageKind::Divider);

    // B gets message
    b.private_event(|event| match event {
        PrivateEvent::MessageAdded(m) if m.peer == a_hex && m.content == "first hello" => Some(()),
        _ => None,
    })
    .await;

    let b_msgs = b.client.private_messages(&a_hex, None, 10).await.unwrap();
    assert_eq!(b_msgs.len(), 2);
    assert_eq!(b_msgs[0].kind, PrivateMessageKind::Text);
    assert_eq!(b_msgs[0].content, "first hello");
    assert_eq!(b_msgs[1].kind, PrivateMessageKind::Divider);
}

#[tokio::test(flavor = "multi_thread")]
async fn moving_a_chat_takes_effect_for_the_mover_and_offers_the_other() {
    let server_x = TestServer::start().await;
    let (mut a, a_x_ready, mut b, b_x_ready) = owner_and_member(&server_x).await;
    let server_y = TestServer::start().await;
    let a_y_ready = a.join_as_owner(&server_y).await;
    let invite_y = a
        .client
        .create_invite(&server_y.address(), None, None, None)
        .await
        .unwrap();
    b.client.add_server(&invite_y.link, None).await.unwrap();
    let b_y_ready = b.ready().await;

    a.open_private(1).await;
    b.open_private(2).await;
    server_x
        .counts_until(a_x_ready.self_user.id, registered)
        .await;
    server_x
        .counts_until(b_x_ready.self_user.id, registered)
        .await;
    server_y
        .counts_until(a_y_ready.self_user.id, registered)
        .await;
    server_y
        .counts_until(b_y_ready.self_user.id, registered)
        .await;

    let b_hex = hex::encode(b.identity.public_key());
    let a_hex = hex::encode(a.identity.public_key());
    let server_x_key = a.client.servers()[0].key.clone();
    let server_y_key = a.client.servers()[1].key.clone();

    // Start on X
    a.client
        .private_open_conversation(&server_x_key, b_x_ready.self_user.id)
        .await
        .unwrap();
    a.client
        .private_send(&b_hex, "on x".to_owned())
        .await
        .unwrap();
    b.private_event(|e| match e {
        PrivateEvent::MessageAdded(m) if m.peer == a_hex && m.content == "on x" => Some(()),
        _ => None,
    })
    .await;

    // A moves to Y
    a.client.private_move(&b_hex, &server_y_key).await.unwrap();

    // A's relay is Y and A has a divider
    let a_contact = a
        .client
        .private_contacts()
        .await
        .unwrap()
        .into_iter()
        .find(|c| c.peer == b_hex)
        .unwrap();
    assert_eq!(
        a_contact.relay_server_key.as_deref(),
        Some(server_y_key.as_str())
    );
    let a_msgs = a.client.private_messages(&b_hex, None, 10).await.unwrap();
    let a_dividers: Vec<_> = a_msgs
        .iter()
        .filter(|m| m.kind == PrivateMessageKind::Divider)
        .collect();
    assert_eq!(a_dividers.len(), 2);

    // B gets a Moved card with Y's server key
    let moved_card = b
        .private_event(|event| match event {
            PrivateEvent::MessageAdded(m)
                if m.peer == a_hex && m.kind == PrivateMessageKind::Moved =>
            {
                Some(m.clone())
            }
            _ => None,
        })
        .await;

    // B's private_move_too -> B's relay is Y, with a divider
    b.client
        .private_move_too(&a_hex, &moved_card.id)
        .await
        .unwrap();
    let b_contact = b
        .client
        .private_contacts()
        .await
        .unwrap()
        .into_iter()
        .find(|c| c.peer == a_hex)
        .unwrap();
    assert_eq!(
        b_contact.relay_server_key.as_deref(),
        Some(server_y_key.as_str())
    );
    let b_msgs = b.client.private_messages(&a_hex, None, 10).await.unwrap();
    let b_dividers: Vec<_> = b_msgs
        .iter()
        .filter(|m| m.kind == PrivateMessageKind::Divider)
        .collect();
    assert_eq!(b_dividers.len(), 2);

    // B's next message travels through Y (Y's pending count for A rises)
    b.client
        .private_send(&a_hex, "now through y".to_owned())
        .await
        .unwrap();
    server_y
        .counts_until(a_y_ready.self_user.id, |c| c.pending >= 1)
        .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn moving_to_a_server_the_other_person_isnt_on_is_refused() {
    let server_x = TestServer::start().await;
    let (mut a, a_x_ready, mut b, b_x_ready) = owner_and_member(&server_x).await;
    let server_z = TestServer::start().await;
    let _a_z_ready = a.join_as_owner(&server_z).await;

    a.open_private(1).await;
    b.open_private(2).await;
    server_x
        .counts_until(a_x_ready.self_user.id, registered)
        .await;
    server_x
        .counts_until(b_x_ready.self_user.id, registered)
        .await;

    let b_hex = hex::encode(b.identity.public_key());
    let server_x_key = a.client.servers()[0].key.clone();
    let server_z_key = a.client.servers()[1].key.clone();

    a.client
        .private_open_conversation(&server_x_key, b_x_ready.self_user.id)
        .await
        .unwrap();

    let err = a
        .client
        .private_move(&b_hex, &server_z_key)
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::InvalidInput { .. }));

    // A's relay is unchanged
    let a_contact = a
        .client
        .private_contacts()
        .await
        .unwrap()
        .into_iter()
        .find(|c| c.peer == b_hex)
        .unwrap();
    assert_eq!(
        a_contact.relay_server_key.as_deref(),
        Some(server_x_key.as_str())
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn move_too_needs_the_server_on_this_device() {
    let server_x = TestServer::start().await;
    let (mut a, a_x_ready, mut b, b_x_ready) = owner_and_member(&server_x).await;
    let server_y = TestServer::start().await;
    let a_y_ready = a.join_as_owner(&server_y).await;
    let invite_y = a
        .client
        .create_invite(&server_y.address(), None, None, None)
        .await
        .unwrap();
    b.client.add_server(&invite_y.link, None).await.unwrap();
    let b_y_ready = b.ready().await;

    a.open_private(1).await;
    b.open_private(2).await;
    server_x
        .counts_until(a_x_ready.self_user.id, registered)
        .await;
    server_x
        .counts_until(b_x_ready.self_user.id, registered)
        .await;
    server_y
        .counts_until(a_y_ready.self_user.id, registered)
        .await;
    server_y
        .counts_until(b_y_ready.self_user.id, registered)
        .await;

    let b_hex = hex::encode(b.identity.public_key());
    let a_hex = hex::encode(a.identity.public_key());
    let server_x_key = a.client.servers()[0].key.clone();
    let server_y_key = a.client.servers()[1].key.clone();
    let b_y_key = b.client.servers()[1].key.clone();

    a.client
        .private_open_conversation(&server_x_key, b_x_ready.self_user.id)
        .await
        .unwrap();
    a.client
        .private_send(&b_hex, "msg on x".to_owned())
        .await
        .unwrap();
    b.private_event(|e| match e {
        PrivateEvent::MessageAdded(m) if m.peer == a_hex && m.content == "msg on x" => Some(()),
        _ => None,
    })
    .await;

    // A moves to Y
    a.client.private_move(&b_hex, &server_y_key).await.unwrap();

    let moved_card = b
        .private_event(|event| match event {
            PrivateEvent::MessageAdded(m)
                if m.peer == a_hex && m.kind == PrivateMessageKind::Moved =>
            {
                Some(m.clone())
            }
            _ => None,
        })
        .await;

    // B removes server Y from this client device
    b.client.remove_server(&b_y_key).await.unwrap();

    // B cannot move_too because Y is no longer on this device
    let err = b
        .client
        .private_move_too(&a_hex, &moved_card.id)
        .await
        .unwrap_err();
    assert!(matches!(err, CoreError::InvalidInput { .. }));
    assert!(err.to_string().starts_with("This device isn't on "));
}

#[tokio::test(flavor = "multi_thread")]
async fn typing_reaches_the_other_person_but_not_through_a_block() {
    let server = TestServer::start().await;
    let (mut a, a_ready, mut b, b_ready) = owner_and_member(&server).await;
    a.open_private(1).await;
    b.open_private(2).await;
    server.counts_until(a_ready.self_user.id, registered).await;
    server.counts_until(b_ready.self_user.id, registered).await;

    let b_hex = hex::encode(b.identity.public_key());
    let a_hex = hex::encode(a.identity.public_key());
    let server_key = a.client.servers()[0].key.clone();

    a.client
        .private_open_conversation(&server_key, b_ready.self_user.id)
        .await
        .unwrap();

    // A types to B
    a.client.private_typing(&b_hex).await.unwrap();

    let typing = b
        .private_event(|event| match event {
            PrivateEvent::Typing { peer } if peer == &a_hex => Some(()),
            _ => None,
        })
        .await;
    assert_eq!(typing, ());

    // B blocks A
    b.client.private_block(&a_hex).await.unwrap();
    let _ = b.drain_private(Duration::from_millis(100)).await;

    // A types again -> B hears nothing
    a.client.private_typing(&b_hex).await.unwrap();
    let seen = b.drain_private(Duration::from_millis(300)).await;
    assert!(
        !seen
            .iter()
            .any(|e| matches!(e, PrivateEvent::Typing { .. }))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn shared_servers_list_only_servers_with_both_people() {
    let server1 = TestServer::start().await;
    let (mut a, a1_ready, mut b, _b1_ready) = owner_and_member(&server1).await;
    let server2 = TestServer::start().await;
    let _a2_ready = a.join_as_owner(&server2).await;
    let server3 = TestServer::start().await;
    let _a3_ready = a.join_as_owner(&server3).await;
    let invite3 = a
        .client
        .create_invite(&server3.address(), None, None, None)
        .await
        .unwrap();
    b.client.add_server(&invite3.link, None).await.unwrap();
    let _b3_ready = b.ready().await;

    a.open_private(1).await;
    b.open_private(2).await;
    server1
        .counts_until(a1_ready.self_user.id, registered)
        .await;

    let b_hex = hex::encode(b.identity.public_key());
    let shared = a.client.private_shared_servers(&b_hex);

    assert_eq!(shared.len(), 2);
    let s1_key = a.client.servers()[0].key.clone();
    let s3_key = a.client.servers()[2].key.clone();
    assert_eq!(shared[0].server_key, s1_key);
    assert_eq!(shared[1].server_key, s3_key);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_hidden_conversation_comes_back_with_the_next_message() {
    let server = TestServer::start().await;
    let (mut a, a_ready, mut b, b_ready) = owner_and_member(&server).await;
    a.open_private(1).await;
    b.open_private(2).await;
    server.counts_until(a_ready.self_user.id, registered).await;
    server.counts_until(b_ready.self_user.id, registered).await;

    let b_hex = hex::encode(b.identity.public_key());
    let a_hex = hex::encode(a.identity.public_key());
    let server_key = a.client.servers()[0].key.clone();

    a.client
        .private_open_conversation(&server_key, b_ready.self_user.id)
        .await
        .unwrap();

    // A hides the conversation
    a.client.private_hide(&b_hex).await.unwrap();
    let a_contact = a
        .client
        .private_contacts()
        .await
        .unwrap()
        .into_iter()
        .find(|c| c.peer == b_hex)
        .unwrap();
    assert!(a_contact.hidden);
    let _ = a.drain_private(Duration::from_millis(50)).await;

    // B sends a message to A
    b.set_relay(&a, &b_ready.server.server_id_hex).await;
    b.client
        .private_send(&a_hex, "wake up".to_owned())
        .await
        .unwrap();

    // A sees unhide via ContactChanged
    a.private_event(|event| match event {
        PrivateEvent::ContactChanged(c) if c.peer == b_hex && !c.hidden => Some(()),
        _ => None,
    })
    .await;

    let a_contact2 = a
        .client
        .private_contacts()
        .await
        .unwrap()
        .into_iter()
        .find(|c| c.peer == b_hex)
        .unwrap();
    assert!(!a_contact2.hidden);
}

#[tokio::test(flavor = "multi_thread")]
async fn reading_clears_the_unread_count() {
    let server = TestServer::start().await;
    let (mut a, a_ready, mut b, b_ready) = owner_and_member(&server).await;
    a.open_private(1).await;
    b.open_private(2).await;
    server.counts_until(a_ready.self_user.id, registered).await;
    server.counts_until(b_ready.self_user.id, registered).await;

    let b_hex = hex::encode(b.identity.public_key());
    let a_hex = hex::encode(a.identity.public_key());

    b.set_relay(&a, &b_ready.server.server_id_hex).await;
    b.client
        .private_send(&a_hex, "one".to_owned())
        .await
        .unwrap();
    b.client
        .private_send(&a_hex, "two".to_owned())
        .await
        .unwrap();

    a.private_event(|event| match event {
        PrivateEvent::MessageAdded(m) if m.peer == b_hex && m.content == "two" => Some(()),
        _ => None,
    })
    .await;

    let a_contact = a
        .client
        .private_contacts()
        .await
        .unwrap()
        .into_iter()
        .find(|c| c.peer == b_hex)
        .unwrap();
    assert_eq!(a_contact.unread, 2);

    // A marks read
    a.client.private_mark_read(&b_hex).await.unwrap();

    let a_contact2 = a
        .client
        .private_contacts()
        .await
        .unwrap()
        .into_iter()
        .find(|c| c.peer == b_hex)
        .unwrap();
    assert_eq!(a_contact2.unread, 0);
}
