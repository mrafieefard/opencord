//! Activities plugins register (plan M3 Task 3, design 2026-11-07 §15.1).
//!
//! Registered the way channel types are (`channel_types.rs`): while a VM
//! starts, what it registers is held aside and replaces the plugin's list
//! when the start finishes; the list from before stays meanwhile, so a
//! restart that registers the same activities changes nothing anyone sees.
//! A running plugin registers straight into its list.
//!
//! Every change is made under the server's write lock and sent before it
//! is released, as `Ready` is built under it too: a session gets either the
//! old list and the change, or the new list.

use std::collections::{BTreeMap, HashMap};
use std::sync::{PoisonError, RwLock, RwLockWriteGuard};
use std::time::Instant;

use opencord_plugins::limits::ACTIVITIES_PER_PLUGIN;
use opencord_proto::v1 as proto;

use crate::gateway::session::ACTIVITIES;
use crate::state::AppState;

/// An activity as a plugin registered it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registered {
    pub name: String,
    pub description: String,
    /// A path under the plugin's `assets/`, or empty.
    pub icon: String,
    /// The manifest's name of the version that registered it.
    pub plugin_name: String,
}

/// The plugin already has [`ACTIVITIES_PER_PLUGIN`] activities.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TooMany;

type Keyed = BTreeMap<String, Registered>;

#[derive(Debug, Default)]
struct PluginActivities {
    /// What apps are shown: `Some` from the moment a start finished until
    /// the plugin stops.
    live: Option<Keyed>,
    /// What the VM now starting registered.
    starting: Option<Keyed>,
}

/// One plugin's activities before and after a change.
#[derive(Debug)]
struct Change {
    plugin_id: String,
    before: Option<Keyed>,
    after: Option<Keyed>,
}

impl Change {
    fn changed(&self) -> bool {
        let empty = Keyed::new();
        self.before.as_ref().unwrap_or(&empty) != self.after.as_ref().unwrap_or(&empty)
    }
}

/// The registered activities, by plugin id, then key. A plugin's entry
/// leaves when it stops, when it's turned off after failing, when a start
/// fails, and when it's uninstalled; a restart replaces it once the new
/// VM's start finishes. Nothing is kept across server restarts.
#[derive(Debug, Default)]
pub struct Activities {
    plugins: RwLock<HashMap<String, PluginActivities>>,
}

impl Activities {
    /// Every activity of a running plugin, by plugin id, then key.
    pub fn list(&self) -> Vec<proto::PluginActivity> {
        let plugins = self.plugins.read().unwrap_or_else(PoisonError::into_inner);
        let mut ids: Vec<&String> = plugins.keys().collect();
        ids.sort_unstable();
        ids.into_iter()
            .filter_map(|id| Some((id, plugins[id].live.as_ref()?)))
            .flat_map(|(id, activities)| {
                activities
                    .iter()
                    .map(|(key, activity)| proto::PluginActivity {
                        plugin_id: id.clone(),
                        activity_key: key.clone(),
                        name: activity.name.clone(),
                        description: activity.description.clone(),
                        icon: activity.icon.clone(),
                        plugin_name: activity.plugin_name.clone(),
                    })
            })
            .collect()
    }

    /// The activity as apps are shown it, if the plugin's finished start
    /// (or a later registration) has it.
    pub fn get(&self, plugin_id: &str, key: &str) -> Option<Registered> {
        let plugins = self.plugins.read().unwrap_or_else(PoisonError::into_inner);
        plugins.get(plugin_id)?.live.as_ref()?.get(key).cloned()
    }

    /// A VM of the plugin is starting: what it registers is held aside.
    pub(crate) fn begin(&self, plugin_id: &str) {
        self.write()
            .entry(plugin_id.to_owned())
            .or_default()
            .starting = Some(Keyed::new());
    }

    /// Registers `key`, replacing an activity with the same key. Ignored
    /// when no VM of the plugin is starting or running; a change to
    /// announce only when the plugin is running.
    fn register(
        &self,
        plugin_id: &str,
        key: &str,
        activity: Registered,
    ) -> Result<Option<Change>, TooMany> {
        let mut plugins = self.write();
        let Some(entry) = plugins.get_mut(plugin_id) else {
            return Ok(None);
        };
        let before = entry.live.clone();
        let staged = entry.starting.is_some();
        let Some(activities) = entry.starting.as_mut().or(entry.live.as_mut()) else {
            return Ok(None);
        };
        if !activities.contains_key(key) && activities.len() >= ACTIVITIES_PER_PLUGIN {
            return Err(TooMany);
        }
        activities.insert(key.to_owned(), activity);
        Ok((!staged).then(|| Change {
            plugin_id: plugin_id.to_owned(),
            before,
            after: entry.live.clone(),
        }))
    }

    /// The starting VM's start finished: what it registered is the list.
    fn finish(&self, plugin_id: &str) -> Change {
        let mut plugins = self.write();
        let entry = plugins.entry(plugin_id.to_owned()).or_default();
        let after = entry.starting.take().unwrap_or_default();
        let before = entry.live.replace(after.clone());
        Change {
            plugin_id: plugin_id.to_owned(),
            before,
            after: Some(after),
        }
    }

    /// The plugin no longer runs: its entry leaves.
    fn end(&self, plugin_id: &str) -> Change {
        let before = self.write().remove(plugin_id).and_then(|entry| entry.live);
        Change {
            plugin_id: plugin_id.to_owned(),
            before,
            after: None,
        }
    }

    fn write(&self) -> RwLockWriteGuard<'_, HashMap<String, PluginActivities>> {
        self.plugins.write().unwrap_or_else(PoisonError::into_inner)
    }
}

/// `ui.register_activity`: registers the activity, and tells apps once the
/// plugin's start has finished (before that, the start's end does).
pub async fn register(
    state: &AppState,
    plugin_id: &str,
    key: &str,
    activity: Registered,
) -> Result<(), TooMany> {
    let _writes = state.write_lock().await;
    let change = state
        .plugins
        .activities
        .register(plugin_id, key, activity)?;
    if let Some(change) = change {
        announce(state, &change);
    }
    Ok(())
}

/// The plugin's start finished: its new list replaces the old one.
pub async fn started(state: &AppState, plugin_id: &str) {
    let _writes = state.write_lock().await;
    let change = state.plugins.activities.finish(plugin_id);
    announce(state, &change);
}

/// The plugin stopped, was turned off, failed to start or was uninstalled:
/// its activities leave the list.
pub async fn stopped(state: &AppState, plugin_id: &str) {
    let _writes = state.write_lock().await;
    let change = state.plugins.activities.end(plugin_id);
    announce(state, &change);
}

/// Sends apps that show activities the whole list, if this plugin's part
/// of it changed. The caller holds the write lock.
fn announce(state: &AppState, change: &Change) {
    if !change.changed() {
        return;
    }
    let activities = state.plugins.activities.list();
    tracing::info!(
        plugin = %change.plugin_id,
        activities = change.after.as_ref().map_or(0, BTreeMap::len),
        "a plugin's activities changed"
    );
    let event = proto::Event {
        kind: Some(proto::event::Kind::PluginActivitiesUpdate(
            proto::PluginActivitiesUpdate { activities },
        )),
    };
    let now = Instant::now();
    for session in state.sessions.all() {
        if session.has_feature(ACTIVITIES) {
            session.push_event(&event, now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paint(name: &str) -> Registered {
        Registered {
            name: name.to_owned(),
            description: String::new(),
            icon: String::new(),
            plugin_name: "Paint".to_owned(),
        }
    }

    #[test]
    fn activities_count_only_once_the_start_finishes() {
        // Fails if a starting VM's activities are shown before its start
        // finishes, or a restart drops the old list while it starts.
        let activities = Activities::default();
        activities.begin("paint");
        activities
            .register("paint", "board", paint("Board"))
            .unwrap();
        assert_eq!(activities.get("paint", "board"), None);
        let first = activities.finish("paint");
        assert!(first.before.is_none() && first.changed());
        assert_eq!(activities.get("paint", "board"), Some(paint("Board")));

        activities.begin("paint");
        assert_eq!(
            activities.get("paint", "board"),
            Some(paint("Board")),
            "a restart keeps the list until its start finishes"
        );
        activities
            .register("paint", "board", paint("Board"))
            .unwrap();
        assert!(!activities.finish("paint").changed());

        assert!(activities.end("paint").changed());
        assert_eq!(activities.get("paint", "board"), None);
        assert!(activities.list().is_empty());
    }

    #[test]
    fn a_running_plugin_registers_straight_into_its_list() {
        // Fails if an activity registered after the start waits for a
        // start that never comes.
        let activities = Activities::default();
        activities.begin("paint");
        activities.finish("paint");
        let change = activities
            .register("paint", "board", paint("Board"))
            .unwrap()
            .expect("a change to announce");
        assert!(change.changed());
        assert_eq!(activities.list().len(), 1);
    }

    #[test]
    fn a_plugin_neither_starting_nor_running_registers_nothing() {
        // Fails if a late call from a stopped VM brings its activities back.
        let activities = Activities::default();
        assert!(
            activities
                .register("paint", "board", paint("Board"))
                .unwrap()
                .is_none()
        );
        assert!(activities.list().is_empty());
    }
}
