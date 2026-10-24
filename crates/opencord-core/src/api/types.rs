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
    /// Its domains as `host:port`, the primary first (Phase 3 plan §7.5).
    pub addresses: Vec<String>,
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
    /// The profile picture's hash, for `avatar_path`; `None` without one.
    pub avatar_hash: Option<String>,
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
    /// The largest file its members may upload; `None` leaves it to their
    /// other roles.
    pub max_upload_bytes: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RoleChanges {
    pub name: Option<String>,
    pub color: Option<u32>,
    pub permissions: Option<i64>,
    pub hoist: Option<bool>,
    pub mentionable: Option<bool>,
    /// `Some(0)` removes the role's limit.
    pub max_upload_bytes: Option<u64>,
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
    /// Voice channels only: bits per second, before the server's cap.
    pub bitrate: u32,
    /// Voice channels only; 0 means no limit.
    pub user_limit: u32,
    /// Voice channels only: whether it holds messages too.
    pub text_in_voice: bool,
}

/// Absent fields stay unchanged. An empty topic clears it; `parent_id` 0
/// moves the channel out of its category. The voice fields are for voice
/// channels only.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChannelChanges {
    pub name: Option<String>,
    pub topic: Option<String>,
    pub parent_id: Option<i64>,
    pub bitrate: Option<u32>,
    pub user_limit: Option<u32>,
    pub text_in_voice: Option<bool>,
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
    /// A command's public result: the command run, by the author.
    pub command: Option<String>,
    /// In the order they were attached.
    pub attachments: Vec<Attachment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invite {
    pub code: String,
    /// `opencord://host:port/invite/CODE#fp=...`, ready to share; without
    /// the fingerprint through a domain with an authority's certificate.
    pub link: String,
    /// `https://host[:port]/invite/CODE`, for a browser, through a domain.
    pub web_link: Option<String>,
    /// The domain the links use, if one does.
    pub domain_id: Option<i64>,
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
    /// The server icon's hash, for `avatar_path`; `None` without one.
    pub icon_hash: Option<String>,
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

/// Someone in a voice channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoiceState {
    pub user_id: i64,
    /// `None` once they have left.
    pub channel_id: Option<i64>,
    /// Whether this device's session holds it. The same user on another
    /// device is `false`.
    pub this_device: bool,
    pub self_mute: bool,
    pub self_deaf: bool,
    pub server_mute: bool,
    pub server_deaf: bool,
    /// Cannot speak: no Speak permission, or in the AFK channel.
    pub suppress: bool,
    pub self_video: bool,
    pub self_stream: bool,
}

/// A screen share preset, as a maximum pixel count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenShareResolution {
    P480,
    P720,
    P1080,
    P1440,
    Source,
}

/// A voice event's sound on this device (Phase 2 §12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VoiceCue {
    Join,
    Leave,
    UserJoin,
    UserLeave,
    Mute,
    Unmute,
    Deafen,
    Undeafen,
    StreamStart,
    StreamStop,
}

/// What a screen share shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamSourceKind {
    Screen,
    Window,
}

/// A screen share in progress (Phase 2 plan §9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenStream {
    /// `stream:<channel_id>:<user_id>`.
    pub stream_key: String,
    pub channel_id: i64,
    pub user_id: i64,
    pub source_kind: StreamSourceKind,
    pub resolution: ScreenShareResolution,
    pub fps: u32,
    pub has_audio: bool,
    pub viewer_count: u32,
}

/// Server-wide voice, video and soundboard settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoiceSettings {
    pub screen_share_max_resolution: ScreenShareResolution,
    pub screen_share_max_fps: u32,
    pub max_stream_viewers: u32,
    pub camera_allowed: bool,
    pub max_camera_participants: u32,
    /// Bits per second.
    pub max_voice_bitrate: u32,
    pub afk_channel_id: Option<i64>,
    pub afk_timeout_s: u32,
    pub soundboard_enabled: bool,
    pub allow_default_sounds: bool,
    pub allow_external_sounds: bool,
    pub sound_cooldown_s: u32,
    pub max_sounds: u32,
}

/// Absent fields stay unchanged; `afk_channel_id` 0 clears the AFK channel.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VoiceSettingsChanges {
    pub screen_share_max_resolution: Option<ScreenShareResolution>,
    pub screen_share_max_fps: Option<u32>,
    pub max_stream_viewers: Option<u32>,
    pub camera_allowed: Option<bool>,
    pub max_camera_participants: Option<u32>,
    pub max_voice_bitrate: Option<u32>,
    pub afk_channel_id: Option<i64>,
    pub afk_timeout_s: Option<u32>,
    pub soundboard_enabled: Option<bool>,
    pub allow_default_sounds: Option<bool>,
    pub allow_external_sounds: Option<bool>,
    pub sound_cooldown_s: Option<u32>,
    pub max_sounds: Option<u32>,
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
    /// Whether the server has voice at all.
    pub voice_enabled: bool,
    /// Everyone in the voice channels this user can view.
    pub voice_states: Vec<VoiceState>,
    /// Screen shares in those channels.
    pub streams: Vec<ScreenStream>,
    pub voice_settings: VoiceSettings,
    /// This server's soundboard.
    pub soundboard_sounds: Vec<SoundboardSound>,
    /// Where the server is reached (Phase 3 plan §7.5).
    pub addresses: ServerAddresses,
    /// The chat commands this user may see (Phase 3 plan §17.1).
    pub commands: CommandList,
    pub upload_settings: UploadSettings,
    pub private_messages: bool,
    pub private_settings: PrivateSettings,
}

/// What the owner chose for private messages on a server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PrivateSettings {
    /// The server relays private messages.
    pub enabled: bool,
    /// Only between two members who are each other's friends.
    pub friends_only: bool,
    /// False when the server's config file turns them off; the owner then
    /// can't turn them on.
    pub allowed_by_config: bool,
}

/// One address a server is reached at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectAddress {
    pub host: String,
    pub port: u32,
    /// Checked like a website, by its certificate authority.
    pub trusted_certificate: bool,
    /// The domain's id; 0 for the IP address.
    pub domain_id: i64,
}

/// Where a server is reached: its active domains (the primary first) and
/// its public IP address.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerAddresses {
    pub primary: Option<ConnectAddress>,
    pub domains: Vec<ConnectAddress>,
    pub ip: Option<ConnectAddress>,
}

/// One of a server's soundboard sounds (Phase 2 plan §11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundboardSound {
    pub id: i64,
    pub name: String,
    pub emoji: Option<String>,
    /// 0–100 %.
    pub volume: u32,
    /// The file's SHA-256, lowercase hex: it is played as `server:<sha256>`.
    pub sha256: String,
    pub size_bytes: u32,
    pub duration_ms: u32,
    pub uploader_id: Option<i64>,
    pub created_at_ms: i64,
}

/// A sound every client has (plan §11.8), played as `default:<id>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultSound {
    pub id: String,
    pub name: String,
    pub emoji: String,
    pub duration_ms: u32,
}

/// Someone in a voice channel played a sound; everyone in it hears it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundEffect {
    pub channel_id: i64,
    pub user_id: i64,
    /// `server:<sha256>`, `external:<sha256>` or `default:<id>`.
    pub sound_ref: String,
    pub name: String,
    pub emoji: Option<String>,
    /// 0–100 %.
    pub volume: u32,
    pub started_at_ms: i64,
    /// How long it plays: the player's soundboard ring shows this long.
    pub duration_ms: u32,
}

/// A file opened for the trim editor (plan §11.3), decoded to 48 kHz mono.
#[derive(Debug, Clone, PartialEq)]
pub struct SoundDraft {
    pub draft_id: u64,
    pub duration_ms: u32,
    /// The loudest sample in each of equal slices of the file, 0–1.
    pub waveform: Vec<f32>,
}

/// What to change about a sound; `None` keeps it. An empty emoji clears it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SoundboardSoundChanges {
    pub name: Option<String>,
    pub emoji: Option<String>,
    /// 0–100 %.
    pub volume: Option<u32>,
}

/// The window of a draft to upload or preview, and how.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundSelection {
    pub draft_id: u64,
    pub start_ms: u32,
    /// At most 5 seconds.
    pub length_ms: u32,
    /// Bring it to −18 LUFS (on by default in the editor).
    pub normalize: bool,
    /// 0–100 %.
    pub volume: u32,
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

// Ready dwarfs the other events, but comes once per connection and is copied
// to Dart either way, so boxing it would save nothing.
#[allow(clippy::large_enum_variant)]
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
    /// Someone joined, left, moved or changed their voice flags. A voice
    /// channel that becomes visible is followed by its participants.
    VoiceStateUpdate(VoiceState),
    VoiceSettingsUpdate(VoiceSettings),
    /// Someone went live, in a voice channel this user can view.
    StreamCreate(ScreenStream),
    /// A stream's quality, audio or viewer count changed.
    StreamUpdate(ScreenStream),
    StreamDelete {
        stream_key: String,
        channel_id: i64,
    },
    /// Who watches this user's own stream.
    StreamViewersUpdate {
        stream_key: String,
        viewer_ids: Vec<i64>,
    },
    SoundboardSoundCreate(SoundboardSound),
    SoundboardSoundUpdate(SoundboardSound),
    SoundboardSoundDelete {
        sound_id: i64,
    },
    /// A sound played in a voice channel this device is connected to on
    /// that server.
    VoiceChannelEffect(SoundEffect),
    /// Where the server is reached changed.
    ServerAddressesUpdate(ServerAddresses),
    /// Domains & network changed, while this session watches it.
    NetworkStatus(NetworkInfo),
    ServerNotice(ServerNotice),
    /// A Lavalink node changed, while this session watches them.
    LavalinkNodeStatus {
        node: LavalinkNode,
        channels_playing: u32,
    },
    LavalinkNodeRemoved {
        node_id: i64,
        channels_playing: u32,
    },
    LavalinkNodeEvent(LavalinkNodeEvent),
    /// A connection test this session started moved on.
    LavalinkTestProgress(LavalinkTestProgress),
    /// The music settings changed.
    MusicSettingsUpdate(MusicSettings),
    /// A voice channel's player changed: to those connected to it and
    /// those who subscribed.
    MusicSession(MusicSession),
    /// What plays in a channel, to everyone who can see it.
    MusicNowPlaying(MusicNowPlaying),
    MusicNotice(MusicNotice),
    /// The commands members can use changed.
    CommandsUpdate(CommandList),
    /// A command's reply only this device sees.
    EphemeralMessage(EphemeralMessage),
    UploadSettingsUpdate(UploadSettings),
    PrivateSettingsUpdate(PrivateSettings),
    /// To the owner only.
    StorageUpdate(StorageInfo),
    /// A file staged with `stage_upload`, at most 10 times a second, and
    /// once at each phase: `Done` when it is ready to send.
    StagedUploadProgress {
        channel_id: i64,
        staged_id: String,
        sent_bytes: u64,
        total_bytes: u64,
        phase: UploadPhase,
    },
    /// A file of a message being sent with `send_message_with_files`, at
    /// most 10 times a second, and once at each phase.
    UploadProgress {
        channel_id: i64,
        nonce: String,
        /// Its place among the message's files.
        file_index: u32,
        sent_bytes: u64,
        total_bytes: u64,
        phase: UploadPhase,
    },
    /// A `download_attachment`, at most 10 times a second, and once at the
    /// end with `done` or `error`.
    DownloadProgress {
        attachment_id: i64,
        received_bytes: u64,
        total_bytes: u64,
        done: bool,
        error: Option<String>,
    },
}

/// Where the server is reached from the internet (Phase 3 plan §5.1).
/// Addresses are empty when unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicAddress {
    /// In use: the override when there is one, else what was detected.
    pub ipv4: String,
    pub ipv6: String,
    pub detected_ipv4: String,
    pub detected_ipv6: String,
    pub override_ipv4: String,
    pub override_ipv6: String,
    pub auto_detect: bool,
    /// When detection last succeeded; 0 for never.
    pub detected_at_ms: i64,
    /// Why the last detection failed, if it did.
    pub detect_error: String,
    pub has_ipv6: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortPurpose {
    Main,
    Voice,
    Music,
    AcmeHttp,
    LocalProxy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortProtocol {
    Tcp,
    Udp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortState {
    Unknown,
    Listening,
    Off,
    Free,
    InUse,
    NotPermitted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MappingState {
    NotUsed,
    Mapped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortStatus {
    pub purpose: PortPurpose,
    pub protocol: PortProtocol,
    pub port: u32,
    pub state: PortState,
    pub mapping: MappingState,
    pub mapping_error: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkSettings {
    /// The main port from the next start; 0 keeps the configured one.
    pub main_port: u32,
    pub upnp_enabled: bool,
    pub local_listener_port: u32,
    pub trusted_proxies: Vec<String>,
    pub dns_resolvers: Vec<String>,
    pub acme_staging: bool,
}

/// Absent fields stay unchanged; `main_port` 0 goes back to the configured
/// port.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetworkSettingsChanges {
    pub main_port: Option<u32>,
    pub upnp_enabled: Option<bool>,
    pub local_listener_port: Option<u32>,
    pub trusted_proxies: Option<Vec<String>>,
    pub dns_resolvers: Option<Vec<String>>,
    pub acme_staging: Option<bool>,
}

/// Domains & network, as the owner sees it (Phase 3 plan §8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkInfo {
    pub public_address: PublicAddress,
    pub ports: Vec<PortStatus>,
    pub settings: NetworkSettings,
    /// The main port in use now.
    pub listening_port: u32,
    /// Whether `settings.main_port` can be used, when it differs.
    pub next_main_port: PortState,
    pub upnp_gateway: String,
    pub upnp_external_ip: String,
    pub upnp_error: String,
    /// The domains attached, in the order they were added.
    pub domains: Vec<Domain>,
    /// DNS providers connected, for the DNS challenge.
    pub dns_providers: Vec<DnsProvider>,
    pub acme: AcmeAccount,
    pub tunnel: TunnelStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TunnelKind {
    /// A random *.trycloudflare.com address; for testing.
    Quick,
    /// The owner's tunnel, by its token.
    Named,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TunnelState {
    Off,
    Starting,
    Running,
    /// cloudflared stopped; it starts again after a pause.
    Restarting,
    /// cloudflared cannot run.
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunnelStatus {
    pub state: TunnelState,
    pub kind: Option<TunnelKind>,
    /// The tunnel's domain; a quick tunnel's once Cloudflare gave it.
    pub hostname: String,
    pub domain_id: i64,
    pub error: String,
    pub restarts: u32,
    /// The cloudflared found; empty when there is none.
    pub cloudflared_version: String,
    /// Downloading cloudflared now.
    pub installing: bool,
}

impl Default for TunnelStatus {
    fn default() -> Self {
        Self {
            state: TunnelState::Off,
            kind: None,
            hostname: String::new(),
            domain_id: 0,
            error: String::new(),
            restarts: 0,
            cloudflared_version: String::new(),
            installing: false,
        }
    }
}

/// A connected DNS provider; its token never leaves the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DnsProvider {
    /// cloudflare, duckdns or desec.
    pub kind: String,
    /// The token's last characters, like `••••abcd`.
    pub masked: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AcmeAccount {
    /// An account exists with the CA in use.
    pub exists: bool,
    pub email: String,
    /// The subscriber agreement to accept before the first certificate.
    pub terms_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertificateState {
    None,
    Requesting,
    Valid,
    /// Valid, but renewing it fails.
    RenewalFailed,
    /// None; the last request failed.
    Failed,
    /// The tunnel's or the proxy's.
    Provided,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertificateStep {
    Account,
    Order,
    Challenge,
    Validation,
    Issuing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertificateStatus {
    pub state: CertificateState,
    pub issuer: String,
    pub not_after_ms: i64,
    /// From Let's Encrypt's test environment: kept, not served.
    pub test: bool,
    /// The owner's own: not renewed by the server.
    pub uploaded: bool,
    /// While requesting.
    pub step: Option<CertificateStep>,
    /// Why the last request or renewal failed.
    pub error: String,
    /// When it may be tried again, after too many failures; 0 when it may.
    pub retry_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainKind {
    /// The owner's own domain, pointed here with A/AAAA records.
    Own,
    /// A DuckDNS or deSEC name Opencord keeps pointed here.
    FreeSubdomain,
    /// Cloudflare Tunnel.
    Tunnel,
    /// Caddy, Nginx, Traefik, ngrok and the like.
    ReverseProxy,
    /// A record proxied by Cloudflare (the orange cloud).
    CloudflareProxy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainStatus {
    Pending,
    Verified,
    /// Verified once; its checks fail since `problem_since_ms`.
    DnsProblem,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckResult {
    Verified,
    PointsElsewhere,
    NoRecord,
    CloudflareProxied,
    PartlyCorrect,
    Failed,
    ConfirmedByOwner,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainCheck {
    pub result: CheckResult,
    pub message: String,
    pub note: String,
    pub found: Vec<String>,
    pub checked_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Domain {
    pub id: i64,
    pub hostname: String,
    pub kind: DomainKind,
    pub public_port: u32,
    pub status: DomainStatus,
    pub active: bool,
    pub primary: bool,
    pub problem_since_ms: i64,
    pub last_check: Option<DomainCheck>,
    /// A check is running now.
    pub checking: bool,
    /// People who connected through it in the last 24 hours.
    pub users_24h: u32,
    pub created_at_ms: i64,
    pub deactivated_at_ms: i64,
    /// Clients check it by its certificate authority.
    pub trusted_certificate: bool,
    pub certificate: CertificateStatus,
}

/// What to change about a domain; absent fields stay.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DomainUpdate {
    pub active: Option<bool>,
    /// Only `Some(true)`: makes it the primary.
    pub primary: Option<bool>,
    pub public_port: Option<u32>,
    /// Vouches for a tunnel or proxy domain whose check could not reach it.
    pub confirm: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeSeverity {
    Info,
    Warning,
}

/// Something the owner or admins should know: the public address changed,
/// a domain's DNS broke, a certificate is about to expire.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerNotice {
    /// One per subject: a newer notice replaces an older one.
    pub key: String,
    pub severity: NoticeSeverity,
    pub title: String,
    pub body: String,
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
    VoiceChannelFull,
    VoiceNotConnected,
    QualityLimit,
    CameraLimit,
    StreamViewerLimit,
    SoundCooldown,
    SoundTooLong,
    SoundInvalid,
    SoundboardFull,
    DomainInvalid,
    DomainExists,
    DomainLimit,
    DomainNotVerified,
    DnsMismatch,
    AcmeFailed,
    AcmeRateLimited,
    PortUnreachable,
    MusicNotConfigured,
    MusicNodeUnavailable,
    MusicNotInChannel,
    MusicBlocked,
    MusicQueueFull,
    MusicMemberLimit,
    MusicTrackTooLong,
    MusicStreamsNotAllowed,
    MusicSourceNotAllowed,
    MusicLinkNotAllowed,
    /// Another track plays now than the action expected.
    MusicStale,
    CommandUnknown,
    CommandDisabled,
    CommandNotAllowedHere,
    CommandInvalidOptions,
    FileTooLarge,
    FileTypeBlocked,
    UploadOffsetMismatch,
    StorageUnavailable,
    StorageFull,
    VoiceMessageInvalid,
    VoiceMessageTooLong,
    FriendRequestsClosed,
    MessageRequestsClosed,
    Undeliverable,
    DeviceListChanged,
    PrivateMessagesUnavailable,
    FriendsOnly,
    Unknown,
}

/// What the app chose for audio (Phase 2 plan §7.12).
#[derive(Debug, Clone, PartialEq)]
pub struct AudioSettings {
    /// A device id from `audio_devices`; `None` follows the system's
    /// default.
    pub input_device: Option<String>,
    pub output_device: Option<String>,
    /// Push-to-talk instead of voice activity.
    pub push_to_talk: bool,
    /// How long push-to-talk keeps sending after the key is let go, 0–2000
    /// ms.
    pub push_to_talk_release_ms: u32,
    /// Voice activity opens on a detected voice, whatever its level;
    /// otherwise at `sensitivity_dbfs`.
    pub automatic_sensitivity: bool,
    /// Manual sensitivity: the level that opens the microphone.
    pub sensitivity_dbfs: f32,
    pub echo_cancellation: bool,
    pub noise_suppression: NoiseSuppressionMode,
    /// Evens out the microphone's level.
    pub automatic_gain: bool,
    /// Microphone gain, 0–2 (200 %).
    pub input_volume: f32,
    /// Everything heard, 0–2 (200 %).
    pub output_volume: f32,
    /// How much other apps are lowered while people talk, 0–100 %; 0 is
    /// off (Phase 2 §12, attenuation).
    pub attenuation_percent: u32,
    pub attenuate_when_others_speak: bool,
    pub attenuate_when_i_speak: bool,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            input_device: None,
            output_device: None,
            push_to_talk: false,
            push_to_talk_release_ms: 200,
            automatic_sensitivity: true,
            sensitivity_dbfs: -45.0,
            echo_cancellation: true,
            noise_suppression: NoiseSuppressionMode::Standard,
            automatic_gain: true,
            input_volume: 1.0,
            output_volume: 1.0,
            attenuation_percent: 0,
            attenuate_when_others_speak: true,
            attenuate_when_i_speak: false,
        }
    }
}

/// Noise suppression on the microphone (Phase 2 plan §7.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoiseSuppressionMode {
    Off,
    /// RNNoise: very light.
    Standard,
    /// DeepFilterNet: much better on keyboards, dogs and fans, heavier.
    High,
}

/// What a global hotkey does (Phase 2 plan §7.13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    PushToTalk,
    PrioritySpeaker,
    ToggleMute,
    ToggleDeafen,
    /// Switches between push-to-talk and voice activity.
    ToggleInputMode,
    /// Mutes or unmutes music in voice channels, for this person.
    ToggleMusicMute,
}

/// A hotkey: a key and the modifiers held with it, as the XDG shortcuts
/// specification writes them, such as `CTRL+SHIFT+m` or `F12`. An action
/// may have several; it is down while any of its keys is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyBinding {
    pub action: HotkeyAction,
    pub accelerator: String,
}

/// Whether hotkeys work while Opencord is in the background.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotkeySupport {
    /// System-wide, through `method`.
    Global { method: String },
    /// Only while Opencord is focused, because of `reason`. `settings_url`
    /// opens the system settings where the user can allow them, when that
    /// is what's missing; the app binds again when its window is focused.
    FocusedOnly {
        reason: String,
        settings_url: Option<String>,
    },
}

/// Someone started or stopped speaking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpeakingChange {
    pub user_id: i64,
    pub speaking: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDevice {
    /// Stable across runs where the system allows; what settings keep.
    pub id: String,
    pub name: String,
}

/// Microphones and speakers the system offers now.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AudioDevices {
    pub inputs: Vec<AudioDevice>,
    pub outputs: Vec<AudioDevice>,
    pub default_input: Option<String>,
    pub default_output: Option<String>,
}

/// A camera (Phase 2 plan §8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraDevice {
    /// Stable while it stays plugged in; what settings keep.
    pub id: String,
    pub name: String,
}

/// This device's camera, on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraStarted {
    /// Its track, for `video_set_wants` (the preview's tile).
    pub track_id: String,
    /// The preview's texture, unmirrored; `None` without `video_init`.
    pub texture_id: Option<i64>,
    /// What the camera captures.
    pub width: u32,
    pub height: u32,
}

/// Whether a screen share's sound can be shared on this system (plan
/// §10.1), and why not.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenAudioSupport {
    pub supported: bool,
    /// For people, when not supported.
    pub reason: Option<String>,
}

/// An app with sound playing that a window share can send (plan §10.4,
/// "Sound from").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenAudioApp {
    pub name: String,
    /// Its icon's name in the desktop's icon theme, when it gives one.
    pub icon: Option<String>,
}

/// What to share (Phase 2 plan §9.1): the quality, within the server's
/// maximum, and whether its sound goes too (V7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenShareRequest {
    pub resolution: ScreenShareResolution,
    /// 15, 30 or 60.
    pub fps: u32,
    pub has_audio: bool,
}

/// This device's screen share, live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenShareStarted {
    pub stream_key: String,
    /// Its track, for `video_set_wants` (the preview's tile).
    pub track_id: String,
    /// The small preview of what is shared; `None` without `video_init`.
    pub texture_id: Option<i64>,
    /// What is captured.
    pub width: u32,
    pub height: u32,
    pub source_kind: StreamSourceKind,
    /// The app whose sound a window share sends, when the system said which
    /// app the window is; `None` and a window: the app asks ("Sound from").
    pub audio_app: Option<String>,
}

/// What this system can do with video (Phase 2 plan §7.11, §8, §9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoSupport {
    /// Others' cameras and screens can be decoded and shown.
    pub receive: bool,
    pub camera: bool,
    pub screen: bool,
    /// Why screens can't be shared, when not.
    pub screen_reason: Option<String>,
    /// The share dialog lists the screens and windows itself
    /// (`screen_sources`); otherwise the system's picker opens after it.
    pub own_screen_picker: bool,
}

/// A screen or window Opencord's own picker offers (plan §9.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenSource {
    /// What `screen_share_start` takes as `source_id`.
    pub id: String,
    pub kind: StreamSourceKind,
    /// "Screen 1 · 2560×1440", or the window's title.
    pub title: String,
    /// The app a window belongs to.
    pub app: Option<String>,
    pub thumbnail: Option<ScreenThumbnail>,
}

/// A small picture of a screen or window, as RGBA rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenThumbnail {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Why a screen share could not start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenProblem {
    /// Not on this system (yet).
    NotSupported,
    /// The user closed the system picker.
    Cancelled,
    Denied,
    /// Sharing needs a voice connection.
    NotInVoice,
    Failed,
}

/// Why the camera could not start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraProblem {
    /// Not on this system yet (only Linux has cameras for now).
    NotSupported,
    NoCamera,
    /// The user (or the system) refused access.
    Denied,
    /// The camera offers nothing Opencord can use.
    NoUsableMode,
    /// Turning the camera on needs a voice connection.
    NotInVoice,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoTrackKind {
    Camera,
    Screen,
}

/// A tile showing a track, in physical pixels; tracks not named are not
/// shown and not received (plan §6, §7.11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoWant {
    pub track_id: String,
    pub width: u32,
    pub height: u32,
}

/// A video texture's counts, for diagnostics and tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureStats {
    /// Pictures the core handed it.
    pub presented: u64,
    /// Times Flutter drew from it.
    pub drawn: u64,
}

/// Where this device's voice connection is (Phase 2 plan §7.14).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceConnectionState {
    /// In a voice channel, waiting to be told which voice node to use.
    AwaitingEndpoint,
    /// Reaching the voice node and identifying.
    Authenticating,
    /// Setting up media.
    RtcConnecting,
    Connected,
    /// The connection broke; getting it back.
    Reconnecting,
    /// Media does not get through; UDP may be blocked.
    NoRoute,
    Disconnected {
        reason: String,
    },
}

/// News from voice media, on its own stream.
#[derive(Debug, Clone, PartialEq)]
pub enum MediaEvent {
    ConnectionState {
        server_key: String,
        channel_id: i64,
        state: VoiceConnectionState,
    },
    /// The chosen device is missing; the system's default stands in.
    DeviceFellBack { output: bool, device: String },
    /// No device could be opened.
    DeviceFailed { output: bool, message: String },
    /// The devices plugged in changed.
    DevicesChanged(AudioDevices),
    /// Who in this device's voice channel started or stopped speaking,
    /// this device's user too; at most every 50 ms (plan §7.5).
    Speaking {
        server_key: String,
        channel_id: i64,
        changes: Vec<SpeakingChange>,
    },
    /// The microphone's level after processing, in dBFS, about 20 times a
    /// second while a meter is open (`audio_set_level_meter`).
    InputLevel { dbfs: f32 },
    /// The microphone heard someone speak while this device was muted (at
    /// most every 30 s); for the "You're muted" reminder.
    SpeakingWhileMuted,
    /// High noise suppression could not keep up on this computer, so
    /// Standard took over (plan §7.3).
    NoiseSuppressionFellBack,
    /// A global hotkey for one of the toggles was pressed (push-to-talk
    /// and the priority key act by themselves).
    HotkeyPressed { action: HotkeyAction },
    /// Someone's camera or screen in this device's voice channel; drawn into
    /// `texture_id` (with `video_init`) while `video_set_wants` asks for it.
    VideoTrackAdded {
        server_key: String,
        channel_id: i64,
        user_id: i64,
        track_id: String,
        kind: VideoTrackKind,
        texture_id: Option<i64>,
        /// Its largest layer's size: the shape to draw it in.
        width: u32,
        height: u32,
    },
    VideoTrackRemoved {
        server_key: String,
        channel_id: i64,
        user_id: i64,
        track_id: String,
    },
    /// This device's camera stopped by itself: unplugged, failed, or the
    /// server no longer allows it.
    CameraStopped { message: String },
    /// This device's screen share ended by itself: the window closed, the
    /// screen went, or the server or voice node stopped it.
    ScreenShareStopped { message: String },
    /// How this device's voice connection is doing, once a second while
    /// connected (Phase 2 §12, connection diagnostics).
    ConnectionStats {
        server_key: String,
        channel_id: i64,
        stats: VoiceStats,
    },
    /// The loudness of the voice message being recorded, 0..1.
    VoiceRecordLevel { level: f32 },
    /// Recording stopped by itself.
    VoiceRecordStopped { reason: VoiceStopReason },
    /// Voice message or media playback status.
    PlaybackUpdate {
        item: PlaybackItem,
        position_ms: u32,
        duration_ms: u32,
        playing: bool,
        buffering: bool,
    },
    /// Voice message or media playback reached the end.
    PlaybackEnded { item: PlaybackItem },
    /// Voice message or media playback failed.
    PlaybackFailed { item: PlaybackItem, message: String },
}

/// What is being played: an attachment on a server, or a locally recorded draft.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaybackItem {
    Attachment {
        server_key: String,
        attachment_id: i64,
    },
    Draft {
        draft_id: u64,
    },
}

/// Why a voice recording stopped by itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceStopReason {
    MaxLength,
    DeviceLost,
}

/// A voice message recorded on this device, ready to send or preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceDraft {
    pub draft_id: u64,
    pub duration_ms: u32,
    pub waveform: Vec<u8>,
}

/// A media attachment opened for playback (Milestone A7, Task 34).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaHandle {
    pub texture_id: Option<i64>,
    pub duration_ms: u32,
    pub width: u32,
    pub height: u32,
    pub has_video: bool,
}

/// A voice connection over the last second (Phase 2 §12).
#[derive(Debug, Clone, PartialEq)]
pub struct VoiceStats {
    /// Round trip to the voice node.
    pub ping_ms: Option<u32>,
    /// How unevenly the voices heard arrive.
    pub jitter_ms: Option<f32>,
    /// Packets lost on the way here and on the way out, 0–100.
    pub loss_in_percent: Option<f32>,
    pub loss_out_percent: Option<f32>,
    /// Bits per second, everything included.
    pub bitrate_in: u64,
    pub bitrate_out: u64,
    /// The voice node's host and port.
    pub voice_node: String,
    /// This device's camera and screen share encoders, while on: "NVENC
    /// (hardware)", say.
    pub camera_encoder: Option<String>,
    pub screen_encoder: Option<String>,
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
    #[error("{message}")]
    Screen {
        problem: ScreenProblem,
        message: String,
    },
    #[error("{message}")]
    Camera {
        problem: CameraProblem,
        message: String,
    },
    #[error("cancelled")]
    Cancelled,
}

/// Lavalink nodes (Phase 3 plan §12). Their passwords never reach clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LavalinkPriority {
    Preferred,
    Normal,
    /// Used only when no Preferred or Normal node can play.
    Backup,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LavalinkHealth {
    /// The first connection is still being made.
    Connecting,
    Healthy,
    /// Connected but struggling: see the reason.
    Degraded,
    Unavailable,
    /// Gets no new channels and keeps its own until they move.
    Draining,
    Disabled,
}

/// What an admin sets for a node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LavalinkNodeSettings {
    pub name: String,
    pub host: String,
    pub port: u32,
    /// https and wss.
    pub secure: bool,
    pub priority: LavalinkPriority,
    /// At most this many channels at once; 0 for no cap.
    pub max_players: u32,
    /// The bridge accepts only this node's addresses.
    pub strict_address: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LavalinkNode {
    pub id: i64,
    pub settings: LavalinkNodeSettings,
    pub draining: bool,
    pub health: LavalinkHealth,
    /// Why it is degraded or unavailable, in plain words.
    pub reason: String,
    pub version: String,
    pub sources: Vec<String>,
    pub plugins: Vec<String>,
    /// Opencord's channels on it, and every player it has.
    pub channels: u32,
    pub players: u32,
    pub playing_players: u32,
    /// System load in percent.
    pub cpu_percent: u32,
    pub ping_ms: u32,
    /// The last thing that went wrong, kept after it recovers.
    pub last_error: String,
    pub connected_since_ms: i64,
    /// Back after being unavailable: new channels wait a little.
    pub recovering: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LavalinkEventKind {
    Connected,
    Disconnected,
    Degraded,
    Recovered,
    Moved,
}

/// A line in the node events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LavalinkNodeEvent {
    pub node_id: i64,
    pub kind: LavalinkEventKind,
    pub text: String,
    pub at_ms: i64,
}

/// What Lavalink needs from this server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LavalinkBridge {
    /// False when music is off on the server.
    pub enabled: bool,
    /// wss://host:port, where Lavalink connects.
    pub url: String,
    pub trusted_certificate: bool,
    /// address:port, where Lavalink sends audio; empty while unknown.
    pub audio_address: String,
    pub audio_port: u32,
    pub audio_listening: bool,
    pub mapping: MappingState,
    /// Lavalink connections now, and what came since the server started.
    pub connections: u32,
    pub packets: u64,
    pub decrypt_failures: u64,
    pub dropped: u64,
    pub jitter_ms: u32,
    /// External voice nodes' bridges, as host:port.
    pub node_bridges: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LavalinkNodes {
    pub nodes: Vec<LavalinkNode>,
    pub bridge: LavalinkBridge,
    /// Newest first.
    pub events: Vec<LavalinkNodeEvent>,
    pub channels_playing: u32,
}

/// Changes to a node; what is `None` stays.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LavalinkNodeChanges {
    pub settings: Option<LavalinkNodeSettings>,
    pub password: Option<String>,
    pub draining: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LavalinkTestStepKind {
    /// REST answers and takes the password.
    Rest,
    /// The WebSocket says ready.
    Websocket,
    /// A player reaches the audio bridge.
    Bridge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LavalinkTestStepState {
    Waiting,
    Running,
    Passed,
    Failed,
    /// Not run: an earlier step failed.
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LavalinkTestStep {
    pub kind: LavalinkTestStepKind,
    pub state: LavalinkTestStepState,
    /// What passed, or what failed and how to fix it.
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LavalinkTestProgress {
    pub node_id: i64,
    pub steps: Vec<LavalinkTestStep>,
    pub done: bool,
}

/// Server settings → Music (Phase 3 plan §16.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicSettings {
    pub enabled: bool,
    /// Search sources members may use; empty for every available one.
    pub enabled_sources: Vec<String>,
    pub default_source: String,
    pub allow_links: bool,
    pub allow_streams: bool,
    /// 0 for no limit.
    pub max_track_ms: u64,
    pub max_tracks_per_member: u32,
    pub max_playlist_tracks: u32,
    pub queue_limit: u32,
    pub vote_skip: bool,
    pub vote_skip_percent: u32,
    pub stop_after_empty_s: u32,
    pub disconnect_after_idle_s: u32,
    /// "primary", or an active domain with a trusted certificate.
    pub bridge_domain: String,
    /// Search sources at least one healthy node offers now.
    pub available_sources: Vec<String>,
    /// Music is on in the server's configuration file and runs there.
    pub supported: bool,
    /// Supported, enabled, and with a Lavalink node: members see music.
    pub configured: bool,
}

/// Changes to the music settings; what is `None` stays.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MusicSettingsChanges {
    pub enabled: Option<bool>,
    pub enabled_sources: Option<Vec<String>>,
    pub default_source: Option<String>,
    pub allow_links: Option<bool>,
    pub allow_streams: Option<bool>,
    pub max_track_ms: Option<u64>,
    pub max_tracks_per_member: Option<u32>,
    pub max_playlist_tracks: Option<u32>,
    pub queue_limit: Option<u32>,
    pub vote_skip: Option<bool>,
    pub vote_skip_percent: Option<u32>,
    pub stop_after_empty_s: Option<u32>,
    pub disconnect_after_idle_s: Option<u32>,
    pub bridge_domain: Option<String>,
}

/// A member blocked from music.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicBlock {
    pub user_id: i64,
    pub reason: String,
    pub blocked_by: i64,
    pub created_at_ms: i64,
}

/// A track as the server shows it (Phase 3 plan §18.3); never its encoded
/// form, which stays on the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicTrack {
    pub title: String,
    pub artist: String,
    /// 0 for streams.
    pub duration_ms: u64,
    pub is_stream: bool,
    pub is_seekable: bool,
    /// Lavalink's source name, like "youtube".
    pub source: String,
    /// "YouTube", "SoundCloud"…
    pub source_display_name: String,
    pub uri: String,
    /// For `music_artwork`; empty without artwork.
    pub artwork_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicSearchResult {
    /// What to add; good for 15 minutes, for this user only.
    pub result_id: String,
    pub track: MusicTrack,
}

/// A playlist a link loaded, shown as one card with "Add all".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicPlaylist {
    pub name: String,
    pub result_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicSearchResults {
    pub results: Vec<MusicSearchResult>,
    pub playlist: Option<MusicPlaylist>,
}

/// A track in a channel's queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicQueueItem {
    pub item_id: String,
    pub track: MusicTrack,
    pub requester_id: i64,
    pub added_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MusicPlayState {
    Idle,
    Loading,
    Playing,
    Paused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MusicRepeat {
    Off,
    Queue,
    Track,
}

/// A voice channel's player (Phase 3 plan §14.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicSession {
    pub channel_id: i64,
    /// Increases with every change.
    pub version: u64,
    pub state: MusicPlayState,
    pub now_playing: Option<MusicQueueItem>,
    /// Upcoming, in play order.
    pub queue: Vec<MusicQueueItem>,
    /// Most recent first.
    pub history: Vec<MusicQueueItem>,
    /// What is heard was `position_ms` at `anchor_ms` on this device's
    /// clock (Unix ms), the server's clock offset and the music's playout
    /// delay taken into account; it moves on while playing.
    pub position_ms: u64,
    pub anchor_ms: i64,
    pub repeat: MusicRepeat,
    pub shuffle: bool,
    /// Every node is unavailable: paused, the queue kept, retrying.
    pub waiting_for_node: bool,
    /// Everyone left: paused until someone comes back.
    pub paused_empty: bool,
    /// Users who voted to skip the current track.
    pub skip_votes: Vec<i64>,
    pub skip_votes_needed: u32,
}

/// What everyone who can see a channel learns about its music, for the
/// now-playing card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicNowPlaying {
    pub channel_id: i64,
    pub title: String,
    pub artist: String,
    pub artwork_id: String,
    pub playing: bool,
    /// Nothing plays any more.
    pub ended: bool,
    /// "YouTube", "SoundCloud"…
    pub source_display_name: String,
}

/// A short activity line for the music panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicNotice {
    pub channel_id: i64,
    pub text: String,
    pub at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MusicPlacement {
    /// At the end, or before `before_item_id`.
    End,
    Next,
    /// Replaces what plays.
    Now,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MusicAction {
    Pause,
    Resume,
    Next,
    Previous,
    Seek,
    Stop,
}

/// The kind of value a command option takes (Phase 3 plan §17.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandOptionKind {
    Text,
    Integer,
    Boolean,
    Choice,
    User,
    Channel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandChoice {
    /// What the user sees.
    pub name: String,
    /// What is sent.
    pub value: String,
    /// A track's artwork, for /play's suggestions; empty otherwise.
    pub artwork_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOptionSpec {
    pub name: String,
    pub description: String,
    pub kind: CommandOptionKind,
    pub required: bool,
    pub autocomplete: bool,
    pub choices: Vec<CommandChoice>,
    /// For integers; both 0 for no bounds.
    pub min: i64,
    pub max: i64,
}

/// A chat command, as the server describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub name: String,
    pub description: String,
    pub options: Vec<CommandOptionSpec>,
    /// "music".
    pub feature: String,
    /// "Control playback".
    pub permission_summary: String,
    pub aliases: Vec<String>,
    /// The permission bits it needs at the least.
    pub permissions: i64,
    pub enabled: bool,
    /// Empty: every channel.
    pub channel_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommandList {
    pub commands: Vec<CommandSpec>,
    /// Results go to the channel, or only to who ran the command.
    pub results_public: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOptionValue {
    pub name: String,
    pub value: String,
}

/// What running a command did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutcome {
    /// The public result's message.
    pub message_id: Option<i64>,
    /// The reply came only to this device, as an `EphemeralMessage`.
    pub ephemeral: bool,
}

/// A reply only this device sees; never stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EphemeralMessage {
    pub channel_id: i64,
    /// The nonce `command_execute` was given.
    pub nonce: String,
    pub content: String,
}

/// Changes to a command on the Commands page; `None` stays.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommandSettingsChanges {
    /// Empty to change only `results_public`.
    pub name: String,
    pub enabled: Option<bool>,
    /// An empty list for every channel.
    pub channel_ids: Option<Vec<i64>>,
    pub results_public: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentKind {
    File,
    Image,
    Video,
    Audio,
    VoiceMessage,
}

/// A file in a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    pub id: i64,
    pub filename: String,
    /// Chosen by the server from the file's first bytes.
    pub content_type: String,
    pub size_bytes: u64,
    pub kind: AttachmentKind,
    /// Images: after EXIF rotation. Videos: the sender's hints.
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Videos (hint) and voice messages (measured).
    pub duration_ms: Option<u32>,
    /// Voice messages: 128 values, 0–255. Empty otherwise.
    pub waveform: Vec<u8>,
    pub has_thumbnail: bool,
    pub thumbnail_width: Option<u32>,
    pub thumbnail_height: Option<u32>,
    /// A GIF or WebP with more than one frame.
    pub animated: bool,
}

/// What may be uploaded to a server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadSettings {
    /// Lowercase, without the dot; may hold inner dots ("tar.gz").
    pub blocked_extensions: Vec<String>,
    pub voice_message_max_seconds: u32,
    /// The server's ceiling: the owner's limit and the most any role allows.
    pub max_file_bytes: u64,
    pub max_attachments: u32,
}

/// Where a file being sent is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadPhase {
    /// Reading it, and removing a photo's location.
    Preparing,
    Uploading,
    /// The server checks it and makes its thumbnail.
    Processing,
    /// Every file is in; the message goes out.
    Sending,
    Done,
    Failed {
        message: String,
    },
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageKind {
    Local,
    S3,
}

/// An S3-compatible bucket. In answers the access key is masked to its
/// last 4 characters and the secret is empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct S3Config {
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub key_prefix: String,
    pub path_style: bool,
    pub allow_http: bool,
    pub access_key_id: String,
    /// Empty in an update keeps the saved one.
    pub secret_access_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageBackend {
    pub id: i64,
    pub kind: StorageKind,
    pub name: String,
    /// New files go here.
    pub active: bool,
    pub s3: Option<S3Config>,
    pub has_secret: bool,
    pub file_count: u64,
    pub bytes: u64,
    /// Why its last test failed.
    pub last_error: Option<String>,
}

/// Moving every file to the active storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageMove {
    pub to_storage_id: i64,
    pub running: bool,
    pub files_done: u64,
    pub files_total: u64,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub failed: u32,
}

/// Where a server keeps its files. Owner only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageInfo {
    pub backends: Vec<StorageBackend>,
    /// The running or last move.
    pub move_status: Option<StorageMove>,
    /// Free space where the server receives uploads.
    pub temp_free_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageTestStep {
    pub name: String,
    pub ok: bool,
    pub detail: String,
}

/// A bucket test: the steps run, up to the first that failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageTestResult {
    pub steps: Vec<StorageTestStep>,
    pub ok: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageTestTarget {
    /// A saved storage; its result is saved as its `last_error`.
    Existing { storage_id: i64 },
    /// Settings not saved yet; nothing is saved.
    Config { s3: S3Config },
    /// A saved storage's edited settings, not saved yet: keys left empty
    /// are its saved ones. Nothing is saved.
    Edited { storage_id: i64, s3: S3Config },
}

/// A file to send: a path on this device, or bytes (a pasted image).
/// Exactly one of the two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutgoingFile {
    pub path: Option<String>,
    pub bytes: Option<Vec<u8>>,
    /// Empty uses the path's file name.
    pub filename: String,
    /// The id it was staged with (`stage_upload`): sending takes that
    /// upload over, and uploads the file again only if it failed.
    pub staged_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentVariant {
    Original,
    Thumbnail,
}

// Private messages (design 2026-10-22). They reach Dart in Task 16.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrivateOpened {
    /// The old database couldn't be read with the key and was moved aside.
    pub started_fresh: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FriendStatus {
    None,
    Outgoing,
    Incoming,
    Friends,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationStatus {
    None,
    Request,
    Accepted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivateMessageKind {
    Text,
    Divider,
    Moved,
    Undecryptable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrivateSendState {
    Received,
    Sending,
    Sent,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivateContact {
    /// Their public key, lowercase hex.
    pub peer: String,
    pub fingerprint: String,
    /// The name last seen.
    pub name: Option<String>,
    pub friend: FriendStatus,
    pub request_spam: bool,
    pub blocked: bool,
    pub conversation: ConversationStatus,
    /// This device's key for the conversation's relay, if it saved that
    /// server.
    pub relay_server_key: Option<String>,
    pub last_activity_ms: Option<i64>,
    /// Incoming text after the last read.
    pub unread: u32,
    pub muted: bool,
    pub hidden: bool,
    /// The first line of the latest text, at most 100 characters.
    pub preview: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivateMessage {
    /// Hex.
    pub id: String,
    pub peer: String,
    pub outgoing: bool,
    pub kind: PrivateMessageKind,
    /// The text; a divider's or card's server name; empty otherwise.
    pub content: String,
    /// A divider's or card's server, if this device saved it.
    pub server_key: Option<String>,
    pub sent_at_ms: i64,
    pub edited_at_ms: Option<i64>,
    pub state: PrivateSendState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedServer {
    pub server_key: String,
    pub name: String,
    pub connected: bool,
    pub private_messages: bool,
    /// Messages go there only between friends.
    pub friends_only: bool,
    pub peer_user_id: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrivacySettingsData {
    pub allow_friend_requests: bool,
    pub allow_message_requests: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivateEvent {
    ContactChanged(PrivateContact),
    ContactRemoved { peer: String },
    MessageAdded(PrivateMessage),
    MessageChanged(PrivateMessage),
    MessageDeleted { peer: String, message_id: String },
    Typing { peer: String },
    FriendRequestReceived { peer: String, spam: bool },
}
