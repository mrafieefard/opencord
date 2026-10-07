//! Data types shared by the Rust API and the Dart bindings. Dart never sees
//! protobuf; these are the shapes it works with.
//!
//! Permission sets are `i64` so Dart gets a plain `int`: the same 64 bits as
//! the protocol's `u64`, so `ADMINISTRATOR` (bit 63) makes the value
//! negative. Test bits with `&`, never with `<` or `>`.

/// A key pair's public half, ready to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityInfo {
    pub public_key_hex: String,
    /// Short form like `ABCD-EFGH-IJKL-MNOP`.
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedIdentity {
    /// Keep in secure storage; it is the identity.
    pub secret: Vec<u8>,
    pub info: IdentityInfo,
}

/// A server in the local list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Server {
    /// `host:port`; identifies the server in every other call.
    pub key: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user_id: Option<i64>,
    /// Pinned certificate fingerprint (lowercase hex), if any.
    pub fingerprint: Option<String>,
}

/// A pinned certificate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedFingerprint {
    /// `host:port`.
    pub address: String,
    /// Lowercase hex SHA-256.
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddServerOutcome {
    Added(Server),
    /// The certificate is not pinned and not from a public authority. Ask
    /// the user, then call `server_trust_fingerprint` and add again.
    NeedsTrust {
        address: String,
        fingerprint: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub id: i64,
    pub public_key_hex: String,
    pub fingerprint: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub user: User,
    pub nickname: Option<String>,
    /// Without @everyone.
    pub role_ids: Vec<i64>,
    pub joined_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Role {
    pub id: i64,
    pub name: String,
    /// 0xRRGGBB; 0 means no color.
    pub color: u32,
    pub position: i32,
    pub permissions: i64,
    pub hoist: bool,
    pub mentionable: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RoleChanges {
    pub name: Option<String>,
    pub color: Option<u32>,
    pub permissions: Option<i64>,
    pub hoist: Option<bool>,
    pub mentionable: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelKind {
    Text,
    Voice,
    Category,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverwriteTargetKind {
    Role,
    Member,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionOverwrite {
    pub target_kind: OverwriteTargetKind,
    pub target_id: i64,
    pub allow: i64,
    pub deny: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Channel {
    pub id: i64,
    pub kind: ChannelKind,
    pub name: String,
    pub topic: Option<String>,
    pub parent_id: Option<i64>,
    pub position: i32,
    pub overwrites: Vec<PermissionOverwrite>,
}

/// Absent fields stay unchanged. An empty topic clears it; `parent_id` 0
/// moves the channel out of its category.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChannelChanges {
    pub name: Option<String>,
    pub topic: Option<String>,
    pub parent_id: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelPosition {
    pub channel_id: i64,
    pub position: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub id: i64,
    pub channel_id: i64,
    pub author_id: i64,
    pub content: String,
    pub created_at_ms: i64,
    pub edited_at_ms: Option<i64>,
    /// Set on messages this client sent, to match pending ones.
    pub nonce: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invite {
    pub code: String,
    /// `opencord://host:port/invite/CODE#fp=...`, ready to share.
    pub link: String,
    pub created_by: Option<i64>,
    pub created_at_ms: i64,
    pub max_uses: Option<u32>,
    pub uses: u32,
    pub expires_at_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ban {
    pub user: User,
    pub reason: Option<String>,
    pub banned_by: Option<i64>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerInfo {
    pub server_id_hex: String,
    pub name: String,
    pub description: String,
    pub owner_id: Option<i64>,
    pub open_join: bool,
    pub everyone_role_id: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerChanges {
    pub name: Option<String>,
    pub description: Option<String>,
    pub open_join: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresenceStatus {
    Online,
    Idle,
    Dnd,
    Offline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Presence {
    pub user_id: i64,
    pub status: PresenceStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelPermissions {
    pub channel_id: i64,
    pub permissions: i64,
}

/// Everything about a server right after connecting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadySnapshot {
    pub self_user: User,
    pub server: ServerInfo,
    /// Only channels this user can view.
    pub channels: Vec<Channel>,
    pub roles: Vec<Role>,
    pub members: Vec<Member>,
    pub presences: Vec<Presence>,
    pub server_permissions: i64,
    pub channel_permissions: Vec<ChannelPermissions>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureReason {
    /// The server refused this identity: not a member, a bad invite or
    /// claim token, or banned before connecting.
    Rejected,
    Kicked,
    Banned,
    /// The certificate no longer matches the pinned fingerprint.
    FingerprintChanged,
    /// The server speaks a different protocol version.
    Incompatible,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Reconnecting {
        attempt: u32,
        retry_in_ms: u32,
    },
    /// No more retries; the user has to act.
    Failed {
        reason: FailureReason,
        message: String,
        /// With `FingerprintChanged`: the pinned fingerprint (when there is
        /// one) and the one the server showed instead, as lowercase hex.
        expected_fingerprint: Option<String>,
        presented_fingerprint: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreEvent {
    pub server_key: String,
    pub payload: CoreEventPayload,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreEventPayload {
    ConnectionState(ConnectionState),
    /// After every fresh handshake; replaces all state for the server.
    Ready(ReadySnapshot),
    MessageCreate(Message),
    MessageUpdate(Message),
    MessageDelete {
        channel_id: i64,
        message_id: i64,
    },
    ChannelCreate(Channel),
    ChannelUpdate(Channel),
    ChannelDelete {
        channel_id: i64,
    },
    RoleCreate(Role),
    RoleUpdate(Role),
    RoleDelete {
        role_id: i64,
    },
    MemberJoin(Member),
    MemberLeave {
        user_id: i64,
    },
    MemberUpdate(Member),
    PresenceUpdate(Presence),
    TypingStart {
        channel_id: i64,
        user_id: i64,
    },
    ServerUpdate(ServerInfo),
    /// The user's own permissions changed.
    PermissionsUpdate {
        server_permissions: i64,
        channel_permissions: Vec<ChannelPermissions>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    Unauthorized,
    Forbidden,
    NotFound,
    InvalidArgument,
    RateLimited,
    InvalidSession,
    Conflict,
    Internal,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CoreError {
    #[error("the core has not been initialized")]
    NotInitialized,
    #[error("no identity has been loaded")]
    NoIdentity,
    #[error("that server is not in the list")]
    UnknownServer,
    #[error("not connected to that server right now")]
    NotConnected,
    #[error("the server did not answer in time")]
    Timeout,
    #[error("{message}")]
    InvalidInput { message: String },
    #[error("{message}")]
    Server {
        code: ErrorCode,
        message: String,
        retry_after_ms: Option<u32>,
    },
    #[error("the server's certificate changed: expected {expected}, got {presented}")]
    FingerprintMismatch { expected: String, presented: String },
    #[error("{message}")]
    Rejected {
        reason: FailureReason,
        message: String,
    },
    #[error("could not connect: {message}")]
    Connection { message: String },
    #[error("could not save local data: {message}")]
    Storage { message: String },
}
