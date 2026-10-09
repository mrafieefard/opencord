//! Voice on the main server: settings, voice states and the tokens that
//! send clients to a voice node (Phase 2 plan §3–§5).

use std::collections::HashSet;
use std::time::Instant;

use opencord_common::limits::{VOICE_GRACE_PERIOD, VOICE_TOKEN_LIFETIME};
use opencord_proto::internal::v1::VoiceTokenClaims;
use opencord_proto::v1 as proto;
use opencord_voice::node::{NodeCommand, NodeEvent};
use opencord_voice::sfu::PeerState;
use proto::event::Kind;

use crate::guild::Guild;
use crate::random;
use crate::state::{AppState, now_ms};
use states::{VoiceConfig, VoiceState, VoiceStates};

pub mod control;
pub mod keys;
pub mod media_token;
pub mod nodes;
pub mod settings;
pub mod states;
pub mod streams;

/// The configuration's voice limits.
pub fn config(state: &AppState) -> VoiceConfig {
    VoiceConfig {
        enabled: state.config.voice.enabled,
        max_participants_per_channel: state.config.voice.max_participants_per_channel,
    }
}

/// Sends a voice state to every session that can view one of `channels`,
/// and to the user's own sessions, and tells the voice nodes: a leave or a
/// move disconnects the old channel's connection, anything else updates it.
pub fn announce(state: &AppState, voice_state: proto::VoiceState, channels: &[i64]) {
    let user_id = voice_state.user_id;
    let current = voice_state.channel_id;
    let peer = PeerState {
        self_mute: voice_state.self_mute,
        self_deaf: voice_state.self_deaf,
        server_mute: voice_state.server_mute,
        server_deaf: voice_state.server_deaf,
        suppress: voice_state.suppress,
    };
    let event = proto::Event {
        kind: Some(Kind::VoiceStateUpdate(proto::VoiceStateUpdate {
            voice_state: Some(voice_state),
        })),
    };
    let now = Instant::now();
    let permissions = {
        let guild = state.guild();
        for session in state.sessions.all() {
            let sees = session.user_id == user_id
                || channels
                    .iter()
                    .any(|channel_id| guild.can_view(session.user_id, *channel_id));
            if sees {
                session.push_event(&event, now);
            }
        }
        current.map(|channel_id| guild.channel_permissions(user_id, channel_id).bits())
    };
    for left in channels.iter().copied().filter(|c| Some(*c) != current) {
        disconnect_from_node(state, user_id, left);
        streams::left_channel(state, user_id, left);
    }
    if let (Some(channel_id), Some(permissions)) = (current, permissions) {
        state.voice_nodes.send(
            channel_id,
            NodeCommand::Update {
                user_id,
                channel_id,
                state: peer,
                permissions,
            },
        );
    }
}

/// Ends the user's media connection in `channel_id`; an empty channel may
/// go to another node next time.
fn disconnect_from_node(state: &AppState, user_id: i64, channel_id: i64) {
    state.voice_nodes.send(
        channel_id,
        NodeCommand::Disconnect {
            user_id,
            channel_id,
        },
    );
    let empty = state.voice().in_channel(channel_id).next().is_none();
    if empty {
        state.voice_nodes.release(channel_id);
    }
}

/// Tells the voice nodes about changed channel limits: a channel's bitrate
/// or user limit, or the server's voice settings.
pub fn limits_changed(state: &AppState, channel_ids: &[i64]) {
    let assigned = state.voice_nodes.assigned();
    let commands: Vec<(i64, NodeCommand)> = {
        let guild = state.guild();
        channel_ids
            .iter()
            .filter(|channel_id| assigned.contains(channel_id))
            .filter_map(|channel_id| {
                let channel = guild.channels.get(channel_id)?;
                Some((
                    *channel_id,
                    NodeCommand::Limits {
                        channel_id: *channel_id,
                        limits: states::limits(&guild, channel),
                    },
                ))
            })
            .collect()
    };
    for (channel_id, command) in commands {
        state.voice_nodes.send(channel_id, command);
    }
}

/// What a voice node reports: a voice connection that ended for good ends
/// its voice state, if the state is still that connection's.
pub async fn on_node_event(state: &AppState, event: NodeEvent) {
    let NodeEvent::Disconnected {
        user_id,
        channel_id,
        session_id,
    } = event
    else {
        return;
    };
    let _writes = state.write_lock().await;
    let removed = {
        let mut voice = state.voice();
        let current = voice
            .get(user_id)
            .is_some_and(|vs| vs.channel_id == channel_id && vs.session_id == session_id);
        if current { voice.remove(user_id) } else { None }
    };
    if let Some(removed) = removed {
        announce(
            state,
            states::left(removed.user_id, &removed.session_id),
            &[channel_id],
        );
    }
}

/// Where `voice_state`'s session connects for voice, with a fresh token;
/// `None` when no voice node is available.
pub fn server_update(
    state: &AppState,
    guild: &Guild,
    voice: &VoiceStates,
    voice_state: &VoiceState,
) -> Option<proto::VoiceServerUpdate> {
    let link = state.voice_nodes.link_for(voice_state.channel_id)?;
    let moderation = voice.moderation(voice_state.user_id);
    let lifetime = i64::try_from(VOICE_TOKEN_LIFETIME.as_millis()).unwrap_or(i64::MAX);
    let claims = VoiceTokenClaims {
        token_id: random::bytes::<16>().to_vec(),
        user_id: voice_state.user_id,
        channel_id: voice_state.channel_id,
        session_id: voice_state.session_id.clone(),
        permissions: guild
            .channel_permissions(voice_state.user_id, voice_state.channel_id)
            .bits(),
        limits: guild
            .channels
            .get(&voice_state.channel_id)
            .map(|channel| states::limits(guild, channel)),
        self_mute: voice_state.self_mute,
        self_deaf: voice_state.self_deaf,
        server_mute: moderation.mute,
        server_deaf: moderation.deaf,
        suppress: voice_state.suppress,
        expires_at_ms: now_ms().saturating_add(lifetime),
    };
    Some(proto::VoiceServerUpdate {
        channel_id: voice_state.channel_id,
        endpoint: link.endpoint,
        certificate_fingerprint: link.fingerprint.to_vec(),
        token: opencord_voice::token::issue(&state.voice_key, &claims),
    })
}

/// Gives every voice state whose channel has no node one, with a fresh
/// token: after a node went away, or when one registers. While no node is
/// available, people keep their voice state and wait.
pub async fn place_unassigned(state: &AppState) {
    let _writes = state.write_lock().await;
    let assigned: HashSet<i64> = state.voice_nodes.assigned().into_iter().collect();
    let updates: Vec<(String, proto::VoiceServerUpdate)> = {
        let guild = state.guild();
        let voice = state.voice();
        voice
            .all()
            .filter(|vs| !assigned.contains(&vs.channel_id))
            .filter_map(|vs| {
                let update = server_update(state, &guild, &voice, vs)?;
                Some((vs.session_id.clone(), update))
            })
            .collect()
    };
    for (session_id, update) in updates {
        send_server_update(state, &session_id, update);
    }
}

/// Sends `update` to one session only.
pub fn send_server_update(state: &AppState, session_id: &str, update: proto::VoiceServerUpdate) {
    if let Some(session) = state.sessions.get(session_id) {
        let event = proto::Event {
            kind: Some(Kind::VoiceServerUpdate(update)),
        };
        session.push_event(&event, Instant::now());
    }
}

/// After permissions, members, channels or settings change: ends the voice
/// states of users who can no longer view their channel, and announces who
/// became (un)suppressed.
pub fn reconcile(state: &AppState) {
    let mut announcements = Vec::new();
    {
        let guild = state.guild();
        let mut voice = state.voice();
        let current: Vec<VoiceState> = voice.all().cloned().collect();
        for voice_state in current {
            if !guild.can_view(voice_state.user_id, voice_state.channel_id) {
                voice.remove(voice_state.user_id);
                announcements.push((
                    states::left(voice_state.user_id, &voice_state.session_id),
                    voice_state.channel_id,
                ));
                continue;
            }
            let suppress = states::suppressed(&guild, voice_state.user_id, voice_state.channel_id);
            if suppress != voice_state.suppress {
                let updated = VoiceState {
                    suppress,
                    ..voice_state
                };
                announcements.push((voice.to_proto(&updated), updated.channel_id));
                voice.put(updated);
            }
        }
    }
    for (voice_state, channel_id) in announcements {
        announce(state, voice_state, &[channel_id]);
    }
    // Permissions may have changed without anyone becoming suppressed:
    // the nodes need them (priority speaker).
    let updates: Vec<(i64, NodeCommand)> = {
        let guild = state.guild();
        let voice = state.voice();
        voice
            .all()
            .map(|vs| {
                let moderation = voice.moderation(vs.user_id);
                (
                    vs.channel_id,
                    NodeCommand::Update {
                        user_id: vs.user_id,
                        channel_id: vs.channel_id,
                        state: PeerState {
                            self_mute: vs.self_mute,
                            self_deaf: vs.self_deaf,
                            server_mute: moderation.mute,
                            server_deaf: moderation.deaf,
                            suppress: vs.suppress,
                        },
                        permissions: guild.channel_permissions(vs.user_id, vs.channel_id).bits(),
                    },
                )
            })
            .collect()
    };
    for (channel_id, command) in updates {
        state.voice_nodes.send(channel_id, command);
    }
    streams::reconcile(state);
}

/// Ends voice states whose session is gone, or has been without a
/// connection for longer than the grace period. Callers hold the write lock.
pub fn expire_detached(state: &AppState, now: Instant) {
    let mut gone = Vec::new();
    {
        let mut voice = state.voice();
        let expired: Vec<VoiceState> = voice
            .all()
            .filter(
                |voice_state| match state.sessions.get(&voice_state.session_id) {
                    None => true,
                    Some(session) => {
                        session.is_ended()
                            || session
                                .detached_for(now)
                                .is_some_and(|detached| detached > VOICE_GRACE_PERIOD)
                    }
                },
            )
            .cloned()
            .collect();
        for voice_state in expired {
            voice.remove(voice_state.user_id);
            gone.push(voice_state);
        }
    }
    for voice_state in gone {
        announce(
            state,
            states::left(voice_state.user_id, &voice_state.session_id),
            &[voice_state.channel_id],
        );
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use ed25519_dalek::SigningKey;
    use opencord_common::snowflake::SnowflakeGenerator;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::config::Config;
    use crate::db::test_pool;
    use crate::gateway::session::{CloseHandle, Connection, Session};
    use crate::guild::tests::sample_guild;

    const ALICE: i64 = 2;
    const CHANNEL: i64 = 77;

    async fn app_state() -> (AppState, tempfile::TempDir) {
        let (pool, dir) = test_pool().await;
        let state = AppState::new(
            Config::default(),
            pool,
            SnowflakeGenerator::new(0).unwrap(),
            [0; 32],
            sample_guild().await,
            SigningKey::from_bytes(&[1; 32]),
            nodes::VoiceNodes::new(None, [0; 32]),
        );
        (state, dir)
    }

    fn session(id: &str) -> Arc<Session> {
        let (outbound, _frames) = mpsc::channel(16);
        let connection = Connection {
            id: 1,
            outbound,
            closer: CloseHandle::new(CancellationToken::new()),
        };
        Arc::new(Session::new(id.to_owned(), ALICE, b"token", connection))
    }

    fn in_voice(session_id: &str) -> VoiceState {
        VoiceState {
            user_id: ALICE,
            channel_id: CHANNEL,
            session_id: session_id.to_owned(),
            self_mute: false,
            self_deaf: false,
            self_video: false,
            self_stream: false,
            suppress: false,
        }
    }

    #[tokio::test]
    async fn voice_outlives_a_dropped_connection_for_the_grace_period() {
        let (state, _dir) = app_state().await;
        let dropped = session("dropped");
        state.sessions.insert(Arc::clone(&dropped));
        state.voice().put(in_voice("dropped"));
        let start = Instant::now();
        dropped.detach(1, start);

        expire_detached(&state, start + VOICE_GRACE_PERIOD);
        assert!(state.voice().get(ALICE).is_some());

        expire_detached(
            &state,
            start + VOICE_GRACE_PERIOD + Duration::from_millis(1),
        );
        assert!(state.voice().get(ALICE).is_none());
    }

    #[tokio::test]
    async fn voice_held_by_a_connected_session_stays() {
        let (state, _dir) = app_state().await;
        state.sessions.insert(session("live"));
        state.voice().put(in_voice("live"));

        expire_detached(&state, Instant::now() + Duration::from_secs(3_600));

        assert!(state.voice().get(ALICE).is_some());
    }

    #[tokio::test]
    async fn voice_of_a_forgotten_session_ends() {
        let (state, _dir) = app_state().await;
        state.voice().put(in_voice("gone"));

        expire_detached(&state, Instant::now());

        assert!(state.voice().get(ALICE).is_none());
    }
}
