//! The control channel between the main server and an external voice node
//! (Phase 2 plan §3.4): what travels on it, and how a node proves it knows
//! its shared secret.

use std::time::Duration;

use hmac::{Hmac, KeyInit, Mac};
use internal::control_envelope::Payload;
use opencord_proto::internal::v1 as internal;
use sha2::Sha256;

use crate::bridge::{BridgeEvent, ExpectedPlayer};
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
        NodeCommand::StreamViewers {
            channel_id,
            user_id,
            viewers,
        } => Payload::StreamViewers(internal::StreamViewers {
            channel_id: *channel_id,
            user_id: *user_id,
            viewer_ids: viewers.clone(),
        }),
        NodeCommand::StopMusic { channel_id } => {
            Payload::CancelMusicBridge(internal::CancelMusicBridge {
                channel_id: *channel_id,
            })
        }
        NodeCommand::ExpectMusic(player) => {
            Payload::ExpectMusicBridge(internal::ExpectMusicBridge {
                channel_id: player.channel_id,
                user_id: player.user_id,
                token: player.token.clone(),
                session_id: player.session_id.clone(),
                strict: player.allowed.is_some(),
                lavalink_addresses: player
                    .allowed
                    .iter()
                    .flatten()
                    .map(ToString::to_string)
                    .collect(),
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
        Payload::StreamViewers(viewers) => NodeCommand::StreamViewers {
            channel_id: viewers.channel_id,
            user_id: viewers.user_id,
            viewers: viewers.viewer_ids,
        },
        Payload::CancelMusicBridge(cancel) => NodeCommand::StopMusic {
            channel_id: cancel.channel_id,
        },
        Payload::ExpectMusicBridge(expect) => NodeCommand::ExpectMusic(ExpectedPlayer {
            channel_id: expect.channel_id,
            user_id: expect.user_id,
            token: expect.token,
            session_id: expect.session_id,
            allowed: expect.strict.then(|| {
                expect
                    .lavalink_addresses
                    .iter()
                    .filter_map(|address| address.parse().ok())
                    .collect()
            }),
        }),
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
        NodeEvent::Spoke {
            user_id,
            channel_id,
        } => Payload::SpeakingActivity(internal::SpeakingActivity {
            entries: vec![internal::speaking_activity::Entry {
                user_id: *user_id,
                channel_id: *channel_id,
                last_spoke_at_ms: unix_ms(),
            }],
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
        // One entry per report; the time is the receiver's own.
        Payload::SpeakingActivity(activity) => {
            activity.entries.first().map(|entry| NodeEvent::Spoke {
                user_id: entry.user_id,
                channel_id: entry.channel_id,
            })
        }
        _ => None,
    }
}

fn unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        })
}

/// What a node's bridge saw, as it travels to the main server.
pub fn envelope_for_bridge_event(event: &BridgeEvent) -> internal::ControlEnvelope {
    use internal::music_bridge_status::Kind;
    let mut status = internal::MusicBridgeStatus {
        channel_id: event.channel_id(),
        ..Default::default()
    };
    let kind = match event {
        BridgeEvent::Connected { connection, .. } => {
            status.connection = *connection;
            Kind::Connected
        }
        BridgeEvent::Closed { code, .. } => {
            status.code = u32::from(*code);
            Kind::Closed
        }
        BridgeEvent::Discovered { from, .. } => {
            status.address = from.to_string();
            Kind::Discovered
        }
        BridgeEvent::Refused { peer, .. } => {
            status.address = peer.to_string();
            Kind::Refused
        }
        BridgeEvent::Audio { .. } => Kind::Audio,
        BridgeEvent::Silent { .. } => Kind::Silent,
    };
    status.kind = kind as i32;
    internal::ControlEnvelope {
        payload: Some(Payload::MusicBridgeStatus(status)),
    }
}

/// The bridge event a node reported, if it reads as one.
pub fn bridge_event_from(status: &internal::MusicBridgeStatus) -> Option<BridgeEvent> {
    use internal::music_bridge_status::Kind;
    let channel_id = status.channel_id;
    let address = || status.address.parse().ok();
    Some(match Kind::try_from(status.kind).ok()? {
        Kind::Connected => BridgeEvent::Connected {
            channel_id,
            connection: status.connection,
        },
        Kind::Closed => BridgeEvent::Closed {
            channel_id,
            code: u16::try_from(status.code).ok()?,
        },
        Kind::Discovered => BridgeEvent::Discovered {
            channel_id,
            from: address()?,
        },
        Kind::Refused => BridgeEvent::Refused {
            channel_id,
            peer: address()?,
        },
        Kind::Audio => BridgeEvent::Audio { channel_id },
        Kind::Silent => BridgeEvent::Silent { channel_id },
        Kind::Unspecified => return None,
    })
}

#[cfg(test)]
mod tests {
    use opencord_proto::voice::v1 as voice;

    use super::*;

    #[test]
    fn speaking_travels_to_the_main_server_and_back() {
        let spoke = NodeEvent::Spoke {
            user_id: 4,
            channel_id: 9,
        };

        let envelope = envelope_for_event(&spoke);

        assert!(matches!(
            envelope.payload,
            Some(Payload::SpeakingActivity(ref activity))
                if activity.entries.len() == 1 && activity.entries[0].last_spoke_at_ms > 0
        ));
        assert_eq!(event_from(&envelope), Some(spoke));
    }

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
            NodeCommand::StreamViewers {
                channel_id: 2,
                user_id: 1,
                viewers: vec![3, 4],
            },
            NodeCommand::StopMusic { channel_id: 2 },
            NodeCommand::ExpectMusic(ExpectedPlayer {
                channel_id: 2,
                user_id: 77,
                token: "t".to_owned(),
                session_id: "s".to_owned(),
                allowed: Some(vec!["203.0.113.5".parse().unwrap()]),
            }),
            NodeCommand::ExpectMusic(ExpectedPlayer {
                channel_id: 2,
                user_id: 77,
                token: "t".to_owned(),
                session_id: "s".to_owned(),
                allowed: None,
            }),
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

    #[test]
    fn bridge_events_survive_the_trip() {
        let ip: std::net::IpAddr = "198.51.100.7".parse().unwrap();
        for event in [
            BridgeEvent::Connected {
                channel_id: 2,
                connection: 9,
            },
            BridgeEvent::Closed {
                channel_id: 2,
                code: 4006,
            },
            BridgeEvent::Discovered {
                channel_id: 2,
                from: ip,
            },
            BridgeEvent::Refused {
                channel_id: 2,
                peer: ip,
            },
            BridgeEvent::Audio { channel_id: 2 },
            BridgeEvent::Silent { channel_id: 2 },
        ] {
            let Some(Payload::MusicBridgeStatus(status)) =
                envelope_for_bridge_event(&event).payload
            else {
                panic!("not a bridge status");
            };
            assert_eq!(bridge_event_from(&status), Some(event));
        }
    }
}
