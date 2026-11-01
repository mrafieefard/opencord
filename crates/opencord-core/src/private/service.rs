//! The private messages service (design 2026-10-22 §9–§12): it opens this
//! identity's database, registers this device on every server that relays
//! private messages, catches up on what waited there, and handles each
//! delivery in one worker, one at a time.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::future::BoxFuture;
use opencord_common::private::{ONE_TIME_KEYS_TARGET, RECEIVED_RECORD_TTL_MS};
use opencord_proto::v1 as proto;
use proto::pending_event::Kind as PendingKind;
use proto::request::Kind as Request;
use proto::response::Result as Response;
use sqlx::{Row, SqliteConnection};
use tokio::runtime::Handle;
use tokio::sync::{mpsc, oneshot};
use tokio::time::MissedTickBehavior;

use super::PrivateError;
use super::contacts::{self, Contact, Conversation, FollowUp, FriendEvent, FriendState};
use super::device::Device;
use super::inbox::{self, Reset, ResetLimiter};
use super::members::{MemberIndex, MemberInfo};
use super::outbox::{self, DeviceCache};
use super::store::PrivateStore;
use crate::api::types::{
    ConversationStatus, CoreError, FriendStatus, PrivateContact, PrivateEvent, PrivateMessage,
    PrivateMessageKind, PrivateOpened, PrivateSendState,
};
use crate::identity::{self, Identity};

/// Acknowledgements waiting for one server are sent once this many wait…
const ACK_BATCH: usize = 100;
/// …or on this tick while any wait.
const ACK_EVERY: Duration = Duration::from_millis(200);
/// How long removing a server waits for `UnregisterDevice`.
const UNREGISTER_TIMEOUT: Duration = Duration::from_secs(5);
/// The most messages one `private_messages` call answers.
const MESSAGES_PAGE: u32 = 100;
/// A contact's preview: the latest text's first line, cut to this many
/// characters.
const PREVIEW_CHARS: usize = 100;
/// The label a message's body is sealed with (its id is the row).
pub const MESSAGE_BODY_LABEL: &str = "messages.body";

/// What the service needs from the client, held weakly by the client's
/// side.
pub trait Transport: Send + Sync {
    fn request(
        &self,
        server_key: &str,
        kind: Request,
    ) -> BoxFuture<'static, Result<Response, CoreError>>;
    /// The saved server with this id (hex), by its key.
    fn server_key_for(&self, server_id: &str) -> Option<String>;
    /// The saved server's id (hex).
    fn server_id_of(&self, server_key: &str) -> Option<String>;
    fn server_name(&self, server_key: &str) -> Option<String>;
    fn identity(&self) -> Option<Identity>;
}

/// The client's one service; a cheap clone.
#[derive(Clone)]
pub struct PrivateService {
    shared: Arc<Shared>,
}

struct Shared {
    runtime: Handle,
    data_dir: PathBuf,
    transport: Arc<dyn Transport>,
    members: MemberIndex,
    /// Verified device lists for sending; see `DeviceCache` for when its
    /// entries leave.
    devices: DeviceCache,
    /// Session resets sent; see `ResetLimiter` for when its entries leave.
    resets: ResetLimiter,
    events: Mutex<Option<mpsc::UnboundedSender<PrivateEvent>>>,
    /// Opening and closing, one at a time.
    opening: tokio::sync::Mutex<()>,
    open: Mutex<Option<Arc<Open>>>,
    /// Registrations running, by server key. An entry leaves when its task
    /// ends.
    registering: Mutex<HashMap<String, Registration>>,
    next_holder: AtomicU64,
    /// The peers last told to the app as friends, so a change can be sent
    /// to the servers that relay only between friends.
    friends: Mutex<HashSet<String>>,
}

struct Registration {
    holder: u64,
    /// Another `Ready` came while it ran: run once more.
    again: bool,
}

/// One identity's open database, device and worker.
pub struct Open {
    identity: [u8; 32],
    store: PrivateStore,
    /// The crypto lock (Working Rule 11): held from loading sessions until
    /// their transaction commits, never across a network request.
    device: tokio::sync::Mutex<Device>,
    queue: mpsc::UnboundedSender<Job>,
    /// Servers where this device registered. An entry leaves when its
    /// server is removed.
    registered: Mutex<HashSet<String>>,
}

impl Open {
    pub fn store(&self) -> &PrivateStore {
        &self.store
    }

    pub fn device(&self) -> &tokio::sync::Mutex<Device> {
        &self.device
    }

    /// This identity's public key.
    pub fn identity(&self) -> [u8; 32] {
        self.identity
    }
}

enum Job {
    Pending {
        server_key: String,
        event: Box<proto::PendingEvent>,
    },
    /// Members as a server last showed them, for their names.
    Names(Vec<MemberInfo>),
    /// Answered once everything queued before it is handled.
    Barrier(oneshot::Sender<()>),
}

/// What handling one pending event left to do.
#[derive(Default)]
struct Handled {
    /// The event's id, to acknowledge.
    ack: Option<i64>,
    /// For this person.
    follow_ups: Vec<([u8; 32], FollowUp)>,
    /// A message from that device couldn't be decrypted.
    reset: Option<Reset>,
}

impl PrivateService {
    pub fn new(runtime: Handle, data_dir: PathBuf, transport: Arc<dyn Transport>) -> Self {
        Self {
            shared: Arc::new(Shared {
                runtime,
                data_dir,
                transport,
                members: MemberIndex::default(),
                devices: DeviceCache::default(),
                resets: ResetLimiter::default(),
                events: Mutex::new(None),
                opening: tokio::sync::Mutex::new(()),
                open: Mutex::new(None),
                registering: Mutex::new(HashMap::new()),
                next_holder: AtomicU64::new(1),
                friends: Mutex::new(HashSet::new()),
            }),
        }
    }

    pub fn transport(&self) -> &Arc<dyn Transport> {
        &self.shared.transport
    }

    pub fn members(&self) -> &MemberIndex {
        &self.shared.members
    }

    pub fn device_cache(&self) -> &DeviceCache {
        &self.shared.devices
    }

    pub fn resets(&self) -> &ResetLimiter {
        &self.shared.resets
    }

    /// The open database, device and worker.
    pub fn opened(&self) -> Result<Arc<Open>, CoreError> {
        self.open_now().ok_or(CoreError::NotInitialized)
    }

    fn open_now(&self) -> Option<Arc<Open>> {
        lock(&self.shared.open).clone()
    }

    /// A new receiver for private events; an earlier one hears nothing
    /// more.
    pub fn events(&self) -> mpsc::UnboundedReceiver<PrivateEvent> {
        let (sender, receiver) = mpsc::unbounded_channel();
        *lock(&self.shared.events) = Some(sender);
        receiver
    }

    pub fn emit(&self, event: PrivateEvent) {
        if self.friends_changed(&event) {
            self.sync_friends();
        }
        if let Some(events) = lock(&self.shared.events).as_ref() {
            // Nobody listening is fine: the app reads the state again when
            // it starts listening.
            let _ = events.send(event);
        }
    }

    /// Opens the identity's database with `master_key`, starts the worker
    /// and registers on every server ready for it. Opening again for the
    /// same identity does nothing; for another, the old one closes first.
    pub async fn open(&self, master_key: [u8; 32]) -> Result<PrivateOpened, CoreError> {
        let identity = self
            .shared
            .transport
            .identity()
            .ok_or(CoreError::NoIdentity)?;
        let public_key = identity.public_key();
        let _opening = self.shared.opening.lock().await;
        if self
            .open_now()
            .is_some_and(|open| open.identity == public_key)
        {
            return Ok(PrivateOpened {
                started_fresh: false,
            });
        }
        let earlier = lock(&self.shared.open).take();
        if let Some(earlier) = earlier {
            earlier.store.pool().close().await;
        }

        let now = now_ms();
        let opened =
            PrivateStore::open(&self.shared.data_dir, &public_key, &master_key, now).await?;
        let store = opened.store;
        let device = match prepare(&store, now).await {
            Ok(device) => device,
            Err(error) => {
                store.close().await;
                return Err(error.into());
            }
        };
        let (queue, jobs) = mpsc::unbounded_channel();
        let open = Arc::new(Open {
            identity: public_key,
            store,
            device: tokio::sync::Mutex::new(device),
            queue,
            registered: Mutex::new(HashSet::new()),
        });
        self.shared.runtime.spawn(run_worker(
            Arc::downgrade(&self.shared),
            Arc::downgrade(&open),
            jobs,
        ));
        *lock(&self.shared.open) = Some(open);
        for key in self.shared.members.private_servers() {
            self.start_registration(&key);
        }
        Ok(PrivateOpened {
            started_fresh: opened.started_fresh,
        })
    }

    /// Every contact.
    pub async fn contacts(&self) -> Result<Vec<PrivateContact>, CoreError> {
        let open = self.opened()?;
        let mut conn = open
            .store
            .pool()
            .acquire()
            .await
            .map_err(PrivateError::from)?;
        let all = contacts::load_all(&mut conn, open.store.sealer()).await?;
        let mut views = Vec::with_capacity(all.len());
        for contact in &all {
            views.push(self.contact_view(&open, &mut conn, contact).await?);
        }
        Ok(views)
    }

    /// The conversation with `peer` (hex), newest first, before the message
    /// `before` (hex); at most 100.
    pub async fn messages(
        &self,
        peer: &str,
        before: Option<String>,
        limit: u32,
    ) -> Result<Vec<PrivateMessage>, CoreError> {
        let peer = parse_hex::<32>(peer, "that person isn't valid")?;
        let before = before
            .map(|id| parse_hex::<16>(&id, "that message isn't valid"))
            .transpose()?;
        let limit = limit.min(MESSAGES_PAGE);
        let open = self.opened()?;
        let mut conn = open
            .store
            .pool()
            .acquire()
            .await
            .map_err(PrivateError::from)?;
        let rows = match before {
            None => {
                sqlx::query(
                    "SELECT id, peer, outgoing, kind, body, server_id, sent_at, edited_at, state \
                     FROM messages WHERE peer = $1 \
                     ORDER BY sent_at DESC, id DESC LIMIT $2",
                )
                .bind(peer.as_slice())
                .bind(i64::from(limit))
                .fetch_all(&mut *conn)
                .await
            }
            Some(before) => {
                let sent_at: Option<i64> =
                    sqlx::query_scalar("SELECT sent_at FROM messages WHERE id = $1 AND peer = $2")
                        .bind(before.as_slice())
                        .bind(peer.as_slice())
                        .fetch_optional(&mut *conn)
                        .await
                        .map_err(PrivateError::from)?;
                let sent_at = sent_at.ok_or_else(|| CoreError::InvalidInput {
                    message: "that message isn't in this conversation".to_owned(),
                })?;
                sqlx::query(
                    "SELECT id, peer, outgoing, kind, body, server_id, sent_at, edited_at, state \
                     FROM messages WHERE peer = $1 \
                     AND (sent_at < $2 OR (sent_at = $2 AND id < $3)) \
                     ORDER BY sent_at DESC, id DESC LIMIT $4",
                )
                .bind(peer.as_slice())
                .bind(sent_at)
                .bind(before.as_slice())
                .bind(i64::from(limit))
                .fetch_all(&mut *conn)
                .await
            }
        }
        .map_err(PrivateError::from)?;
        rows.iter()
            .map(|row| Ok(self.message_view(&open, &MessageRow::from_row(row)?)?))
            .collect()
    }

    /// The app's view of a contact.
    pub async fn contact_view(
        &self,
        open: &Open,
        conn: &mut SqliteConnection,
        contact: &Contact,
    ) -> Result<PrivateContact, PrivateError> {
        let peer = contact.public_key.as_slice();
        let unread: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM messages \
             WHERE peer = $1 AND outgoing = 0 AND kind = 0 AND sent_at > $2",
        )
        .bind(peer)
        .bind(contact.last_read_at)
        .fetch_one(&mut *conn)
        .await?;
        let latest: Option<(Vec<u8>, Option<Vec<u8>>)> = sqlx::query_as(
            "SELECT id, body FROM messages WHERE peer = $1 AND kind = 0 \
             ORDER BY sent_at DESC, id DESC LIMIT 1",
        )
        .bind(peer)
        .fetch_optional(&mut *conn)
        .await?;
        let preview = match latest {
            Some((id, Some(body))) => {
                let text = open_text(&open.store, &id, &body)?;
                Some(
                    text.lines()
                        .next()
                        .unwrap_or_default()
                        .chars()
                        .take(PREVIEW_CHARS)
                        .collect(),
                )
            }
            _ => None,
        };
        Ok(PrivateContact {
            peer: hex::encode(contact.public_key),
            fingerprint: identity::public_key_fingerprint(&contact.public_key),
            name: contact.name.clone(),
            friend: match contact.friend_state {
                FriendState::None => FriendStatus::None,
                FriendState::Outgoing => FriendStatus::Outgoing,
                FriendState::Incoming => FriendStatus::Incoming,
                FriendState::Friends => FriendStatus::Friends,
            },
            request_spam: contact.request.as_ref().is_some_and(|request| request.spam),
            blocked: contact.blocked,
            conversation: match contact.conversation {
                Conversation::None => ConversationStatus::None,
                Conversation::Request => ConversationStatus::Request,
                Conversation::Accepted => ConversationStatus::Accepted,
            },
            relay_server_key: contact
                .relay_server_id
                .as_deref()
                .and_then(|id| self.shared.transport.server_key_for(id)),
            last_activity_ms: contact.last_activity_at,
            unread: u32::try_from(unread).unwrap_or(u32::MAX),
            muted: contact.muted,
            hidden: contact.hidden,
            preview,
        })
    }

    /// The app's view of a stored message.
    pub fn message_view(
        &self,
        open: &Open,
        row: &MessageRow,
    ) -> Result<PrivateMessage, PrivateError> {
        let content = match &row.body {
            Some(body) => open_text(&open.store, &row.id, body)?,
            None => String::new(),
        };
        Ok(PrivateMessage {
            id: hex::encode(&row.id),
            peer: hex::encode(&row.peer),
            outgoing: row.outgoing,
            kind: match row.kind {
                0 => PrivateMessageKind::Text,
                1 => PrivateMessageKind::Divider,
                2 => PrivateMessageKind::Moved,
                3 => PrivateMessageKind::Undecryptable,
                _ => {
                    return Err(PrivateError::Storage(
                        "a stored message has an unknown kind".to_owned(),
                    ));
                }
            },
            content,
            server_key: row
                .server_id
                .as_deref()
                .and_then(|id| self.shared.transport.server_key_for(id)),
            sent_at_ms: row.sent_at,
            edited_at_ms: row.edited_at,
            state: match row.state {
                0 => PrivateSendState::Received,
                1 => PrivateSendState::Sending,
                2 => PrivateSendState::Sent,
                3 => PrivateSendState::Failed,
                _ => {
                    return Err(PrivateError::Storage(
                        "a stored message has an unknown state".to_owned(),
                    ));
                }
            },
        })
    }

    // From the connections.

    /// Before the app hears `Ready`: the member list, names, and
    /// registration when the server relays private messages.
    pub fn on_ready(&self, key: &str, server_id: &str, ready: &proto::Ready) {
        let members = self.shared.members.on_ready(key, server_id, ready);
        let Some(open) = self.open_now() else {
            return;
        };
        if !members.is_empty() {
            queue(&open, Job::Names(members));
        }
        if ready.private_messages {
            self.start_registration(key);
        }
    }

    /// Whether `event` adds someone to the friends or takes them off.
    fn friends_changed(&self, event: &PrivateEvent) -> bool {
        let mut friends = lock(&self.shared.friends);
        match event {
            PrivateEvent::ContactChanged(contact) => {
                if contact.friend == FriendStatus::Friends && !contact.blocked {
                    friends.insert(contact.peer.clone())
                } else {
                    friends.remove(&contact.peer)
                }
            }
            PrivateEvent::ContactRemoved { peer } => friends.remove(peer),
            _ => false,
        }
    }

    /// Sends the friend list again to each server that relays only between
    /// friends, in the background.
    fn sync_friends(&self) {
        let servers = self.shared.members.friends_only_servers();
        let Some(open) = self.open_now() else {
            return;
        };
        for key in servers {
            let service = self.clone();
            let open = open.clone();
            self.shared.runtime.spawn(async move {
                if let Err(error) =
                    super::actions::sync_privacy_to_server(&service, &open, &key).await
                {
                    tracing::warn!(%error, server_key = %crate::log::alias::server(&key), "could not send the friend list");
                }
            });
        }
    }

    /// `PrivateSettingsUpdate`: the owner turned private messages on or
    /// off, or friends-only. A server that starts relaying gets this
    /// device; one that turns friends-only on gets the friend list.
    pub fn on_private_settings(&self, key: &str, settings: &proto::PrivateSettings) {
        let relayed = self.shared.members.on_private_settings(key, settings);
        if !settings.enabled {
            return;
        }
        if !relayed {
            self.start_registration(key);
        } else if settings.friends_only
            && let Some(open) = self.open_now()
        {
            let service = self.clone();
            let key = key.to_owned();
            self.shared.runtime.spawn(async move {
                if let Err(error) =
                    super::actions::sync_privacy_to_server(&service, &open, &key).await
                {
                    tracing::warn!(%error, server_key = %crate::log::alias::server(&key), "could not send the friend list");
                }
            });
        }
    }

    /// `MemberJoin`, `MemberUpdate` and `MemberLeave`.
    pub fn on_member_event(&self, key: &str, kind: &proto::event::Kind) {
        let member = match kind {
            proto::event::Kind::MemberJoin(event) => event.member.as_ref(),
            proto::event::Kind::MemberUpdate(event) => event.member.as_ref(),
            proto::event::Kind::MemberLeave(event) => {
                self.shared.members.remove_member(key, event.user_id);
                return;
            }
            _ => return,
        };
        let Some(member) = member
            .and_then(|member| member.user.as_ref())
            .and_then(MemberInfo::of)
        else {
            return;
        };
        self.shared.members.upsert(key, member.clone());
        if let Some(open) = self.open_now() {
            queue(&open, Job::Names(vec![member.clone()]));
            if matches!(kind, proto::event::Kind::MemberJoin(_)) {
                let service = self.clone();
                let server_key = key.to_owned();
                let peer = member.public_key;
                self.shared.runtime.spawn(async move {
                    let Ok(mut conn) = open.store().pool().acquire().await else {
                        return;
                    };
                    let is_blocked = contacts::load(&mut conn, open.store().sealer(), peer)
                        .await
                        .ok()
                        .flatten()
                        .is_some_and(|c| c.blocked);
                    if is_blocked {
                        let _ =
                            super::actions::sync_privacy_to_server(&service, &open, &server_key)
                                .await;
                    }
                });
            }
        }
    }

    /// A delivery pushed by the server. Without an open database it stays
    /// on the server until the next catch-up.
    pub fn on_pending(&self, key: &str, event: proto::PendingEvent) {
        if let Some(open) = self.open_now() {
            queue(
                &open,
                Job::Pending {
                    server_key: key.to_owned(),
                    event: Box::new(event),
                },
            );
        }
    }

    /// `PrivateTypingStart`: the app hears it unless that person is blocked.
    pub fn on_typing(&self, key: &str, user_id: i64) {
        let Some(peer) = self.shared.members.public_key_of(key, user_id) else {
            return;
        };
        let Some(open) = self.open_now() else {
            return;
        };
        let service = self.clone();
        self.shared.runtime.spawn(async move {
            match blocked(&open, peer).await {
                Ok(false) => service.emit(PrivateEvent::Typing {
                    peer: hex::encode(peer),
                }),
                Ok(true) => {}
                Err(error) => tracing::warn!(
                    error = private_kind(&error),
                    "couldn't look up who is typing"
                ),
            }
        });
    }

    pub fn on_connection(&self, key: &str, connected: bool) {
        self.shared.members.set_connected(key, connected);
        // Back on a server this device registered on: a resumed session
        // brings no `Ready`, so catch up on what failed or waited meanwhile.
        if connected
            && self
                .open_now()
                .is_some_and(|open| lock(&open.registered).contains(key))
        {
            self.start_registration(key);
        }
    }

    /// Before the server is removed: this device leaves it.
    pub async fn before_remove(&self, key: &str) {
        let registered = self
            .open_now()
            .is_some_and(|open| lock(&open.registered).remove(key));
        if registered {
            let unregister = self
                .shared
                .transport
                .request(key, Request::UnregisterDevice(proto::UnregisterDevice {}));
            match tokio::time::timeout(UNREGISTER_TIMEOUT, unregister).await {
                Ok(Ok(_)) => {}
                Ok(Err(error)) => warn_core("unregistering this device failed", key, &error),
                Err(_) => warn_core("unregistering this device failed", key, &CoreError::Timeout),
            }
        }
        self.shared.devices.forget_server(key);
        self.shared.members.remove_server(key);
    }

    // Registration.

    /// Registers on `key` in its own task, at most one at a time per
    /// server: a `Ready` meanwhile makes the running one go again.
    fn start_registration(&self, key: &str) {
        let holder = {
            let mut running = lock(&self.shared.registering);
            if let Some(running) = running.get_mut(key) {
                running.again = true;
                return;
            }
            let holder = self.shared.next_holder.fetch_add(1, Ordering::Relaxed);
            running.insert(
                key.to_owned(),
                Registration {
                    holder,
                    again: false,
                },
            );
            holder
        };
        let service = self.clone();
        let key = key.to_owned();
        self.shared.runtime.spawn(async move {
            let _slot = RegistrationSlot {
                shared: Arc::downgrade(&service.shared),
                key: key.clone(),
                holder,
            };
            loop {
                let Some(open) = service.open_now() else {
                    break;
                };
                if let Err(error) = service.register(&open, &key).await {
                    warn_core("registering this device failed", &key, &error);
                }
                if !service.take_again(&key, holder) {
                    break;
                }
            }
        });
    }

    fn take_again(&self, key: &str, holder: u64) -> bool {
        match lock(&self.shared.registering).get_mut(key) {
            Some(running) if running.holder == holder && running.again => {
                running.again = false;
                true
            }
            _ => false,
        }
    }

    /// Steps 1–5 of a registration; the first failure stops it until the
    /// next `Ready`.
    async fn register(&self, open: &Open, key: &str) -> Result<(), CoreError> {
        let identity = self.identity_for(open)?;
        let pickle_key = open.store.pickle_key();
        // 1.
        let (keys, fallback) = {
            let mut device = open.device.lock().await;
            if device.rotate_fallback_if_due(now_ms()) {
                let mut conn = open
                    .store
                    .pool()
                    .acquire()
                    .await
                    .map_err(PrivateError::from)?;
                device.save(&mut conn, pickle_key).await?;
            }
            (device.keys(&identity), device.fallback_prekey())
        };
        // 2.
        let response = self
            .request(
                key,
                Request::RegisterDevice(proto::RegisterDevice {
                    device: Some(keys),
                    one_time_keys: Vec::new(),
                    fallback_key: Some(fallback),
                }),
            )
            .await?;
        let Response::DeviceRegistration(registration) = response else {
            return Err(crate::client::unexpected(&response));
        };
        lock(&open.registered).insert(key.to_owned());
        // 3.
        let left = usize::try_from(registration.one_time_keys_left).unwrap_or(usize::MAX);
        if left < ONE_TIME_KEYS_TARGET {
            let one_time_keys = {
                let mut device = open.device.lock().await;
                let keys = device.new_one_time_keys(ONE_TIME_KEYS_TARGET - left);
                let mut conn = open
                    .store
                    .pool()
                    .acquire()
                    .await
                    .map_err(PrivateError::from)?;
                device.save(&mut conn, pickle_key).await?;
                keys
            };
            self.request(
                key,
                Request::UploadKeys(proto::UploadKeys {
                    one_time_keys,
                    fallback_key: None,
                }),
            )
            .await?;
        }
        // 4.
        super::actions::sync_privacy_to_server(self, open, key).await?;
        // 5.
        self.catch_up(open, key).await?;
        // Then what waited to be sent through this server.
        outbox::resend_unsent(self, key).await
    }

    /// Fetches what waited for this device, a page at a time: each page
    /// goes through the worker before the next is fetched.
    async fn catch_up(&self, open: &Open, key: &str) -> Result<(), CoreError> {
        let mut after_id = 0;
        loop {
            let response = self
                .request(
                    key,
                    Request::FetchPendingEvents(proto::FetchPendingEvents { after_id, limit: 0 }),
                )
                .await?;
            let Response::PendingEvents(page) = response else {
                return Err(crate::client::unexpected(&response));
            };
            let Some(last) = page.events.last().map(|event| event.id) else {
                return Ok(());
            };
            for event in page.events {
                send_job(
                    open,
                    Job::Pending {
                        server_key: key.to_owned(),
                        event: Box::new(event),
                    },
                )?;
            }
            let (done, handled) = oneshot::channel();
            send_job(open, Job::Barrier(done))?;
            handled.await.map_err(|_| worker_stopped())?;
            if !page.more {
                return Ok(());
            }
            after_id = last;
        }
    }

    // The worker's work.

    /// Step 1 and 2 for one event: skip it if it was handled before, else
    /// apply it and record it in one transaction.
    async fn handle_pending(
        &self,
        open: &Open,
        key: &str,
        event: proto::PendingEvent,
    ) -> Result<Handled, PrivateError> {
        let Some(server_id) = self
            .shared
            .members
            .server_id(key)
            .or_else(|| self.shared.transport.server_id_of(key))
        else {
            tracing::warn!(
                event_id = event.id,
                "a private event from a server with no id"
            );
            return Ok(Handled::default());
        };
        let sealer = open.store.sealer();
        let (seen, privacy) = {
            let mut conn = open.store.pool().acquire().await?;
            let seen: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM received_events WHERE server_id = $1 AND event_id = $2",
            )
            .bind(&server_id)
            .bind(event.id)
            .fetch_optional(&mut *conn)
            .await?;
            (seen.is_some(), contacts::privacy(&mut conn).await?)
        };
        if seen {
            return Ok(Handled {
                ack: Some(event.id),
                ..Handled::default()
            });
        }
        let friend = match &event.kind {
            Some(PendingKind::FriendRequestReceived(received)) => FriendEvent::Received(received),
            Some(PendingKind::FriendRequestSent(sent)) => FriendEvent::Sent(sent),
            Some(PendingKind::FriendRequestAnswered(answered)) => FriendEvent::Answered(answered),
            Some(PendingKind::FriendRequestCancelled(cancelled)) => {
                FriendEvent::Cancelled(cancelled)
            }
            Some(PendingKind::FriendRemoved(removed)) => FriendEvent::Removed(removed),
            Some(PendingKind::PrivateMessage(message)) => {
                let reset = inbox::receive(
                    self,
                    open,
                    key,
                    (&server_id, event.id),
                    message.clone(),
                    &privacy,
                )
                .await?;
                return Ok(Handled {
                    ack: Some(event.id),
                    follow_ups: Vec::new(),
                    reset,
                });
            }
            // A kind this version doesn't know: nothing to do with it.
            None => {
                let mut conn = open.store.pool().acquire().await?;
                record_received(&mut conn, &server_id, event.id, now_ms()).await?;
                return Ok(Handled {
                    ack: Some(event.id),
                    ..Handled::default()
                });
            }
        };
        let now = now_ms();
        // It reads the contact, then writes: take the write lock up front.
        let mut tx = open.store.pool().begin_with("BEGIN IMMEDIATE").await?;
        let outcome = contacts::apply_friend_event(
            &mut tx,
            sealer,
            &open.identity,
            &server_id,
            friend,
            &privacy,
            now,
        )
        .await?;
        record_received(&mut tx, &server_id, event.id, now).await?;
        tx.commit().await?;

        let mut follow_ups = Vec::new();
        if let Some(contact) = &outcome.changed {
            let peer = hex::encode(contact.public_key);
            self.emit_contact(open, contact).await?;
            if let Some(spam) = outcome.new_request {
                self.emit(PrivateEvent::FriendRequestReceived { peer, spam });
            }
            follow_ups = outcome
                .follow_ups
                .into_iter()
                .map(|follow_up| (contact.public_key, follow_up))
                .collect();
        }
        Ok(Handled {
            ack: Some(event.id),
            follow_ups,
            reset: None,
        })
    }

    /// Answers or cancels after a friend event; on success the contact
    /// changes to match. A failure leaves it as it was.
    async fn run_follow_up(
        &self,
        open: &Open,
        peer: [u8; 32],
        follow_up: FollowUp,
    ) -> Result<(), CoreError> {
        match follow_up {
            FollowUp::Answer {
                server_id,
                request_id,
                accept,
            } => {
                let key = self.server_key(&server_id)?;
                let slip = if accept {
                    self.identity_for(open)?.slip_for(&peer).to_vec()
                } else {
                    Vec::new()
                };
                self.request(
                    &key,
                    Request::AnswerFriendRequest(proto::AnswerFriendRequest {
                        request_id,
                        accept,
                        slip,
                    }),
                )
                .await?;
                self.change_contact(open, peer, |contact| {
                    contacts::after_my_answer(contact, accept, &server_id);
                    true
                })
                .await
            }
            FollowUp::Cancel {
                server_id,
                request_id,
            } => {
                let key = self.server_key(&server_id)?;
                self.request(
                    &key,
                    Request::CancelFriendRequest(proto::CancelFriendRequest { request_id }),
                )
                .await?;
                // Only the request cancelled: after crossed requests, the
                // accept before it already cleared it.
                self.change_contact(open, peer, |contact| {
                    let cancelled = contact.request.as_ref().is_some_and(|request| {
                        request.server_id == server_id && request.request_id == request_id
                    });
                    if cancelled {
                        contacts::after_my_cancel(contact);
                    }
                    cancelled
                })
                .await
            }
        }
    }

    /// Loads the contact, changes it, and saves and emits it when `change`
    /// says it changed.
    async fn change_contact(
        &self,
        open: &Open,
        peer: [u8; 32],
        change: impl FnOnce(&mut Contact) -> bool,
    ) -> Result<(), CoreError> {
        let sealer = open.store.sealer();
        let mut conn = open
            .store
            .pool()
            .acquire()
            .await
            .map_err(PrivateError::from)?;
        let Some(mut contact) = contacts::load(&mut conn, sealer, peer).await? else {
            return Ok(());
        };
        if !change(&mut contact) {
            return Ok(());
        }
        contacts::save(&mut conn, sealer, &contact, now_ms()).await?;
        drop(conn);
        self.emit_contact(open, &contact).await?;
        Ok(())
    }

    /// Each member who is a contact gets the name last seen.
    async fn update_names(&self, open: &Open, members: &[MemberInfo]) -> Result<(), PrivateError> {
        let names: HashMap<[u8; 32], &str> = members
            .iter()
            .filter(|member| !member.name.is_empty())
            .map(|member| (member.public_key, member.name.as_str()))
            .collect();
        let sealer = open.store.sealer();
        let mut conn = open.store.pool().acquire().await?;
        let renamed: Vec<Contact> = contacts::load_all(&mut conn, sealer)
            .await?
            .into_iter()
            .filter_map(|mut contact| {
                let name = *names.get(&contact.public_key)?;
                if contact.name.as_deref() == Some(name) {
                    return None;
                }
                contact.name = Some(name.to_owned());
                Some(contact)
            })
            .collect();
        for contact in &renamed {
            contacts::save(&mut conn, sealer, contact, now_ms()).await?;
            let view = self.contact_view(open, &mut conn, contact).await?;
            self.emit(PrivateEvent::ContactChanged(view));
        }
        Ok(())
    }

    pub(super) async fn emit_contact(
        &self,
        open: &Open,
        contact: &Contact,
    ) -> Result<(), PrivateError> {
        let mut conn = open.store.pool().acquire().await?;
        let view = self.contact_view(open, &mut conn, contact).await?;
        self.emit(PrivateEvent::ContactChanged(view));
        Ok(())
    }

    /// Sends the acknowledgements in their own task: a failure is logged,
    /// and the server delivers those events again.
    fn send_acks(&self, key: String, ids: Vec<i64>) {
        let count = ids.len();
        let acknowledge = self.shared.transport.request(
            &key,
            Request::AckPendingEvents(proto::AckPendingEvents { ids }),
        );
        self.shared.runtime.spawn(async move {
            if let Err(error) = acknowledge.await {
                tracing::warn!(
                    server = %key,
                    count,
                    error = core_kind(&error),
                    "acknowledging private events failed; the server will deliver them again"
                );
            }
        });
    }

    pub(super) async fn request(&self, key: &str, kind: Request) -> Result<Response, CoreError> {
        self.shared.transport.request(key, kind).await
    }

    fn server_key(&self, server_id: &str) -> Result<String, CoreError> {
        self.shared
            .transport
            .server_key_for(server_id)
            .ok_or(CoreError::UnknownServer)
    }

    /// The identity, while it's still the one whose database is open.
    pub(super) fn identity_for(&self, open: &Open) -> Result<Identity, CoreError> {
        self.shared
            .transport
            .identity()
            .filter(|identity| identity.public_key() == open.identity)
            .ok_or(CoreError::NoIdentity)
    }
}

/// Releases its server's registration entry when the task ends, unless
/// another task holds it by then.
struct RegistrationSlot {
    shared: Weak<Shared>,
    key: String,
    holder: u64,
}

impl Drop for RegistrationSlot {
    fn drop(&mut self) {
        let Some(shared) = self.shared.upgrade() else {
            return;
        };
        let mut running = lock(&shared.registering);
        if running
            .get(&self.key)
            .is_some_and(|running| running.holder == self.holder)
        {
            running.remove(&self.key);
        }
    }
}

/// A stored message's columns.
pub struct MessageRow {
    pub id: Vec<u8>,
    pub peer: Vec<u8>,
    pub outgoing: bool,
    pub kind: i64,
    pub body: Option<Vec<u8>>,
    pub server_id: Option<String>,
    pub sent_at: i64,
    pub edited_at: Option<i64>,
    pub state: i64,
}

impl MessageRow {
    pub fn from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Self, PrivateError> {
        Ok(Self {
            id: row.try_get("id")?,
            peer: row.try_get("peer")?,
            outgoing: row.try_get::<i64, _>("outgoing")? != 0,
            kind: row.try_get("kind")?,
            body: row.try_get("body")?,
            server_id: row.try_get("server_id")?,
            sent_at: row.try_get("sent_at")?,
            edited_at: row.try_get("edited_at")?,
            state: row.try_get("state")?,
        })
    }
}

/// The one worker: deliveries and names, one at a time, and the batched
/// acknowledgements. It ends when its database closes.
async fn run_worker(
    shared: Weak<Shared>,
    open: Weak<Open>,
    mut jobs: mpsc::UnboundedReceiver<Job>,
) {
    // Waiting acknowledgements by server key; an entry leaves when it's
    // sent.
    let mut acks: HashMap<String, Vec<i64>> = HashMap::new();
    let mut tick = tokio::time::interval(ACK_EVERY);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            job = jobs.recv() => {
                let Some(job) = job else {
                    break;
                };
                let (Some(shared), Some(open)) = (shared.upgrade(), open.upgrade()) else {
                    break;
                };
                let service = PrivateService { shared };
                match job {
                    Job::Pending { server_key, event } => {
                        let event_id = event.id;
                        let handled = match service.handle_pending(&open, &server_key, *event).await {
                            Ok(handled) => handled,
                            Err(error) => {
                                tracing::warn!(
                                    server = %server_key,
                                    event_id,
                                    error = private_kind(&error),
                                    "a private event couldn't be handled; it stays on the server"
                                );
                                Handled::default()
                            }
                        };
                        if let Some(id) = handled.ack {
                            let waiting = acks.entry(server_key.clone()).or_default();
                            waiting.push(id);
                            if waiting.len() >= ACK_BATCH
                                && let Some(ids) = acks.remove(&server_key)
                            {
                                service.send_acks(server_key.clone(), ids);
                            }
                        }
                        for (peer, follow_up) in handled.follow_ups {
                            if let Err(error) = service.run_follow_up(&open, peer, follow_up).await {
                                warn_core("a friend request follow-up failed", &server_key, &error);
                            }
                        }
                        if let Some(reset) = handled.reset
                            && let Err(error) = inbox::reset_session(&service, reset).await
                        {
                            warn_core("a session reset failed", &server_key, &error);
                        }
                    }
                    Job::Names(members) => {
                        if let Err(error) = service.update_names(&open, &members).await {
                            tracing::warn!(error = private_kind(&error), "updating contact names failed");
                        }
                    }
                    Job::Barrier(done) => {
                        // The catch-up that waited may have given up.
                        let _ = done.send(());
                    }
                }
            }
            _ = tick.tick(), if !acks.is_empty() => {
                let Some(shared) = shared.upgrade() else {
                    break;
                };
                let service = PrivateService { shared };
                for (key, ids) in acks.drain() {
                    service.send_acks(key, ids);
                }
            }
        }
    }
}

/// The device, and received-event records older than 35 days gone.
async fn prepare(store: &PrivateStore, now: i64) -> Result<Device, PrivateError> {
    let device = Device::load_or_create(store, now).await?;
    sqlx::query("DELETE FROM received_events WHERE received_at < $1")
        .bind(now.saturating_sub(RECEIVED_RECORD_TTL_MS))
        .execute(store.pool())
        .await?;
    Ok(device)
}

/// The event was handled: a delivery of it again is only acknowledged.
pub(super) async fn record_received(
    conn: &mut SqliteConnection,
    server_id: &str,
    event_id: i64,
    now: i64,
) -> Result<(), PrivateError> {
    sqlx::query(
        "INSERT INTO received_events (server_id, event_id, received_at) VALUES ($1, $2, $3) \
         ON CONFLICT (server_id, event_id) DO NOTHING",
    )
    .bind(server_id)
    .bind(event_id)
    .bind(now)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

async fn blocked(open: &Open, peer: [u8; 32]) -> Result<bool, PrivateError> {
    let mut conn = open.store.pool().acquire().await?;
    Ok(contacts::load(&mut conn, open.store.sealer(), peer)
        .await?
        .is_some_and(|contact| contact.blocked))
}

pub(super) fn open_text(
    store: &PrivateStore,
    id: &[u8],
    body: &[u8],
) -> Result<String, PrivateError> {
    let text = store.sealer().open(MESSAGE_BODY_LABEL, id, body)?;
    String::from_utf8(text)
        .map_err(|_| PrivateError::Invalid("a stored message isn't text".to_owned()))
}

/// Queues work for the worker from a connection; a worker that stopped
/// means the database closed, and the work waits for the next opening.
fn queue(open: &Open, job: Job) {
    if open.queue.send(job).is_err() {
        tracing::debug!("the private messages worker has stopped");
    }
}

fn send_job(open: &Open, job: Job) -> Result<(), CoreError> {
    open.queue.send(job).map_err(|_| worker_stopped())
}

fn worker_stopped() -> CoreError {
    PrivateError::Storage("the private messages worker stopped".to_owned()).into()
}

pub(crate) fn parse_hex<const N: usize>(text: &str, message: &str) -> Result<[u8; N], CoreError> {
    hex::decode(text)
        .ok()
        .and_then(|bytes| <[u8; N]>::try_from(bytes.as_slice()).ok())
        .ok_or_else(|| CoreError::InvalidInput {
            message: message.to_owned(),
        })
}

pub(super) fn warn_core(what: &'static str, key: &str, error: &CoreError) {
    match error {
        CoreError::Server { code, .. } => {
            tracing::warn!(server = %crate::log::alias::server(key), error = core_kind(error), code = ?code, "{what}");
        }
        _ => {
            tracing::warn!(server = %crate::log::alias::server(key), error = core_kind(error), "{what}")
        }
    }
}

/// An error's kind, for logs: never its message.
fn core_kind(error: &CoreError) -> &'static str {
    match error {
        CoreError::NotInitialized => "not initialized",
        CoreError::NoIdentity => "no identity",
        CoreError::UnknownServer => "unknown server",
        CoreError::NotConnected => "not connected",
        CoreError::Timeout => "timeout",
        CoreError::InvalidInput { .. } => "invalid input",
        CoreError::Server { .. } => "server",
        CoreError::FingerprintMismatch { .. } => "fingerprint mismatch",
        CoreError::Rejected { .. } => "rejected",
        CoreError::Connection { .. } => "connection",
        CoreError::Storage { .. } => "storage",
        CoreError::Screen { .. } => "screen",
        CoreError::Camera { .. } => "camera",
        CoreError::Cancelled => "cancelled",
    }
}

fn private_kind(error: &PrivateError) -> &'static str {
    match error {
        PrivateError::Storage(_) => "storage",
        PrivateError::Invalid(_) => "invalid",
        PrivateError::Crypto(_) => "crypto",
        PrivateError::Request(error) => core_kind(error),
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(super) fn now_ms() -> i64 {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from(since_epoch.as_millis()).unwrap_or(i64::MAX)
}
