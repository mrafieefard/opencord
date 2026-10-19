//! The client: an identity, the saved servers, and one connection task per
//! server. Every event from every server arrives on one channel.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, RwLock, Weak};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use opencord_common::address::{
    DEFAULT_PORT, Fingerprint, InviteLink, ServerAddress, format_fingerprint, parse_fingerprint,
};
use opencord_proto::v1 as proto;
use proto::request::Kind as Request;
use proto::response::Result as Response;
use tokio::runtime::Handle;
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::api::types::{
    AddServerOutcome, AudioSettings, Ban, CameraStarted, Channel, ChannelChanges, ChannelKind,
    ChannelPosition, CoreError, CoreEvent, CoreEventPayload, ErrorCode, HotkeyBinding,
    HotkeySupport, IdentityInfo, Invite, MediaEvent, Member, Message, OverwriteTargetKind,
    PermissionOverwrite, PresenceStatus, Role, RoleChanges, S3Config, ScreenProblem,
    ScreenShareRequest, ScreenShareStarted, ScreenStream, Server, ServerChanges, ServerInfo,
    SoundDraft, SoundSelection, SoundboardSound, SoundboardSoundChanges, StorageInfo,
    StorageTestResult, StorageTestTarget, StreamSourceKind, UploadSettings, User, VideoWant,
    VoiceCue, VoiceSettings, VoiceSettingsChanges, VoiceState,
};
use crate::artwork::ArtworkCache;
use crate::connection::{
    self, Clocks, Command, Connection, Context, Credentials, Established, HandshakeError,
    MediaTokens, Route, Routes,
};
use crate::convert;
use crate::http::MediaHttp;
use crate::identity::Identity;
use crate::media::{self, Media, MediaOptions, MediaTarget};
use crate::music_listening::MusicListening;
use crate::soundboard::Soundboard;
use crate::store::{SavedServer, Store, StoreError};
use crate::voice::VoiceServer;
use opencord_media::hotkeys::Hotkeys;

const INVITE_SCHEME: &str = "opencord://";
/// Voice server updates kept for a slow listener.
const VOICE_SERVER_BACKLOG: usize = 16;
/// A rejoin told to slow down tries this many times in all, waiting at
/// least [`REJOIN_WAIT`] between tries.
const REJOIN_TRIES: u32 = 5;
const REJOIN_WAIT: Duration = Duration::from_secs(1);
/// How often this device's idle time is looked at while in voice, and how
/// often the server hears it again while away (§12, AFK moves).
const IDLE_CHECK_EVERY: Duration = Duration::from_secs(5);
const IDLE_REPEAT_EVERY: Duration = Duration::from_secs(60);
/// How often the wall clock is compared with a timer, and how far it must
/// run ahead to mean the computer slept (§12: rejoin after sleep).
const WAKE_CHECK_EVERY: Duration = Duration::from_secs(2);
const SLEEP_GAP: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
}

struct Inner {
    runtime: Handle,
    /// Where connection tasks send events; the client looks at each before
    /// passing it on.
    events: mpsc::UnboundedSender<CoreEvent>,
    /// Events for the app.
    outward: mpsc::UnboundedSender<CoreEvent>,
    voice_servers: broadcast::Sender<VoiceServer>,
    store: Arc<Mutex<Store>>,
    credentials: RwLock<Option<Credentials>>,
    connections: Mutex<HashMap<String, Connection>>,
    voice: Mutex<Voice>,
    /// Voice media, once the app turned it on.
    media: OnceLock<Media>,
    /// The system's global hotkeys, once the app asked for some.
    hotkeys: tokio::sync::Mutex<Option<Hotkeys>>,
    /// Each server's media token, kept by its connection.
    media_tokens: MediaTokens,
    /// How each server was last reached.
    routes: Routes,
    soundboard: Arc<Soundboard>,
    artwork: Arc<ArtworkCache>,
    clocks: Clocks,
    /// How this person hears music, kept for media that starts later.
    music_listening: Mutex<MusicListening>,
}

/// The app's id, as its desktop file names it; the system's portals ask for
/// it.
const APP_ID: &str = "dev.opencord.opencord";

/// This device's voice: at most one channel across all servers (Phase 2
/// plan §3.5), and the self flags it joins with.
#[derive(Debug, Default)]
struct Voice {
    /// The server and channel this device is in, or is getting back into.
    target: Option<(String, i64)>,
    mute: bool,
    deaf: bool,
    /// This device's camera is on.
    video: bool,
    /// The server's side of this device's voice state.
    server_mute: bool,
    server_deaf: bool,
    suppress: bool,
    /// The user's id on each server, from its last `Ready`.
    self_ids: HashMap<String, i64>,
    /// The portal's token for the last screen shared, so sharing it again
    /// can skip the picker.
    screen_restore_token: Option<String>,
}

impl Voice {
    /// What voice media needs to know.
    fn media_target(&self) -> Option<MediaTarget> {
        self.target.as_ref().map(|(key, channel_id)| MediaTarget {
            server_key: key.clone(),
            channel_id: *channel_id,
            muted: self.mute || self.server_mute || self.suppress,
            deafened: self.deaf || self.server_deaf,
        })
    }
}

impl From<StoreError> for CoreError {
    fn from(error: StoreError) -> Self {
        Self::Storage {
            message: error.to_string(),
        }
    }
}

impl Client {
    /// Opens the local store in `data_dir`. Connection tasks run on
    /// `runtime`; their events arrive on the returned receiver.
    pub fn new(
        data_dir: &Path,
        runtime: Handle,
    ) -> Result<(Self, mpsc::UnboundedReceiver<CoreEvent>), CoreError> {
        let store = Arc::new(Mutex::new(Store::open(data_dir)?));
        let (events, mut incoming) = mpsc::unbounded_channel();
        let (outward, receiver) = mpsc::unbounded_channel();
        let media_tokens = MediaTokens::default();
        let routes = Routes::default();
        let soundboard = Arc::new(Soundboard::new(
            runtime.clone(),
            data_dir,
            Arc::clone(&routes),
            Arc::clone(&media_tokens),
        ));
        let artwork = Arc::new(ArtworkCache::new(
            data_dir,
            MediaHttp::new(Arc::clone(&routes), Arc::clone(&media_tokens)),
        ));
        let client = Self {
            inner: Arc::new(Inner {
                runtime: runtime.clone(),
                events,
                outward: outward.clone(),
                voice_servers: broadcast::channel(VOICE_SERVER_BACKLOG).0,
                store,
                credentials: RwLock::new(None),
                connections: Mutex::new(HashMap::new()),
                voice: Mutex::new(Voice::default()),
                media: OnceLock::new(),
                hotkeys: tokio::sync::Mutex::new(None),
                media_tokens,
                routes,
                soundboard,
                artwork,
                clocks: Clocks::default(),
                music_listening: Mutex::default(),
            }),
        };
        client.spawn_wake_watch();
        let watcher: Weak<Inner> = Arc::downgrade(&client.inner);
        runtime.spawn(async move {
            while let Some(event) = incoming.recv().await {
                if let Some(inner) = watcher.upgrade() {
                    Self { inner }.observe(&event);
                }
                if outward.send(event).is_err() {
                    return;
                }
            }
        });
        Ok((client, receiver))
    }

    /// The identity and display name used for every server.
    pub fn set_identity(&self, identity: Identity, display_name: String) {
        *self
            .inner
            .credentials
            .write()
            .unwrap_or_else(PoisonError::into_inner) = Some(Credentials {
            identity,
            display_name,
        });
    }

    pub fn identity_info(&self) -> Option<IdentityInfo> {
        self.credentials().map(|credentials| IdentityInfo {
            public_key_hex: hex::encode(credentials.identity.public_key()),
            fingerprint: credentials.identity.fingerprint(),
        })
    }

    pub fn servers(&self) -> Vec<Server> {
        let store = self.lock_store();
        store
            .servers()
            .iter()
            .map(|saved| server_view(saved, store.pin(&saved.key)))
            .collect()
    }

    /// Starts a connection task for every saved server that has none.
    pub fn connect_saved_servers(&self) {
        let Some(credentials) = self.credentials() else {
            return;
        };
        let saved: Vec<SavedServer> = self.lock_store().servers().to_vec();
        let mut connections = self.lock_connections();
        for server in saved {
            if connections.contains_key(&server.key) {
                continue;
            }
            let Ok(address) = ServerAddress::parse(&server.key) else {
                continue;
            };
            let connection = connection::spawn(
                &self.inner.runtime,
                self.context(server.key.clone(), address, credentials.clone()),
                None,
            );
            connections.insert(server.key, connection);
        }
    }

    /// Adds a server from an invite link or a `host:port` address, with an
    /// optional owner claim token. Unknown self-signed certificates need
    /// the user's trust first (see [`AddServerOutcome::NeedsTrust`]).
    /// Adding a saved server again starts a fresh session, for example to
    /// rejoin with a new invite after a kick, or to claim ownership.
    pub async fn add_server(
        &self,
        link_or_address: &str,
        claim_token: Option<String>,
    ) -> Result<AddServerOutcome, CoreError> {
        let credentials = self.credentials().ok_or(CoreError::NoIdentity)?;
        let target = parse_target(link_or_address)?;
        let key = target.address.to_string();
        let pinned = self.lock_store().pin(&key).or(target.fingerprint);
        let mut opened = match connection::open(&target.address, pinned).await {
            Ok(opened) => opened,
            Err(HandshakeError::Untrusted(presented)) => {
                return Ok(AddServerOutcome::NeedsTrust {
                    address: key,
                    fingerprint: format_fingerprint(&presented),
                });
            }
            Err(error) => return Err(error.into()),
        };
        let claim_token = claim_token.filter(|token| !token.trim().is_empty());
        let ready =
            connection::identify(&mut opened, &credentials, target.invite_code, claim_token)
                .await?;

        // The same server at another address stays one entry (plan §7.5).
        let server_id = hex::encode(&opened.hello.server_id);
        let known = self.lock_store().server_with_id(&server_id).cloned();
        let (key, address) = match &known {
            Some(known) if known.key != key => (
                known.key.clone(),
                ServerAddress::parse(&known.key).unwrap_or_else(|_| target.address.clone()),
            ),
            _ => (key, target.address.clone()),
        };
        let earlier = self.lock_store().server(&key).cloned();
        let saved = SavedServer {
            key: key.clone(),
            host: address.host.clone(),
            port: address.port,
            name: ready
                .server
                .as_ref()
                .map(|server| server.name.clone())
                .unwrap_or_default(),
            user_id: ready.self_user.as_ref().map(|user| user.id),
            added_at_ms: earlier
                .as_ref()
                .map_or_else(unix_ms, |known| known.added_at_ms),
            server_id: Some(server_id),
            addresses: earlier
                .as_ref()
                .map(|known| known.addresses.clone())
                .unwrap_or_default(),
            ip: earlier.and_then(|known| known.ip),
        };
        // A pin belongs to the address it was made for: an entry reached
        // again by another address keeps its own.
        let pin = pinned
            .map(|_| opened.presented)
            .filter(|_| address == target.address);
        {
            let mut store = self.lock_store();
            if let Some(pin) = &pin {
                store.set_pin(&key, pin)?;
            }
            store.upsert_server(saved.clone())?;
        }
        let pin = pin.or_else(|| self.lock_store().pin(&key));
        self.inner
            .routes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key.clone(), Route::of(&opened, &target.address));
        let previous = self.lock_connections().remove(&key);
        if let Some(previous) = previous {
            stop(previous).await;
        }
        let connection = connection::spawn(
            &self.inner.runtime,
            self.context(key.clone(), address, credentials),
            Some(Established::identified(opened, ready)),
        );
        self.lock_connections().insert(key, connection);
        Ok(AddServerOutcome::Added(server_view(&saved, pin)))
    }

    /// Pins a certificate fingerprint (hex) for `host:port`.
    pub fn trust_fingerprint(&self, address: &str, fingerprint: &str) -> Result<(), CoreError> {
        let address = parse_address(address)?;
        let fingerprint = parse_fingerprint(fingerprint).map_err(invalid_input)?;
        self.lock_store()
            .set_pin(&address.to_string(), &fingerprint)?;
        Ok(())
    }

    /// Every pinned `(host:port, fingerprint)`.
    pub fn trusted_fingerprints(&self) -> Vec<(String, String)> {
        self.lock_store()
            .pins()
            .into_iter()
            .map(|(key, fingerprint)| (key, format_fingerprint(&fingerprint)))
            .collect()
    }

    /// Disconnects and forgets the server and its pinned certificate.
    pub async fn remove_server(&self, key: &str) -> Result<(), CoreError> {
        let connection = self.lock_connections().remove(key);
        if let Some(connection) = connection {
            stop(connection).await;
        }
        let mut store = self.lock_store();
        if !store.remove_server(key)? {
            return Err(CoreError::UnknownServer);
        }
        store.remove_pin(key)?;
        Ok(())
    }

    pub fn setting(&self, key: &str) -> Option<String> {
        self.lock_store().setting(key).map(str::to_owned)
    }

    pub fn set_setting(&self, key: &str, value: Option<String>) -> Result<(), CoreError> {
        Ok(self.lock_store().set_setting(key, value)?)
    }

    /// Disconnects from every server.
    pub async fn shutdown(&self) {
        let connections: Vec<Connection> =
            self.lock_connections().drain().map(|(_, c)| c).collect();
        for connection in connections {
            stop(connection).await;
        }
    }

    pub async fn send_message(
        &self,
        key: &str,
        channel_id: i64,
        content: String,
        nonce: String,
    ) -> Result<Message, CoreError> {
        let request = Request::SendMessage(proto::SendMessage {
            channel_id,
            content,
            nonce,
            attachment_ids: Vec::new(),
        });
        expect_message(self.request(key, request).await?)
    }

    pub async fn edit_message(
        &self,
        key: &str,
        message_id: i64,
        content: String,
    ) -> Result<Message, CoreError> {
        let request = Request::EditMessage(proto::EditMessage {
            message_id,
            content,
        });
        expect_message(self.request(key, request).await?)
    }

    pub async fn delete_message(&self, key: &str, message_id: i64) -> Result<(), CoreError> {
        let request = Request::DeleteMessage(proto::DeleteMessage { message_id });
        expect_ack(self.request(key, request).await?)
    }

    /// Newest first.
    pub async fn fetch_messages(
        &self,
        key: &str,
        channel_id: i64,
        before: Option<i64>,
        limit: u32,
    ) -> Result<Vec<Message>, CoreError> {
        let request = Request::FetchMessages(proto::FetchMessages {
            channel_id,
            before,
            limit,
        });
        match self.request(key, request).await? {
            Response::Messages(list) => {
                Ok(list.messages.into_iter().map(convert::message).collect())
            }
            other => Err(unexpected(&other)),
        }
    }

    pub async fn start_typing(&self, key: &str, channel_id: i64) -> Result<(), CoreError> {
        let request = Request::StartTyping(proto::StartTyping { channel_id });
        expect_ack(self.request(key, request).await?)
    }

    pub async fn create_channel(
        &self,
        key: &str,
        kind: ChannelKind,
        name: String,
        topic: Option<String>,
        parent_id: Option<i64>,
    ) -> Result<Channel, CoreError> {
        let request = Request::CreateChannel(proto::CreateChannel {
            kind: convert::channel_kind_to_proto(kind) as i32,
            name,
            topic,
            parent_id,
        });
        expect_channel(self.request(key, request).await?)
    }

    pub async fn update_channel(
        &self,
        key: &str,
        channel_id: i64,
        changes: ChannelChanges,
    ) -> Result<Channel, CoreError> {
        let request = Request::UpdateChannel(proto::UpdateChannel {
            channel_id,
            name: changes.name,
            topic: changes.topic,
            parent_id: changes.parent_id,
            bitrate: changes.bitrate,
            user_limit: changes.user_limit,
            text_in_voice: changes.text_in_voice,
        });
        expect_channel(self.request(key, request).await?)
    }

    pub async fn delete_channel(&self, key: &str, channel_id: i64) -> Result<(), CoreError> {
        let request = Request::DeleteChannel(proto::DeleteChannel { channel_id });
        expect_ack(self.request(key, request).await?)
    }

    pub async fn reorder_channels(
        &self,
        key: &str,
        positions: Vec<ChannelPosition>,
    ) -> Result<(), CoreError> {
        let request = Request::ReorderChannels(proto::ReorderChannels {
            positions: positions
                .into_iter()
                .map(|position| proto::ChannelPosition {
                    channel_id: position.channel_id,
                    position: position.position,
                })
                .collect(),
        });
        expect_ack(self.request(key, request).await?)
    }

    pub async fn set_channel_overwrite(
        &self,
        key: &str,
        channel_id: i64,
        overwrite: PermissionOverwrite,
    ) -> Result<Channel, CoreError> {
        let request = Request::SetChannelOverwrite(proto::SetChannelOverwrite {
            channel_id,
            overwrite: Some(proto::PermissionOverwrite {
                target_kind: convert::target_kind_to_proto(overwrite.target_kind) as i32,
                target_id: overwrite.target_id,
                allow: convert::bits_from_api(overwrite.allow),
                deny: convert::bits_from_api(overwrite.deny),
            }),
        });
        expect_channel(self.request(key, request).await?)
    }

    pub async fn delete_channel_overwrite(
        &self,
        key: &str,
        channel_id: i64,
        target_kind: OverwriteTargetKind,
        target_id: i64,
    ) -> Result<Channel, CoreError> {
        let request = Request::DeleteChannelOverwrite(proto::DeleteChannelOverwrite {
            channel_id,
            target_kind: convert::target_kind_to_proto(target_kind) as i32,
            target_id,
        });
        expect_channel(self.request(key, request).await?)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_role(
        &self,
        key: &str,
        name: String,
        color: u32,
        permissions: i64,
        hoist: bool,
        mentionable: bool,
        max_upload_bytes: Option<u64>,
    ) -> Result<Role, CoreError> {
        let request = Request::CreateRole(proto::CreateRole {
            name,
            color,
            permissions: convert::bits_from_api(permissions),
            hoist,
            mentionable,
            max_upload_bytes,
        });
        expect_role(self.request(key, request).await?)
    }

    pub async fn update_role(
        &self,
        key: &str,
        role_id: i64,
        changes: RoleChanges,
    ) -> Result<Role, CoreError> {
        let request = Request::UpdateRole(proto::UpdateRole {
            role_id,
            name: changes.name,
            color: changes.color,
            permissions: changes.permissions.map(convert::bits_from_api),
            hoist: changes.hoist,
            mentionable: changes.mentionable,
            max_upload_bytes: changes.max_upload_bytes,
        });
        expect_role(self.request(key, request).await?)
    }

    pub async fn delete_role(&self, key: &str, role_id: i64) -> Result<(), CoreError> {
        let request = Request::DeleteRole(proto::DeleteRole { role_id });
        expect_ack(self.request(key, request).await?)
    }

    /// The listed roles, lowest first, swap among the positions they hold.
    pub async fn reorder_roles(&self, key: &str, role_ids: Vec<i64>) -> Result<(), CoreError> {
        let request = Request::ReorderRoles(proto::ReorderRoles { role_ids });
        expect_ack(self.request(key, request).await?)
    }

    pub async fn add_member_role(
        &self,
        key: &str,
        user_id: i64,
        role_id: i64,
    ) -> Result<Member, CoreError> {
        let request = Request::AddMemberRole(proto::AddMemberRole { user_id, role_id });
        expect_member(self.request(key, request).await?)
    }

    pub async fn remove_member_role(
        &self,
        key: &str,
        user_id: i64,
        role_id: i64,
    ) -> Result<Member, CoreError> {
        let request = Request::RemoveMemberRole(proto::RemoveMemberRole { user_id, role_id });
        expect_member(self.request(key, request).await?)
    }

    pub async fn kick_member(
        &self,
        key: &str,
        user_id: i64,
        reason: Option<String>,
    ) -> Result<(), CoreError> {
        let request = Request::KickMember(proto::KickMember { user_id, reason });
        expect_ack(self.request(key, request).await?)
    }

    pub async fn ban_member(
        &self,
        key: &str,
        user_id: i64,
        reason: Option<String>,
    ) -> Result<(), CoreError> {
        let request = Request::BanMember(proto::BanMember { user_id, reason });
        expect_ack(self.request(key, request).await?)
    }

    pub async fn unban_member(&self, key: &str, user_id: i64) -> Result<(), CoreError> {
        let request = Request::UnbanMember(proto::UnbanMember { user_id });
        expect_ack(self.request(key, request).await?)
    }

    pub async fn fetch_bans(&self, key: &str) -> Result<Vec<Ban>, CoreError> {
        match self
            .request(key, Request::FetchBans(proto::FetchBans {}))
            .await?
        {
            Response::Bans(list) => Ok(list.bans.into_iter().map(convert::ban).collect()),
            other => Err(unexpected(&other)),
        }
    }

    /// `None` or empty clears the nickname.
    pub async fn update_nickname(
        &self,
        key: &str,
        user_id: i64,
        nickname: Option<String>,
    ) -> Result<Member, CoreError> {
        let request = Request::UpdateNickname(proto::UpdateNickname { user_id, nickname });
        expect_member(self.request(key, request).await?)
    }

    pub async fn update_profile(&self, key: &str, display_name: String) -> Result<User, CoreError> {
        let request = Request::UpdateProfile(proto::UpdateProfile { display_name });
        match self.request(key, request).await? {
            Response::User(user) => Ok(convert::user(user)),
            other => Err(unexpected(&other)),
        }
    }

    pub async fn update_presence(
        &self,
        key: &str,
        status: PresenceStatus,
    ) -> Result<(), CoreError> {
        let request = Request::UpdatePresence(proto::UpdatePresence {
            status: convert::presence_status_to_proto(status) as i32,
        });
        expect_ack(self.request(key, request).await?)
    }

    /// An invite, through `domain_id`'s domain, or the primary one when
    /// there is one.
    pub async fn create_invite(
        &self,
        key: &str,
        max_uses: Option<u32>,
        expires_in_s: Option<u32>,
        domain_id: Option<i64>,
    ) -> Result<Invite, CoreError> {
        let request = Request::CreateInvite(proto::CreateInvite {
            max_uses,
            expires_in_s,
            domain_id,
        });
        match self.request(key, request).await? {
            Response::Invite(invite) => Ok(self.invite_view(key, invite)),
            other => Err(unexpected(&other)),
        }
    }

    pub async fn fetch_invites(&self, key: &str) -> Result<Vec<Invite>, CoreError> {
        match self
            .request(key, Request::FetchInvites(proto::FetchInvites {}))
            .await?
        {
            Response::Invites(list) => Ok(list
                .invites
                .into_iter()
                .map(|invite| self.invite_view(key, invite))
                .collect()),
            other => Err(unexpected(&other)),
        }
    }

    pub async fn revoke_invite(&self, key: &str, code: String) -> Result<(), CoreError> {
        let request = Request::RevokeInvite(proto::RevokeInvite { code });
        expect_ack(self.request(key, request).await?)
    }

    pub async fn update_server(
        &self,
        key: &str,
        changes: ServerChanges,
    ) -> Result<ServerInfo, CoreError> {
        let request = Request::UpdateServer(proto::UpdateServer {
            name: changes.name,
            description: changes.description,
            open_join: changes.open_join,
        });
        match self.request(key, request).await? {
            Response::Server(info) => Ok(convert::server_info(info)),
            other => Err(unexpected(&other)),
        }
    }

    /// Tries a server again now: skips the wait before the next attempt,
    /// or starts over after a failure.
    pub async fn retry_now(&self, key: &str) -> Result<(), CoreError> {
        let commands = self
            .lock_connections()
            .get(key)
            .map(|connection| connection.commands.clone());
        let Some(commands) = commands else {
            return Err(if self.lock_store().server(key).is_some() {
                CoreError::NotConnected
            } else {
                CoreError::UnknownServer
            });
        };
        commands
            .send(Command::RetryNow)
            .await
            .map_err(|_| CoreError::NotConnected)
    }

    /// Where to connect for voice, each time a server says so: after a
    /// join, a move or a voice node failing over.
    pub fn voice_servers(&self) -> broadcast::Receiver<VoiceServer> {
        self.inner.voice_servers.subscribe()
    }

    /// Runs voice media for this device: connects to voice nodes and plays
    /// and captures audio. Without it, joining voice changes only the
    /// voice state, as for the voicebot, which brings its own media. The
    /// first call decides; later calls return `None`.
    pub fn enable_media(
        &self,
        options: MediaOptions,
    ) -> Option<mpsc::UnboundedReceiver<MediaEvent>> {
        let (events, receiver) = mpsc::unbounded_channel();
        let media = Media::new(self.inner.runtime.clone(), options, events);
        media.set_music_listening(
            *self
                .inner
                .music_listening
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        );
        self.inner.media.set(media).ok()?;
        let mut servers = self.inner.voice_servers.subscribe();
        let watcher = Arc::downgrade(&self.inner);
        self.inner.runtime.spawn(async move {
            loop {
                match servers.recv().await {
                    Ok(server) => {
                        let Some(inner) = watcher.upgrade() else {
                            return;
                        };
                        let client = Self { inner };
                        if let Some(media) = client.inner.media.get() {
                            media.on_voice_server(&client, server);
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
        });
        self.sync_media();
        self.spawn_idle_reports();
        Some(receiver)
    }

    /// Notices the computer waking up: the wall clock runs on while it
    /// sleeps, but a timer here does not. Then every connection is checked
    /// at once instead of at its next heartbeat, so the call comes back in
    /// seconds (Phase 2 §12).
    fn spawn_wake_watch(&self) {
        let watcher = Arc::downgrade(&self.inner);
        self.inner.runtime.spawn(async move {
            let mut before = SystemTime::now();
            loop {
                tokio::time::sleep(WAKE_CHECK_EVERY).await;
                let now = SystemTime::now();
                let woke = slept(before, now, WAKE_CHECK_EVERY);
                before = now;
                let Some(inner) = watcher.upgrade() else {
                    return;
                };
                if woke {
                    tracing::info!("the computer woke up; checking the connections");
                    Self { inner }.check_connections().await;
                }
            }
        });
    }

    /// Checks every server connection now: a heartbeat answered within a
    /// few seconds, or a reconnect.
    pub async fn check_connections(&self) {
        let commands: Vec<mpsc::Sender<Command>> = self
            .lock_connections()
            .values()
            .map(|connection| connection.commands.clone())
            .collect();
        for commands in commands {
            let _ = commands.send(Command::Probe).await;
        }
    }

    /// While in voice, tells the server when this device has been idle
    /// past [`opencord_media::idle::THRESHOLD`] and when it is in use
    /// again, so it can move the user to the AFK channel (§12).
    fn spawn_idle_reports(&self) {
        let watcher = Arc::downgrade(&self.inner);
        self.inner.runtime.spawn(async move {
            let mut every = tokio::time::interval(IDLE_CHECK_EVERY);
            let mut sent: Option<IdleReport> = None;
            loop {
                every.tick().await;
                let Some(inner) = watcher.upgrade() else {
                    return;
                };
                let client = Self { inner };
                let target = client.lock_voice().target.clone();
                let Some(target) = target else {
                    sent = None;
                    continue;
                };
                let Some(idle) = opencord_media::idle::idle_time().await else {
                    continue;
                };
                let Some(input_idle_ms) = idle_report(sent.as_ref(), &target, idle) else {
                    continue;
                };
                let request =
                    Request::UpdateVoiceActivity(proto::UpdateVoiceActivity { input_idle_ms });
                if client.request(&target.0, request).await.is_ok() {
                    sent = Some(IdleReport {
                        target,
                        away: input_idle_ms.is_some(),
                        at: std::time::Instant::now(),
                    });
                }
            }
        });
    }

    /// Audio devices, volumes and input mode, for voice media.
    pub fn apply_audio_settings(&self, settings: AudioSettings) {
        if let Some(media) = self.inner.media.get() {
            media.apply_settings(settings);
        }
    }

    /// Push-to-talk's key went down or up.
    pub fn set_push_to_talk(&self, held: bool) {
        if let Some(media) = self.inner.media.get() {
            media.set_push_to_talk(held);
        }
    }

    /// Binds global hotkeys (plan §7.13), replacing the ones bound before;
    /// none unbinds them. Push-to-talk and the priority key act on voice
    /// media directly; the toggles come back as
    /// `MediaEvent::HotkeyPressed`. Says whether they work while Opencord
    /// is in the background.
    pub async fn set_hotkeys(
        &self,
        bindings: &[HotkeyBinding],
    ) -> Result<HotkeySupport, CoreError> {
        let bindings = media::hotkey_bindings(bindings)?;
        let mut hotkeys = self.inner.hotkeys.lock().await;
        let hotkeys = hotkeys.get_or_insert_with(|| {
            let (events, mut received) = mpsc::unbounded_channel();
            let watcher = Arc::downgrade(&self.inner);
            self.inner.runtime.spawn(async move {
                while let Some(event) = received.recv().await {
                    let Some(inner) = watcher.upgrade() else {
                        return;
                    };
                    if let Some(media) = inner.media.get() {
                        media.on_hotkey(event);
                    }
                }
            });
            Hotkeys::new(APP_ID, events)
        });
        Ok(media::hotkey_support_from(hotkeys.set(&bindings).await))
    }

    /// The priority speaker key went down or up.
    pub fn set_priority_speaker(&self, held: bool) {
        if let Some(media) = self.inner.media.get() {
            media.set_priority_speaker(held);
        }
    }

    /// Report the microphone's level while a meter shows it.
    pub fn set_level_meter(&self, on: bool) {
        if let Some(media) = self.inner.media.get() {
            media.set_level_meter(on);
        }
    }

    /// Hear what would be sent, Opus and all.
    pub fn set_mic_test(&self, on: bool) {
        if let Some(media) = self.inner.media.get() {
            media.set_mic_test(on);
        }
    }

    /// How loud `user_id` on `key` sounds to this device, 0–2.
    pub fn set_user_volume(&self, key: &str, user_id: i64, volume: f32) {
        if let Some(media) = self.inner.media.get() {
            media.set_user_volume(key, user_id, volume);
        }
    }

    /// Silences `user_id` on `key` for this device only.
    pub fn set_user_local_mute(&self, key: &str, user_id: i64, muted: bool) {
        if let Some(media) = self.inner.media.get() {
            media.set_user_local_mute(key, user_id, muted);
        }
    }

    /// Asks for a fresh voice server update, after a voice connection
    /// failed for good.
    pub async fn refresh_voice_server(&self, key: &str) {
        let request = Request::RefreshVoiceServer(proto::RefreshVoiceServer {});
        // Not in voice there any more, or not connected: news of that
        // reaches voice media through the voice state.
        let _ = self.request(key, request).await;
    }

    /// Tells voice media where this device's voice is now.
    fn sync_media(&self) {
        let Some(media) = self.inner.media.get() else {
            return;
        };
        let target = self.lock_voice().media_target();
        media.on_target(self, target);
    }

    /// Joins a voice channel, leaving any other one first, on any server.
    pub async fn voice_join(&self, key: &str, channel_id: i64) -> Result<VoiceState, CoreError> {
        let elsewhere = {
            let voice = self.lock_voice();
            voice
                .target
                .as_ref()
                .filter(|(target_key, _)| target_key != key)
                .map(|(target_key, _)| target_key.clone())
        };
        if let Some(other) = elsewhere {
            let _ = self.request(&other, leave_voice()).await;
            {
                let mut voice = self.lock_voice();
                if voice
                    .target
                    .as_ref()
                    .is_some_and(|(target, _)| *target == other)
                {
                    voice.target = None;
                    voice.video = false;
                }
            }
            self.sync_media();
        }
        let request = self.voice_update(channel_id);
        let state = expect_voice_state(self.request(key, request).await?)?;
        self.lock_voice().target = Some((key.to_owned(), channel_id));
        self.sync_media();
        Ok(convert::voice_state(state.clone(), &state.session_id))
    }

    /// Leaves voice, wherever this device is.
    pub async fn voice_leave(&self) -> Result<(), CoreError> {
        // Leaving stops the camera (the media session does), so the next
        // join must not say it's on.
        let target = {
            let mut voice = self.lock_voice();
            voice.video = false;
            voice.target.take()
        };
        self.sync_media();
        match target {
            Some((key, _)) => self.request(&key, leave_voice()).await.map(|_| ()),
            None => Ok(()),
        }
    }

    /// Self mute and deafen. They hold outside voice too, for the next
    /// join; in voice, the server hears about them in the background.
    pub fn set_voice_self(&self, mute: Option<bool>, deaf: Option<bool>) {
        let (changed, target) = {
            let mut voice = self.lock_voice();
            let before = (voice.mute, voice.deaf);
            voice.mute = mute.unwrap_or(voice.mute);
            voice.deaf = deaf.unwrap_or(voice.deaf);
            (before != (voice.mute, voice.deaf), voice.target.clone())
        };
        self.sync_media();
        // Unchanged, nothing to tell: each update counts against the
        // server's voice state limit (10 in 10 s).
        if !changed {
            return;
        }
        if let Some((key, channel_id)) = target {
            let client = self.clone();
            self.inner.runtime.spawn(async move {
                let request = client.voice_update(channel_id);
                let _ = client.request(&key, request).await;
            });
        }
    }

    /// Tells the server whether this device's camera is on.
    pub fn set_voice_video(&self, on: bool) {
        let target = {
            let mut voice = self.lock_voice();
            if voice.video == on {
                return;
            }
            voice.video = on;
            voice.target.clone()
        };
        if let Some((key, channel_id)) = target {
            let client = self.clone();
            self.inner.runtime.spawn(async move {
                let request = client.voice_update(channel_id);
                let _ = client.request(&key, request).await;
            });
        }
    }

    /// Video draws into this Flutter engine's textures from now on.
    pub fn video_init(&self, engine_handle: i64) {
        #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
        if let Some(media) = self.inner.media.get() {
            media
                .video()
                .set_textures(Arc::new(crate::video::flutter::FlutterTextures::new(
                    engine_handle,
                )));
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        let _ = engine_handle;
    }

    /// Turns this device's camera on in its voice channel.
    pub async fn camera_start(
        &self,
        device_id: Option<String>,
    ) -> Result<CameraStarted, CoreError> {
        let media = self.inner.media.get().ok_or(CoreError::NotInitialized)?;
        let started = media.video().start_camera(device_id).await?;
        self.set_voice_video(true);
        Ok(started)
    }

    pub fn camera_stop(&self) {
        if let Some(media) = self.inner.media.get() {
            media.video().stop_camera();
        }
        self.set_voice_video(false);
    }

    /// Goes live: picks the source (the system's picker, or `source_id`
    /// from Opencord's own), creates the stream on the server, then
    /// publishes and sends the screen. Live already, it changes the source
    /// and keeps the stream and its viewers.
    pub async fn screen_share_start(
        &self,
        request: ScreenShareRequest,
        source_id: Option<String>,
    ) -> Result<ScreenShareStarted, CoreError> {
        use crate::video::screen;

        let media = self.inner.media.get().ok_or(CoreError::NotInitialized)?;
        let not_in_voice =
            || screen::screen_error(ScreenProblem::NotInVoice, "not in a voice channel");
        let (server_key, channel_id) = self.lock_voice().target.clone().ok_or_else(not_in_voice)?;
        let token = self.lock_voice().screen_restore_token.clone();
        let pending = screen::pick(screen::shape(&request), token, source_id).await?;
        if pending.restore_token.is_some() {
            self.lock_voice().screen_restore_token = pending.restore_token.clone();
        }
        let created = self
            .create_stream(&server_key, channel_id, pending.kind, request)
            .await;
        let stream_key = match created {
            Ok(stream) => stream.stream_key,
            Err(error) => {
                drop_pending(pending);
                return Err(error);
            }
        };
        let started = media
            .video()
            .publish_screen(pending, server_key.clone(), stream_key.clone(), request)
            .await;
        if started.is_err() {
            self.delete_stream(&server_key, &stream_key).await;
        }
        started
    }

    /// A new quality: the server checks it against its maximum first.
    pub async fn screen_share_update(&self, request: ScreenShareRequest) -> Result<(), CoreError> {
        let media = self.inner.media.get().ok_or(CoreError::NotInitialized)?;
        let Some((server_key, stream_key)) = media.video().screen_share() else {
            return Err(CoreError::InvalidInput {
                message: "not sharing a screen".to_owned(),
            });
        };
        let update = Request::UpdateStream(proto::UpdateStream {
            stream_key,
            resolution: Some(convert::screen_share_resolution_to_proto(request.resolution) as i32),
            fps: Some(request.fps),
            has_audio: Some(request.has_audio),
        });
        match self.request(&server_key, update).await? {
            Response::Stream(_) => {}
            other => return Err(unexpected(&other)),
        }
        media.video().reshape_screen(request).await
    }

    pub async fn screen_share_stop(&self) -> Result<(), CoreError> {
        let media = self.inner.media.get().ok_or(CoreError::NotInitialized)?;
        if let Some((server_key, stream_key)) = media.video().stop_screen() {
            self.delete_stream(&server_key, &stream_key).await;
        }
        Ok(())
    }

    /// Goes live on the server (a stream in `channel_id`, where this user
    /// is in voice); the media is the caller's.
    pub async fn create_stream(
        &self,
        server_key: &str,
        channel_id: i64,
        kind: StreamSourceKind,
        request: ScreenShareRequest,
    ) -> Result<ScreenStream, CoreError> {
        let create = Request::CreateStream(proto::CreateStream {
            channel_id,
            source_kind: convert::stream_source_kind_to_proto(kind) as i32,
            resolution: convert::screen_share_resolution_to_proto(request.resolution) as i32,
            fps: request.fps,
            has_audio: request.has_audio,
        });
        match self.request(server_key, create).await? {
            Response::Stream(stream) => Ok(convert::stream(stream)),
            other => Err(unexpected(&other)),
        }
    }

    /// Ends a stream on the server; one already gone is fine.
    pub async fn delete_stream(&self, server_key: &str, stream_key: &str) {
        let request = Request::DeleteStream(proto::DeleteStream {
            stream_key: stream_key.to_owned(),
        });
        if let Err(error) = self.request(server_key, request).await {
            tracing::debug!(%error, "the stream could not be ended on the server");
        }
    }

    /// Starts or stops watching a stream in this device's voice channel.
    pub async fn stream_watch(&self, stream_key: &str, watching: bool) -> Result<(), CoreError> {
        let Some((server_key, _)) = self.lock_voice().target.clone() else {
            return Err(CoreError::Server {
                code: ErrorCode::VoiceNotConnected,
                message: "not in a voice channel".to_owned(),
                retry_after_ms: None,
            });
        };
        let key = stream_key.to_owned();
        let request = match watching {
            true => Request::WatchStream(proto::WatchStream { stream_key: key }),
            false => Request::UnwatchStream(proto::UnwatchStream { stream_key: key }),
        };
        match self.request(&server_key, request).await? {
            Response::Ack(_) => {}
            other => return Err(unexpected(&other)),
        }
        // `stream:<channel>:<user>`: no longer watched, its sound stops.
        let streamer = stream_key
            .rsplit(':')
            .next()
            .and_then(|user| user.parse::<i64>().ok());
        if let (false, Some(user_id), Some(media)) = (watching, streamer, self.inner.media.get()) {
            media.stop_stream_audio(user_id);
        }
        Ok(())
    }

    /// How loud someone's screen share sound is on this device, apart from
    /// their voice (plan §10.5).
    pub fn set_stream_volume(&self, server_key: &str, user_id: i64, volume: f32) {
        if let Some(media) = self.inner.media.get() {
            media.set_stream_volume(server_key, user_id, volume.clamp(0.0, 2.0));
        }
    }

    pub fn set_stream_mute(&self, server_key: &str, user_id: i64, muted: bool) {
        if let Some(media) = self.inner.media.get() {
            media.set_stream_mute(server_key, user_id, muted);
        }
    }

    /// Whose sound this device's window share sends: an app by name (from
    /// [`crate::api::client::screen_audio_apps`]), or none.
    pub fn screen_share_audio_from(&self, app: Option<String>) -> Result<(), CoreError> {
        let media = self.inner.media.get().ok_or(CoreError::NotInitialized)?;
        media.video().share_audio_from(app)
    }

    /// The tiles the app shows video in, and their sizes.
    pub fn video_set_wants(&self, wants: Vec<VideoWant>) {
        if let Some(media) = self.inner.media.get() {
            media.video().set_wants(wants);
        }
    }

    pub async fn server_mute(&self, key: &str, user_id: i64, value: bool) -> Result<(), CoreError> {
        let request = Request::ServerMuteMember(proto::ServerMuteMember { user_id, value });
        expect_voice_state(self.request(key, request).await?).map(|_| ())
    }

    pub async fn server_deafen(
        &self,
        key: &str,
        user_id: i64,
        value: bool,
    ) -> Result<(), CoreError> {
        let request = Request::ServerDeafenMember(proto::ServerDeafenMember { user_id, value });
        expect_voice_state(self.request(key, request).await?).map(|_| ())
    }

    pub async fn move_member(
        &self,
        key: &str,
        user_id: i64,
        channel_id: i64,
    ) -> Result<(), CoreError> {
        let request = Request::MoveMember(proto::MoveMember {
            user_id,
            channel_id,
        });
        expect_voice_state(self.request(key, request).await?).map(|_| ())
    }

    pub async fn disconnect_member(&self, key: &str, user_id: i64) -> Result<(), CoreError> {
        let request = Request::DisconnectMember(proto::DisconnectMember { user_id });
        expect_ack(self.request(key, request).await?)
    }

    /// Plays a sound in `channel_id` for everyone there (plan §11.5). For a
    /// sound from another server, `external` is that server's key and the
    /// sound.
    pub async fn soundboard_play(
        &self,
        key: &str,
        channel_id: i64,
        sound_ref: &str,
        external: Option<(String, SoundboardSound)>,
    ) -> Result<(), CoreError> {
        self.inner
            .soundboard
            .play(self, key, channel_id, sound_ref, external)
            .await
    }

    /// A sound as the mixer would play it: mono samples at 48 kHz, from the
    /// cache or fetched, and its volume in percent. For headless clients
    /// such as the voicebot, which bring their own media.
    pub async fn soundboard_samples(
        &self,
        key: &str,
        sound_ref: &str,
    ) -> Result<(Arc<[f32]>, u32), CoreError> {
        self.inner.soundboard.samples(self, key, sound_ref).await
    }

    /// Plays a sound on this device only.
    pub async fn soundboard_preview(&self, key: &str, sound_ref: &str) -> Result<(), CoreError> {
        self.inner.soundboard.preview(self, key, sound_ref).await
    }

    /// Plays a voice event's sound here only, at `volume` (0–1): in the
    /// call when there is one, else through an engine started for it.
    pub fn play_cue(&self, cue: VoiceCue, volume: f32) {
        let Some(media) = self.media() else {
            return;
        };
        match cue_samples(cue) {
            Ok(samples) => media.preview(samples, volume.clamp(0.0, 1.0)),
            Err(error) => tracing::warn!(%error, ?cue, "a voice cue could not be decoded"),
        }
    }

    /// The soundboard's volume (0–1) and "Mute all soundboards".
    pub fn soundboard_set_listening(&self, volume: f32, muted: bool) {
        self.inner.soundboard.set_listening(volume, muted);
    }

    pub fn soundboard_mute_player(&self, key: &str, user_id: i64, muted: bool) {
        self.inner.soundboard.set_player_muted(key, user_id, muted);
    }

    pub async fn update_soundboard_sound(
        &self,
        key: &str,
        sound_id: i64,
        changes: SoundboardSoundChanges,
    ) -> Result<SoundboardSound, CoreError> {
        let request = Request::UpdateSoundboardSound(proto::UpdateSoundboardSound {
            sound_id,
            name: changes.name,
            emoji: changes.emoji,
            volume: changes.volume,
        });
        match self.request(key, request).await? {
            Response::SoundboardSound(sound) => Ok(convert::soundboard_sound(sound)),
            other => Err(unexpected(&other)),
        }
    }

    pub async fn delete_soundboard_sound(&self, key: &str, sound_id: i64) -> Result<(), CoreError> {
        let request = Request::DeleteSoundboardSound(proto::DeleteSoundboardSound { sound_id });
        expect_ack(self.request(key, request).await?)
    }

    /// Opens a file for the trim editor (plan §11.3).
    pub async fn soundboard_open_draft(&self, path: String) -> Result<SoundDraft, CoreError> {
        self.inner.soundboard.open_draft(path).await
    }

    pub async fn soundboard_preview_draft(
        &self,
        selection: SoundSelection,
    ) -> Result<(), CoreError> {
        self.inner.soundboard.preview_draft(self, selection).await
    }

    pub async fn soundboard_upload(
        &self,
        key: &str,
        selection: SoundSelection,
        name: String,
        emoji: Option<String>,
    ) -> Result<SoundboardSound, CoreError> {
        self.inner
            .soundboard
            .upload(self, key, selection, name, emoji)
            .await
    }

    pub fn soundboard_close_draft(&self, draft_id: u64) {
        self.inner.soundboard.close_draft(draft_id);
    }

    pub async fn update_voice_settings(
        &self,
        key: &str,
        changes: VoiceSettingsChanges,
    ) -> Result<VoiceSettings, CoreError> {
        let request = Request::UpdateVoiceSettings(convert::voice_settings_changes(changes));
        match self.request(key, request).await? {
            Response::VoiceSettings(settings) => Ok(convert::voice_settings(settings)),
            other => Err(unexpected(&other)),
        }
    }

    /// Follows this device's voice state through the events: moves and
    /// disconnects by moderators, losing the channel, another device taking
    /// over, and getting back in after a fresh session.
    fn observe(&self, event: &CoreEvent) {
        self.inner.soundboard.observe(self, event);
        let key = &event.server_key;
        match &event.payload {
            CoreEventPayload::Ready(snapshot) => {
                let self_id = snapshot.self_user.id;
                let rejoin = {
                    let mut voice = self.lock_voice();
                    voice.self_ids.insert(key.clone(), self_id);
                    voice
                        .target
                        .as_ref()
                        .filter(|(target_key, _)| target_key == key)
                        .map(|(_, channel_id)| *channel_id)
                        .filter(|channel_id| {
                            !snapshot.voice_states.iter().any(|state| {
                                state.user_id == self_id
                                    && state.this_device
                                    && state.channel_id == Some(*channel_id)
                            })
                        })
                };
                if let Some(channel_id) = rejoin {
                    let client = self.clone();
                    let key = key.clone();
                    self.inner
                        .runtime
                        .spawn(async move { client.rejoin(&key, channel_id).await });
                }
            }
            CoreEventPayload::VoiceStateUpdate(state) => {
                {
                    let mut voice = self.lock_voice();
                    if voice.self_ids.get(key) != Some(&state.user_id) {
                        return;
                    }
                    let here = voice
                        .target
                        .as_ref()
                        .is_some_and(|(target_key, _)| target_key == key);
                    if state.this_device {
                        match state.channel_id {
                            Some(channel_id) => {
                                voice.target = Some((key.clone(), channel_id));
                                voice.server_mute = state.server_mute;
                                voice.server_deaf = state.server_deaf;
                                voice.suppress = state.suppress;
                            }
                            None if here => {
                                voice.target = None;
                                voice.video = false;
                            }
                            None => {}
                        }
                    } else if here {
                        voice.target = None;
                        voice.video = false;
                    }
                }
                self.sync_media();
            }
            CoreEventPayload::StreamDelete { stream_key, .. } => {
                if let Some(media) = self.inner.media.get() {
                    media.video().screen_ended_by_server(stream_key);
                }
            }
            _ => {}
        }
    }

    /// Gets back into `channel_id` after a fresh session. If the server says
    /// no, this device has left, and the app hears so. Told to slow down, it
    /// tries again once the server's wait is over, while it still wants that
    /// channel.
    async fn rejoin(&self, key: &str, channel_id: i64) {
        let mut tries = 0;
        let code = loop {
            let request = self.voice_update(channel_id);
            let Err(CoreError::Server {
                code,
                retry_after_ms,
                ..
            }) = self.request(key, request).await
            else {
                return;
            };
            tries += 1;
            if code != ErrorCode::RateLimited || tries == REJOIN_TRIES {
                break code;
            }
            let wait = retry_after_ms.map_or(REJOIN_WAIT, |ms| {
                Duration::from_millis(u64::from(ms)).max(REJOIN_WAIT)
            });
            tokio::time::sleep(wait).await;
            if self.lock_voice().target != Some((key.to_owned(), channel_id)) {
                return;
            }
        };
        if matches!(code, ErrorCode::RateLimited | ErrorCode::Internal) {
            return;
        }
        let self_id = {
            let mut voice = self.lock_voice();
            if voice.target != Some((key.to_owned(), channel_id)) {
                return;
            }
            voice.target = None;
            voice.video = false;
            voice.self_ids.get(key).copied()
        };
        self.sync_media();
        if let Some(user_id) = self_id {
            let _ = self.inner.outward.send(CoreEvent {
                server_key: key.to_owned(),
                payload: CoreEventPayload::VoiceStateUpdate(VoiceState {
                    user_id,
                    channel_id: None,
                    this_device: true,
                    self_mute: false,
                    self_deaf: false,
                    server_mute: false,
                    server_deaf: false,
                    suppress: false,
                    self_video: false,
                    self_stream: false,
                }),
            });
        }
    }

    fn voice_update(&self, channel_id: i64) -> Request {
        let voice = self.lock_voice();
        Request::UpdateVoiceState(proto::UpdateVoiceState {
            channel_id: Some(channel_id),
            self_mute: voice.mute,
            self_deaf: voice.deaf,
            self_video: voice.video,
            self_stream: false,
        })
    }

    fn lock_voice(&self) -> MutexGuard<'_, Voice> {
        self.inner
            .voice
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) async fn request(&self, key: &str, kind: Request) -> Result<Response, CoreError> {
        let commands = self
            .lock_connections()
            .get(key)
            .map(|connection| connection.commands.clone());
        let Some(commands) = commands else {
            return Err(if self.lock_store().server(key).is_some() {
                CoreError::NotConnected
            } else {
                CoreError::UnknownServer
            });
        };
        let (reply, response) = oneshot::channel();
        commands
            .send(Command::Request { kind, reply })
            .await
            .map_err(|_| CoreError::NotConnected)?;
        match response.await.map_err(|_| CoreError::NotConnected)?? {
            Response::Error(error) => Err(convert::error(error)),
            response => Ok(response),
        }
    }

    /// An invite's links: through its domain when it has one (no
    /// fingerprint when an authority vouches for the domain), else through
    /// the address the server was added with (plan §7.6).
    fn invite_view(&self, key: &str, invite: proto::Invite) -> Invite {
        let pin = self.lock_store().pin(key);
        let (link, web_link) = match &invite.address {
            Some(domain) => {
                let port = u16::try_from(domain.port).unwrap_or(DEFAULT_PORT);
                let link = InviteLink {
                    address: ServerAddress {
                        host: domain.host.clone(),
                        port,
                    },
                    code: invite.code.clone(),
                    fingerprint: pin.filter(|_| !domain.trusted_certificate),
                };
                let web = match port {
                    443 => format!("https://{}/invite/{}", domain.host, invite.code),
                    port => format!("https://{}:{port}/invite/{}", domain.host, invite.code),
                };
                (link.to_string(), Some(web))
            }
            None => {
                let link = ServerAddress::parse(key)
                    .map(|address| {
                        InviteLink {
                            address,
                            code: invite.code.clone(),
                            fingerprint: pin,
                        }
                        .to_string()
                    })
                    .unwrap_or_default();
                (link, None)
            }
        };
        convert::invite(invite, link, web_link)
    }

    fn context(&self, key: String, address: ServerAddress, credentials: Credentials) -> Context {
        Context {
            key,
            address,
            credentials,
            store: Arc::clone(&self.inner.store),
            events: self.inner.events.clone(),
            voice_servers: self.inner.voice_servers.clone(),
            media_tokens: Arc::clone(&self.inner.media_tokens),
            routes: Arc::clone(&self.inner.routes),
            clocks: Arc::clone(&self.inner.clocks),
        }
    }

    /// This person's music volume (0–2), mute and "Lower music while people
    /// talk", for every server (Phase 3 plan §19.8). Kept on this device.
    pub fn music_set_listening(&self, volume: f32, muted: bool, duck: bool) {
        let listening = MusicListening::clamped(volume, muted, duck);
        *self
            .inner
            .music_listening
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = listening;
        if let Some(media) = self.media() {
            media.set_music_listening(listening);
        }
    }

    /// The server's clock minus this device's, in ms; 0 until known.
    pub(crate) fn server_clock_offset(&self, key: &str) -> i64 {
        self.inner
            .clocks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(key)
            .map_or(0, crate::server_clock::ServerClock::offset_ms)
    }

    fn credentials(&self) -> Option<Credentials> {
        self.inner
            .credentials
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn lock_store(&self) -> MutexGuard<'_, Store> {
        self.inner
            .store
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    fn lock_connections(&self) -> MutexGuard<'_, HashMap<String, Connection>> {
        self.inner
            .connections
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

struct Target {
    address: ServerAddress,
    invite_code: Option<String>,
    fingerprint: Option<Fingerprint>,
}

fn parse_target(input: &str) -> Result<Target, CoreError> {
    let input = input.trim();
    if input.starts_with(INVITE_SCHEME) {
        let link = InviteLink::parse(input).map_err(invalid_input)?;
        Ok(Target {
            address: link.address,
            invite_code: Some(link.code),
            fingerprint: link.fingerprint,
        })
    } else {
        Ok(Target {
            address: parse_address(input)?,
            invite_code: None,
            fingerprint: None,
        })
    }
}

fn parse_address(input: &str) -> Result<ServerAddress, CoreError> {
    ServerAddress::parse(input).map_err(invalid_input)
}

fn invalid_input(error: impl std::fmt::Display) -> CoreError {
    CoreError::InvalidInput {
        message: error.to_string(),
    }
}

fn server_view(saved: &SavedServer, pin: Option<Fingerprint>) -> Server {
    Server {
        key: saved.key.clone(),
        name: saved.name.clone(),
        host: saved.host.clone(),
        port: saved.port,
        user_id: saved.user_id,
        fingerprint: pin.as_ref().map(format_fingerprint),
        addresses: saved
            .addresses
            .iter()
            .map(|address| {
                ServerAddress {
                    host: address.host.clone(),
                    port: address.port,
                }
                .to_string()
            })
            .collect(),
    }
}

async fn stop(connection: Connection) {
    let _ = connection.commands.send(Command::Shutdown).await;
    let _ = connection.task.await;
}

fn leave_voice() -> Request {
    Request::UpdateVoiceState(proto::UpdateVoiceState::default())
}

fn expect_voice_state(response: Response) -> Result<proto::VoiceState, CoreError> {
    match response {
        Response::VoiceState(state) => Ok(state),
        other => Err(unexpected(&other)),
    }
}

impl Client {
    /// Voice media, once the app turned it on.
    pub(crate) fn media(&self) -> Option<&Media> {
        self.inner.media.get()
    }

    pub(crate) fn soundboard(&self) -> Arc<Soundboard> {
        Arc::clone(&self.inner.soundboard)
    }

    pub(crate) fn artwork(&self) -> Arc<ArtworkCache> {
        Arc::clone(&self.inner.artwork)
    }

    /// Owner or MANAGE_SERVER. `None` leaves a setting as it is.
    pub async fn update_upload_settings(
        &self,
        key: &str,
        blocked_extensions: Option<Vec<String>>,
        voice_message_max_seconds: Option<u32>,
    ) -> Result<UploadSettings, CoreError> {
        let request = Request::UpdateUploadSettings(proto::UpdateUploadSettings {
            blocked_extensions: blocked_extensions
                .map(|extensions| proto::BlockedExtensions { extensions }),
            voice_message_max_seconds,
        });
        match self.request(key, request).await? {
            Response::UploadSettings(settings) => Ok(convert::upload_settings(settings)),
            other => Err(unexpected(&other)),
        }
    }

    pub async fn storage_get(&self, key: &str) -> Result<StorageInfo, CoreError> {
        expect_storage(
            self.request(key, Request::GetStorage(proto::GetStorage {}))
                .await?,
        )
    }

    pub async fn storage_add(
        &self,
        key: &str,
        name: String,
        s3: S3Config,
    ) -> Result<StorageInfo, CoreError> {
        let request = Request::AddStorage(proto::AddStorage {
            name,
            s3: Some(convert::s3_config_to_proto(s3)),
        });
        expect_storage(self.request(key, request).await?)
    }

    pub async fn storage_update(
        &self,
        key: &str,
        storage_id: i64,
        name: Option<String>,
        s3: Option<S3Config>,
    ) -> Result<StorageInfo, CoreError> {
        let request = Request::UpdateStorage(proto::UpdateStorage {
            storage_id,
            name,
            s3: s3.map(convert::s3_config_to_proto),
        });
        expect_storage(self.request(key, request).await?)
    }

    pub async fn storage_test(
        &self,
        key: &str,
        target: StorageTestTarget,
    ) -> Result<StorageTestResult, CoreError> {
        let target = match target {
            StorageTestTarget::Existing { storage_id } => {
                proto::test_storage::Target::StorageId(storage_id)
            }
            StorageTestTarget::Config { s3 } => {
                proto::test_storage::Target::S3(convert::s3_config_to_proto(s3))
            }
        };
        let request = Request::TestStorage(proto::TestStorage {
            target: Some(target),
        });
        match self.request(key, request).await? {
            Response::StorageTest(test) => Ok(convert::storage_test(test)),
            other => Err(unexpected(&other)),
        }
    }

    pub async fn storage_activate(
        &self,
        key: &str,
        storage_id: i64,
    ) -> Result<StorageInfo, CoreError> {
        let request = Request::ActivateStorage(proto::ActivateStorage { storage_id });
        expect_storage(self.request(key, request).await?)
    }

    pub async fn storage_remove(
        &self,
        key: &str,
        storage_id: i64,
    ) -> Result<StorageInfo, CoreError> {
        let request = Request::RemoveStorage(proto::RemoveStorage { storage_id });
        expect_storage(self.request(key, request).await?)
    }

    pub async fn storage_move_start(&self, key: &str) -> Result<StorageInfo, CoreError> {
        let request = Request::StartStorageMove(proto::StartStorageMove {});
        expect_storage(self.request(key, request).await?)
    }

    pub async fn storage_move_stop(&self, key: &str) -> Result<StorageInfo, CoreError> {
        let request = Request::StopStorageMove(proto::StopStorageMove {});
        expect_storage(self.request(key, request).await?)
    }

    /// A request answered with an ack.
    pub(crate) async fn ack(&self, key: &str, kind: Request) -> Result<(), CoreError> {
        expect_ack(self.request(key, kind).await?)
    }
}

fn expect_ack(response: Response) -> Result<(), CoreError> {
    match response {
        Response::Ack(_) => Ok(()),
        other => Err(unexpected(&other)),
    }
}

fn expect_message(response: Response) -> Result<Message, CoreError> {
    match response {
        Response::Message(message) => Ok(convert::message(message)),
        other => Err(unexpected(&other)),
    }
}

fn expect_channel(response: Response) -> Result<Channel, CoreError> {
    match response {
        Response::Channel(channel) => Ok(convert::channel(channel)),
        other => Err(unexpected(&other)),
    }
}

fn expect_storage(response: Response) -> Result<StorageInfo, CoreError> {
    match response {
        Response::Storage(storage) => Ok(convert::storage(storage)),
        other => Err(unexpected(&other)),
    }
}

fn expect_role(response: Response) -> Result<Role, CoreError> {
    match response {
        Response::Role(role) => Ok(convert::role(role)),
        other => Err(unexpected(&other)),
    }
}

fn expect_member(response: Response) -> Result<Member, CoreError> {
    match response {
        Response::Member(member) => Ok(convert::member(member)),
        other => Err(unexpected(&other)),
    }
}

/// A picked screen that will not go live: its capture stops off the async
/// runtime.
fn drop_pending(pending: crate::video::screen::Pending) {
    std::thread::spawn(move || drop(pending));
}

pub(crate) fn unexpected(response: &Response) -> CoreError {
    CoreError::Connection {
        message: format!("the server sent an unexpected response: {response:?}"),
    }
}

fn unix_ms() -> i64 {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from(since_epoch.as_millis()).unwrap_or(i64::MAX)
}

/// Whether the wall clock ran well past `expected` between `before` and
/// `now`: the computer slept. A clock set back is not sleep.
fn slept(before: SystemTime, now: SystemTime, expected: Duration) -> bool {
    now.duration_since(before)
        .is_ok_and(|elapsed| elapsed > expected + SLEEP_GAP)
}

/// What the server last heard about this device's idle time.
struct IdleReport {
    target: (String, i64),
    away: bool,
    at: std::time::Instant,
}

/// What to tell the server, if anything, with `idle` measured now in
/// `target`: `Some(Some(ms))` away, `Some(None)` back, `None` nothing new.
fn idle_report(
    sent: Option<&IdleReport>,
    target: &(String, i64),
    idle: Duration,
) -> Option<Option<u64>> {
    let away = idle >= opencord_media::idle::THRESHOLD;
    let due = match sent {
        None => away,
        Some(sent) => {
            sent.target != *target
                || sent.away != away
                || (away && sent.at.elapsed() >= IDLE_REPEAT_EVERY)
        }
    };
    due.then(|| away.then(|| u64::try_from(idle.as_millis()).unwrap_or(u64::MAX)))
}

/// A cue's samples, decoded once.
fn cue_samples(cue: VoiceCue) -> Result<Arc<[f32]>, opencord_media::soundboard::SoundError> {
    use opencord_media::soundboard::cues::Cue;

    static DECODED: OnceLock<Mutex<HashMap<VoiceCue, Arc<[f32]>>>> = OnceLock::new();
    let decoded = DECODED.get_or_init(Mutex::default);
    if let Some(samples) = decoded
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&cue)
    {
        return Ok(Arc::clone(samples));
    }
    let file = match cue {
        VoiceCue::Join => Cue::Join,
        VoiceCue::Leave => Cue::Leave,
        VoiceCue::UserJoin => Cue::UserJoin,
        VoiceCue::UserLeave => Cue::UserLeave,
        VoiceCue::Mute => Cue::Mute,
        VoiceCue::Unmute => Cue::Unmute,
        VoiceCue::Deafen => Cue::Deafen,
        VoiceCue::Undeafen => Cue::Undeafen,
        VoiceCue::StreamStart => Cue::StreamStart,
        VoiceCue::StreamStop => Cue::StreamStop,
    }
    .file();
    let samples: Arc<[f32]> = Arc::from(opencord_media::soundboard::decode_mono(file)?);
    decoded
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(cue, Arc::clone(&samples));
    Ok(samples)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_are_invite_links_or_addresses() {
        let link = parse_target(&format!(
            "opencord://chat.example.com:7711/invite/abc#fp={}",
            "ab".repeat(32)
        ))
        .unwrap();
        let address = parse_target(" chat.example.com ").unwrap();

        assert_eq!(link.address.to_string(), "chat.example.com:7711");
        assert_eq!(link.invite_code.as_deref(), Some("abc"));
        assert_eq!(link.fingerprint, Some([0xab; 32]));
        assert_eq!(address.address.to_string(), "chat.example.com:7710");
        assert_eq!(address.invite_code, None);
        assert!(matches!(
            parse_target("opencord://host/join/x"),
            Err(CoreError::InvalidInput { .. })
        ));
    }

    #[test]
    fn every_voice_cue_decodes_once_and_is_kept() {
        for cue in [
            VoiceCue::Join,
            VoiceCue::Leave,
            VoiceCue::UserJoin,
            VoiceCue::UserLeave,
            VoiceCue::Mute,
            VoiceCue::Unmute,
            VoiceCue::Deafen,
            VoiceCue::Undeafen,
            VoiceCue::StreamStart,
            VoiceCue::StreamStop,
        ] {
            let first = cue_samples(cue).unwrap();
            assert!(first.len() > 48_000 / 20, "{cue:?} is too short");
            assert!(Arc::ptr_eq(&first, &cue_samples(cue).unwrap()));
        }
    }

    #[test]
    fn the_server_hears_of_idle_time_once_it_crosses_a_minute_and_when_it_ends() {
        let general = ("server".to_owned(), 1);
        let minute = opencord_media::idle::THRESHOLD;
        let report = |away, target: &(String, i64)| IdleReport {
            target: target.clone(),
            away,
            at: std::time::Instant::now(),
        };

        // In use, and nothing said yet: nothing to say.
        assert_eq!(idle_report(None, &general, Duration::ZERO), None);
        assert_eq!(
            idle_report(None, &general, minute),
            Some(Some(60_000)),
            "away"
        );
        let away = report(true, &general);
        assert_eq!(idle_report(Some(&away), &general, minute * 2), None);
        assert_eq!(
            idle_report(Some(&away), &general, Duration::ZERO),
            Some(None),
            "back"
        );
        // Moved elsewhere while away: the new channel hears it too.
        let other = ("server".to_owned(), 2);
        assert_eq!(idle_report(Some(&away), &other, minute), Some(Some(60_000)));
        let back = report(false, &general);
        assert_eq!(idle_report(Some(&back), &general, Duration::ZERO), None);
    }

    #[test]
    fn a_wall_clock_far_ahead_of_the_timer_means_the_computer_slept() {
        let before = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000);
        let every = Duration::from_secs(2);

        assert!(!slept(before, before + every, every));
        assert!(!slept(before, before + Duration::from_secs(6), every));
        assert!(slept(before, before + Duration::from_secs(90), every));
        // Set back: not sleep.
        assert!(!slept(before, before - Duration::from_secs(90), every));
    }
}
