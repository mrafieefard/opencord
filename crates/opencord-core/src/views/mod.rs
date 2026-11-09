//! Plugin views (spec §11): a plugin's client script, running on this
//! device in its own Luau VM, one per open view, each on its own thread.
//!
//! The VM runs `init` and `render`, checks each tree against the block
//! schema, diffs it with the last one and hands the renderer JSON batches
//! of ops (the plan's "view wire format", `opencord_plugins::view`). The
//! renderer only draws and sends inputs back, as JSON too. A broken or
//! hostile script stops its own view and nothing else: every call into Lua
//! has a CPU budget, the VM has a memory cap, and none of it runs on the
//! caller's thread.

mod api_app;
mod api_ui;
mod carry;
mod dialogs;
mod draw;
mod handlers;
mod lists;
mod modals;
mod runtime;
#[cfg(test)]
mod test_support;

use std::collections::BTreeMap;
use std::sync::mpsc::RecvTimeoutError;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

pub use runtime::start;

/// How long [`ViewHandle::close`] waits for the view's thread to end.
const JOIN_WAIT: Duration = Duration::from_secs(1);

/// Everything a view needs to start.
#[derive(Debug, Clone)]
pub struct ViewSpec {
    pub view_id: String,
    pub plugin_id: String,
    /// The client script to run first, a key of `sources`
    /// (`client/main.luau`).
    pub entry: String,
    /// Every `client/…` file of the plugin, by its path in the package.
    pub sources: BTreeMap<String, String>,
    pub context: ViewContext,
    pub viewer: Viewer,
    pub place: Place,
    /// Whether an `error` op carries the Lua error and its traceback (dev
    /// mode, or a viewer who manages plugins).
    pub show_errors: bool,
}

/// The plugin area the view is drawn in, as the renderer's `context`
/// input describes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ViewContext {
    pub width: f64,
    pub height: f64,
    pub theme: Theme,
    pub input: InputKind,
    pub safe_area: SafeArea,
    pub text_scale: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    Dark,
    Light,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputKind {
    Mouse,
    Touch,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct SafeArea {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

/// Who is looking: ids are decimal strings, as everywhere in Lua.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Viewer {
    pub id: String,
    pub name: String,
    pub roles: Vec<String>,
}

/// Where the view is open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Place {
    Channel { channel_id: String },
}

/// Where a view's output goes. Called on the view's own thread, so each
/// call must return quickly: queue the work, never wait on it.
pub trait ViewSink: Send + Sync {
    /// A batch of ops for the renderer, as JSON.
    fn batch(&self, json: String);
    /// `app.send(name, data)`: an event for the plugin's server script;
    /// `data_json` is at most `VIEW_EVENT_MAX_BYTES`.
    fn send(&self, name: String, data_json: String);
    /// An error in the client script, for the plugin's log (Claude's call
    /// 7); at most `VIEW_ERROR_REPORT_MAX_CHARS` characters.
    fn report_error(&self, text: String);
}

/// `text` cut to `max` characters.
fn cut(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((end, _)) => text[..end].to_owned(),
        None => text.to_owned(),
    }
}

/// What the view's thread is asked to do, in order.
enum Command {
    /// An input from the renderer, as JSON.
    Input(String),
    ServerMessage {
        name: String,
        data_json: String,
    },
    Reload {
        entry: String,
        sources: BTreeMap<String, String>,
    },
    Close,
}

/// An open view. Dropping it closes the view without waiting.
pub struct ViewHandle {
    view_id: String,
    commands: mpsc::UnboundedSender<Command>,
    thread: Mutex<Option<ViewThread>>,
}

struct ViewThread {
    join: std::thread::JoinHandle<()>,
    /// Disconnects when the thread is done.
    done: std::sync::mpsc::Receiver<()>,
}

impl ViewHandle {
    /// An input from the renderer (the wire format's inputs), as JSON.
    pub fn input(&self, json: &str) {
        self.command(Command::Input(json.to_owned()));
    }

    /// An event from the plugin's server script, for `app.on(name, fn)`.
    pub fn server_message(&self, name: String, data_json: String) {
        self.command(Command::ServerMessage { name, data_json });
    }

    /// New client files (dev mode): the view keeps its state (§11.6), run
    /// through the new script's `migrate(old)` when it has one.
    pub fn reload(&self, entry: String, sources: BTreeMap<String, String>) {
        self.command(Command::Reload { entry, sources });
    }

    /// Runs the script's `close` listeners within one handler's budget,
    /// then ends the thread, waiting for it at most [`JOIN_WAIT`].
    pub fn close(&self) {
        self.command(Command::Close);
        let thread = self
            .thread
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        let Some(ViewThread { join, done }) = thread else {
            return;
        };
        match done.recv_timeout(JOIN_WAIT) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => {
                if join.join().is_err() {
                    tracing::error!(view = %self.view_id, "the view's thread panicked");
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                tracing::warn!(
                    view = %self.view_id,
                    wait_ms = JOIN_WAIT.as_millis() as u64,
                    "the view's thread was still closing; left to finish"
                );
            }
        }
    }

    fn command(&self, command: Command) {
        if self.commands.send(command).is_err() {
            // The thread ends only after close, or when it couldn't start
            // (which the sink already heard), so nothing is waiting for this.
            tracing::debug!(view = %self.view_id, "a closed view was sent work");
        }
    }
}
