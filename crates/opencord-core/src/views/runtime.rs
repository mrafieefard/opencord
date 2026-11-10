//! One view's thread (§11.1–§11.3): its Luau VM, its single-threaded Tokio
//! runtime, and the loop that takes inputs in order, runs handlers and
//! redraws.
//!
//! - **Start:** the API, the entry script and `init(ctx)` share the start
//!   budget (`VIEW_START_TIME_FACTOR` handler times); the first redraw
//!   follows.
//! - **Handlers** (a press, a server event) run as async Lua threads with
//!   one handler's budget over all their polls. An error in one is
//!   reported and the view goes on.
//! - **Redraw:** after each input or server message, if anything called
//!   `app.set`, `render(state)` runs once, its tree is checked and diffed,
//!   and one batch goes out with the tree's ops and then any side effects.
//! - **Errors in start, `render` or the tree** stop the view: an `error` op,
//!   and no more Lua until a reload. Every error goes to `report_error`.
//! - **Reload** (dev mode) carries the state as plain data into a new VM
//!   (Claude's call 8) and diffs the new tree against the one drawn.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::StreamExt as _;
use mlua::{Compiler, Function, Lua, LuaSerdeExt as _, Table, Value};
use opencord_plugins::limits::{
    VIEW_ERROR_REPORT_MAX_CHARS, VIEW_HANDLER_TIME, VIEW_MEMORY_BYTES, VIEW_START_TIME_FACTOR,
};

use super::carry::carry_state;
use opencord_plugins::lua_api::install_require;
use opencord_plugins::sandbox::{Budgeted, Vm, VmError, VmLimits};
use opencord_plugins::view::{Batch, Patch};
use serde::Deserialize;
use tokio::sync::mpsc;

use super::api_app::{self, LISTEN_KEY, ON_KEY, Phase, Shared};
use super::api_ui;
use super::context::{self, PLATFORM};
use super::draw::{Drawing, Failure, Scene};
use super::frames;
use super::handlers::{self, Finished, HandlerKind, Handlers, Running};
use super::modals;
use super::timers;
use super::{Command, Place, ViewContext, ViewHandle, ViewSink, ViewSpec, ViewThread, Viewer, cut};

/// Luau caps its C call depth; this leaves it room, as on the server.
const VM_STACK_BYTES: usize = 8 * 1024 * 1024;
/// How long `close` listeners may take, waiting included.
const CLOSE_WAIT: Duration = Duration::from_millis(500);

/// Opens a view: its thread starts the script and sends the first batch.
/// Nothing of the script runs on the caller's thread.
pub fn start(spec: ViewSpec, sink: Arc<dyn ViewSink>) -> ViewHandle {
    let (commands, receiver) = mpsc::unbounded_channel();
    let (finished, done) = std::sync::mpsc::channel::<()>();
    let view_id = spec.view_id.clone();
    let show_errors = spec.show_errors;
    let thread_sink = sink.clone();
    let spawned = std::thread::Builder::new()
        .name(format!("view {view_id}"))
        .stack_size(VM_STACK_BYTES)
        .spawn(move || {
            run_thread(spec, thread_sink, receiver);
            // Dropped last, so `close` sees the thread done only now.
            drop(finished);
        });
    let thread = match spawned {
        Ok(join) => Some(ViewThread { join, done }),
        Err(error) => {
            tracing::error!(view = %view_id, %error, "could not start a view's thread");
            let failure = Failure::host(
                "starting the view",
                format!("could not start its thread: {error}"),
            );
            sink.report_error(failure.report());
            let batch = Batch {
                view: view_id.clone(),
                seq: 1,
                ops: vec![failure.op(show_errors)],
            };
            send_batch(sink.as_ref(), &batch);
            None
        }
    };
    ViewHandle {
        view_id,
        commands,
        thread: Mutex::new(thread),
    }
}

fn run_thread(spec: ViewSpec, sink: Arc<dyn ViewSink>, commands: mpsc::UnboundedReceiver<Command>) {
    let mut view = View::new(spec, sink);
    match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime.block_on(view.run(commands)),
        Err(error) => {
            view.fail(Failure::host(
                "starting the view",
                format!("could not start its runtime: {error}"),
            ));
            view.flush();
        }
    }
}

/// An input from the renderer (the wire format's inputs). Kinds this
/// version doesn't handle yet are read as `Other` and ignored.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Input {
    Event {
        node: u64,
        event: String,
        #[serde(default = "no_data")]
        data: serde_json::Value,
    },
    /// A lazy list's visible items, `end` not included.
    Range {
        node: u64,
        start: u64,
        end: u64,
    },
    /// A quick dialog's answer.
    Dialog {
        id: u64,
        #[serde(default)]
        result: serde_json::Value,
    },
    ModalDismiss {
        name: String,
    },
    Frame {
        dt: f64,
        /// The last batch the renderer has drawn.
        seq: u64,
    },
    SoundEnd {
        id: u64,
    },
    Context(ViewContext),
    Lifecycle {
        event: Lifecycle,
    },
    Key {
        event: KeyEvent,
        key: String,
        code: String,
        shift: bool,
        ctrl: bool,
        alt: bool,
        meta: bool,
        repeat: bool,
    },
    #[serde(other)]
    Other,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Lifecycle {
    Pause,
    Resume,
    Focus,
    Blur,
    ConnectionLost,
    ConnectionBack,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum KeyEvent {
    KeyDown,
    KeyUp,
}

impl KeyEvent {
    fn name(self) -> &'static str {
        match self {
            KeyEvent::KeyDown => "key_down",
            KeyEvent::KeyUp => "key_up",
        }
    }
}

fn no_data() -> serde_json::Value {
    serde_json::Value::Object(serde_json::Map::new())
}

/// A VM that started: its `app` table, `json.decode` and the state.
struct Live {
    vm: Vm,
    app: Table,
    decode: Function,
    state: Table,
}

struct View {
    view_id: String,
    plugin_id: String,
    entry: String,
    sources: Arc<BTreeMap<String, String>>,
    context: ViewContext,
    viewer: Viewer,
    place: Place,
    show_errors: bool,
    shared: Arc<Shared>,
    live: Option<Live>,
    /// Set by an error in start, `render` or the tree: no Lua runs until a
    /// reload.
    stopped: bool,
    drawing: Drawing,
    seq: u64,
    handlers: Handlers,
    running: Running,
    /// Hidden: timers and frames wait. Kept across a reload, as is the
    /// context: both are the renderer's, not the script's.
    paused: bool,
    /// The last save failed and that was reported; it's retried each turn.
    storage_failing: bool,
}

impl View {
    fn new(spec: ViewSpec, sink: Arc<dyn ViewSink>) -> View {
        View {
            view_id: spec.view_id,
            plugin_id: spec.plugin_id,
            entry: spec.entry,
            sources: Arc::new(spec.sources),
            context: spec.context,
            viewer: spec.viewer,
            place: spec.place,
            show_errors: spec.show_errors,
            shared: Arc::new(Shared::new(sink, spec.storage_dir)),
            live: None,
            stopped: false,
            drawing: Drawing::default(),
            seq: 0,
            handlers: Handlers::default(),
            running: Running::default(),
            paused: false,
            storage_failing: false,
        }
    }

    async fn run(&mut self, mut commands: mpsc::UnboundedReceiver<Command>) {
        let began = Instant::now();
        match self.start_vm(None) {
            Ok(()) => tracing::info!(
                view = %self.view_id,
                plugin = %self.plugin_id,
                after_ms = began.elapsed().as_millis() as u64,
                "view started"
            ),
            Err(failure) => self.fail(failure),
        }
        self.flush();
        loop {
            let due = self.shared.timers.next_due();
            tokio::select! {
                biased;
                Some(finished) = self.running.next(), if !self.running.is_empty() => {
                    self.finished(finished);
                }
                command = commands.recv() => match command {
                    Some(Command::Input(json)) => self.input(&json),
                    Some(Command::ServerMessage { name, data_json }) => {
                        self.server_message(&name, data_json);
                    }
                    Some(Command::Reload { entry, sources }) => self.reload(entry, sources),
                    Some(Command::Finish(op)) => {
                        self.shared.push_effect(op);
                        self.flush();
                        break;
                    }
                    Some(Command::Close) | None => break,
                },
                () = timers::wait(due) => self.fire_timer(),
            }
            self.flush();
        }
        self.close().await;
        self.save_storage();
        self.drop_vm();
        tracing::info!(view = %self.view_id, plugin = %self.plugin_id, "view closed");
    }

    /// Makes a VM and runs the script's start: `init(ctx)`, or with
    /// `carried` (a reload) the old state through `migrate`.
    fn start_vm(&mut self, carried: Option<String>) -> Result<(), Failure> {
        self.shared.set_phase(Phase::Starting);
        let limits = VmLimits {
            memory_bytes: VIEW_MEMORY_BYTES,
            handler_time: VIEW_HANDLER_TIME,
        };
        let vm = Vm::new(&limits).map_err(|error| Failure::lua("starting the view", error))?;
        let limit = VIEW_HANDLER_TIME * VIEW_START_TIME_FACTOR;
        vm.budget().start(limit);
        let started = self.prepare(&vm, carried);
        vm.budget().stop();
        match started {
            Ok((app, decode, state)) => {
                self.live = Some(Live {
                    vm,
                    app,
                    decode,
                    state,
                });
                self.shared.set_phase(Phase::Running);
                self.shared.mark_dirty();
                Ok(())
            }
            Err((what, error)) => {
                dispose(vm, &self.view_id);
                Err(match error {
                    Started::Lua(VmError::BudgetExceeded { .. }) => Failure::host(
                        what,
                        format!("starting took longer than {} ms", limit.as_millis()),
                    ),
                    Started::Lua(error) => Failure::lua(what, error),
                    Started::Host(message) => Failure::host(what, message),
                })
            }
        }
    }

    /// The start itself, inside the start budget. An error says where.
    fn prepare(
        &self,
        vm: &Vm,
        carried: Option<String>,
    ) -> Result<(Table, Function, Table), (String, Started)> {
        let lua = &vm.lua;
        // Luau otherwise resolves `app.theme` when a chunk loads and keeps
        // the value in its bytecode; `app`'s context fields change after.
        lua.set_compiler(Compiler::new().set_mutable_globals(["app"]));
        let setup = |error: mlua::Error| ("starting the view".to_owned(), Started::from(error));
        api_ui::install(lua).map_err(setup)?;
        let app = api_app::install(lua, &self.shared).map_err(setup)?;
        context::install(lua, &app, &self.viewer, &self.place, &self.context).map_err(setup)?;
        let sources = self.sources.clone();
        install_require(lua, "client", move |path| sources.get(path).cloned()).map_err(setup)?;
        let decode = lua
            .globals()
            .get::<Table>("json")
            .and_then(|json| json.get::<Function>("decode"))
            .map_err(setup)?;

        let loading = format!("loading {}", self.entry);
        let Some(source) = self.sources.get(&self.entry) else {
            let missing = format!("the plugin has no {}", self.entry);
            return Err((loading, Started::Host(missing)));
        };
        vm.load(&self.entry, source)
            .and_then(|chunk| chunk.call::<()>(()).map_err(VmError::from))
            .map_err(|error| (loading, Started::Lua(error)))?;

        let state = match carried {
            None => self.init(lua)?,
            Some(text) => {
                let at = |error: mlua::Error| ("migrate".to_owned(), Started::from(error));
                let old = plain_value(decode.call::<Value>(text).map_err(at)?);
                match global_function(lua, "migrate").map_err(at)? {
                    Some(migrate) => {
                        let state = migrate.call::<Value>(old).map_err(at)?;
                        as_state(state, "migrate").map_err(|e| ("migrate".to_owned(), e))?
                    }
                    None => as_state(old, "the state").map_err(|e| ("reload".to_owned(), e))?,
                }
            }
        };
        api_app::set_state(&app, &state).map_err(setup)?;
        Ok((app, decode, state))
    }

    /// `init(ctx)`: ctx holds the viewer, the place, the size, the theme
    /// and the platform (§11.2).
    fn init(&self, lua: &Lua) -> Result<Table, (String, Started)> {
        let at = |error: mlua::Error| ("init".to_owned(), Started::from(error));
        let Some(init) = global_function(lua, "init").map_err(at)? else {
            let missing = "the client script has no init function".to_owned();
            return Err(("init".to_owned(), Started::Host(missing)));
        };
        let ctx = serde_json::json!({
            "viewer": self.viewer,
            "place": self.place,
            "size": { "width": self.context.width, "height": self.context.height },
            "theme": self.context.theme,
            "platform": PLATFORM,
        });
        let options = mlua::SerializeOptions::new()
            .serialize_none_to_null(false)
            .serialize_unit_to_null(false)
            .set_array_metatable(false);
        let ctx = lua.to_value_with(&ctx, options).map_err(at)?;
        let state = init.call::<Value>(ctx).map_err(at)?;
        as_state(state, "init").map_err(|e| ("init".to_owned(), e))
    }

    fn input(&mut self, json: &str) {
        let input = match serde_json::from_str::<Input>(json) {
            Ok(input) => input,
            Err(error) => {
                // The renderer is ours, so this is a bug; the error's kind
                // and place, never the input's data.
                tracing::warn!(
                    view = %self.view_id,
                    kind = ?error.classify(),
                    column = error.column(),
                    "a view input couldn't be read"
                );
                return;
            }
        };
        match input {
            // These two are the renderer's facts: a stopped view keeps them
            // for its reload.
            Input::Context(next) => self.set_context(next),
            Input::Lifecycle { event } => self.lifecycle(event),
            input if !self.stopped => self.script_input(input),
            _ => {}
        }
    }

    /// An input a running script handles.
    fn script_input(&mut self, input: Input) {
        match input {
            Input::Event { node, event, data } => {
                // No handler for this node and event: ignored (§11.3).
                let Some(handler) = self.handlers.get(node, &event) else {
                    return;
                };
                let kind = HandlerKind::of_event(&event);
                self.spawn(format!("on_{event}"), kind, handler, data.to_string());
            }
            Input::Range { node, start, end } => {
                if self.drawing.set_range(node, start, end) {
                    self.shared.mark_dirty();
                }
            }
            Input::Dialog { id, result } => {
                if !self.shared.waits.answer(id, result) {
                    tracing::debug!(view = %self.view_id, dialog = id, "an answer no handler waits for");
                }
            }
            Input::ModalDismiss { name } => self.dismiss(name),
            Input::Frame { dt, seq } => self.frame(dt, seq),
            Input::SoundEnd { id } => self.sound_end(id),
            Input::Key {
                event,
                key,
                code,
                shift,
                ctrl,
                alt,
                meta,
                repeat,
            } => {
                let fields = serde_json::json!({
                    "key": key, "code": code, "shift": shift, "ctrl": ctrl,
                    "alt": alt, "meta": meta, "repeat": repeat,
                });
                self.notify(event.name(), fields.to_string());
            }
            Input::Other => {
                tracing::debug!(view = %self.view_id, "a view input of a kind not handled yet");
            }
            // Handled by `input`.
            Input::Context(_) | Input::Lifecycle { .. } => {}
        }
    }

    /// A `context` input: `app`'s fields follow it, a redraw shows it, and
    /// the script hears what changed.
    fn set_context(&mut self, next: ViewContext) {
        if next == self.context {
            return;
        }
        let resized = next.width != self.context.width || next.height != self.context.height;
        let themed = next.theme != self.context.theme;
        self.context = next;
        if let Some(live) = &self.live
            && let Err(error) = context::update(&live.vm.lua, &live.app, &self.context)
        {
            tracing::warn!(view = %self.view_id, %error, "a view's context couldn't be set");
            return;
        }
        self.shared.mark_dirty();
        if resized {
            let size = serde_json::json!({
                "width": self.context.width,
                "height": self.context.height,
                "breakpoint": context::breakpoint(self.context.width),
            });
            self.notify("resize", size.to_string());
        }
        if themed {
            let theme = serde_json::Value::from(self.context.theme.name());
            self.notify("theme_change", theme.to_string());
        }
    }

    fn lifecycle(&mut self, event: Lifecycle) {
        match event {
            Lifecycle::Pause => {
                if self.set_paused(true) {
                    self.notify("pause", "null".to_owned());
                }
            }
            Lifecycle::Resume => {
                if self.set_paused(false) {
                    self.notify("resume", "null".to_owned());
                }
            }
            Lifecycle::Focus => self.notify("focus", "null".to_owned()),
            Lifecycle::Blur => self.notify("blur", "null".to_owned()),
            Lifecycle::ConnectionLost => {
                let change = serde_json::json!({ "connected": false });
                self.notify("connection_change", change.to_string());
            }
            Lifecycle::ConnectionBack => {
                let change = serde_json::json!({ "connected": true });
                self.notify("connection_change", change.to_string());
            }
        }
    }

    /// Hidden or shown again; `false` when that's no change.
    fn set_paused(&mut self, paused: bool) -> bool {
        if self.paused == paused {
            return false;
        }
        self.paused = paused;
        let now = tokio::time::Instant::now();
        if paused {
            self.shared.timers.pause(now);
        } else {
            self.shared.timers.resume(now);
        }
        self.shared.frames.set_paused(paused);
        true
    }

    fn fire_timer(&mut self) {
        let now = tokio::time::Instant::now();
        let Some(fired) = self.shared.timers.take_due(now) else {
            return;
        };
        let Some(live) = &self.live else { return };
        match timers::handler(&live.vm.lua, &fired) {
            Ok(Some(handler)) => {
                let what = if fired.repeating {
                    "app.every"
                } else {
                    "app.after"
                };
                self.spawn(
                    what.to_owned(),
                    HandlerKind::Plain,
                    handler,
                    "null".to_owned(),
                );
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(view = %self.view_id, %error, "a view's timer couldn't be read");
            }
        }
    }

    /// A `frame` input: the script's frame function, unless the renderer is
    /// behind the batches sent, so a slow one never queues frames.
    fn frame(&mut self, dt: f64, seq: u64) {
        if self.paused || seq < self.seq {
            return;
        }
        let Some(live) = &self.live else { return };
        match frames::handler(&live.vm.lua) {
            Ok(Some(handler)) => {
                let what = "app.on_frame".to_owned();
                self.spawn(what, HandlerKind::Frame, handler, dt.to_string());
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(view = %self.view_id, %error, "a view's frame function couldn't be read");
            }
        }
    }

    fn sound_end(&mut self, id: u64) {
        let Some(live) = &self.live else { return };
        match api_app::take_on_end(&live.vm.lua, &self.shared, id) {
            Ok(Some(on_end)) => {
                let what = "on_end".to_owned();
                self.spawn(what, HandlerKind::Plain, on_end, "null".to_owned());
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(view = %self.view_id, %error, "a sound's on_end couldn't be read");
            }
        }
    }

    fn server_message(&mut self, name: &str, data_json: String) {
        if self.stopped {
            return;
        }
        let what = format!("app.on(\"{}\")", cut(name, 64));
        self.run_listeners(ON_KEY, name, what, data_json);
    }

    /// Runs the functions `app.listen(name, fn)` was given.
    fn notify(&mut self, name: &str, data_json: String) {
        if self.stopped {
            return;
        }
        let what = format!("app.listen(\"{name}\")");
        self.run_listeners(LISTEN_KEY, name, what, data_json);
    }

    fn run_listeners(&mut self, key: &str, name: &str, what: String, data_json: String) {
        let Some(live) = &self.live else { return };
        let listeners = match api_app::handlers_for(&live.vm.lua, key, name) {
            Ok(listeners) => listeners,
            Err(error) => {
                tracing::warn!(view = %self.view_id, %error, "a view's listeners couldn't be read");
                return;
            }
        };
        for handler in listeners {
            self.spawn(what.clone(), HandlerKind::Plain, handler, data_json.clone());
        }
    }

    /// Starts `handler` with `data_json` decoded as its argument; it runs
    /// at the loop's next turn, before the redraw that follows it.
    fn spawn(&mut self, what: String, kind: HandlerKind, handler: Function, data_json: String) {
        let Some(live) = &self.live else { return };
        let decode = live.decode.clone();
        let call = async move {
            let data = plain_value(decode.call::<Value>(data_json)?);
            handler.call_async::<()>(data).await
        };
        let budgeted = Budgeted::new(live.vm.budget().clone(), kind.limit(), call);
        self.running
            .push(handlers::handler(what, kind, self.shared.clone(), budgeted));
    }

    fn finished(&mut self, finished: Finished) {
        let Err(error) = finished.result else { return };
        if finished.kind == HandlerKind::Frame
            && matches!(error, VmError::BudgetExceeded { .. })
            && let Some(live) = &self.live
            && let Err(error) = frames::stop(&live.vm.lua, &self.shared)
        {
            tracing::warn!(view = %self.view_id, %error, "a view's frame function couldn't be cleared");
        }
        let failure = Failure::lua(finished.what, error);
        tracing::info!(view = %self.view_id, kind = failure.kind, "a view handler failed");
        self.report(failure.report());
    }

    /// Writes the plugin's storage if anything changed. A failure is
    /// reported once; the next turn tries again.
    fn save_storage(&mut self) {
        let began = Instant::now();
        match self.shared.storage.save() {
            Ok(()) => self.storage_failing = false,
            Err(error) => {
                if !self.storage_failing {
                    self.storage_failing = true;
                    tracing::warn!(
                        view = %self.view_id,
                        plugin = %self.plugin_id,
                        kind = ?error.kind(),
                        after_ms = began.elapsed().as_millis() as u64,
                        "a view's device storage couldn't be saved"
                    );
                    self.report(format!(
                        "app.storage: the device storage couldn't be saved: {:?}",
                        error.kind()
                    ));
                }
            }
        }
    }

    fn reload(&mut self, entry: String, sources: BTreeMap<String, String>) {
        let began = Instant::now();
        let carried = match self.live.as_ref().map(|live| carry_state(&live.state)) {
            Some(Ok(text)) => Some(text),
            Some(Err(message)) => {
                self.report(format!(
                    "reload: the state couldn't be kept, so init runs again: {message}"
                ));
                None
            }
            None => None,
        };
        self.drop_vm();
        self.entry = entry;
        self.sources = Arc::new(sources);
        self.stopped = false;
        match self.start_vm(carried) {
            Ok(()) => tracing::info!(
                view = %self.view_id,
                plugin = %self.plugin_id,
                after_ms = began.elapsed().as_millis() as u64,
                "view reloaded"
            ),
            Err(failure) => self.fail(failure),
        }
    }

    /// Redraws if anything asked to, then sends the tree's ops and the side
    /// effects waiting, as one batch.
    fn flush(&mut self) {
        let mut ops = Vec::new();
        if !self.stopped && self.shared.take_dirty() {
            match self.redraw() {
                Ok(tree) => ops = tree,
                Err(failure) => self.fail(failure),
            }
        }
        ops.extend(self.shared.take_effects());
        ops.extend(self.shared.frames.change());
        self.save_storage();
        if !ops.is_empty() {
            self.seq += 1;
            let batch = Batch {
                view: self.view_id.clone(),
                seq: self.seq,
                ops,
            };
            send_batch(self.shared.sink.as_ref(), &batch);
        }
    }

    /// `render(state)`, the lists' items and the open modals, with one
    /// handler's budget, checked, then diffed with what's drawn (`draw`).
    fn redraw(&mut self) -> Result<Vec<Patch>, Failure> {
        let Some(live) = &self.live else {
            return Ok(Vec::new());
        };
        let Some(render) = global_function(&live.vm.lua, "render")
            .map_err(|error| Failure::lua("render", VmError::from(error)))?
        else {
            return Err(Failure::host(
                "render",
                "the client script has no render function",
            ));
        };
        self.shared.set_phase(Phase::Rendering);
        let drawn = self.drawing.redraw(Scene {
            lua: &live.vm.lua,
            budget: live.vm.budget(),
            state: &live.state,
            render: &render,
        });
        self.shared.set_phase(Phase::Running);
        let drawn = drawn?;
        self.handlers.replace(drawn.handlers);
        Ok(drawn.ops)
    }

    /// A `modal_dismiss` input: closes the named modal, unless it was drawn
    /// as not dismissible. The closing is Lua's, so it runs as a handler.
    fn dismiss(&mut self, name: String) {
        if !self.drawing.dismissible(&name) {
            return;
        }
        let Some(live) = &self.live else { return };
        match modals::dismisser(&live.vm.lua) {
            Ok(dismiss) => {
                let data = serde_json::Value::String(name).to_string();
                self.spawn(
                    "modal_dismiss".to_owned(),
                    HandlerKind::Plain,
                    dismiss,
                    data,
                );
            }
            Err(error) => {
                tracing::warn!(view = %self.view_id, %error, "a view's modal dismiss couldn't be read");
            }
        }
    }

    /// Stops the view: the `error` op goes with the next batch, and no Lua
    /// runs until a reload.
    fn fail(&mut self, failure: Failure) {
        tracing::warn!(
            view = %self.view_id,
            plugin = %self.plugin_id,
            kind = failure.kind,
            during = %failure.what,
            "a view stopped on an error"
        );
        self.report(failure.report());
        self.stopped = true;
        self.shared.forget_vm();
        self.drawing.forget_shown();
        self.handlers.clear();
        self.running = Running::default();
        self.shared.take_dirty();
        self.shared.push_effect(failure.op(self.show_errors));
    }

    fn report(&self, text: String) {
        self.shared
            .sink
            .report_error(cut(&text, VIEW_ERROR_REPORT_MAX_CHARS));
    }

    /// Runs the `close` listeners, together within one handler's budget and
    /// [`CLOSE_WAIT`].
    async fn close(&mut self) {
        if self.stopped {
            return;
        }
        let Some(live) = &self.live else { return };
        let listeners = match api_app::handlers_for(&live.vm.lua, LISTEN_KEY, "close") {
            Ok(listeners) => listeners,
            Err(error) => {
                tracing::warn!(view = %self.view_id, %error, "a view's close listeners couldn't be read");
                return;
            }
        };
        if listeners.is_empty() {
            return;
        }
        let call = async move {
            let mut errors = Vec::new();
            for listener in listeners {
                if let Err(error) = listener.call_async::<()>(()).await {
                    errors.push(VmError::from(error));
                }
            }
            errors
        };
        let budgeted = Budgeted::new(live.vm.budget().clone(), VIEW_HANDLER_TIME, call);
        match tokio::time::timeout(CLOSE_WAIT, budgeted).await {
            Ok(errors) => {
                for error in errors {
                    self.report(Failure::lua("close listener", error).report());
                }
            }
            Err(_) => self.report(format!(
                "close listener: still waiting after {} ms, so it was stopped",
                CLOSE_WAIT.as_millis()
            )),
        }
    }

    /// Ends the VM and everything that points into it.
    fn drop_vm(&mut self) {
        self.shared.forget_vm();
        self.handlers.clear();
        self.running = Running::default();
        if let Some(Live {
            vm,
            app,
            decode,
            state,
        }) = self.live.take()
        {
            drop((app, decode, state));
            dispose(vm, &self.view_id);
        }
    }
}

/// Drops a VM. The Rust halves of calls still waiting live in Lua userdata
/// holding the VM; a collection drops them, so the VM can close.
fn dispose(vm: Vm, view_id: &str) {
    if let Err(error) = vm.lua.gc_collect() {
        tracing::warn!(view = %view_id, %error, "could not clear a closed view's calls");
    }
    drop(vm);
}

fn send_batch(sink: &dyn ViewSink, batch: &Batch) {
    match serde_json::to_string(batch) {
        Ok(json) => sink.batch(json),
        // Ops hold only strings, numbers and JSON values, which always
        // write; failing here is a bug in the ops, and it's logged as one.
        Err(error) => {
            tracing::error!(view = %batch.view, %error, "a view's batch couldn't be written")
        }
    }
}

/// A global the script defined, if it's a function.
fn global_function(lua: &Lua, name: &str) -> mlua::Result<Option<Function>> {
    match lua.globals().get::<Value>(name)? {
        Value::Function(function) => Ok(Some(function)),
        _ => Ok(None),
    }
}

/// `json.null` as nil: a handler given no data gets nil, not a value that
/// is true in an `if`.
fn plain_value(value: Value) -> Value {
    match value {
        Value::LightUserData(data) if data.0.is_null() => Value::Nil,
        other => other,
    }
}

/// A state is a table; `what` returned something else.
fn as_state(value: Value, what: &str) -> Result<Table, Started> {
    match value {
        Value::Table(state) => Ok(state),
        other => Err(Started::Host(format!(
            "{what} must return the view's state, a table, not a {}",
            other.type_name()
        ))),
    }
}

/// What stopped a start.
enum Started {
    Lua(VmError),
    Host(String),
}

impl From<mlua::Error> for Started {
    fn from(error: mlua::Error) -> Self {
        Started::Lua(VmError::from(error))
    }
}

impl From<VmError> for Started {
    fn from(error: VmError) -> Self {
        Started::Lua(error)
    }
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;

// A child of this module to reach `View`, which its paused clock runs in
// the test's own runtime.
#[cfg(test)]
#[path = "timers_tests.rs"]
mod timers_tests;
