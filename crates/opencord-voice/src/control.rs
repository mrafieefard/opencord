//! The control channel between the main server and an external voice node
//! (Phase 2 plan §3.4): what travels on it, and how a node proves it knows
//! its shared secret.

use std::time::Duration;

use hmac::{Hmac, KeyInit, Mac};
use internal::control_envelope::Payload;
use opencord_proto::internal::v1 as internal;
use sha2::Sha256;

use crate::node::{NodeCommand, NodeEvent};
use crate::sfu::PeerState;

/// Where the main server listens for nodes.
pub const CONTROL_PATH: &str = "/internal/voice";
/// How often a node reports its load.
pub const LOAD_EVERY: Duration = Duration::from_secs(5);
/// A node silent this long is gone.
pub const SILENT_FOR: Duration = Duration::from_secs(15);
/// Bytes of the challenge nonce.
pub const NONCE_LEN: usize = 32;

/// HMAC-SHA256 of the challenge, keyed with the shared secret.
pub fn proof(secret: &[u8], nonce: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("HMAC takes keys of any length");
    mac.update(nonce);
    mac.finalize().into_bytes().to_vec()
}

/// Whether `proof` answers the challenge, compared in constant time.
pub fn proof_matches(secret: &[u8], nonce: &[u8], proof: &[u8]) -> bool {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("HMAC takes keys of any length");
    mac.update(nonce);
    mac.verify_slice(proof).is_ok()
}

/// A command as it travels to an external node.
pub fn envelope_for(command: &NodeCommand) -> internal::ControlEnvelope {
    let payload = match command {
        NodeCommand::Update {
            user_id,
            channel_id,
            state,
            permissions,
        } => Payload::ParticipantUpdate(internal::ParticipantUpdate {
            user_id: *user_id,
            channel_id: *channel_id,
            permissions: *permissions,
            self_mute: state.self_mute,
            self_deaf: state.self_deaf,
            server_mute: state.server_mute,
            server_deaf: state.server_deaf,
            suppress: state.suppress,
        }),
        NodeCommand::Disconnect {
            user_id,
            channel_id,
        } => Payload::DisconnectParticipant(internal::DisconnectParticipant {
            user_id: *user_id,
            channel_id: *channel_id,
        }),
        NodeCommand::Limits { channel_id, limits } => {
            Payload::ChannelUpdate(internal::ChannelUpdate {
                channel_id: *channel_id,
                limits: Some(*limits),
            })
        }
    };
    internal::ControlEnvelope {
        payload: Some(payload),
    }
}

/// The command an envelope from the main server carries, if any.
pub fn command_from(envelope: internal::ControlEnvelope) -> Option<NodeCommand> {
    Some(match envelope.payload? {
        Payload::ParticipantUpdate(update) => NodeCommand::Update {
            user_id: update.user_id,
            channel_id: update.channel_id,
            state: PeerState {
                self_mute: update.self_mute,
                self_deaf: update.self_deaf,
                server_mute: update.server_mute,
                server_deaf: update.server_deaf,
                suppress: update.suppress,
            },
            permissions: update.permissions,
        },
        Payload::DisconnectParticipant(gone) => NodeCommand::Disconnect {
            user_id: gone.user_id,
            channel_id: gone.channel_id,
        },
        Payload::ChannelUpdate(update) => NodeCommand::Limits {
            channel_id: update.channel_id,
            limits: update.limits.unwrap_or_default(),
        },
        _ => return None,
    })
}

/// A node event as it travels to the main server.
pub fn envelope_for_event(event: &NodeEvent) -> internal::ControlEnvelope {
    let payload = match event {
        NodeEvent::Connected {
            user_id,
            channel_id,
            session_id,
        } => Payload::ParticipantConnected(internal::ParticipantConnected {
            user_id: *user_id,
            channel_id: *channel_id,
            session_id: session_id.clone(),
        }),
        NodeEvent::Disconnected {
            user_id,
            channel_id,
            session_id,
        } => Payload::ParticipantDisconnected(internal::ParticipantDisconnected {
            user_id: *user_id,
            channel_id: *channel_id,
            session_id: session_id.clone(),
        }),
    };
    internal::ControlEnvelope {
        payload: Some(payload),
    }
}

/// The node event an envelope from a node carries, if any.
pub fn event_from(envelope: &internal::ControlEnvelope) -> Option<NodeEvent> {
    match envelope.payload.as_ref()? {
        Payload::ParticipantConnected(connected) => Some(NodeEvent::Connected {
            user_id: connected.user_id,
            channel_id: connected.channel_id,
            session_id: connected.session_id.clone(),
        }),
        Payload::ParticipantDisconnected(gone) => Some(NodeEvent::Disconnected {
            user_id: gone.user_id,
            channel_id: gone.channel_id,
            session_id: gone.session_id.clone(),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use opencord_proto::voice::v1 as voice;

    use super::*;

    #[test]
    fn only_the_right_secret_answers_the_challenge() {
        let nonce = [7u8; NONCE_LEN];
        let answer = proof(b"shared secret", &nonce);

        assert!(proof_matches(b"shared secret", &nonce, &answer));
        assert!(!proof_matches(b"another secret", &nonce, &answer));
        assert!(!proof_matches(b"shared secret", &[8u8; NONCE_LEN], &answer));
        assert!(!proof_matches(b"shared secret", &nonce, &answer[..31]));
    }

    #[test]
    fn commands_survive_the_trip() {
        let commands = [
            NodeCommand::Update {
                user_id: 1,
                channel_id: 2,
                state: PeerState {
                    server_mute: true,
                    suppress: true,
                    ..PeerState::default()
                },
                permissions: 1 << 23,
            },
            NodeCommand::Disconnect {
                user_id: 1,
                channel_id: 2,
            },
            NodeCommand::Limits {
                channel_id: 2,
                limits: voice::Limits {
                    voice_bitrate: 64_000,
                    ..Default::default()
                },
            },
        ];

        for command in commands {
            assert_eq!(command_from(envelope_for(&command)), Some(command));
        }
    }

    #[test]
    fn events_survive_the_trip() {
        let events = [
            NodeEvent::Connected {
                user_id: 1,
                channel_id: 2,
                session_id: "s".to_owned(),
            },
            NodeEvent::Disconnected {
                user_id: 1,
                channel_id: 2,
                session_id: "s".to_owned(),
            },
        ];

        for event in events {
            assert_eq!(event_from(&envelope_for_event(&event)), Some(event));
        }
    }
}
