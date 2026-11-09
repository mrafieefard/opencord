// SPDX-License-Identifier: MPL-2.0

//! Open views of plugin channels (plan M2 Task 10, design 2026-11-07
//! §10.3, §10.4 and §14.3).
//!
//! A view is opened by a session, under the server's write lock, after its
//! access is checked. Everything that can take that access away (a role,
//! member or overwrite change, a kick or ban, deleting the channel, the
//! channel's plugin stopping or dropping its type) also runs under the
//! write lock and closes the views it affects before releasing it, so no
//! view outlives its member's access.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use opencord_plugins::limits::{
    FROM_VIEW_BYTES_PER_SECOND, FROM_VIEW_EVENTS_PER_SECOND, TO_VIEW_BYTES_PER_SECOND,
    TO_VIEW_EVENTS_PER_SECOND,
};
use opencord_proto::v1 as proto;

use super::PluginEvent;
use super::events::{id, user_table};
use crate::state::AppState;

/// What the event limits count over.
const WINDOW: Duration = Duration::from_secs(1);
/// The longest name of a view event, in bytes.
const EVENT_NAME_MAX: usize = 64;

/// Whether `name` may name a view event: `[a-z0-9_.:-]`, 1 to 64.
pub fn valid_event_name(name: &str) -> bool {
    (1..=EVENT_NAME_MAX).contains(&name.len())
        && name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_.:-".contains(&byte)
        })
}

/// Events and bytes in the current second.
#[derive(Debug)]
struct Window {
    started: Instant,
    events: u32,
    bytes: u64,
}

impl Window {
    fn new(now: Instant) -> Self {
        Self {
            started: now,
            events: 0,
            bytes: 0,
        }
    }

    /// Counts an event of `bytes`, or says how long until one fits.
    fn take(&mut self, now: Instant, bytes: u64, events: u32, max: u64) -> Result<(), Duration> {
        let since = now.saturating_duration_since(self.started);
        if since >= WINDOW {
            *self = Self::new(now);
        }
        if self.events >= events || self.bytes.saturating_add(bytes) > max {
            return Err(WINDOW.saturating_sub(since.min(WINDOW)));
        }
        self.events += 1;
        self.bytes += bytes;
        Ok(())
    }
}

/// One open view.
#[derive(Debug)]
struct Entry {
    info: ViewInfo,
    /// Events from the plugin to the view.
    to: Window,
    /// Events from the view's client to the plugin.
    from: Window,
    /// When the view last reported an error.
    last_error_report: Option<Instant>,
}

/// What a view is: whose, where, in which session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewInfo {
    pub id: i64,
    pub plugin_id: String,
    pub user_id: i64,
    pub session_id: String,
    pub channel_id: i64,
    /// `"desktop"` or `"phone"`.
    pub platform: String,
}

impl ViewInfo {
    /// The view as plugins see it: `{ id, user, place, platform }`.
    pub fn to_json(&self, state: &AppState) -> serde_json::Value {
        let user = user_table(&state.guild(), self.user_id);
        serde_json::json!({
            "id": id(self.id),
            "user": user,
            "place": { "channel_id": id(self.channel_id) },
            "platform": self.platform,
        })
    }
}

/// The view's limit for this direction is used up for now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limited(pub Duration);

/// Why a view closed, and what its session and plugin are told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Closed {
    /// `CloseView` from its own session.
    ByClient,
    /// Its session ended.
    SessionEnded,
    LostAccess,
    ChannelDeleted,
    /// The plugin runs without the channel's type.
    Unavailable,
    /// The plugin stopped, was turned off or uninstalled.
    PluginStopped,
}

impl Closed {
    /// The reason in `view_close`.
    fn plugin_reason(self) -> &'static str {
        match self {
            Closed::ByClient => "closed",
            Closed::SessionEnded => "disconnected",
            Closed::LostAccess => "lost_access",
            Closed::ChannelDeleted => "channel_deleted",
            Closed::Unavailable => "unavailable",
            Closed::PluginStopped => "plugin_stopping",
        }
    }

    /// The reason the session is told in `ViewClosed`; none when the
    /// session closed it or is gone.
    fn wire(self) -> Option<proto::ViewCloseReason> {
        match self {
            Closed::ByClient | Closed::SessionEnded => None,
            Closed::LostAccess => Some(proto::ViewCloseReason::LostAccess),
            Closed::ChannelDeleted => Some(proto::ViewCloseReason::ChannelDeleted),
            Closed::Unavailable => Some(proto::ViewCloseReason::Unavailable),
            Closed::PluginStopped => Some(proto::ViewCloseReason::PluginStopped),
        }
    }
}

/// Every open view, by id. In memory only.
///
/// **An entry leaves** on `CloseView`, when its session ends (the reaper's
/// sweep), when the server closes the view (lost access, the channel
/// deleted or unavailable, the plugin stopped, turned off or uninstalled),
/// and when the server restarts. A plugin's restart keeps its views.
#[derive(Debug, Default)]
pub struct Views {
    views: Mutex<HashMap<i64, Entry>>,
}

impl Views {
    /// Opens a view. The caller checked its access under the write lock.
    pub(crate) fn insert(&self, info: ViewInfo) {
        let now = Instant::now();
        self.lock().insert(
            info.id,
            Entry {
                info,
                to: Window::new(now),
                from: Window::new(now),
                last_error_report: None,
            },
        );
    }

    /// How many views the session has open.
    pub fn count_for_session(&self, session_id: &str) -> usize {
        self.lock()
            .values()
            .filter(|entry| entry.info.session_id == session_id)
            .count()
    }

    /// The view, while it's open.
    pub fn info(&self, view_id: i64) -> Option<ViewInfo> {
        self.lock().get(&view_id).map(|entry| entry.info.clone())
    }

    /// The plugin's open views, oldest first.
    pub fn of_plugin(&self, plugin_id: &str) -> Vec<ViewInfo> {
        let mut views: Vec<ViewInfo> = self
            .lock()
            .values()
            .filter(|entry| entry.info.plugin_id == plugin_id)
            .map(|entry| entry.info.clone())
            .collect();
        views.sort_unstable_by_key(|view| view.id);
        views
    }

    /// Records an error report from the session's own view. Returns
    /// `Ok(Some(info))` if within rate (at most 1 every 10 s), `Ok(None)` if
    /// throttled, or `Err(NotFound)` if not the session's view.
    pub fn report_error(
        &self,
        view_id: i64,
        session_id: &str,
    ) -> Result<Option<ViewInfo>, crate::error::ApiError> {
        let mut views = self.lock();
        let entry = views
            .get_mut(&view_id)
            .filter(|entry| entry.info.session_id == session_id)
            .ok_or_else(|| crate::error::ApiError::not_found("view"))?;

        let now = Instant::now();
        const COOLDOWN: Duration = Duration::from_secs(10);
        if let Some(last) = entry.last_error_report
            && now.saturating_duration_since(last) < COOLDOWN
        {
            return Ok(None);
        }
        entry.last_error_report = Some(now);
        Ok(Some(entry.info.clone()))
    }

    /// Counts an event from the session's own view: the view, or `None`
    /// when the session has no such view.
    pub fn take_from(
        &self,
        view_id: i64,
        session_id: &str,
        bytes: u64,
    ) -> Option<Result<ViewInfo, Limited>> {
        let mut views = self.lock();
        let entry = views
            .get_mut(&view_id)
            .filter(|entry| entry.info.session_id == session_id)?;
        Some(
            entry
                .from
                .take(
                    Instant::now(),
                    bytes,
                    FROM_VIEW_EVENTS_PER_SECOND,
                    FROM_VIEW_BYTES_PER_SECOND,
                )
                .map(|()| entry.info.clone())
                .map_err(Limited),
        )
    }

    /// Counts an event from the plugin to its view: the view's session, or
    /// `None` when the plugin has no such open view.
    fn take_to(
        &self,
        plugin_id: &str,
        view_id: i64,
        bytes: u64,
    ) -> Option<Result<String, Limited>> {
        let mut views = self.lock();
        let entry = views
            .get_mut(&view_id)
            .filter(|entry| entry.info.plugin_id == plugin_id)?;
        Some(
            entry
                .to
                .take(
                    Instant::now(),
                    bytes,
                    TO_VIEW_EVENTS_PER_SECOND,
                    TO_VIEW_BYTES_PER_SECOND,
                )
                .map(|()| entry.info.session_id.clone())
                .map_err(Limited),
        )
    }

    /// Removes and returns the views `close` picks.
    fn remove_where(&self, mut close: impl FnMut(&ViewInfo) -> bool) -> Vec<ViewInfo> {
        let mut views = self.lock();
        let ids: Vec<i64> = views
            .values()
            .filter(|entry| close(&entry.info))
            .map(|entry| entry.info.id)
            .collect();
        let mut removed: Vec<ViewInfo> = ids
            .iter()
            .filter_map(|view_id| views.remove(view_id))
            .map(|entry| entry.info)
            .collect();
        removed.sort_unstable_by_key(|view| view.id);
        removed
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<i64, Entry>> {
        self.views.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Tells the view's plugin it opened. Its VM announces it once: a view
/// opened while the VM starts is announced when the start finishes.
pub fn opened(state: &AppState, view: &ViewInfo) {
    tracing::debug!(
        plugin = %view.plugin_id,
        view = view.id,
        channel = view.channel_id,
        "a view opened"
    );
    state.plugins.deliver(
        &view.plugin_id,
        PluginEvent {
            name: "view_open".to_owned(),
            payload: serde_json::json!({ "view_id": view.id }),
            reply: None,
            command: None,
        },
    );
}

/// `CloseView`: closes the session's own view; false if it has no such
/// view.
pub fn close_own(state: &AppState, session_id: &str, view_id: i64) -> bool {
    let closed = state
        .plugins
        .views
        .remove_where(|view| view.id == view_id && view.session_id == session_id);
    let found = !closed.is_empty();
    tell(state, closed, Closed::ByClient);
    found
}

/// Closes the views whose user can no longer see their channel, or whose
/// channel is gone. Runs after every change to who sees what; the caller
/// holds the write lock and no guild guard.
pub fn check_access(state: &AppState) {
    let mut deleted = Vec::new();
    let lost = {
        let guild = state.guild();
        state.plugins.views.remove_where(|view| {
            if !guild.channels.contains_key(&view.channel_id) {
                deleted.push(view.id);
                true
            } else {
                !guild.can_view(view.user_id, view.channel_id)
            }
        })
    };
    let (gone, lost): (Vec<ViewInfo>, Vec<ViewInfo>) = lost
        .into_iter()
        .partition(|view| deleted.contains(&view.id));
    tell(state, gone, Closed::ChannelDeleted);
    tell(state, lost, Closed::LostAccess);
}

/// The channel can no longer be opened: its views close, with why.
/// The caller holds the write lock and no guild guard.
pub fn channel_unavailable(state: &AppState, channel_id: i64, now: proto::PluginChannelState) {
    let why = match now {
        proto::PluginChannelState::TypeGone => Closed::Unavailable,
        _ => Closed::PluginStopped,
    };
    let closed = state
        .plugins
        .views
        .remove_where(|view| view.channel_id == channel_id);
    tell(state, closed, why);
}

/// The plugin is about to stop: its views close, and it hears each close
/// as it stops.
pub fn plugin_stopping(state: &AppState, plugin_id: &str) {
    let closed = state
        .plugins
        .views
        .remove_where(|view| view.plugin_id == plugin_id);
    tell(state, closed, Closed::PluginStopped);
}

/// Closes the views of sessions that ended or were forgotten.
pub fn sessions_ended(state: &AppState) {
    let closed = state.plugins.views.remove_where(|view| {
        state
            .sessions
            .get(&view.session_id)
            .is_none_or(|session| session.is_ended())
    });
    tell(state, closed, Closed::SessionEnded);
}

/// Tells each closed view's session (unless it closed it, or is gone) and
/// its plugin.
fn tell(state: &AppState, closed: Vec<ViewInfo>, why: Closed) {
    let now = Instant::now();
    for view in closed {
        tracing::debug!(
            plugin = %view.plugin_id,
            view = view.id,
            reason = why.plugin_reason(),
            "a view closed"
        );
        if let Some(reason) = why.wire()
            && let Some(session) = state.sessions.get(&view.session_id)
        {
            let event = proto::Event {
                kind: Some(proto::event::Kind::ViewClosed(proto::ViewClosed {
                    view_id: view.id,
                    reason: reason as i32,
                })),
            };
            session.push_event(&event, now);
        }
        let payload = serde_json::json!({
            "view_id": view.id,
            "view": view.to_json(state),
            "reason": why.plugin_reason(),
        });
        state.plugins.deliver(
            &view.plugin_id,
            PluginEvent {
                name: "view_close".to_owned(),
                payload,
                reply: None,
                command: None,
            },
        );
    }
}

/// A client's event, for the plugin's `ui.on` handlers.
pub fn from_client(state: &AppState, view: &ViewInfo, name: String, data: serde_json::Value) {
    state.plugins.deliver(
        &view.plugin_id,
        PluginEvent {
            name: "view_event".to_owned(),
            payload: serde_json::json!({ "view_id": view.id, "name": name, "data": data }),
            reply: None,
            command: None,
        },
    );
}

/// Sends `name` with `data_json` to the plugin's view: true if it went
/// out, false if the view isn't open (or its session is gone).
pub fn send(
    state: &AppState,
    plugin_id: &str,
    view_id: i64,
    name: &str,
    data_json: &str,
) -> Result<bool, Limited> {
    let bytes = (name.len() + data_json.len()) as u64;
    let Some(session_id) = state.plugins.views.take_to(plugin_id, view_id, bytes) else {
        return Ok(false);
    };
    let Some(session) = state.sessions.get(&session_id?) else {
        return Ok(false);
    };
    let event = proto::Event {
        kind: Some(proto::event::Kind::ViewMessage(proto::ViewMessage {
            view_id,
            name: name.to_owned(),
            data_json: data_json.to_owned(),
        })),
    };
    session.push_event(&event, Instant::now());
    Ok(true)
}

/// Tells sessions with an open view of `plugin_id` that its client files updated.
pub fn scripts_updated(state: &AppState, plugin_id: &str) {
    let views = state.plugins.views.of_plugin(plugin_id);
    if views.is_empty() {
        return;
    }
    let Some(handle) = state.plugins.get(plugin_id) else {
        return;
    };
    let Some(entry) = handle.client.entry() else {
        return;
    };
    let files = crate::plugins::assets::client_files(&handle);
    let event = proto::Event {
        kind: Some(proto::event::Kind::ViewScriptsUpdate(
            proto::ViewScriptsUpdate {
                plugin_id: plugin_id.to_owned(),
                entry: entry.to_owned(),
                files,
            },
        )),
    };
    let now = Instant::now();
    let mut sent_sessions = std::collections::HashSet::new();
    for view in views {
        if sent_sessions.insert(view.session_id.clone())
            && let Some(session) = state.sessions.get(&view.session_id)
        {
            session.push_event(&event, now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fails if the window lets a 31st event or a byte past the limit into
    // one second, or never opens again.
    #[test]
    fn a_window_counts_events_and_bytes_per_second() {
        let start = Instant::now();
        let mut window = Window::new(start);
        for _ in 0..30 {
            window.take(start, 1, 30, 100).unwrap();
        }
        assert!(window.take(start, 1, 30, 100).is_err());
        let later = start + WINDOW;
        window.take(later, 100, 30, 100).unwrap();
        let wait = window.take(later, 1, 30, 100).unwrap_err();
        assert_eq!(wait, WINDOW);
    }

    // Fails if an event name outside `[a-z0-9_.:-]{1,64}` is taken.
    #[test]
    fn event_names_follow_the_pattern() {
        assert!(valid_event_name("ticket.claim:v2-x_1"));
        assert!(valid_event_name(&"a".repeat(64)));
        assert!(!valid_event_name(""));
        assert!(!valid_event_name(&"a".repeat(65)));
        assert!(!valid_event_name("Ticket"));
        assert!(!valid_event_name("a b"));
    }
}
