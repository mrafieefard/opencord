//! Channel types plugins register, and whether each plugin channel can be
//! opened (plan Task 8, design 2026-11-07 §14.1 and §14.4).
//!
//! A plugin's types are judged only once its start has finished. While a
//! VM starts, what it registers is held aside and replaces the plugin's
//! list when the start finishes; the list from before stays meanwhile, so
//! a restart that registers the same types changes nothing anyone sees.
//!
//! Every change is made under the server's write lock and sent before it
//! is released, as `Ready` is built under it too: a session gets either the
//! old list and the change, or the new list.

use std::collections::{BTreeMap, HashMap};
use std::sync::{PoisonError, RwLock};
use std::time::Instant;

use opencord_common::channel::ChannelKind;
use opencord_plugins::limits::CHANNEL_TYPES_PER_PLUGIN;
use opencord_proto::v1 as proto;

use crate::gateway::session::PLUGIN_CHANNELS;
use crate::guild::PluginChannelRef;
use crate::state::{AppState, Audience};

/// A channel type as a plugin registered it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelType {
    pub name: String,
    pub description: String,
    /// A path under the plugin's `assets/`, or empty.
    pub icon: String,
    /// The manifest's name of the version that registered it.
    pub plugin_name: String,
}

/// The plugin already has [`CHANNEL_TYPES_PER_PLUGIN`] types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TooMany;

type Types = BTreeMap<String, ChannelType>;

#[derive(Debug, Default)]
struct Registered {
    /// What the plugin's channels are judged by: `Some` from the moment a
    /// start finished until the plugin stops.
    live: Option<Types>,
    /// What the VM now starting registered.
    starting: Option<Types>,
}

/// One plugin's types before and after a change.
#[derive(Debug)]
struct Change {
    plugin_id: String,
    before: Option<Types>,
    after: Option<Types>,
}

/// The registered channel types, by plugin id, then type key. A plugin's
/// entry leaves when it stops, when it's turned off after failing, when a
/// start fails, and when it's uninstalled; a restart replaces it once the
/// new VM's start finishes (a restarted plugin registers again as it
/// starts). Nothing is kept across server restarts.
#[derive(Debug, Default)]
pub struct ChannelTypes {
    plugins: RwLock<HashMap<String, Registered>>,
}

impl ChannelTypes {
    /// Whether a channel of this plugin and type can be opened.
    pub fn state(&self, plugin: &PluginChannelRef) -> proto::PluginChannelState {
        let plugins = self.plugins.read().unwrap_or_else(PoisonError::into_inner);
        let live = plugins
            .get(&plugin.plugin_id)
            .and_then(|registered| registered.live.as_ref());
        state_in(live, &plugin.type_key)
    }

    /// Whether this plugin currently registers `type_key` (either starting or live).
    pub fn is_registered_by(&self, plugin_id: &str, type_key: &str) -> bool {
        let plugins = self.plugins.read().unwrap_or_else(PoisonError::into_inner);
        let Some(registered) = plugins.get(plugin_id) else {
            return false;
        };
        registered
            .starting
            .as_ref()
            .is_some_and(|types| types.contains_key(type_key))
            || registered
                .live
                .as_ref()
                .is_some_and(|types| types.contains_key(type_key))
    }

    /// Every type of a running plugin, by plugin id, then key.
    pub fn list(&self) -> Vec<proto::PluginChannelType> {
        let plugins = self.plugins.read().unwrap_or_else(PoisonError::into_inner);
        let mut ids: Vec<&String> = plugins.keys().collect();
        ids.sort_unstable();
        ids.into_iter()
            .filter_map(|id| Some((id, plugins[id].live.as_ref()?)))
            .flat_map(|(id, types)| {
                types
                    .iter()
                    .map(|(key, channel_type)| proto::PluginChannelType {
                        plugin_id: id.clone(),
                        type_key: key.clone(),
                        name: channel_type.name.clone(),
                        description: channel_type.description.clone(),
                        icon: channel_type.icon.clone(),
                        plugin_name: channel_type.plugin_name.clone(),
                    })
            })
            .collect()
    }

    /// A VM of the plugin is starting: what it registers is held aside.
    pub(crate) fn begin(&self, plugin_id: &str) {
        self.write()
            .entry(plugin_id.to_owned())
            .or_default()
            .starting = Some(Types::new());
    }

    /// Registers `key`, replacing a type with the same key. Ignored when no
    /// VM of the plugin is starting or running.
    fn register(
        &self,
        plugin_id: &str,
        key: &str,
        channel_type: ChannelType,
    ) -> Result<Option<Change>, TooMany> {
        let mut plugins = self.write();
        let Some(registered) = plugins.get_mut(plugin_id) else {
            return Ok(None);
        };
        let before = registered.live.clone();
        let staged = registered.starting.is_some();
        let Some(types) = registered.starting.as_mut().or(registered.live.as_mut()) else {
            return Ok(None);
        };
        if !types.contains_key(key) && types.len() >= CHANNEL_TYPES_PER_PLUGIN {
            return Err(TooMany);
        }
        types.insert(key.to_owned(), channel_type);
        Ok((!staged).then(|| Change {
            plugin_id: plugin_id.to_owned(),
            before,
            after: registered.live.clone(),
        }))
    }

    /// The starting VM's start finished: what it registered is the list.
    fn finish(&self, plugin_id: &str) -> Change {
        let mut plugins = self.write();
        let registered = plugins.entry(plugin_id.to_owned()).or_default();
        let after = registered.starting.take().unwrap_or_default();
        let before = registered.live.replace(after.clone());
        Change {
            plugin_id: plugin_id.to_owned(),
            before,
            after: Some(after),
        }
    }

    /// The plugin no longer runs: its entry leaves.
    fn end(&self, plugin_id: &str) -> Change {
        let before = self
            .write()
            .remove(plugin_id)
            .and_then(|registered| registered.live);
        Change {
            plugin_id: plugin_id.to_owned(),
            before,
            after: None,
        }
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, HashMap<String, Registered>> {
        self.plugins.write().unwrap_or_else(PoisonError::into_inner)
    }
}

fn state_in(live: Option<&Types>, key: &str) -> proto::PluginChannelState {
    match live {
        Some(types) if types.contains_key(key) => proto::PluginChannelState::Available,
        Some(_) => proto::PluginChannelState::TypeGone,
        None => proto::PluginChannelState::PluginStopped,
    }
}

/// `ui.register_channel_type`: registers the type, and tells everyone once
/// the plugin's start has finished (before that, the start's end does).
pub async fn register(
    state: &AppState,
    plugin_id: &str,
    key: &str,
    channel_type: ChannelType,
) -> Result<(), TooMany> {
    let _writes = state.write_lock().await;
    let change = state
        .plugins
        .channel_types
        .register(plugin_id, key, channel_type)?;
    if let Some(change) = change {
        announce(state, &change);
    }
    Ok(())
}

/// The plugin's start finished: its new list replaces the old one.
pub async fn started(state: &AppState, plugin_id: &str) {
    let _writes = state.write_lock().await;
    let change = state.plugins.channel_types.finish(plugin_id);
    announce(state, &change);
}

/// The plugin stopped, was turned off, failed to start or was uninstalled:
/// its channels can't be opened.
pub async fn stopped(state: &AppState, plugin_id: &str) {
    let _writes = state.write_lock().await;
    let change = state.plugins.channel_types.end(plugin_id);
    announce(state, &change);
}

/// Sends apps that show plugin channels the new list, if it changed, and a
/// `ChannelUpdate` for each of the plugin's channels whose state changed.
/// The caller holds the write lock.
fn announce(state: &AppState, change: &Change) {
    let empty = Types::new();
    let before = change.before.as_ref();
    let after = change.after.as_ref();
    if before.unwrap_or(&empty) != after.unwrap_or(&empty) {
        let types = state.plugins.channel_types.list();
        tracing::info!(
            plugin = %change.plugin_id,
            types = after.map_or(0, BTreeMap::len),
            "a plugin's channel types changed"
        );
        let event = proto::Event {
            kind: Some(proto::event::Kind::PluginChannelTypesUpdate(
                proto::PluginChannelTypesUpdate { types },
            )),
        };
        let now = Instant::now();
        for session in state.sessions.all() {
            if session.has_feature(PLUGIN_CHANNELS) {
                session.push_event(&event, now);
            }
        }
    }
    let updated: Vec<proto::Channel> = {
        let guild = state.guild();
        guild
            .channels
            .values()
            .filter(|channel| {
                channel.kind == ChannelKind::Plugin
                    && channel.plugin.as_ref().is_some_and(|plugin| {
                        plugin.plugin_id == change.plugin_id
                            && state_in(before, &plugin.type_key)
                                != state_in(after, &plugin.type_key)
                    })
            })
            .map(|channel| channel.to_proto(&state.plugins.channel_types))
            .collect()
    };
    for channel in updated {
        let channel_id = channel.id;
        let now = channel.plugin_state();
        if now != proto::PluginChannelState::Available {
            super::views::channel_unavailable(state, channel_id, now);
        }
        state.broadcast(
            proto::event::Kind::ChannelUpdate(proto::ChannelUpdate {
                channel: Some(channel),
            }),
            Audience::Channel(channel_id),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tickets(name: &str) -> ChannelType {
        ChannelType {
            name: name.to_owned(),
            description: String::new(),
            icon: String::new(),
            plugin_name: "Tickets".to_owned(),
        }
    }

    fn reference(key: &str) -> PluginChannelRef {
        PluginChannelRef {
            plugin_id: "tickets".to_owned(),
            type_key: key.to_owned(),
        }
    }

    #[test]
    fn types_count_only_once_the_start_finishes() {
        // Fails if a starting VM's types are judged before its start
        // finishes, or a restart drops the old list while it starts.
        let types = ChannelTypes::default();
        types.begin("tickets");
        types.register("tickets", "desk", tickets("Desk")).unwrap();
        assert_eq!(
            types.state(&reference("desk")),
            proto::PluginChannelState::PluginStopped
        );
        let first = types.finish("tickets");
        assert!(first.before.is_none());
        assert_eq!(
            types.state(&reference("desk")),
            proto::PluginChannelState::Available
        );
        assert_eq!(
            types.state(&reference("other")),
            proto::PluginChannelState::TypeGone
        );

        types.begin("tickets");
        assert_eq!(
            types.state(&reference("desk")),
            proto::PluginChannelState::Available,
            "a restart keeps the list until its start finishes"
        );
        types.register("tickets", "desk", tickets("Desk")).unwrap();
        let again = types.finish("tickets");
        assert_eq!(again.before, again.after);

        types.end("tickets");
        assert_eq!(
            types.state(&reference("desk")),
            proto::PluginChannelState::PluginStopped
        );
        assert!(types.list().is_empty());
    }

    #[test]
    fn a_running_plugin_registers_straight_into_its_list() {
        // Fails if a type registered after the start waits for a start
        // that never comes.
        let types = ChannelTypes::default();
        types.begin("tickets");
        types.finish("tickets");
        let change = types
            .register("tickets", "desk", tickets("Desk"))
            .unwrap()
            .expect("a change to announce");
        assert_eq!(change.before, Some(Types::new()));
        assert_eq!(types.list().len(), 1);
    }

    #[test]
    fn the_21st_type_is_too_many_but_a_replacement_is_not() {
        // Fails if the count ignores the limit or counts a replacement.
        let types = ChannelTypes::default();
        types.begin("tickets");
        for index in 0..CHANNEL_TYPES_PER_PLUGIN {
            types
                .register("tickets", &format!("t{index}"), tickets("T"))
                .unwrap();
        }
        assert_eq!(
            types
                .register("tickets", "one_more", tickets("T"))
                .unwrap_err(),
            TooMany
        );
        types.register("tickets", "t0", tickets("Renamed")).unwrap();
        types.finish("tickets");
        let list = types.list();
        assert_eq!(list.len(), CHANNEL_TYPES_PER_PLUGIN);
        assert_eq!(list[0].name, "Renamed");
    }

    #[test]
    fn a_plugin_neither_starting_nor_running_registers_nothing() {
        // Fails if a late call from a stopped VM brings its types back.
        let types = ChannelTypes::default();
        assert!(
            types
                .register("tickets", "desk", tickets("Desk"))
                .unwrap()
                .is_none()
        );
        assert!(types.list().is_empty());
    }
}
