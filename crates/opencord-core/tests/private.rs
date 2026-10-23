//! Private messages in the client core, against real servers.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use opencord_common::address::format_fingerprint;
use opencord_core::api::types::{
    ConnectionState, ConversationStatus, CoreError, CoreEvent, CoreEventPayload, ErrorCode,
    FriendStatus, PrivateEvent, PrivateMessage, PrivateMessageKind, PrivateSendState,
    ReadySnapshot,
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
            b,
            b_ready,
            b_device,
        }
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
    assert_eq!(stored, vec![sent]);
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
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].state, PrivateSendState::Failed);
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
