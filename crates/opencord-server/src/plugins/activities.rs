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
//!
//! The activity sessions running in calls live here too (plan M3 Task 4,
//! §15.2, §15.4). Every road that ends one goes through [`end`]: a member's
//! `EndActivity`, the plugin's `activity.finish`, its call emptying, its
//! plugin stopping, and its plugin running without the activity.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, MutexGuard, PoisonError, RwLock, RwLockWriteGuard};
use std::time::Instant;

use opencord_plugins::limits::ACTIVITIES_PER_PLUGIN;
use opencord_proto::v1 as proto;

use super::PluginEvent;
use super::events::{id, user_table};
use super::views::{self, Views};
use crate::gateway::session::ACTIVITIES;
use crate::guild::Guild;
use crate::state::AppState;

/// One activity running in one call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub id: i64,
    /// The voice channel.
    pub channel_id: i64,
    pub plugin_id: String,
    pub activity_key: String,
    /// The activity's name, icon and plugin's name when it started.
    pub name: String,
    pub icon: String,
    pub plugin_name: String,
    pub starter_id: i64,
    pub started_at_ms: i64,
}

impl Session {
    pub fn to_proto(&self, viewer_ids: Vec<i64>) -> proto::ActivitySession {
        proto::ActivitySession {
            id: self.id,
            channel_id: self.channel_id,
            plugin_id: self.plugin_id.clone(),
            activity_key: self.activity_key.clone(),
            name: self.name.clone(),
            icon: self.icon.clone(),
            plugin_name: self.plugin_name.clone(),
            starter_id: self.starter_id,
            started_at_ms: self.started_at_ms,
            viewer_ids,
        }
    }

    /// The session as plugins see it: `{ id, activity, channel_id,
    /// starter_id, started_at_ms }`.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": id(self.id),
            "activity": self.activity_key,
            "channel_id": id(self.channel_id),
            "starter_id": id(self.starter_id),
            "started_at_ms": self.started_at_ms,
        })
    }

    /// As apps are sent it now, with who has it open (plan M3 Task 5).
    pub fn shown(&self, views: &Views) -> proto::ActivitySession {
        self.to_proto(views.viewers_of(self.id))
    }
}

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
///
/// The sessions, by id: at most [`ACTIVITY_SESSIONS_PER_CALL`] per voice
/// channel, since each needs someone in its call. A session leaves only
/// through [`end`], and with the server.
///
/// [`ACTIVITY_SESSIONS_PER_CALL`]: opencord_plugins::limits::ACTIVITY_SESSIONS_PER_CALL
#[derive(Debug, Default)]
pub struct Activities {
    plugins: RwLock<HashMap<String, PluginActivities>>,
    sessions: Mutex<BTreeMap<i64, Session>>,
}

impl Activities {
    /// The session, while it runs.
    pub fn session(&self, session_id: i64) -> Option<Session> {
        self.lock_sessions().get(&session_id).cloned()
    }

    /// The sessions running in the voice channel, oldest first.
    pub fn sessions_in(&self, channel_id: i64) -> Vec<Session> {
        self.sessions_where(|session| session.channel_id == channel_id)
    }

    /// The plugin's running sessions, oldest first.
    pub fn sessions_of(&self, plugin_id: &str) -> Vec<Session> {
        self.sessions_where(|session| session.plugin_id == plugin_id)
    }

    /// The sessions in the voice channels `user_id` can see, as `Ready`
    /// holds them.
    pub fn visible_to(
        &self,
        guild: &Guild,
        views: &Views,
        user_id: i64,
    ) -> Vec<proto::ActivitySession> {
        self.sessions_where(|session| guild.can_view(user_id, session.channel_id))
            .iter()
            .map(|session| session.shown(views))
            .collect()
    }

    fn sessions_where(&self, pick: impl Fn(&Session) -> bool) -> Vec<Session> {
        self.lock_sessions()
            .values()
            .filter(|session| pick(session))
            .cloned()
            .collect()
    }

    fn lock_sessions(&self) -> MutexGuard<'_, BTreeMap<i64, Session>> {
        self.sessions.lock().unwrap_or_else(PoisonError::into_inner)
    }

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

/// The plugin's start finished: its new list replaces the old one, and its
/// sessions of activities the list no longer has end.
pub async fn started(state: &AppState, plugin_id: &str) {
    let _writes = state.write_lock().await;
    let change = state.plugins.activities.finish(plugin_id);
    announce(state, &change);
    let activities = &state.plugins.activities;
    for session in activities.sessions_of(plugin_id) {
        if activities.get(plugin_id, &session.activity_key).is_none() {
            end(state, session.id, proto::ActivityEndReason::Unavailable);
        }
    }
}

/// The plugin stopped, was turned off, failed to start or was uninstalled:
/// its activities leave the list, and its sessions end.
pub async fn stopped(state: &AppState, plugin_id: &str) {
    let _writes = state.write_lock().await;
    let change = state.plugins.activities.end(plugin_id);
    announce(state, &change);
    end_plugins_sessions(state, plugin_id);
}

/// The plugin is about to stop: its sessions end now, so its VM hears
/// each `activity_end` as it stops.
pub async fn plugin_stopping(state: &AppState, plugin_id: &str) {
    let _writes = state.write_lock().await;
    end_plugins_sessions(state, plugin_id);
}

fn end_plugins_sessions(state: &AppState, plugin_id: &str) {
    for session in state.plugins.activities.sessions_of(plugin_id) {
        end(state, session.id, proto::ActivityEndReason::PluginStopped);
    }
}

/// Adds a session that `StartActivity` checked, tells who can see its
/// channel, and tells its plugin. The caller holds the write lock.
pub fn add(state: &AppState, session: Session) -> proto::ActivitySession {
    tracing::info!(
        plugin = %session.plugin_id,
        session = session.id,
        channel = session.channel_id,
        "an activity session started"
    );
    let shown = session.shown(&state.plugins.views);
    state
        .plugins
        .activities
        .lock_sessions()
        .insert(session.id, session.clone());
    tell_audience(
        state,
        session.channel_id,
        proto::event::Kind::ActivitySessionCreate(proto::ActivitySessionCreate {
            session: Some(shown.clone()),
        }),
    );
    // The start as it is now rides along: should the session end before
    // the VM takes this event, the VM still hears it start, then end.
    let start = start_payload(state, &session);
    state.plugins.deliver(
        &session.plugin_id,
        PluginEvent {
            name: "activity_start".to_owned(),
            payload: serde_json::json!({ "session_id": session.id, "start": start }),
            reply: None,
            command: None,
        },
    );
    shown
}

/// Ends the session, if it still runs: its views close, it leaves,
/// everyone who can see its channel hears, and its plugin gets
/// `activity_end` after every `view_close` of it. False if it was already
/// gone. The caller holds the write lock.
pub fn end(state: &AppState, session_id: i64, reason: proto::ActivityEndReason) -> bool {
    if state.plugins.activities.session(session_id).is_none() {
        return false;
    }
    // While it's still there, so each view's place names its activity.
    views::activity_ended(state, session_id);
    let removed = state.plugins.activities.lock_sessions().remove(&session_id);
    let Some(session) = removed else {
        return false;
    };
    tracing::info!(
        plugin = %session.plugin_id,
        session = session.id,
        channel = session.channel_id,
        reason = plugin_reason(reason),
        "an activity session ended"
    );
    tell_audience(
        state,
        session.channel_id,
        proto::event::Kind::ActivitySessionDelete(proto::ActivitySessionDelete {
            session_id,
            channel_id: session.channel_id,
            reason: reason as i32,
        }),
    );
    state.plugins.deliver(
        &session.plugin_id,
        PluginEvent {
            name: "activity_end".to_owned(),
            payload: serde_json::json!({
                "session_id": session.id,
                "session": session.to_json(),
                "reason": plugin_reason(reason),
            }),
            reply: None,
            command: None,
        },
    );
    true
}

/// Who has the session open changed: everyone who can see its channel
/// hears the session with its viewers. The caller holds the write lock.
pub fn viewers_changed(state: &AppState, session_id: i64) {
    let Some(session) = state.plugins.activities.session(session_id) else {
        return;
    };
    let shown = session.shown(&state.plugins.views);
    tracing::debug!(
        plugin = %session.plugin_id,
        session = session.id,
        viewers = shown.viewer_ids.len(),
        "an activity session's viewers changed"
    );
    tell_audience(
        state,
        session.channel_id,
        proto::event::Kind::ActivitySessionUpdate(proto::ActivitySessionUpdate {
            session: Some(shown),
        }),
    );
}

/// `activity.finish`: ends the plugin's own session; false if it has no such
/// session running.
pub async fn end_own(state: &AppState, plugin_id: &str, session_id: i64) -> bool {
    let _writes = state.write_lock().await;
    let own = state
        .plugins
        .activities
        .session(session_id)
        .is_some_and(|session| session.plugin_id == plugin_id);
    own && end(state, session_id, proto::ActivityEndReason::Plugin)
}

/// Nobody is left in the voice channel's call: its sessions end. The
/// caller holds the write lock and no voice states guard.
pub fn call_emptied(state: &AppState, channel_id: i64) {
    for session in state.plugins.activities.sessions_in(channel_id) {
        end(state, session.id, proto::ActivityEndReason::CallEmpty);
    }
}

/// `activity_start`'s argument: `{ session, starter, call }`, the call's
/// members as they are now, ascending.
pub fn start_payload(state: &AppState, session: &Session) -> serde_json::Value {
    let mut members: Vec<i64> = state
        .voice()
        .in_channel(session.channel_id)
        .map(|voice_state| voice_state.user_id)
        .collect();
    members.sort_unstable();
    let starter = user_table(&state.guild(), session.starter_id);
    serde_json::json!({
        "session": session.to_json(),
        "starter": starter,
        "call": {
            "channel_id": id(session.channel_id),
            "member_ids": members.into_iter().map(id).collect::<Vec<_>>(),
        },
    })
}

/// The reason `activity_end` gives.
fn plugin_reason(reason: proto::ActivityEndReason) -> &'static str {
    match reason {
        proto::ActivityEndReason::Ended => "ended",
        proto::ActivityEndReason::Plugin => "plugin",
        proto::ActivityEndReason::CallEmpty => "call_empty",
        proto::ActivityEndReason::PluginStopped => "plugin_stopping",
        proto::ActivityEndReason::Unavailable => "unavailable",
        proto::ActivityEndReason::Unspecified => "unspecified",
    }
}

/// Sends `kind` to the sessions that show activities and whose user can
/// see `channel_id`.
fn tell_audience(state: &AppState, channel_id: i64, kind: proto::event::Kind) {
    let event = proto::Event { kind: Some(kind) };
    let now = Instant::now();
    let guild = state.guild();
    for session in state.sessions.all() {
        if session.has_feature(ACTIVITIES) && guild.can_view(session.user_id, channel_id) {
            session.push_event(&event, now);
        }
    }
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
