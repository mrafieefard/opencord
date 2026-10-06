//! Which channels each connected user can see, so that permission changes
//! can be announced as channels appearing or disappearing.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::time::Instant;

use opencord_proto::v1 as proto;
use proto::event::Kind;

use crate::state::AppState;

#[derive(Debug)]
pub struct Visibility {
    by_user: HashMap<i64, HashSet<i64>>,
}

impl Visibility {
    /// Visible channels of every user with a session. Take it before
    /// changing anything; the guild must not be locked by the caller.
    pub fn capture(state: &AppState) -> Self {
        let users: HashSet<i64> = state
            .sessions
            .all()
            .iter()
            .map(|session| session.user_id)
            .collect();
        let guild = state.guild();
        Self {
            by_user: users
                .into_iter()
                .map(|user_id| (user_id, guild.visible_channel_ids(user_id)))
                .collect(),
        }
    }

    /// Compares with the current state and tells each user about channels
    /// that appeared (`ChannelCreate`) or disappeared (`ChannelDelete`) for
    /// them, plus `ChannelUpdate` for `updated` channels they still see.
    pub fn announce(self, state: &AppState, updated: &[i64]) {
        let after = Self::capture(state);
        let mut deliveries = Vec::new();
        {
            let guild = state.guild();
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
                        Kind::ChannelDelete(proto::ChannelDelete { channel_id }),
                    ));
                }
                for channel_id in added {
                    if let Some(channel) = guild.channels.get(&channel_id) {
                        deliveries.push((
                            *user_id,
                            Kind::ChannelCreate(proto::ChannelCreate {
                                channel: Some(channel.to_proto()),
                            }),
                        ));
                    }
                }
                for channel_id in updated {
                    let still_visible =
                        visible_before.contains(channel_id) && visible_now.contains(channel_id);
                    if let Some(channel) = guild.channels.get(channel_id).filter(|_| still_visible)
                    {
                        deliveries.push((
                            *user_id,
                            Kind::ChannelUpdate(proto::ChannelUpdate {
                                channel: Some(channel.to_proto()),
                            }),
                        ));
                    }
                }
            }
        }
        let now = Instant::now();
        for (user_id, kind) in deliveries {
            let event = proto::Event { kind: Some(kind) };
            for session in state.sessions.for_user(user_id) {
                session.push_event(&event, now);
            }
        }
    }
}
