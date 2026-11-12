//! Which channels each connected user can see, so that permission changes
//! can be announced as channels appearing or disappearing.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::time::Instant;

use opencord_common::channel::ChannelKind;
use opencord_proto::v1 as proto;
use proto::event::Kind;

use crate::plugins::events::Scope;
use crate::state::AppState;

#[derive(Debug)]
pub struct Visibility {
    by_user: HashMap<i64, HashSet<i64>>,
    /// Plugin channels, kept so a deleted one's `ChannelDelete` still
    /// skips apps that never heard of it.
    plugin_channels: HashSet<i64>,
}

impl Visibility {
    /// Visible channels of every user with a session, and of every plugin
    /// taking events. Take it before changing anything; the guild must not
    /// be locked by the caller.
    pub fn capture(state: &AppState) -> Self {
        let users: HashSet<i64> = state
            .sessions
            .all()
            .iter()
            .map(|session| session.user_id)
            .chain(state.plugins.listening_user_ids())
            .collect();
        let guild = state.guild();
        Self {
            by_user: users
                .into_iter()
                .map(|user_id| (user_id, guild.visible_channel_ids(user_id)))
                .collect(),
            plugin_channels: guild
                .channels
                .values()
                .filter(|channel| channel.kind == ChannelKind::Plugin)
                .map(|channel| channel.id)
                .collect(),
        }
    }

    /// Compares with the current state and tells each user about channels
    /// that appeared (`ChannelCreate`, then who is in them if they are voice
    /// channels) or disappeared (`ChannelDelete`) for them, plus
    /// `ChannelUpdate` for `updated` channels they still see. Apps that don't
    /// show plugin channels hear nothing about them. Then ends or updates
    /// voice states the change affects.
    pub fn announce(self, state: &AppState, updated: &[i64]) {
        let after = Self::capture(state);
        let mut deliveries = Vec::new();
        {
            let guild = state.guild();
            let voice = state.voice();
            for (user_id, visible_now) in &after.by_user {
                let empty = HashSet::new();
                let visible_before = self.by_user.get(user_id).unwrap_or(&empty);
                let removed: BTreeSet<i64> =
                    visible_before.difference(visible_now).copied().collect();
                let added: BTreeSet<i64> =
                    visible_now.difference(visible_before).copied().collect();
                for channel_id in removed {
                    deliveries.push((
                        *user_id,
                        channel_id,
                        Kind::ChannelDelete(proto::ChannelDelete { channel_id }),
                    ));
                }
                for channel_id in added {
                    if let Some(channel) = guild.channels.get(&channel_id) {
                        deliveries.push((
                            *user_id,
                            channel_id,
                            Kind::ChannelCreate(proto::ChannelCreate {
                                channel: Some(channel.to_proto(&state.plugins.channel_types)),
                            }),
                        ));
                        for voice_state in voice.in_channel(channel_id) {
                            deliveries.push((
                                *user_id,
                                channel_id,
                                Kind::VoiceStateUpdate(proto::VoiceStateUpdate {
                                    voice_state: Some(voice.to_proto(voice_state)),
                                }),
                            ));
                        }
                    }
                }
                for channel_id in updated {
                    let still_visible =
                        visible_before.contains(channel_id) && visible_now.contains(channel_id);
                    if let Some(channel) = guild.channels.get(channel_id).filter(|_| still_visible)
                    {
                        deliveries.push((
                            *user_id,
                            *channel_id,
                            Kind::ChannelUpdate(proto::ChannelUpdate {
                                channel: Some(channel.to_proto(&state.plugins.channel_types)),
                            }),
                        ));
                    }
                }
            }
        }
        {
            let guild = state.guild();
            for (user_id, _, kind) in &deliveries {
                if guild.plugin_users.contains_key(user_id) {
                    state
                        .plugins
                        .observe_in(&guild, kind, Scope::User(*user_id));
                }
            }
        }
        let now = Instant::now();
        for (user_id, channel_id, kind) in deliveries {
            let plugin_channel = self.plugin_channels.contains(&channel_id)
                || after.plugin_channels.contains(&channel_id);
            let event = proto::Event { kind: Some(kind) };
            for session in state.sessions.for_user(user_id) {
                if !plugin_channel || session.sees_kind(ChannelKind::Plugin) {
                    session.push_event(&event, now);
                }
            }
        }
        crate::plugins::views::check_access(state);
        crate::voice::reconcile(state);
    }
}
