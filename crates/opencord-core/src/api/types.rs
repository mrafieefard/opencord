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
}

/// A hotkey: a key and the modifiers held with it, as the XDG shortcuts
/// specification writes them, such as `CTRL+SHIFT+m` or `F12`.
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
    /// Only while Opencord is focused, because of `reason`.
    FocusedOnly { reason: String },
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
}

/// Why a screen share could not start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenProblem {
    /// Not on this system yet (only Linux shares screens for now).
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
}
