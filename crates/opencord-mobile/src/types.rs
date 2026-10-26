//! What crosses to JavaScript (design §5.3). Ids are decimal strings; times
//! are milliseconds since the Unix epoch and sizes, durations and positions
//! are `f64`; volumes are `f32`, 0.0–2.0.

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct IdentityInfo {
    pub public_key_hex: String,
    /// Short form like `ABCD-EFGH-IJKL-MNOP`.
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct NewIdentity {
    /// Keep in secure storage; it is the identity.
    pub secret: Vec<u8>,
    pub info: IdentityInfo,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ServerEntry {
    /// `host:port`; names the server in every other call.
    pub key: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user_id: Option<String>,
    /// The pinned certificate fingerprint (lowercase hex), if any.
    pub fingerprint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum AddServerResult {
    Added {
        server: ServerEntry,
    },
    /// The certificate is neither pinned nor from a public authority: ask,
    /// then `server_trust` and add again.
    NeedsTrust {
        address: String,
        fingerprint: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FailureKind {
    /// Not a member, a bad invite or claim token, or banned before joining.
    Rejected,
    Kicked,
    Banned,
    /// The certificate no longer matches the pinned fingerprint.
    FingerprintChanged,
    /// The server speaks another protocol version.
    Incompatible,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum ConnectionRecord {
    Connecting,
    Connected,
    Reconnecting {
        attempt: u32,
        retry_in_ms: u32,
    },
    /// No more retries; the user has to act.
    Failed {
        reason: FailureKind,
        message: String,
        expected_fingerprint: Option<String>,
        presented_fingerprint: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct UserRecord {
    pub id: String,
    pub display_name: String,
    /// For `avatar_file`; `None` without a picture.
    pub avatar_hash: Option<String>,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MemberRecord {
    pub user: UserRecord,
    pub nickname: Option<String>,
    pub role_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ChannelKind {
    Text,
    Voice,
    Category,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ChannelRecord {
    pub id: String,
    pub kind: ChannelKind,
    pub name: String,
    pub topic: Option<String>,
    /// The category it sits in.
    pub parent_id: Option<String>,
    pub position: i32,
    /// Voice: 0 for no limit.
    pub user_limit: u32,
    /// Voice: has a chat of its own.
    pub text_in_voice: bool,
}

/// What this user may do in a channel, from its permission bits.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ChannelAccess {
    pub channel_id: String,
    pub view: bool,
    pub read_history: bool,
    pub send: bool,
    pub attach: bool,
    pub connect: bool,
    pub speak: bool,
    pub manage_messages: bool,
    pub music_add: bool,
    pub music_control: bool,
    pub music_manage_queue: bool,
    pub music_manage: bool,
}

/// Everything a `Ready` says about a server; it replaces what came before.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ServerSnapshot {
    pub server_key: String,
    pub self_user: UserRecord,
    pub name: String,
    pub description: String,
    pub owner_id: Option<String>,
    pub icon_hash: Option<String>,
    pub channels: Vec<ChannelRecord>,
    pub access: Vec<ChannelAccess>,
    pub members: Vec<MemberRecord>,
    pub voice_states: Vec<VoiceStateRecord>,
    pub voice_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MessageRecord {
    pub id: String,
    pub channel_id: String,
    pub author_id: String,
    pub content: String,
    pub created_at_ms: f64,
    pub edited_at_ms: Option<f64>,
    /// The sender's nonce, on the echo of this device's own send.
    pub nonce: Option<String>,
    /// The command that made this message, for command replies.
    pub command: Option<String>,
    pub attachments: Vec<AttachmentRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum AttachmentKind {
    File,
    Image,
    Video,
    Audio,
    VoiceMessage,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct AttachmentRecord {
    pub id: String,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: f64,
    pub kind: AttachmentKind,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration_ms: Option<u32>,
    pub has_thumbnail: bool,
    pub thumbnail_width: Option<u32>,
    pub thumbnail_height: Option<u32>,
    /// A GIF or WebP with more than one frame.
    pub animated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum AttachmentVariant {
    Original,
    Thumbnail,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct VoiceStateRecord {
    pub user_id: String,
    /// `None`: not in voice any more.
    pub channel_id: Option<String>,
    pub this_device: bool,
    pub self_mute: bool,
    pub self_deaf: bool,
    pub server_mute: bool,
    pub server_deaf: bool,
}

/// Where this device's voice connection is.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum VoiceLinkState {
    AwaitingEndpoint,
    Authenticating,
    RtcConnecting,
    Connected,
    Reconnecting,
    /// Media doesn't get through; UDP may be blocked.
    NoRoute,
    Disconnected {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SpeakingChangeRecord {
    pub user_id: String,
    pub speaking: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum UploadPhaseRecord {
    /// Reading it, and checking it against what the server has.
    Preparing,
    Uploading,
    /// The server checks it.
    Processing,
    /// Every file is in; the message goes out.
    Sending,
    Done,
    Failed {
        message: String,
    },
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct OutgoingFileRecord {
    pub path: String,
    /// Empty uses the path's file name.
    pub filename: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MusicTrackRecord {
    pub title: String,
    pub artist: String,
    pub duration_ms: f64,
    pub is_stream: bool,
    pub is_seekable: bool,
    pub source: String,
    /// "YouTube", "SoundCloud", "Local"…
    pub source_display_name: String,
    pub uri: String,
    pub artwork_id: String,
    /// An uploaded song's id; `None` for Lavalink's tracks.
    pub local_song_id: Option<String>,
    pub removable: bool,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MusicQueueItemRecord {
    pub item_id: String,
    pub track: MusicTrackRecord,
    pub requester_id: String,
    pub added_at_ms: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MusicPlayState {
    Idle,
    Loading,
    Playing,
    Paused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MusicRepeat {
    Off,
    Queue,
    Track,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MusicPlacement {
    /// At the end, or before `before_item_id`.
    End,
    Next,
    /// Replaces what plays.
    Now,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MusicAction {
    Pause,
    Resume,
    Next,
    Previous,
    Seek,
    Stop,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MusicSessionRecord {
    pub channel_id: String,
    /// Grows with every change; an older one is stale.
    pub version: f64,
    pub state: MusicPlayState,
    pub now_playing: Option<MusicQueueItemRecord>,
    pub queue: Vec<MusicQueueItemRecord>,
    pub history: Vec<MusicQueueItemRecord>,
    /// Where playback was at `anchor_ms`.
    pub position_ms: f64,
    /// On this device's clock.
    pub anchor_ms: f64,
    pub repeat: MusicRepeat,
    pub shuffle: bool,
    pub waiting_for_node: bool,
    pub paused_empty: bool,
    pub skip_votes: Vec<String>,
    pub skip_votes_needed: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MusicNowPlayingRecord {
    pub channel_id: String,
    pub title: String,
    pub artist: String,
    pub artwork_id: String,
    pub playing: bool,
    /// Nothing plays any more.
    pub ended: bool,
    pub source_display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MusicSettingsRecord {
    /// Music is on in the server's configuration file.
    pub supported: bool,
    pub enabled: bool,
    /// Supported and enabled: members see music.
    pub configured: bool,
    /// A Lavalink node is added: search reaches its sources.
    pub has_nodes: bool,
    /// The server takes uploaded songs.
    pub local_songs: bool,
    pub available_sources: Vec<String>,
    pub default_source: String,
    pub allow_links: bool,
    pub vote_skip: bool,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MusicSearchResultRecord {
    pub result_id: String,
    pub track: MusicTrackRecord,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct MusicPlaylistRecord {
    pub name: String,
    pub result_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MusicSearchResultsRecord {
    pub results: Vec<MusicSearchResultRecord>,
    pub playlist: Option<MusicPlaylistRecord>,
}
