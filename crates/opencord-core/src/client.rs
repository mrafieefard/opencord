//! The client: an identity, the saved servers, and one connection task per
//! server. Every event from every server arrives on one channel.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use opencord_common::address::{
    Fingerprint, InviteLink, ServerAddress, format_fingerprint, parse_fingerprint,
};
use opencord_proto::v1 as proto;
use proto::request::Kind as Request;
use proto::response::Result as Response;
use tokio::runtime::Handle;
use tokio::sync::{mpsc, oneshot};

use crate::api::types::{
    AddServerOutcome, Ban, Channel, ChannelChanges, ChannelKind, ChannelPosition, CoreError,
    CoreEvent, IdentityInfo, Invite, Member, Message, OverwriteTargetKind, PermissionOverwrite,
    PresenceStatus, Role, RoleChanges, Server, ServerChanges, ServerInfo, User,
};
use crate::connection::{
    self, Command, Connection, Context, Credentials, Established, HandshakeError,
};
use crate::convert;
use crate::identity::Identity;
use crate::store::{SavedServer, Store, StoreError};

const INVITE_SCHEME: &str = "opencord://";

#[derive(Clone)]
pub struct Client {
    inner: Arc<Inner>,
}

struct Inner {
    runtime: Handle,
    events: mpsc::UnboundedSender<CoreEvent>,
    store: Arc<Mutex<Store>>,
    credentials: RwLock<Option<Credentials>>,
    connections: Mutex<HashMap<String, Connection>>,
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
        let (events, receiver) = mpsc::unbounded_channel();
        let client = Self {
            inner: Arc::new(Inner {
                runtime,
                events,
                store: Arc::new(Mutex::new(store)),
                credentials: RwLock::new(None),
                connections: Mutex::new(HashMap::new()),
            }),
        };
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
    pub async fn add_server(
        &self,
        link_or_address: &str,
        claim_token: Option<String>,
    ) -> Result<AddServerOutcome, CoreError> {
        let credentials = self.credentials().ok_or(CoreError::NoIdentity)?;
        let target = parse_target(link_or_address)?;
        let key = target.address.to_string();
        if let Some(saved) = self.lock_store().server(&key) {
            let pin = self.lock_store().pin(&key);
            return Ok(AddServerOutcome::Added(server_view(saved, pin)));
        }
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
        let ready = connection::identify(
            &mut opened.socket,
            &opened.hello,
            &credentials,
            target.invite_code,
            claim_token,
        )
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
            added_at_ms: unix_ms(),
        };
        let pin = pinned.map(|_| opened.presented);
        {
            let mut store = self.lock_store();
            if let Some(pin) = &pin {
                store.set_pin(&key, pin)?;
            }
            store.upsert_server(saved.clone())?;
        }
        let connection = connection::spawn(
            &self.inner.runtime,
            self.context(key.clone(), target.address, credentials),
            Some(Established::identified(opened, ready)),
        );
        if let Some(previous) = self.lock_connections().insert(key, connection) {
            previous.task.abort();
        }
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
