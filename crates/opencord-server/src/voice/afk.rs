//! AFK moves (Phase 2 plan §12): someone who has not spoken, and whose
//! device reports no keyboard or mouse input, for the server's AFK timeout
//! is moved to the AFK channel, where they are suppressed. Someone
//! streaming is never moved.

use std::time::{Duration, Instant};

use super::states::{Activity, VoiceState, suppressed};
use super::{announce, send_server_update, server_update};
use crate::state::AppState;

/// Whether `activity` is idle enough to move, `timeout` being the server's.
pub fn is_idle(activity: Activity, timeout: Duration, now: Instant) -> bool {
    let quiet = now.saturating_duration_since(activity.active_at) >= timeout;
    let away = activity
        .input_idle_since
        .is_some_and(|since| now.saturating_duration_since(since) >= timeout);
    quiet && away
}

/// Moves everyone idle at `now` to the AFK channel. Callers hold the write
/// lock.
pub fn move_idle(state: &AppState, now: Instant) {
    let moves: Vec<(VoiceState, i64, opencord_proto::v1::VoiceServerUpdate)> = {
        let guild = state.guild();
        let settings = &guild.voice_settings;
        let Some(afk) = settings
            .afk_channel_id
            .filter(|id| guild.channels.contains_key(id))
        else {
            return;
        };
        let timeout = Duration::from_secs(u64::from(settings.afk_timeout_s));
        let mut voice = state.voice();
        let idle: Vec<VoiceState> = voice
            .all()
            .filter(|voice_state| voice_state.channel_id != afk && !voice_state.self_stream)
            .filter(|voice_state| {
                voice
                    .activity(voice_state.user_id)
                    .is_some_and(|activity| is_idle(activity, timeout, now))
            })
            .cloned()
            .collect();
        let mut moves = Vec::new();
        for current in idle {
            let moved = VoiceState {
                channel_id: afk,
                suppress: suppressed(&guild, current.user_id, afk),
                ..current.clone()
            };
            let Some(update) = server_update(state, &guild, &voice, &moved) else {
                continue;
            };
            voice.put(moved.clone());
            moves.push((moved, current.channel_id, update));
        }
        moves
    };
    for (moved, from, update) in moves {
        let shown = state.voice().to_proto(&moved);
        tracing::info!(user_id = moved.user_id, from, "moved to the AFK channel");
        announce(state, shown, &[from, moved.channel_id]);
        send_server_update(state, &moved.session_id, update);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: Duration = Duration::from_secs(60);

    #[test]
    fn idle_needs_both_silence_and_no_input_for_the_timeout() {
        let start = Instant::now();
        let later = start + 5 * MINUTE;
        let activity = |spoke: Duration, idle: Option<Duration>| Activity {
            active_at: start + spoke,
            input_idle_since: idle.map(|idle| start + idle),
        };

        assert!(is_idle(
            activity(Duration::ZERO, Some(Duration::ZERO)),
            5 * MINUTE,
            later
        ));
        // Spoke a minute ago.
        assert!(!is_idle(
            activity(4 * MINUTE, Some(Duration::ZERO)),
            5 * MINUTE,
            later
        ));
        // Input a minute ago.
        assert!(!is_idle(
            activity(Duration::ZERO, Some(4 * MINUTE)),
            5 * MINUTE,
            later
        ));
        // A device that never said it was idle is never moved.
        assert!(!is_idle(
            activity(Duration::ZERO, None),
            5 * MINUTE,
            later + 60 * MINUTE
        ));
    }
}
