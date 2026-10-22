//! Where to connect for voice. The media engine needs it; the app does not.

/// A `VoiceServerUpdate`, with the session and user it was sent to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceServer {
    pub server_key: String,
    pub user_id: i64,
    /// The main gateway session that joined.
    pub session_id: String,
    pub channel_id: i64,
    /// `wss://` URL of the voice gateway; empty for the server's own node.
    pub endpoint: String,
    /// SHA-256 of the voice node's certificate, to pin.
    pub certificate_fingerprint: Vec<u8>,
    /// Single use, valid for 60 seconds.
    pub token: Vec<u8>,
}

impl VoiceServer {
    /// The voice gateway to connect to: the endpoint, or the main server's
    /// own at `/voice`.
    pub fn gateway_url(&self) -> String {
        if self.endpoint.is_empty() {
            format!("wss://{}/voice", self.server_key)
        } else {
            self.endpoint.clone()
        }
    }
}
