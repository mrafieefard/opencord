//! The Luau sandbox every plugin script runs in (spec §10.1, §10.2).
//!
//! A [`Vm`] is one Luau state in sandbox mode, with a memory cap and a
//! CPU budget enforced by Luau's interrupt. Lua runs only while the host
//! has a budget running: [`Budget::start`] before calling into the VM,
//! [`Budget::stop`] after. Host functions that wait (HTTP, database,
//! files) wrap the wait in [`paused`], so waiting doesn't count.

use std::cell::RefCell;
use std::fmt;
use std::future::Future;
use std::io::Write as _;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use mlua::{ChunkMode, Function, Lua, LuaOptions, StdLib, Table, Value, Variadic, VmState};
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};

use crate::limits::LOG_LINE_MAX_CHARS;

/// What one VM may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VmLimits {
    pub memory_bytes: usize,
    pub handler_time: Duration,
}

/// Why Lua stopped.
#[derive(Debug, Clone)]
pub enum VmError {
    /// The running Lua used up its budget, or ran with none started.
    BudgetExceeded {
        limit: Duration,
    },
    OutOfMemory,
    Lua {
        message: String,
        traceback: Option<String>,
    },
}

impl fmt::Display for VmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VmError::BudgetExceeded { limit } => {
                write!(f, "the handler ran longer than {} ms", limit.as_millis())
            }
            VmError::OutOfMemory => f.write_str("the plugin ran out of memory"),
            VmError::Lua { message, .. } => f.write_str(message),
        }
    }
}

impl std::error::Error for VmError {}

impl From<mlua::Error> for VmError {
    fn from(error: mlua::Error) -> Self {
        classify(&error, None)
    }
}

/// Finds the budget or memory failure under mlua's wrappers; anything else
/// is the Lua message, with the traceback split off.
fn classify(error: &mlua::Error, traceback: Option<&str>) -> VmError {
    match error {
        mlua::Error::CallbackError {
            cause,
            traceback: inner,
        } => classify(cause, Some(inner)),
        mlua::Error::WithContext { cause, .. } => match classify(cause, traceback) {
            VmError::Lua { .. } => lua_error(error.to_string(), traceback),
            found => found,
        },
        mlua::Error::ExternalError(cause) => match cause.downcast_ref::<OutOfBudget>() {
            Some(out) => VmError::BudgetExceeded { limit: out.limit },
            None => lua_error(cause.to_string(), traceback),
        },
        mlua::Error::MemoryError(_) => VmError::OutOfMemory,
        mlua::Error::RuntimeError(text) => lua_error(text.clone(), traceback),
        other => lua_error(other.to_string(), traceback),
    }
}

fn lua_error(text: String, traceback: Option<&str>) -> VmError {
    const MARK: &str = "\nstack traceback:\n";
    match text.split_once(MARK) {
        Some((message, trace)) => VmError::Lua {
            message: message.to_owned(),
            traceback: Some(trace.to_owned()),
        },
        None => VmError::Lua {
            message: text,
            traceback: traceback
                .map(|trace| trace.trim_start_matches("stack traceback:\n").to_owned()),
        },
    }
}

/// The interrupt's error, found again by [`classify`].
#[derive(Debug)]
struct OutOfBudget {
    limit: Duration,
}

impl fmt::Display for OutOfBudget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        VmError::BudgetExceeded { limit: self.limit }.fmt(f)
    }
}

impl std::error::Error for OutOfBudget {}

/// The deadline when no budget is running: Lua is stopped at once.
const IDLE: u64 = u64::MAX;
/// The deadline while every running budget waits on the host.
const PAUSED: u64 = u64::MAX - 1;
const LAST_DEADLINE: u64 = u64::MAX - 2;

/// One VM's CPU budget: running time since [`Budget::start`], not counting
/// time [`Budget::pause`]d.
#[derive(Clone)]
pub struct Budget {
    inner: Arc<BudgetInner>,
}

struct BudgetInner {
    epoch: Instant,
    /// When the running Lua must stop, in nanoseconds since `epoch`; or
    /// [`IDLE`] or [`PAUSED`]. Read by the interrupt without the lock;
    /// written only under it.
    deadline: AtomicU64,
    state: Mutex<BudgetState>,
}

#[derive(Default)]
struct BudgetState {
    /// Counts `start`s, so a pause from an earlier run can't resume this one.
    run: u64,
    active: bool,
    limit: Duration,
    /// This run's running time before `since`.
    used: Duration,
    /// All runs' running time before `since`.
    total: Duration,
    /// When Lua last started running; `None` while paused or idle.
    since: Option<Instant>,
    pauses: u32,
}

impl BudgetState {
    fn close(&mut self, now: Instant) {
        if let Some(since) = self.since.take() {
            let ran = now.saturating_duration_since(since);
            self.used += ran;
            self.total += ran;
        }
    }

    fn running(&self, now: Instant) -> Duration {
        self.since
            .map_or(Duration::ZERO, |since| now.saturating_duration_since(since))
    }
}

impl Budget {
    fn new() -> Budget {
        Budget {
            inner: Arc::new(BudgetInner {
                epoch: Instant::now(),
                deadline: AtomicU64::new(IDLE),
                state: Mutex::new(BudgetState::default()),
            }),
        }
    }

    /// Starts a run with `limit` of running time, ending any run before it.
    pub fn start(&self, limit: Duration) {
        let now = Instant::now();
        let mut state = self.lock();
        state.close(now);
        state.run += 1;
        state.active = true;
        state.limit = limit;
        state.used = Duration::ZERO;
        state.pauses = 0;
        state.since = Some(now);
        self.publish(&state);
    }

    /// Stops counting until the matching [`Budget::resume`]. Pauses nest.
    pub fn pause(&self) {
        let mut state = self.lock();
        self.pause_locked(&mut state);
    }

    pub fn resume(&self) {
        let mut state = self.lock();
        if state.active && state.pauses > 0 {
            self.resume_locked(&mut state);
        }
    }

    /// Ends the run. Lua that runs after this is stopped at once.
    pub fn stop(&self) {
        let now = Instant::now();
        let mut state = self.lock();
        state.close(now);
        state.active = false;
        state.pauses = 0;
        self.publish(&state);
    }

    /// Running time of the current (or last) run.
    pub fn used(&self) -> Duration {
        let now = Instant::now();
        let state = self.lock();
        state.used + state.running(now)
    }

    /// Running time of every run since the VM was made.
    pub fn total_used(&self) -> Duration {
        let now = Instant::now();
        let state = self.lock();
        state.total + state.running(now)
    }

    /// Pauses the current run until the guard is dropped. A guard dropped
    /// after its run ended leaves later runs alone.
    fn pause_guard(&self) -> PauseGuard {
        let mut state = self.lock();
        let run = self.pause_locked(&mut state).then_some(state.run);
        PauseGuard {
            budget: self.clone(),
            run,
        }
    }

    /// Returns whether a run was active to pause.
    fn pause_locked(&self, state: &mut BudgetState) -> bool {
        if !state.active {
            return false;
        }
        state.pauses += 1;
        if state.pauses == 1 {
            state.close(Instant::now());
            self.publish(state);
        }
        true
    }

    fn resume_locked(&self, state: &mut BudgetState) {
        state.pauses -= 1;
        if state.pauses == 0 {
            state.since = Some(Instant::now());
            self.publish(state);
        }
    }

    fn publish(&self, state: &BudgetState) {
        let deadline = match (state.active, state.since) {
            (false, _) => IDLE,
            (true, None) => PAUSED,
            (true, Some(since)) => {
                let remaining = state.limit.saturating_sub(state.used);
                self.nanos(since)
                    .saturating_add(u64::try_from(remaining.as_nanos()).unwrap_or(u64::MAX))
                    .min(LAST_DEADLINE)
            }
        };
        // The lock orders writers; the interrupt only needs to see a
        // recent value, on the thread that runs Lua.
        self.inner.deadline.store(deadline, Ordering::Relaxed);
    }

    fn nanos(&self, at: Instant) -> u64 {
        u64::try_from(at.saturating_duration_since(self.inner.epoch).as_nanos())
            .unwrap_or(LAST_DEADLINE)
    }

    /// Luau's interrupt: at every call and loop, stop Lua past its deadline.
    fn check(&self) -> mlua::Result<VmState> {
        let deadline = self.inner.deadline.load(Ordering::Relaxed);
        if deadline == PAUSED || (deadline != IDLE && self.nanos(Instant::now()) <= deadline) {
            return Ok(VmState::Continue);
        }
        let limit = {
            let state = self.lock();
            if state.active {
                state.limit
            } else {
                Duration::ZERO
            }
        };
        Err(mlua::Error::external(OutOfBudget { limit }))
    }

    fn lock(&self) -> MutexGuard<'_, BudgetState> {
        self.inner
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

struct PauseGuard {
    budget: Budget,
    /// The run this guard paused, if any.
    run: Option<u64>,
}

impl Drop for PauseGuard {
    fn drop(&mut self) {
        let Some(run) = self.run else { return };
        let mut state = self.budget.lock();
        if state.active && state.run == run && state.pauses > 0 {
            self.budget.resume_locked(&mut state);
        }
    }
}

/// Runs `f` with the budget paused: time spent waiting on the host isn't
/// the plugin's. Resumes when `f` finishes or is dropped.
pub async fn paused<F: Future>(budget: &Budget, f: F) -> F::Output {
    let _resume = budget.pause_guard();
    f.await
}

/// A future whose Lua gets `limit` of running time over all its polls.
/// Each poll starts the VM's budget with what is left, so waiting between
/// polls doesn't count, and futures polled in turn each keep their own time.
pub struct Budgeted<F> {
    call: Pin<Box<F>>,
    budget: Budget,
    left: Duration,
}

impl<F> Budgeted<F> {
    pub fn new(budget: Budget, limit: Duration, call: F) -> Self {
        Budgeted {
            call: Box::pin(call),
            budget,
            left: limit,
        }
    }
}

impl<F: Future> Future for Budgeted<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        let this = &mut *self;
        this.budget.start(this.left);
        let poll = this.call.as_mut().poll(cx);
        this.left = this.left.saturating_sub(this.budget.used());
        this.budget.stop();
        poll
    }
}

type PrintHook = Arc<RwLock<Option<Arc<dyn Fn(String) + Send + Sync>>>>;

/// One plugin's Luau state.
pub struct Vm {
    pub lua: Lua,
    budget: Budget,
    print_hook: PrintHook,
}

/// Luau's libraries a VM loads. `os` is cut down to three functions below.
fn libraries() -> StdLib {
    StdLib::COROUTINE
        | StdLib::TABLE
        | StdLib::OS
        | StdLib::STRING
        | StdLib::UTF8
        | StdLib::BIT
        | StdLib::BUFFER
        | StdLib::MATH
}

/// Every global a VM keeps from Luau's environment; the rest are removed.
/// `collectgarbage` only answers `"count"` once sandboxed.
const KEPT_GLOBALS: &[&str] = &[
    "string",
    "table",
    "math",
    "utf8",
    "bit32",
    "buffer",
    "coroutine",
    "assert",
    "error",
    "ipairs",
    "pairs",
    "next",
    "select",
    "tonumber",
    "tostring",
    "type",
    "typeof",
    "pcall",
    "xpcall",
    "rawequal",
    "rawget",
    "rawset",
    "rawlen",
    "setmetatable",
    "getmetatable",
    "unpack",
    "collectgarbage",
];

const OS_FUNCTIONS: &[&str] = &["time", "clock", "date"];

impl Vm {
    pub fn new(limits: &VmLimits) -> Result<Vm, VmError> {
        let lua = Lua::new_with(libraries(), LuaOptions::new())?;
        lua.set_memory_limit(limits.memory_bytes)?;
        let print_hook = PrintHook::default();
        build_globals(&lua, &print_hook, limits.memory_bytes)?;
        lua.sandbox(true)?;
        let budget = Budget::new();
        let interrupt = budget.clone();
        lua.set_interrupt(move |_| interrupt.check());
        Ok(Vm {
            lua,
            budget,
            print_hook,
        })
    }

    /// Compiles `source` as text; bytecode is refused. Errors name the
    /// chunk as given, without Lua's `[string "…"]`.
    pub fn load(&self, chunk_name: &str, source: &str) -> Result<Function, VmError> {
        Ok(self
            .lua
            .load(source)
            .set_name(format!("={chunk_name}"))
            .set_mode(ChunkMode::Text)
            .into_function()?)
    }

    pub fn budget(&self) -> &Budget {
        &self.budget
    }

    /// Where `print` goes. Without a hook, printed lines are dropped.
    pub fn on_print(&self, hook: Box<dyn Fn(String) + Send + Sync>) {
        *self
            .print_hook
            .write()
            .unwrap_or_else(PoisonError::into_inner) = Some(Arc::from(hook));
    }
}

/// Replaces Luau's environment with the plugin's, before `sandbox` freezes it.
fn build_globals(lua: &Lua, print_hook: &PrintHook, memory_bytes: usize) -> mlua::Result<()> {
    let globals = lua.globals();
    let tostring: Function = globals.raw_get("tostring")?;
    let os: Table = globals.raw_get("os")?;

    let names = globals
        .pairs::<Value, Value>()
        .map(|pair| pair.map(|(name, _)| name))
        .collect::<mlua::Result<Vec<_>>>()?;
    for name in names {
        let kept = match &name {
            Value::String(name) => KEPT_GLOBALS
                .iter()
                .any(|kept| kept.as_bytes() == &*name.as_bytes()),
            _ => false,
        };
        if !kept {
            globals.raw_set(name, Value::Nil)?;
        }
    }

    let safe_os = lua.create_table()?;
    for name in OS_FUNCTIONS {
        safe_os.raw_set(*name, os.raw_get::<Function>(*name)?)?;
    }
    globals.raw_set("os", safe_os)?;
    globals.raw_set("json", json_table(lua, memory_bytes)?)?;
    globals.raw_set("print", print_function(lua, tostring, print_hook.clone())?)?;
    Ok(())
}

/// `print`: arguments through `tostring`, joined by tabs, cut at a log
/// line's length so a huge argument never reaches the host whole.
fn print_function(lua: &Lua, tostring: Function, hook: PrintHook) -> mlua::Result<Function> {
    lua.create_function(move |_, args: Variadic<Value>| {
        let hook = hook.read().unwrap_or_else(PoisonError::into_inner).clone();
        let Some(hook) = hook else { return Ok(()) };
        let mut line = String::new();
        let mut room = LOG_LINE_MAX_CHARS;
        for (index, value) in args.into_iter().enumerate() {
            if index > 0 {
                push_capped(&mut line, &mut room, "\t");
            }
            if room == 0 {
                break;
            }
            let text = match value {
                Value::String(text) => text,
                other => tostring.call::<mlua::String>(other)?,
            };
            let bytes = text.as_bytes();
            let prefix = &bytes[..bytes.len().min(room.saturating_mul(4))];
            push_capped(&mut line, &mut room, &String::from_utf8_lossy(prefix));
        }
        hook(line);
        Ok(())
    })
}

fn push_capped(line: &mut String, room: &mut usize, text: &str) {
    for c in text.chars().take(*room) {
        line.push(c);
        *room -= 1;
    }
}

/// How deep `json.encode` follows nested tables; `json.decode` has
/// serde_json's limit, the same number.
const JSON_MAX_DEPTH: usize = 128;

/// Turns the Rust halves' `(false, message)` into a Lua string error, as
/// every error raised into Lua is.
const JSON_WRAPPER: &str = r#"
local encode, decode = ...
local function check(ok, result)
    if not ok then
        error(result, 0)
    end
    return result
end
return {
    encode = function(value) return check(encode(value)) end,
    decode = function(text) return check(decode(text)) end,
}
"#;

enum JsonError {
    /// Input the plugin can fix; raised as `INVALID_ARGUMENT: …`.
    Invalid(String),
    /// Lua itself failed, out of memory for one.
    Lua(mlua::Error),
}

impl From<mlua::Error> for JsonError {
    fn from(error: mlua::Error) -> Self {
        JsonError::Lua(error)
    }
}

fn invalid<T>(message: impl Into<String>) -> Result<T, JsonError> {
    Err(JsonError::Invalid(message.into()))
}

fn json_answer(lua: &Lua, result: Result<Value, JsonError>) -> mlua::Result<(bool, Value)> {
    match result {
        Ok(value) => Ok((true, value)),
        Err(JsonError::Invalid(message)) => Ok((
            false,
            Value::String(lua.create_string(format!("INVALID_ARGUMENT: {message}"))?),
        )),
        Err(JsonError::Lua(error)) => Err(error),
    }
}

/// `json`: `encode`, `decode` and `null`. Arrays `decode` makes carry a
/// marker metatable, so an empty one encodes as `[]` again.
fn json_table(lua: &Lua, memory_bytes: usize) -> mlua::Result<Table> {
    let arrays = lua.create_table()?;

    let marker = arrays.clone();
    let encode = lua.create_function(move |lua, value: Value| {
        let mut out = Vec::new();
        let result = encode_json(&value, &marker, &mut out, 0, memory_bytes)
            .and_then(|()| Ok(Value::String(lua.create_string(&out)?)));
        json_answer(lua, result)
    })?;

    let decode = lua.create_function(move |lua, text: Value| {
        let result = match text {
            Value::String(text) => decode_json(lua, &text.as_bytes(), &arrays),
            _ => invalid("json.decode takes a string"),
        };
        json_answer(lua, result)
    })?;

    let json: Table = lua
        .load(JSON_WRAPPER)
        .set_name("=json")
        .set_mode(ChunkMode::Text)
        .call((encode, decode))?;
    json.raw_set("null", Value::NULL)?;
    Ok(json)
}

fn encode_json(
    value: &Value,
    arrays: &Table,
    out: &mut Vec<u8>,
    depth: usize,
    max_bytes: usize,
) -> Result<(), JsonError> {
    let written = match value {
        Value::Nil => out.write_all(b"null"),
        Value::LightUserData(data) if data.0.is_null() => out.write_all(b"null"),
        Value::Boolean(true) => out.write_all(b"true"),
        Value::Boolean(false) => out.write_all(b"false"),
        Value::Integer(number) => write!(out, "{number}"),
        Value::Number(number) => {
            if !number.is_finite() {
                return invalid("json.encode can't encode NaN or infinity");
            }
            // Whole numbers Luau holds exactly print without a fraction.
            const EXACT: f64 = 9_007_199_254_740_992.0;
            if number.fract() == 0.0 && number.abs() < EXACT {
                write!(out, "{}", *number as i64)
            } else {
                serde_json::to_writer(&mut *out, number).map_err(std::io::Error::from)
            }
        }
        Value::String(text) => {
            let Ok(text) = text.to_str() else {
                return invalid("json.encode takes text in UTF-8 only");
            };
            if out.len().saturating_add(text.len()) > max_bytes {
                return too_large();
            }
            serde_json::to_writer(&mut *out, &*text).map_err(std::io::Error::from)
        }
        Value::Table(table) => return encode_table(table, arrays, out, depth, max_bytes),
        other => return invalid(format!("json.encode can't encode a {}", other.type_name())),
    };
    if let Err(error) = written {
        return Err(JsonError::Lua(mlua::Error::external(error)));
    }
    if out.len() > max_bytes {
        return too_large();
    }
    Ok(())
}

fn too_large<T>() -> Result<T, JsonError> {
    invalid("json.encode: the result is larger than the plugin's memory")
}

fn encode_table(
    table: &Table,
    arrays: &Table,
    out: &mut Vec<u8>,
    depth: usize,
    max_bytes: usize,
) -> Result<(), JsonError> {
    if depth >= JSON_MAX_DEPTH {
        return invalid(format!(
            "json.encode: tables nest deeper than {JSON_MAX_DEPTH} levels, or contain themselves"
        ));
    }
    if is_array(table, arrays)? {
        out.push(b'[');
        for index in 1..=table.raw_len() {
            if index > 1 {
                out.push(b',');
            }
            let item: Value = table.raw_get(index)?;
            encode_json(&item, arrays, out, depth + 1, max_bytes)?;
        }
        out.push(b']');
        return Ok(());
    }
    out.push(b'{');
    for (index, pair) in table.pairs::<Value, Value>().enumerate() {
        let (key, item) = pair?;
        if index > 0 {
            out.push(b',');
        }
        if !matches!(key, Value::String(_)) {
            return invalid("json.encode: an object's keys must be strings");
        }
        encode_json(&key, arrays, out, depth + 1, max_bytes)?;
        out.push(b':');
        encode_json(&item, arrays, out, depth + 1, max_bytes)?;
    }
    out.push(b'}');
    Ok(())
}

/// A table is an array when its keys are exactly 1 to its length: at least
/// one, or none when `json.decode` made it as an array.
fn is_array(table: &Table, arrays: &Table) -> mlua::Result<bool> {
    let length = table.raw_len();
    let mut count = 0;
    for pair in table.pairs::<Value, Value>() {
        let (key, _) = pair?;
        let in_range = match key {
            Value::Integer(index) => {
                usize::try_from(index).is_ok_and(|i| (1..=length).contains(&i))
            }
            Value::Number(index) => index.fract() == 0.0 && index >= 1.0 && index <= length as f64,
            _ => false,
        };
        if !in_range {
            return Ok(false);
        }
        count += 1;
    }
    if count > 0 {
        return Ok(count == length);
    }
    Ok(table
        .metatable()
        .is_some_and(|metatable| metatable.to_pointer() == arrays.to_pointer()))
}

/// Parses straight into Lua values, so every byte the result takes counts
/// against the VM's memory cap, never the server's.
fn decode_json(lua: &Lua, text: &[u8], arrays: &Table) -> Result<Value, JsonError> {
    let failure = RefCell::new(None);
    let seed = LuaSeed {
        lua,
        arrays,
        failure: &failure,
    };
    let mut parser = serde_json::Deserializer::from_slice(text);
    let parsed = seed.deserialize(&mut parser).and_then(|value| {
        parser.end()?;
        Ok(value)
    });
    if let Some(error) = failure.into_inner() {
        return Err(JsonError::Lua(error));
    }
    parsed.or_else(|error| invalid(format!("json.decode: {error}")))
}

#[derive(Clone, Copy)]
struct LuaSeed<'a> {
    lua: &'a Lua,
    arrays: &'a Table,
    /// The Lua error that stopped the parse, returned in place of serde's.
    failure: &'a RefCell<Option<mlua::Error>>,
}

impl LuaSeed<'_> {
    fn fail<E: de::Error>(&self, error: mlua::Error) -> E {
        self.failure.borrow_mut().get_or_insert(error);
        E::custom("lua failed")
    }
}

impl<'de> DeserializeSeed<'de> for LuaSeed<'_> {
    type Value = Value;

    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<Value, D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for LuaSeed<'_> {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JSON")
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Value, E> {
        Ok(Value::Boolean(value))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Value, E> {
        Ok(Value::Number(value as f64))
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Value, E> {
        Ok(Value::Number(value as f64))
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Value, E> {
        Ok(Value::Number(value))
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Value, E> {
        match self.lua.create_string(value) {
            Ok(text) => Ok(Value::String(text)),
            Err(error) => Err(self.fail(error)),
        }
    }

    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::NULL)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut items: A) -> Result<Value, A::Error> {
        let table = self.lua.create_table().map_err(|e| self.fail(e))?;
        table
            .set_metatable(Some(self.arrays.clone()))
            .map_err(|e| self.fail(e))?;
        let mut index = 1;
        while let Some(item) = items.next_element_seed(self)? {
            table.raw_set(index, item).map_err(|e| self.fail(e))?;
            index += 1;
        }
        Ok(Value::Table(table))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut entries: A) -> Result<Value, A::Error> {
        let table = self.lua.create_table().map_err(|e| self.fail(e))?;
        while let Some(key) = entries.next_key_seed(self)? {
            let item = entries.next_value_seed(self)?;
            table.raw_set(key, item).map_err(|e| self.fail(e))?;
        }
        Ok(Value::Table(table))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use super::*;

    const MIB: usize = 1024 * 1024;

    fn vm(memory_bytes: usize, handler_ms: u64) -> Vm {
        Vm::new(&VmLimits {
            memory_bytes,
            handler_time: Duration::from_millis(handler_ms),
        })
        .unwrap()
    }

    /// Runs `source` inside a budget of the VM's own size.
    fn run<R: mlua::FromLuaMulti>(vm: &Vm, source: &str) -> Result<R, VmError> {
        vm.budget().start(Duration::from_secs(5));
        let result = vm
            .load("test", source)
            .and_then(|f| f.call::<R>(()).map_err(VmError::from));
        vm.budget().stop();
        result
    }

    #[test]
    fn missing_globals_are_nil() {
        let vm = vm(16 * MIB, 100);
        for name in [
            "io",
            "debug",
            "loadstring",
            "load",
            "dofile",
            "loadfile",
            "getfenv",
            "setfenv",
            "require",
            "os.execute",
            "os.getenv",
            "os.exit",
            "os.remove",
            "newproxy",
            "gcinfo",
            "_G",
        ] {
            let is_nil: bool = run(&vm, &format!("return {name} == nil")).unwrap();
            assert!(is_nil, "{name} is reachable");
        }
        let time: f64 = run(&vm, "return os.time()").unwrap();
        assert!(time > 1.0e9);
        // Only "count" is left of collectgarbage.
        let _: f64 = run(&vm, "return collectgarbage('count')").unwrap();
        assert!(run::<()>(&vm, "collectgarbage('collect')").is_err());
    }

    #[test]
    fn listed_globals_are_present() {
        let vm = vm(16 * MIB, 100);
        for name in [
            "string",
            "table",
            "math",
            "utf8",
            "bit32",
            "buffer",
            "coroutine",
            "assert",
            "error",
            "ipairs",
            "pairs",
            "next",
            "select",
            "tonumber",
            "tostring",
            "type",
            "typeof",
            "pcall",
            "xpcall",
            "rawequal",
            "rawget",
            "rawset",
            "rawlen",
            "setmetatable",
            "getmetatable",
            "unpack",
            "os.time",
            "os.clock",
            "os.date",
            "json.encode",
            "json.decode",
            "json.null",
            "print",
        ] {
            let present: bool = run(&vm, &format!("return {name} ~= nil")).unwrap();
            assert!(present, "{name} is missing");
        }
    }

    // Catches: loading in auto mode, where Luau takes bytecode as well.
    #[test]
    fn bytecode_is_refused() {
        let vm = vm(16 * MIB, 100);
        assert!(vm.load("escape", "\x1bLua").is_err());

        let bytecode = mlua::Compiler::new().compile("return 1").unwrap();
        let bytecode = String::from_utf8_lossy(&bytecode).into_owned();
        assert!(bytecode.as_bytes()[0] < b'\n', "not Luau bytecode");
        assert!(vm.load("bytecode", &bytecode).is_err());

        let one: i32 = run(&vm, "return 1").unwrap();
        assert_eq!(one, 1);
    }

    #[test]
    fn an_endless_loop_stops_at_the_budget() {
        let vm = vm(16 * MIB, 50);
        let function = vm.load("loop", "while true do end").unwrap();
        let began = Instant::now();
        vm.budget().start(Duration::from_millis(50));
        let result = function.call::<()>(());
        vm.budget().stop();
        let error = VmError::from(result.unwrap_err());
        assert!(
            matches!(error, VmError::BudgetExceeded { .. }),
            "got {error:?}"
        );
        assert_eq!(error.to_string(), "the handler ran longer than 50 ms");
        assert!(began.elapsed() < Duration::from_millis(500));
    }

    // Catches: a loop that catches the budget's error and carries on.
    #[test]
    fn pcall_cannot_outlast_the_budget() {
        let vm = vm(16 * MIB, 50);
        let function = vm
            .load(
                "loop",
                "while true do pcall(function() while true do end end) end",
            )
            .unwrap();
        let began = Instant::now();
        vm.budget().start(Duration::from_millis(50));
        let result = function.call::<()>(());
        vm.budget().stop();
        let error = VmError::from(result.unwrap_err());
        assert!(
            matches!(error, VmError::BudgetExceeded { .. }),
            "got {error:?}"
        );
        assert!(began.elapsed() < Duration::from_millis(500));
    }

    // Catches: a host forgetting `start`, which would leave Lua unlimited.
    #[test]
    fn lua_outside_a_budget_is_stopped() {
        let vm = vm(16 * MIB, 50);
        // A long loop, not an endless one: without the guard it ends and
        // the test fails rather than hangs.
        let function = vm.load("loop", "for i = 1, 1e8 do end").unwrap();
        let error = VmError::from(function.call::<()>(()).unwrap_err());
        assert!(
            matches!(error, VmError::BudgetExceeded { .. }),
            "got {error:?}"
        );
    }

    // Catches: wall-clock budgets.
    #[tokio::test]
    async fn time_waiting_on_async_calls_does_not_count() {
        let vm = vm(16 * MIB, 100);
        let budget = vm.budget().clone();
        let wait = vm
            .lua
            .create_async_function(move |_, ()| {
                let budget = budget.clone();
                async move {
                    paused(&budget, tokio::time::sleep(Duration::from_millis(300))).await;
                    Ok(())
                }
            })
            .unwrap();
        vm.lua.globals().set("wait", wait).unwrap();

        let function = vm.load("waits", "wait() wait() wait() return 1").unwrap();
        let began = Instant::now();
        vm.budget().start(Duration::from_millis(100));
        let result = function.call_async::<i32>(()).await;
        vm.budget().stop();
        assert_eq!(result.map_err(VmError::from).unwrap(), 1);
        assert!(began.elapsed() >= Duration::from_millis(900));
        assert!(vm.budget().used() < Duration::from_millis(100));
        assert!(vm.budget().total_used() < Duration::from_millis(100));
    }

    // Catches: a resume from an earlier run reopening the current one.
    #[test]
    fn a_late_resume_does_not_touch_the_next_run() {
        let vm = vm(16 * MIB, 100);
        let budget = vm.budget().clone();
        budget.start(Duration::from_millis(100));
        let stale = budget.pause_guard();
        budget.stop();

        budget.start(Duration::from_millis(100));
        budget.pause();
        drop(stale);
        // Still paused: waiting does not count.
        std::thread::sleep(Duration::from_millis(150));
        assert!(budget.used() < Duration::from_millis(100));
        budget.resume();
        assert!(run::<()>(&vm, "local x = 0 for i = 1, 10 do x += i end").is_ok());
    }

    #[test]
    fn a_memory_bomb_stops_at_the_cap() {
        let vm = vm(8 * MIB, 100);
        // Luau interns strings, so each one differs or they'd be one string.
        let error = run::<()>(
            &vm,
            "local t = {} while true do table.insert(t, string.rep('x', 65536) .. #t) end",
        )
        .unwrap_err();
        assert!(matches!(error, VmError::OutOfMemory), "got {error:?}");
        assert_eq!(error.to_string(), "the plugin ran out of memory");

        let two: i32 = run(&vm, "return 1 + 1").unwrap();
        assert_eq!(two, 2);
    }

    #[test]
    fn builtins_cannot_be_changed() {
        let vm = vm(16 * MIB, 100);
        assert!(run::<()>(&vm, "string.upper = nil").is_err());
        assert!(run::<()>(&vm, "json.encode = nil").is_err());
        assert!(run::<()>(&vm, "os.time = nil").is_err());
        assert!(run::<()>(&vm, "getmetatable('').__index = {}").is_err());
        let upper: String = run(&vm, "return string.upper('a')").unwrap();
        assert_eq!(upper, "A");
    }

    #[test]
    fn json_round_trips() {
        let vm = vm(16 * MIB, 100);
        let same: bool = run(
            &vm,
            r#"
            local function equal(a, b)
                if type(a) ~= type(b) then return false end
                if type(a) ~= "table" then return a == b end
                for k, v in pairs(a) do if not equal(v, b[k]) then return false end end
                for k in pairs(b) do if a[k] == nil then return false end end
                return true
            end
            local input = {a = 1, b = {"x", "y"}, c = true}
            return equal(json.decode(json.encode(input)), input)
            "#,
        )
        .unwrap();
        assert!(same);

        let null: bool = run(&vm, "return json.decode('null') == json.null").unwrap();
        assert!(null);

        let texts: Vec<String> = run(
            &vm,
            r#"return {
                json.encode({1, 2.5, "a\n\"b\"", json.null, false}),
                json.encode(json.decode("[]")),
                json.encode({}),
                json.encode(json.decode('{"k":[{"n":null}]}')),
            }"#,
        )
        .unwrap();
        assert_eq!(
            texts,
            [
                r#"[1,2.5,"a\n\"b\"",null,false]"#,
                "[]",
                "{}",
                r#"{"k":[{"n":null}]}"#,
            ]
        );
    }

    // Catches: mlua's own serde, whose recursion has no depth limit and
    // whose output has no size limit; both take the server down.
    #[test]
    fn json_refuses_what_it_cannot_hold() {
        let vm = vm(16 * MIB, 100);
        for source in [
            "local t = {} t.self = t json.encode(t)",
            "local t = {} for i = 1, 1000 do t = {t} end json.encode(t)",
            "json.encode({print})",
            "json.encode({[{}] = 1})",
            "json.encode(0/0)",
            "local s = string.rep('x', 1024 * 1024) local t = {} \
             for i = 1, 4000 do t[i] = s end json.encode(t)",
            "json.decode(string.rep('[', 100000))",
            "json.decode('{')",
        ] {
            let error = run::<()>(&vm, source).unwrap_err();
            assert!(
                matches!(&error, VmError::Lua { message, .. } if message.contains("INVALID_ARGUMENT: json.")),
                "{source}: {error:?}"
            );
        }

        // Caught, the error is the plain string every error into Lua is.
        let caught: String = run(&vm, "local ok, e = pcall(json.decode, '{') return e").unwrap();
        assert!(
            caught.starts_with("INVALID_ARGUMENT: json.decode: "),
            "{caught}"
        );
    }

    #[test]
    fn vms_share_nothing() {
        let first = vm(16 * MIB, 100);
        let second = vm(16 * MIB, 100);
        run::<()>(&first, "shared = 1").unwrap();
        let one: i32 = run(&first, "return shared").unwrap();
        assert_eq!(one, 1);
        let is_nil: bool = run(&second, "return shared == nil").unwrap();
        assert!(is_nil);
    }

    #[test]
    fn print_reaches_the_hook() {
        let vm = vm(16 * MIB, 100);
        // Without a hook, print is dropped.
        run::<()>(&vm, "print('nobody hears this')").unwrap();

        let lines = Arc::new(Mutex::new(Vec::new()));
        let sink = lines.clone();
        vm.on_print(Box::new(move |line| sink.lock().unwrap().push(line)));
        run::<()>(&vm, "print('a', 1)").unwrap();
        run::<()>(&vm, "print(string.rep('y', 5000))").unwrap();

        let lines = lines.lock().unwrap();
        assert_eq!(lines[0], "a\t1");
        assert_eq!(lines[1].chars().count(), crate::limits::LOG_LINE_MAX_CHARS);
    }

    #[test]
    fn lua_errors_keep_their_message_and_traceback() {
        let vm = vm(16 * MIB, 100);
        let error =
            run::<()>(&vm, "local function inner() error('boom') end\ninner()").unwrap_err();
        let VmError::Lua { message, traceback } = &error else {
            panic!("got {error:?}");
        };
        assert!(message.ends_with("boom"), "{message}");
        assert!(!message.contains("stack traceback"), "{message}");
        assert!(traceback.as_deref().is_some_and(|t| t.contains("inner")));
        assert_eq!(error.to_string(), *message);

        let syntax = vm.load("broken", "return +").unwrap_err();
        assert!(matches!(syntax, VmError::Lua { .. }), "got {syntax:?}");
    }
}
