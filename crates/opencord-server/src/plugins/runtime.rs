//! The plugin runtime (spec §10.1, plan Task 7).
//!
//! Each running plugin's VM lives on its own thread with its own
//! single-threaded Tokio runtime, so a busy plugin never holds up the
//! server's. A supervisor task on the server's runtime starts the VM,
//! counts its failures, restarts it with backoff, and turns the plugin off
//! when it keeps failing.
//!
//! Lua runs only while a handler's future is polled, and each poll starts
//! the VM's budget with what is left of that handler's time. So waiting on
//! the host doesn't count, and handlers waiting at the same time each keep
//! their own time.

use std::collections::{HashMap, HashSet, VecDeque};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLock, Weak};
use std::time::{Duration, Instant};

use mlua::{Function, Lua, LuaSerdeExt as _, Table, Value, Variadic};
use opencord_plugins::limits::{
    FAILURE_WINDOW, FAILURES_BEFORE_RESTART, RESTART_BACKOFF_FIRST, RESTART_BACKOFF_MAX,
    RESTART_WINDOW, RESTARTS_BEFORE_OFF, START_TIME_FACTOR, STOP_GRACE,
};
use opencord_plugins::lua_api;
pub(super) use opencord_plugins::sandbox::Budgeted;
use opencord_plugins::sandbox::{Budget, Vm, VmError, VmLimits, paused};
use opencord_proto::v1 as proto;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinSet;

use super::api::plugin::handlers_for;
use super::api::{self, raise};
use super::command_calls;
use super::cpu_share::{CpuShare, lower_this_threads_priority};
use super::database::{self, PluginDb};
use super::net::{NetGuard, PluginNet};
use super::permissions::PluginPermission;
use super::{InstalledPlugin, PluginHandle, StartError};
use super::{activities, channel_types};
use crate::config::PluginsSection;
use crate::error::ApiError;
use crate::state::AppState;

/// Luau caps its C call depth; this leaves it room.
const VM_STACK_BYTES: usize = 8 * 1024 * 1024;
/// How often a VM's CPU and memory are sampled.
const STATS_EVERY: Duration = Duration::from_secs(1);
/// Samples kept: CPU is measured over the last five seconds.
const CPU_SAMPLES: usize = 6;
/// Dropped events are logged at most this often.
const DROPS_LOGGED_EVERY: Duration = Duration::from_secs(60);

/// What a plugin's API calls run with: one per VM.
pub struct PluginEnv {
    pub state: Weak<AppState>,
    pub plugin_id: String,
    pub user_id: i64,
    pub version: String,
    /// The manifest's name.
    pub plugin_name: String,
    /// Every path under `assets/` in the installed version.
    pub assets: Arc<HashSet<String>>,
    /// Shared with the plugin's handle, which changes it.
    pub granted: Arc<RwLock<HashSet<PluginPermission>>>,
    pub log: Arc<super::logbook::PluginLog>,
    pub budget: Budget,
    /// The server's runtime. Server work runs there ([`PluginEnv::on_server`]),
    /// so it finishes even if the plugin stops halfway.
    pub server: tokio::runtime::Handle,
    /// The plugin's internet access. Its connections live on the VM's
    /// runtime and end with it.
    pub net: super::net::PluginNet,
    /// The plugin's database, open while the VM runs.
    pub db: PluginDb,
    /// `files` in the plugin's folder, and its quota.
    pub files_dir: PathBuf,
    pub files_bytes: u64,
    /// Hook paths registered via `hooks.on`.
    pub hooks: Arc<RwLock<HashSet<String>>>,
    pub queue: Arc<super::queue::EventQueue>,
    pub timers: Arc<Mutex<HashMap<u64, tokio::task::JoinHandle<()>>>>,
    pub next_timer_id: Arc<AtomicU64>,
    pub config_values: Arc<RwLock<HashMap<String, super::ConfigValue>>>,
    pub files_lock: Arc<tokio::sync::Mutex<()>>,
    pub files_reads: Arc<tokio::sync::Semaphore>,
    /// The views this VM heard open and not yet close: each is announced
    /// to a VM once, and only those it heard open are heard closing.
    pub views_heard: Mutex<HashSet<i64>>,
    /// The activity sessions this VM heard start and not yet end, the
    /// same way.
    pub sessions_heard: Mutex<HashSet<i64>>,
    /// When a broadcast to views last logged skipping limited ones.
    pub views_skip_logged: Mutex<Option<Instant>>,
}

impl PluginEnv {
    pub fn has(&self, permission: PluginPermission) -> bool {
        self.granted
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(&permission)
    }

    pub fn require(&self, permission: PluginPermission) -> mlua::Result<()> {
        if self.has(permission) {
            Ok(())
        } else {
            Err(raise(format!(
                "PERMISSION_DENIED: the plugin doesn't have {}",
                permission.name()
            )))
        }
    }

    pub fn state(&self) -> mlua::Result<Arc<AppState>> {
        self.state
            .upgrade()
            .filter(|state| !state.shutdown.is_cancelled())
            .ok_or_else(|| raise("STOPPING: the server is stopping"))
    }

    /// Checks the general per-user request rate limit for this plugin's synthetic user.
    pub fn check_rate_limit(&self) -> mlua::Result<()> {
        let state = self.state()?;
        match state.rate_limits.check_request(self.user_id) {
            Ok(()) => Ok(()),
            Err(_) => Err(raise("RATE_LIMITED: slow down")),
        }
    }

    /// `"<CODE>: <message>"`, as a member's request would have failed.
    pub fn api_error(error: ApiError) -> mlua::Error {
        let code = error.code.as_str_name();
        let code = code.strip_prefix("ERROR_CODE_").unwrap_or(code);
        raise(format!("{code}: {}", error.message))
    }

    /// Runs `work` on the server's runtime and waits for it, paused.
    pub async fn on_server<F>(&self, work: F) -> mlua::Result<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        match paused(&self.budget, self.server.spawn(work)).await {
            Ok(output) => Ok(output),
            Err(error) if error.is_panic() => {
                tracing::error!(plugin = %self.plugin_id, "a plugin's call into the server panicked");
                Err(raise("INTERNAL: the server failed"))
            }
            Err(_) => Err(raise("STOPPING: the server is stopping")),
        }
    }
}

pub(crate) type Sleep = Pin<Box<dyn Future<Output = ()> + Send>>;

/// Time for the supervisor, so tests can move it.
pub(crate) trait Clock: Send + Sync {
    fn now(&self) -> Instant;
    fn sleep(&self, wait: Duration) -> Sleep;
}

pub(crate) struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn sleep(&self, wait: Duration) -> Sleep {
        Box::pin(tokio::time::sleep(wait))
    }
}

/// What a VM is made from; the supervisor keeps it across restarts.
pub(crate) struct Spec {
    id: String,
    user_id: i64,
    version: String,
    name: String,
    /// Every path under `assets/`.
    assets: Arc<HashSet<String>>,
    scripts: Vec<String>,
    /// Every `server/*.lua` and `server/*.luau`, by path.
    sources: Arc<HashMap<String, String>>,
    limits: VmLimits,
    /// The manifest's domains.
    domains: Arc<NetGuard>,
    /// `data.sqlite` in the plugin's folder, and its quota.
    database: PathBuf,
    database_bytes: u64,
    /// `files` in the plugin's folder, and its quota.
    files_dir: PathBuf,
    files_bytes: u64,
}

impl Spec {
    /// `data_dir` is the plugin's folder, `data/plugins/<id>`.
    pub(crate) fn new(
        installed: &InstalledPlugin,
        config: &PluginsSection,
        data_dir: PathBuf,
    ) -> Result<Spec, StartError> {
        let mut sources = HashMap::new();
        let assets = installed
            .package
            .paths()
            .filter(|path| path.starts_with("assets/"))
            .map(str::to_owned)
            .collect();
        for path in installed.package.paths() {
            if path.starts_with("server/") && (path.ends_with(".luau") || path.ends_with(".lua")) {
                let bytes = installed.package.get(path).unwrap_or_default();
                let text = std::str::from_utf8(bytes)
                    .map_err(|_| StartError(format!("{path} isn't UTF-8 text")))?;
                sources.insert(path.to_owned(), text.to_owned());
            }
        }
        let memory = u64::from(config.memory_mb) * 1024 * 1024;
        let files_bytes = u64::from(config.files_mb) * 1024 * 1024;
        Ok(Spec {
            id: installed.id.clone(),
            user_id: installed.user_id,
            version: installed.manifest.version.to_string(),
            name: installed.manifest.name.clone(),
            assets: Arc::new(assets),
            scripts: installed.manifest.server.clone(),
            sources: Arc::new(sources),
            limits: VmLimits {
                memory_bytes: usize::try_from(memory).unwrap_or(usize::MAX),
                handler_time: Duration::from_millis(config.handler_ms.into()),
            },
            domains: Arc::new(NetGuard::new(&installed.manifest.domains)),
            database: data_dir.join("data.sqlite"),
            database_bytes: u64::from(config.database_mb) * 1024 * 1024,
            files_dir: data_dir.join(super::assets::FILES_FOLDER),
            files_bytes,
        })
    }

    /// The manifest's domains.
    pub(crate) fn domains(&self) -> Arc<NetGuard> {
        self.domains.clone()
    }
}

pub(super) enum Report {
    Started(Result<(), String>),
    Failed { error: String, out_of_memory: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    Run,
    /// Deliver `stop`, give it the grace, end.
    Stop,
    /// End now: the VM is broken.
    Kill,
}

fn report(reports: &mpsc::UnboundedSender<Report>, sent: Report) {
    // The supervisor stops listening only once it has decided the VM's
    // end, so a report it misses changes nothing.
    reports.send(sent).ok();
}

/// Starts a VM on its own thread. It reports on `reports`, whose senders
/// all drop once the thread is done with Lua.
fn spawn_vm(
    spec: Arc<Spec>,
    handle: Arc<PluginHandle>,
    reports: mpsc::UnboundedSender<Report>,
    commands: watch::Receiver<Command>,
    state: Weak<AppState>,
    db: PluginDb,
) -> std::io::Result<()> {
    let server = tokio::runtime::Handle::current();
    std::thread::Builder::new()
        .name(format!("plugin {}", spec.id))
        .stack_size(VM_STACK_BYTES)
        .spawn(move || {
            lower_this_threads_priority();
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    let error = format!("could not start the plugin's runtime: {error}");
                    return report(&reports, Report::Started(Err(error)));
                }
            };
            let vm = match Vm::new(&spec.limits) {
                Ok(vm) => vm,
                Err(error) => return report(&reports, Report::Started(Err(error.to_string()))),
            };
            // A client per VM: its pooled connections belong to this
            // thread's runtime, which a restart replaces.
            let net = match PluginNet::new(spec.domains.clone()) {
                Ok(net) => net,
                Err(error) => {
                    let error = format!("could not set up the plugin's internet access: {error}");
                    return report(&reports, Report::Started(Err(error)));
                }
            };
            let env = Arc::new(PluginEnv {
                state,
                plugin_id: spec.id.clone(),
                user_id: spec.user_id,
                version: spec.version.clone(),
                plugin_name: spec.name.clone(),
                assets: spec.assets.clone(),
                granted: handle.granted.clone(),
                log: handle.log.clone(),
                budget: vm.budget().clone(),
                server,
                net,
                db,
                files_dir: spec.files_dir.clone(),
                files_bytes: spec.files_bytes,
                hooks: handle.hooks.clone(),
                queue: handle.queue.clone(),
                timers: handle.timers.clone(),
                next_timer_id: handle.next_timer_id.clone(),
                config_values: handle.config_values.clone(),
                files_lock: handle.files_lock.clone(),
                files_reads: handle.files_reads.clone(),
                views_heard: Mutex::new(HashSet::new()),
                sessions_heard: Mutex::new(HashSet::new()),
                views_skip_logged: Mutex::new(None),
            });
            let last_report = reports.clone();
            runtime.block_on(run_vm(&vm, &env, &spec, &handle, reports, commands));
            handle.clear_timers();
            handle.clear_hooks();
            if let Some(state) = env.state.upgrade() {
                state.plugins.bus.unlisten_all(&env.plugin_id);
                if state.commands.remove_for_plugin(&env.plugin_id) {
                    crate::commands::announce(&state);
                }
                let plugin_id = env.plugin_id.clone();
                env.server.spawn(async move {
                    crate::commands::thinking::abandon(&state, &plugin_id).await;
                });
            }
            // Handlers still waiting go with the runtime. The Rust halves of
            // their calls live in Lua userdata, holding the VM: a collection
            // drops them, so the VM can close.
            drop(runtime);
            if let Err(error) = vm.lua.gc_collect() {
                tracing::warn!(plugin = %spec.id, %error, "could not clear a stopped plugin's calls");
            }
            drop(vm);
            // The supervisor goes on once every sender is gone.
            drop(last_report);
        })?;
    Ok(())
}

async fn run_vm(
    vm: &Vm,
    env: &Arc<PluginEnv>,
    spec: &Spec,
    handle: &PluginHandle,
    reports: mpsc::UnboundedSender<Report>,
    mut commands: watch::Receiver<Command>,
) {
    let started = tokio::select! {
        biased;
        _ = commands.wait_for(|command| *command != Command::Run) => return,
        started = start_vm(vm, env, spec) => started,
    };
    let failed = started.is_err();
    let mut stats = Stats::default();
    if !failed {
        stats.sample(vm, handle);
    }
    report(&reports, Report::Started(started));
    if failed {
        return;
    }
    let mut running = JoinSet::new();
    // Views open across a restart are announced before any waiting event.
    super::view_calls::announce(vm, env, spec.limits.handler_time, &mut running, &reports);
    let mut tick = tokio::time::interval(STATS_EVERY);
    let mut share = CpuShare::default();
    loop {
        share.record(Instant::now(), vm.budget().total_used());
        let hold = share.pause_for();
        tokio::select! {
            biased;
            changed = commands.changed() => {
                let command = match changed {
                    Ok(()) => *commands.borrow_and_update(),
                    Err(_) => Command::Kill,
                };
                match command {
                    Command::Run => {}
                    Command::Stop => {
                        stop_vm(vm, env, spec, &reports).await;
                        break;
                    }
                    Command::Kill => break,
                }
            }
            event = handle.queue.pop(), if hold.is_zero() => {
                dispatch(vm, env, spec, event, &mut running, &reports);
                // Lets the handler run before the next event is taken, so
                // a backlog can't all start before any of it is counted.
                tokio::task::yield_now().await;
            }
            () = tokio::time::sleep(hold), if !hold.is_zero() => {}
            Some(_) = running.join_next(), if !running.is_empty() => {}
            _ = tick.tick() => stats.sample(vm, handle),
        }
    }
}

/// Builds the API, runs the manifest's scripts in order and delivers
/// `start`, all within the start budget.
async fn start_vm(vm: &Vm, env: &Arc<PluginEnv>, spec: &Spec) -> Result<(), String> {
    let limit = spec.limits.handler_time * START_TIME_FACTOR;
    // Building the API runs Lua, which runs only inside a budget.
    vm.budget().start(limit);
    let installed = api::install(vm, env).and_then(|()| install_require(&vm.lua, spec));
    let used = vm.budget().used();
    vm.budget().stop();
    if let Err(error) = installed {
        return Err(start_failed(env, VmError::from(error), limit));
    }
    let run = async {
        for path in &spec.scripts {
            let source = spec
                .sources
                .get(path)
                .map(String::as_str)
                .unwrap_or_default();
            vm.load(path, source)?.call_async::<()>(()).await?;
        }
        let payload = serde_json::json!({});
        for handler in handlers_for(&vm.lua, "start")? {
            let argument = event_table(&vm.lua, &payload, None)?;
            handler.call_async::<()>(argument).await?;
        }
        Ok::<(), VmError>(())
    };
    Budgeted::new(vm.budget().clone(), limit.saturating_sub(used), run)
        .await
        .map_err(|error| start_failed(env, error, limit))
}

/// Logs why start failed and returns it.
fn start_failed(env: &PluginEnv, error: VmError, limit: Duration) -> String {
    let (message, traceback) = match error {
        VmError::BudgetExceeded { .. } => (
            format!("starting took longer than {} ms", limit.as_millis()),
            None,
        ),
        VmError::Lua { message, traceback } => (message, traceback),
        other => (other.to_string(), None),
    };
    let text = match &traceback {
        Some(traceback) => format!("start failed: {message}\n{traceback}"),
        None => format!("start failed: {message}"),
    };
    env.log.push(proto::PluginLogLevel::Error, &text);
    message
}

/// `require(name)`: `server/<name>.luau` or `.lua`, run once per VM.
fn install_require(lua: &Lua, spec: &Spec) -> mlua::Result<()> {
    let sources = spec.sources.clone();
    lua_api::install_require(lua, "server", move |path| sources.get(path).cloned())
}

type ReplySlot = Arc<Mutex<Option<oneshot::Sender<serde_json::Value>>>>;

/// `payload` as a Lua value, the way handlers see it.
pub(super) fn payload_value(lua: &Lua, payload: &serde_json::Value) -> mlua::Result<Value> {
    let options = mlua::SerializeOptions::new()
        .serialize_none_to_null(false)
        .serialize_unit_to_null(false)
        .set_array_metatable(false);
    lua.to_value_with(payload, options)
}

/// The event's payload as a table, with `reply` when someone waits for one.
fn event_table(
    lua: &Lua,
    payload: &serde_json::Value,
    reply: Option<&ReplySlot>,
) -> mlua::Result<Value> {
    let value = payload_value(lua, payload)?;
    if let (Value::Table(table), Some(reply)) = (&value, reply) {
        let reply = reply.clone();
        let function = api::function(lua, move |lua, args: Variadic<Value>| {
            let answer = to_json(lua, args)?;
            let sender = reply.lock().unwrap_or_else(PoisonError::into_inner).take();
            Ok(sender.is_some_and(|sender| sender.send(answer).is_ok()))
        })?;
        table.raw_set("reply", function)?;
    }
    Ok(value)
}

/// The arguments as a JSON array, through `json.encode` and its limits.
fn to_json(lua: &Lua, args: Variadic<Value>) -> mlua::Result<serde_json::Value> {
    let encode: Function = lua.globals().get::<Table>("json")?.get("encode")?;
    let list = lua.create_sequence_from(args)?;
    let text: mlua::String = encode.call(list).map_err(|error| match error {
        mlua::Error::RuntimeError(message) => {
            let message = message
                .split("\nstack traceback:")
                .next()
                .unwrap_or_default();
            raise(message)
        }
        other => other,
    })?;
    serde_json::from_slice(&text.as_bytes())
        .map_err(|error| raise(format!("INVALID_ARGUMENT: {error}")))
}

fn parse_hook_response(res: &mlua::Result<Value>) -> serde_json::Value {
    let Ok(Value::Table(table)) = res else {
        return serde_json::json!({ "status": 500 });
    };
    let status = match table.get::<Option<i64>>("status") {
        Ok(Some(s)) if (100..=599).contains(&s) => s,
        _ => return serde_json::json!({ "status": 500 }),
    };
    let mut headers = serde_json::Map::new();
    if let Ok(Some(headers_table)) = table.get::<Option<Table>>("headers") {
        for pair in headers_table.pairs::<Value, Value>() {
            if let Ok((Value::String(k), v_val)) = pair
                && let Ok(k_str) = k.to_str()
                && !k_str.eq_ignore_ascii_case("set-cookie")
            {
                let v_str = match v_val {
                    Value::String(s) => s.to_str().map(|b| b.to_string()).unwrap_or_default(),
                    Value::Integer(i) => i.to_string(),
                    Value::Number(n) => n.to_string(),
                    Value::Boolean(b) => b.to_string(),
                    _ => String::new(),
                };
                headers.insert(k_str.to_ascii_lowercase(), serde_json::Value::String(v_str));
            }
        }
    }
    let body = match table.get::<Option<Value>>("body") {
        Ok(Some(Value::String(s))) => s.to_str().map(|b| b.to_string()).unwrap_or_default(),
        Ok(Some(Value::Buffer(b))) => String::from_utf8_lossy(&b.to_vec()).into_owned(),
        _ => String::new(),
    };
    serde_json::json!({
        "status": status,
        "headers": serde_json::Value::Object(headers),
        "body": body,
    })
}

/// Runs each handler for the event in its own task, with its own budget.
fn dispatch(
    vm: &Vm,
    env: &Arc<PluginEnv>,
    spec: &Spec,
    event: super::PluginEvent,
    running: &mut JoinSet<()>,
    reports: &mpsc::UnboundedSender<Report>,
) {
    let limit = spec.limits.handler_time;
    if event.name == "command" {
        return command_calls::run_command(vm, env, limit, event, running, reports);
    }
    if event.name == "command_autocomplete" {
        return command_calls::run_autocomplete(vm, env, limit, event, running, reports);
    }
    if super::view_calls::NAMES.contains(&event.name.as_str()) {
        return super::view_calls::dispatch(vm, env, limit, event, running, reports);
    }
    let super::PluginEvent {
        name,
        payload,
        reply,
        command: _,
    } = event;

    if name == "hook" {
        let path = payload
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let hooks_registry: Table = match vm.lua.named_registry_value(api::hooks::HOOKS) {
            Ok(t) => t,
            Err(error) => return failed(env, reports, &name, error, limit),
        };
        let handler: Option<Function> = hooks_registry.raw_get(path).ok();
        let Some(handler) = handler else {
            return;
        };
        let options = mlua::SerializeOptions::new()
            .serialize_none_to_null(false)
            .serialize_unit_to_null(false)
            .set_array_metatable(false);
        let request_val = match vm.lua.to_value_with(&payload, options) {
            Ok(v) => v,
            Err(error) => return failed(env, reports, &name, error, limit),
        };
        let (env_c, reports_c, name_c) = (env.clone(), reports.clone(), name.clone());
        let call = Budgeted::new(env.budget.clone(), limit, async move {
            let res = handler.call_async::<Value>(request_val).await;
            if let Some(reply) = reply {
                let answer = parse_hook_response(&res);
                if reply.send(answer).is_err() {
                    tracing::debug!("hook reply receiver dropped before response was sent");
                }
            }
            res.map(|_| ())
        });
        running.spawn(async move {
            if let Err(error) = call.await {
                failed(&env_c, &reports_c, &name_c, error, limit);
            }
        });
        return;
    }

    if name == "timer" {
        let timer_id = payload
            .get("timer_id")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let repeating = payload
            .get("repeating")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let timers_reg: Table = match vm.lua.named_registry_value(api::timers::TIMERS) {
            Ok(t) => t,
            Err(error) => return failed(env, reports, &name, error, limit),
        };
        let handler: Option<Function> = timers_reg.raw_get(timer_id).ok();
        if !repeating {
            timers_reg.raw_remove(timer_id).ok();
            env.timers
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&timer_id);
        }
        let Some(handler) = handler else {
            return;
        };
        let (env_c, reports_c, name_c) = (env.clone(), reports.clone(), name.clone());
        let call = Budgeted::new(env.budget.clone(), limit, async move {
            handler.call_async::<()>(()).await
        });
        running.spawn(async move {
            if let Err(error) = call.await {
                failed(&env_c, &reports_c, &name_c, error, limit);
            }
        });
        return;
    }

    if name == "plugin_bus" {
        let sender = payload
            .get("sender")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let event_name = payload
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let data_val = payload
            .get("data")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        let listeners: Table = match vm.lua.named_registry_value(api::bus::BUS_LISTENERS) {
            Ok(t) => t,
            Err(error) => return failed(env, reports, &name, error, limit),
        };
        let key = format!("{sender}:{event_name}");
        let handlers: Vec<Function> = match listeners.raw_get::<Table>(&*key) {
            Ok(list) => list
                .sequence_values::<Function>()
                .filter_map(Result::ok)
                .collect(),
            Err(_) => Vec::new(),
        };
        let options = mlua::SerializeOptions::new()
            .serialize_none_to_null(false)
            .serialize_unit_to_null(false)
            .set_array_metatable(false);
        let lua_data = match vm.lua.to_value_with(&data_val, options) {
            Ok(v) => v,
            Err(error) => return failed(env, reports, &name, error, limit),
        };
        for handler in handlers {
            let (env_c, reports_c, name_c) = (env.clone(), reports.clone(), name.clone());
            let data_clone = lua_data.clone();
            let call = Budgeted::new(env.budget.clone(), limit, async move {
                handler.call_async::<()>(data_clone).await
            });
            running.spawn(async move {
                if let Err(error) = call.await {
                    failed(&env_c, &reports_c, &name_c, error, limit);
                }
            });
        }
        return;
    }

    let handlers = match handlers_for(&vm.lua, &name) {
        Ok(handlers) => handlers,
        Err(error) => return failed(env, reports, &name, error, limit),
    };
    // Without handlers the reply sender drops here, and its waiter hears.
    let reply = reply.map(|sender| Arc::new(Mutex::new(Some(sender))));
    for handler in handlers {
        let argument = match event_table(&vm.lua, &payload, reply.as_ref()) {
            Ok(argument) => argument,
            Err(error) => return failed(env, reports, &name, error, limit),
        };
        let call = Budgeted::new(
            env.budget.clone(),
            limit,
            handler.call_async::<()>(argument),
        );
        let (env, reports, name) = (env.clone(), reports.clone(), name.clone());
        running.spawn(async move {
            if let Err(error) = call.await {
                failed(&env, &reports, &name, error, limit);
            }
        });
    }
}

/// Logs a failed handler with its traceback, and counts it.
pub(super) fn failed(
    env: &PluginEnv,
    reports: &mpsc::UnboundedSender<Report>,
    event: &str,
    error: mlua::Error,
    limit: Duration,
) {
    let error = match VmError::from(error) {
        // The budget's own limit is only what was left of the handler's.
        VmError::BudgetExceeded { .. } => VmError::BudgetExceeded { limit },
        other => other,
    };
    let summary = format!("{event} handler failed: {error}");
    let text = match &error {
        VmError::Lua {
            traceback: Some(traceback),
            ..
        } => format!("{summary}\n{traceback}"),
        _ => summary.clone(),
    };
    env.log.push(proto::PluginLogLevel::Error, &text);
    let out_of_memory = matches!(error, VmError::OutOfMemory);
    report(
        reports,
        Report::Failed {
            error: summary,
            out_of_memory,
        },
    );
}

/// Delivers `stop` and waits for its handlers, at most the grace. Other
/// handlers keep running meanwhile, and end with the VM.
async fn stop_vm(
    vm: &Vm,
    env: &Arc<PluginEnv>,
    spec: &Spec,
    reports: &mpsc::UnboundedSender<Report>,
) {
    let limit = spec.limits.handler_time;
    let handlers = match handlers_for(&vm.lua, "stop") {
        Ok(handlers) => handlers,
        Err(error) => return failed(env, reports, "stop", error, limit),
    };
    let payload = serde_json::json!({});
    let mut calls = Vec::new();
    for handler in handlers {
        match event_table(&vm.lua, &payload, None) {
            Ok(argument) => calls.push(Budgeted::new(
                env.budget.clone(),
                limit,
                handler.call_async::<()>(argument),
            )),
            Err(error) => failed(env, reports, "stop", error, limit),
        }
    }
    // Views closed as it stops are heard within the same grace.
    let closing = super::view_calls::closing(vm, env);
    let close_handlers = match handlers_for(&vm.lua, "view_close") {
        Ok(handlers) => handlers,
        Err(error) => {
            failed(env, reports, "view_close", error, limit);
            Vec::new()
        }
    };
    for argument in closing {
        for handler in &close_handlers {
            calls.push(Budgeted::new(
                env.budget.clone(),
                limit,
                handler.call_async::<()>(argument.clone()),
            ));
        }
    }
    // Then sessions ended as it stops, after their views closed, within
    // the same grace.
    let ending = super::view_calls::ending(vm, env);
    let end_handlers = match handlers_for(&vm.lua, "activity_end") {
        Ok(handlers) => handlers,
        Err(error) => {
            failed(env, reports, "activity_end", error, limit);
            Vec::new()
        }
    };
    let mut ends = Vec::new();
    for argument in ending {
        for handler in &end_handlers {
            ends.push(Budgeted::new(
                env.budget.clone(),
                limit,
                handler.call_async::<()>(argument.clone()),
            ));
        }
    }
    let all = async {
        let mut results = futures_util::future::join_all(calls).await;
        results.extend(futures_util::future::join_all(ends).await);
        results
    };
    match tokio::time::timeout(STOP_GRACE, all).await {
        Ok(results) => {
            for error in results.into_iter().filter_map(Result::err) {
                failed(env, reports, "stop", error, limit);
            }
        }
        Err(_) => env.log.push(
            proto::PluginLogLevel::Warn,
            "stop handlers were cut off: they ran past the 1 s a stopping plugin gets",
        ),
    }
}

#[derive(Default)]
struct Stats {
    samples: VecDeque<(Instant, Duration)>,
    dropped: u64,
    dropped_logged: Option<Instant>,
}

impl Stats {
    fn sample(&mut self, vm: &Vm, handle: &PluginHandle) {
        let now = Instant::now();
        let total = vm.budget().total_used();
        self.samples.push_back((now, total));
        while self.samples.len() > CPU_SAMPLES {
            self.samples.pop_front();
        }
        let permille = self.samples.front().map_or(0, |&(at, before)| {
            let span = now.saturating_duration_since(at).as_secs_f64();
            if span > 0.0 {
                (total.saturating_sub(before).as_secs_f64() / span * 1000.0).round() as u32
            } else {
                0
            }
        });
        handle.cpu_permille.store(permille, Ordering::Relaxed);
        handle
            .memory_bytes
            .store(vm.lua.used_memory() as u64, Ordering::Relaxed);

        self.dropped += handle.queue.take_dropped();
        if self.dropped > 0
            && self
                .dropped_logged
                .is_none_or(|at| now.saturating_duration_since(at) >= DROPS_LOGGED_EVERY)
        {
            handle.log.push(
                proto::PluginLogLevel::Warn,
                &format!(
                    "{} events were dropped: the plugin was too slow",
                    self.dropped
                ),
            );
            self.dropped = 0;
            self.dropped_logged = Some(now);
        }
    }
}

/// Failures and restarts, for the failure policy (spec §10.1).
#[derive(Default)]
struct Health {
    failures: VecDeque<Instant>,
    breaks: VecDeque<Instant>,
}

impl Health {
    /// Counts a failed handler; true when the VM must restart.
    fn failed(&mut self, now: Instant) -> bool {
        prune(&mut self.failures, now, FAILURE_WINDOW);
        self.failures.push_back(now);
        self.failures.len() >= FAILURES_BEFORE_RESTART
    }

    /// Counts a broken VM: how long to wait before restarting it, or `None`
    /// when this is the [`RESTARTS_BEFORE_OFF`]th in the window and the
    /// plugin is turned off instead.
    fn broke(&mut self, now: Instant) -> Option<Duration> {
        self.failures.clear();
        prune(&mut self.breaks, now, RESTART_WINDOW);
        self.breaks.push_back(now);
        let count = self.breaks.len();
        (count < RESTARTS_BEFORE_OFF).then(|| backoff(count))
    }
}

fn prune(times: &mut VecDeque<Instant>, now: Instant, window: Duration) {
    while times
        .front()
        .is_some_and(|&at| now.saturating_duration_since(at) >= window)
    {
        times.pop_front();
    }
}

/// 1, 2, 4 … seconds, at most [`RESTART_BACKOFF_MAX`].
fn backoff(restarts: usize) -> Duration {
    let doublings = u32::try_from(restarts.saturating_sub(1))
        .unwrap_or(u32::MAX)
        .min(16);
    RESTART_BACKOFF_FIRST
        .saturating_mul(1 << doublings)
        .min(RESTART_BACKOFF_MAX)
}

/// How one VM's life ended.
enum Life {
    Stopped,
    /// The first start failed: `Plugins::start` and the handle have the
    /// error.
    NeverStarted,
    Broke(String),
}

/// Runs a plugin until it is stopped, fails its first start, or is turned
/// off. Its state lives in `handle`.
pub(crate) async fn supervise(
    state: Weak<AppState>,
    handle: Arc<PluginHandle>,
    spec: Arc<Spec>,
    clock: Arc<dyn Clock>,
    mut stopping: watch::Receiver<bool>,
    started: oneshot::Sender<Result<(), String>>,
) {
    let mut started = Some(started);
    let mut health = Health::default();
    loop {
        let life = run_once(
            &state,
            &handle,
            &spec,
            &*clock,
            &mut health,
            &mut stopping,
            &mut started,
        )
        .await;
        let error = match life {
            Life::Stopped => return,
            Life::NeverStarted => return,
            Life::Broke(error) => error,
        };
        let Some(wait) = health.broke(clock.now()) else {
            return turn_off(&state, &handle, &error).await;
        };
        handle.set_status(
            proto::PluginState::Starting,
            Some(format!("restarting after: {error}")),
        );
        handle.log.push(
            proto::PluginLogLevel::Warn,
            &format!("restarting in {} s after: {error}", wait.as_secs()),
        );
        tokio::select! {
            () = clock.sleep(wait) => {}
            _ = stopping.wait_for(|stop| *stop) => return,
        }
    }
}

async fn run_once(
    state: &Weak<AppState>,
    handle: &Arc<PluginHandle>,
    spec: &Arc<Spec>,
    clock: &dyn Clock,
    health: &mut Health,
    stopping: &mut watch::Receiver<bool>,
    started: &mut Option<oneshot::Sender<Result<(), String>>>,
) -> Life {
    let first_failed =
        |started: &mut Option<oneshot::Sender<_>>, error: String| match started.take() {
            Some(waiter) => {
                // The handle first, so `Plugins::start` returns with it set.
                handle.set_status(proto::PluginState::Stopped, Some(error.clone()));
                // `Plugins::start` may have given up waiting; the handle says it.
                waiter.send(Err(error)).ok();
                Life::NeverStarted
            }
            None => Life::Broke(error),
        };
    // Opened here, on the server's runtime, where its connections stay.
    let db = match database::open(
        &spec.database,
        spec.database_bytes,
        spec.limits.memory_bytes,
    )
    .await
    {
        Ok(db) => db,
        Err(error) => {
            return first_failed(
                started,
                format!("could not open the plugin's database: {error}"),
            );
        }
    };
    let (reports_sender, mut reports) = mpsc::unbounded_channel();
    let (commands, commands_receiver) = watch::channel(Command::Run);
    if let Some(state) = state.upgrade() {
        state.plugins.channel_types.begin(&handle.id);
        state.plugins.activities.begin(&handle.id);
    }
    if let Err(error) = spawn_vm(
        spec.clone(),
        handle.clone(),
        reports_sender,
        commands_receiver,
        state.clone(),
        db.clone(),
    ) {
        db.close().await;
        return first_failed(
            started,
            format!("could not start the plugin's thread: {error}"),
        );
    }
    let life = loop {
        let sent = tokio::select! {
            biased;
            _ = stopping.wait_for(|stop| *stop) => {
                commands.send_replace(Command::Stop);
                break Life::Stopped;
            }
            sent = reports.recv() => sent,
        };
        match sent {
            Some(Report::Started(Ok(()))) => {
                handle.set_status(proto::PluginState::Running, None);
                // Sent before the start returns, so whoever started it
                // hears the channels change first.
                if let Some(state) = state.upgrade() {
                    channel_types::started(&state, &handle.id).await;
                    activities::started(&state, &handle.id).await;
                }
                if let Some(waiter) = started.take() {
                    // A caller that gave up waiting reads the handle.
                    waiter.send(Ok(())).ok();
                }
            }
            Some(Report::Started(Err(error))) => break first_failed(started, error),
            Some(Report::Failed {
                error,
                out_of_memory,
            }) => {
                if out_of_memory || health.failed(clock.now()) {
                    commands.send_replace(Command::Kill);
                    break Life::Broke(error);
                }
            }
            None => break first_failed(started, "the plugin's thread ended".to_owned()),
        }
    };
    // The thread holds a sender until its runtime and VM are gone.
    while reports.recv().await.is_some() {}
    // Transactions the VM left open roll back once its calls are dropped,
    // which happened with the VM; then every connection closes.
    db.close().await;
    life
}

/// Turns the plugin off after it kept failing: saved, shown, announced.
async fn turn_off(state: &Weak<AppState>, handle: &PluginHandle, error: &str) {
    let reason =
        format!("turned off after failing {RESTARTS_BEFORE_OFF} times in an hour: {error}");
    tracing::warn!(plugin = %handle.id, "a plugin was turned off after failing again and again");
    let state = state.upgrade();
    if let Some(state) = &state
        && let Err(error) = super::save_crashed_reason(state, &handle.id, Some(&reason)).await
    {
        // It is off in memory either way; the next boot starts it again.
        tracing::warn!(plugin = %handle.id, %error, "could not save why a plugin was turned off");
    }
    handle.set_status(proto::PluginState::Crashed, Some(reason.clone()));
    handle.queue.clear();
    handle.log.push(proto::PluginLogLevel::Error, &reason);
    if let Some(state) = &state {
        channel_types::stopped(state, &handle.id).await;
        activities::stopped(state, &handle.id).await;
        super::announce(state, &handle.id).await;
    }
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
