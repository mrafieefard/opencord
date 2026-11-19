//! Private messages and friends as JavaScript sees them (phone P3). Field
//! names are the core's (`opencord_core::api::types`). A `peer` is a public
//! key in lowercase hex and a message or device id is hex: they cross as the
//! core's strings, unchanged. User ids are decimal strings; times are
//! milliseconds since the Unix epoch as `f64`.

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct PrivateOpenedRecord {
    /// The old database couldn't be read with the key and was moved aside.
    pub started_fresh: bool,
    /// The database was from before chat keys: its chats were removed.
    pub keys_reset: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FriendStatus {
    None,
    Outgoing,
    Incoming,
    Friends,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ConversationStatus {
    None,
    Request,
    Accepted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PrivateMessageKind {
    Text,
    Divider,
    Moved,
    Undecryptable,
    /// "*Name*'s keys changed." or, outgoing, "Your keys changed."
    KeysChanged,
    /// "*Name* signed in on a new device."
    NewDevice,
    /// From a device whose chat key this device hasn't accepted; not shown.
    UnacceptedKeys,
}

/// Whether two people compared their words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PrivateVerification {
    Unverified,
    Verified,
    /// They said the words didn't match.
    Mismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum OwnDeviceState {
    /// Listed with another chat key.
    Other,
    /// It offered this device its chat key, waiting for They match.
    Offering,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PrivateSendState {
    Received,
    Sending,
    Sent,
    Failed,
}

/// 16 words to compare, and the keys they were made from (hex). Hand `keys`
/// back unchanged with the answer, so a tap acts only on what was shown.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct KeyWordsRecord {
    pub words: Vec<String>,
    pub keys: String,
}

/// Another device signed in as this person, with a chat key of its own.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct OwnDeviceRecord {
    /// Hex.
    pub device_id: String,
    pub state: OwnDeviceState,
}

/// This person's other devices with other chat keys, and whether this
/// device has any chats.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct OwnDevicesRecord {
    pub devices: Vec<OwnDeviceRecord>,
    pub has_chats: bool,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PrivateContactRecord {
    /// Their public key, lowercase hex.
    pub peer: String,
    /// The name last seen.
    pub name: Option<String>,
    pub friend: FriendStatus,
    pub request_spam: bool,
    pub blocked: bool,
    pub conversation: ConversationStatus,
    /// This device's key for the conversation's relay, if it saved that
    /// server.
    pub relay_server_key: Option<String>,
    pub last_activity_ms: Option<f64>,
    /// Incoming text after the last read.
    pub unread: u32,
    pub muted: bool,
    pub hidden: bool,
    /// The first line of the latest text, at most 100 characters.
    pub preview: Option<String>,
    pub verification: PrivateVerification,
    /// A device of theirs is signed by a chat key this device hasn't
    /// accepted.
    pub new_keys: bool,
    /// At the last look, a device of theirs had the accepted chat key.
    pub reachable: bool,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PrivateMessageRecord {
    /// Hex.
    pub id: String,
    pub peer: String,
    pub outgoing: bool,
    pub kind: PrivateMessageKind,
    /// The text; a divider's or card's server name; empty otherwise.
    pub content: String,
    /// A divider's or card's server, if this device saved it.
    pub server_key: Option<String>,
    pub sent_at_ms: f64,
    pub edited_at_ms: Option<f64>,
    pub state: PrivateSendState,
}

/// A server both people are on, where their messages can go.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SharedServerRecord {
    pub server_key: String,
    pub name: String,
    pub connected: bool,
    pub private_messages: bool,
    /// Messages go there only between friends.
    pub friends_only: bool,
    /// Their user id on that server, a decimal string.
    pub peer_user_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct PrivacySettingsRecord {
    pub allow_friend_requests: bool,
    pub allow_message_requests: bool,
}
