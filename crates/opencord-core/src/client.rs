//! The client: an identity, the saved servers, and one connection task per
//! server. Every event from every server arrives on one channel.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, RwLock, Weak};
use std::time::{SystemTime, UNIX_EPOCH};

use opencord_common::address::{
    Fingerprint, InviteLink, ServerAddress, format_fingerprint, parse_fingerprint,
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
    PermissionOverwrite, PresenceStatus, Role, RoleChanges, Server, ServerChanges, ServerInfo,
    User, VideoWant, VoiceSettings, VoiceSettingsChanges, VoiceState,
};
use crate::connection::{
    self, Command, Connection, Context, Credentials, Established, HandshakeError,
};
use crate::convert;
use crate::identity::Identity;
use crate::media::{self, Media, MediaOptions, MediaTarget};
use crate::store::{SavedServer, Store, StoreError};
use crate::voice::VoiceServer;
use opencord_media::hotkeys::Hotkeys;

const INVITE_SCHEME: &str = "opencord://";
/// Voice server updates kept for a slow listener.
const VOICE_SERVER_BACKLOG: usize = 16;

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
        let store = Store::open(data_dir)?;
        let (events, mut incoming) = mpsc::unbounded_channel();
        let (outward, receiver) = mpsc::unbounded_channel();
        let client = Self {
            inner: Arc::new(Inner {
                runtime: runtime.clone(),
                events,
                outward: outward.clone(),
                voice_servers: broadcast::channel(VOICE_SERVER_BACKLOG).0,
                store: Arc::new(Mutex::new(store)),
                credentials: RwLock::new(None),
                connections: Mutex::new(HashMap::new()),
                voice: Mutex::new(Voice::default()),
                media: OnceLock::new(),
                hotkeys: tokio::sync::Mutex::new(None),
            }),
        };
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

        let saved = SavedServer {
            key: key.clone(),
            host: target.address.host.clone(),
            port: target.address.port,
            name: ready
                .server
                .as_ref()
                .map(|server| server.name.clone())
                .unwrap_or_default(),
            user_id: ready.self_user.as_ref().map(|user| user.id),
            added_at_ms: self
                .lock_store()
                .server(&key)
                .map_or_else(unix_ms, |known| known.added_at_ms),
        };
        let pin = pinned.map(|_| opened.presented);
        {
            let mut store = self.lock_store();
            if let Some(pin) = &pin {
                store.set_pin(&key, pin)?;
            }
            store.upsert_server(saved.clone())?;
        }
        let previous = self.lock_connections().remove(&key);
        if let Some(previous) = previous {
            stop(previous).await;
        }
        let connection = connection::spawn(
            &self.inner.runtime,
            self.context(key.clone(), target.address, credentials),
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

    pub async fn create_role(
        &self,
        key: &str,
        name: String,
        color: u32,
        permissions: i64,
        hoist: bool,
        mentionable: bool,
    ) -> Result<Role, CoreError> {
        let request = Request::CreateRole(proto::CreateRole {
            name,
            color,
            permissions: convert::bits_from_api(permissions),
            hoist,
            mentionable,
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

    pub async fn create_invite(
        &self,
        key: &str,
        max_uses: Option<u32>,
        expires_in_s: Option<u32>,
    ) -> Result<Invite, CoreError> {
        let request = Request::CreateInvite(proto::CreateInvite {
            max_uses,
            expires_in_s,
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
        Some(receiver)
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
        let target = self.lock_voice().target.take();
        self.sync_media();
        match target {
            Some((key, _)) => self.request(&key, leave_voice()).await.map(|_| ()),
            None => Ok(()),
        }
    }

    /// Self mute and deafen. They hold outside voice too, for the next
    /// join; in voice, the server hears about them in the background.
    pub fn set_voice_self(&self, mute: Option<bool>, deaf: Option<bool>) {
        let target = {
            let mut voice = self.lock_voice();
            voice.mute = mute.unwrap_or(voice.mute);
            voice.deaf = deaf.unwrap_or(voice.deaf);
            voice.target.clone()
        };
        self.sync_media();
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
        #[cfg(target_os = "linux")]
        if let Some(media) = self.inner.media.get() {
            media
                .video()
                .set_textures(Arc::new(crate::video::flutter::FlutterTextures::new(
                    engine_handle,
                )));
        }
        #[cfg(not(target_os = "linux"))]
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
            _ => {}
        }
    }

    /// Gets back into `channel_id` after a fresh session. If the server says
    /// no, this device has left, and the app hears so.
    async fn rejoin(&self, key: &str, channel_id: i64) {
        let request = self.voice_update(channel_id);
        let Err(CoreError::Server { code, .. }) = self.request(key, request).await else {
            return;
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

    async fn request(&self, key: &str, kind: Request) -> Result<Response, CoreError> {
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

    fn invite_view(&self, key: &str, invite: proto::Invite) -> Invite {
        let link = ServerAddress::parse(key)
            .map(|address| {
                InviteLink {
                    address,
                    code: invite.code.clone(),
                    fingerprint: self.lock_store().pin(key),
                }
                .to_string()
            })
            .unwrap_or_default();
        convert::invite(invite, link)
    }

    fn context(&self, key: String, address: ServerAddress, credentials: Credentials) -> Context {
        Context {
            key,
            address,
            credentials,
            store: Arc::clone(&self.inner.store),
            events: self.inner.events.clone(),
            voice_servers: self.inner.voice_servers.clone(),
        }
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

fn unexpected(response: &Response) -> CoreError {
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
}
