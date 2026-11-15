//! Timers (§11.4): `app.after`, `app.every` and `app.cancel`.
//!
//! The schedule is here; each timer's function is in a Lua table in the
//! registry, so nothing in Rust keeps the VM alive. The view's loop sleeps
//! until the next is due and fires one per turn, after any commands, so
//! however many are due a close still gets through.
//!
//! While the view is paused no time passes for a timer: resuming moves
//! every one forward by the pause's length. An entry leaves when an
//! `after` fires, on `app.cancel`, and with the VM (reload, an error that
//! stops the view, close).

use std::collections::HashMap;
use std::future::pending;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use mlua::{Function, Lua, Table, Value};
use opencord_plugins::limits::{VIEW_MAX_TIMERS, VIEW_TIMER_MAX, VIEW_TIMER_MIN_EVERY};
use opencord_plugins::lua_api::{function, raise};
use tokio::time::{Instant, sleep_until};

use super::api_app::{Shared, finite};

/// Registry key of the table of timer functions, by timer id.
const FUNCTIONS_KEY: &str = "opencord.view.timers";
/// The largest whole number a Lua number holds exactly.
const MAX_ID: f64 = 9_007_199_254_740_992.0;

#[derive(Default)]
pub(super) struct Timers {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    /// Never reset, so an id kept in a reloaded state can't name a new timer.
    last_id: u64,
    entries: HashMap<u64, Timer>,
    /// When the view was paused; `None` while it isn't.
    paused_at: Option<Instant>,
}

struct Timer {
    due: Instant,
    every: Option<Duration>,
}

/// A timer whose time came.
pub(super) struct Fired {
    id: u64,
    pub(super) repeating: bool,
}

impl Timers {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A new timer's id; `None` when the view has as many as it may.
    fn add(&self, delay: Duration, every: Option<Duration>) -> Option<u64> {
        let mut inner = self.lock();
        if inner.entries.len() >= VIEW_MAX_TIMERS {
            return None;
        }
        // Made during a pause, it starts counting at the resume, which
        // moves it forward by the whole pause.
        let from = inner.paused_at.unwrap_or_else(Instant::now);
        inner.last_id += 1;
        let id = inner.last_id;
        let due = from + delay;
        inner.entries.insert(id, Timer { due, every });
        Some(id)
    }

    fn cancel(&self, id: u64) {
        self.lock().entries.remove(&id);
    }

    /// When the next timer is due; `None` when nothing will fire.
    pub(super) fn next_due(&self) -> Option<Instant> {
        let inner = self.lock();
        if inner.paused_at.is_some() {
            return None;
        }
        inner.entries.values().map(|timer| timer.due).min()
    }

    /// Takes the timer due first, if one is due: an `after` leaves, an
    /// `every` is set for its next time. A view that fell behind doesn't
    /// catch up with a burst.
    pub(super) fn take_due(&self, now: Instant) -> Option<Fired> {
        let mut inner = self.lock();
        if inner.paused_at.is_some() {
            return None;
        }
        let id = inner
            .entries
            .iter()
            .filter(|(_, timer)| timer.due <= now)
            .min_by_key(|(id, timer)| (timer.due, **id))
            .map(|(id, _)| *id)?;
        let every = inner.entries.get(&id).and_then(|timer| timer.every);
        match (every, inner.entries.get_mut(&id)) {
            (Some(every), Some(timer)) => {
                let next = timer.due + every;
                timer.due = if next > now { next } else { now + every };
            }
            _ => {
                inner.entries.remove(&id);
            }
        }
        Some(Fired {
            id,
            repeating: every.is_some(),
        })
    }

    pub(super) fn pause(&self, now: Instant) {
        let mut inner = self.lock();
        if inner.paused_at.is_none() {
            inner.paused_at = Some(now);
        }
    }

    pub(super) fn resume(&self, now: Instant) {
        let mut inner = self.lock();
        if let Some(paused_at) = inner.paused_at.take() {
            let paused = now.saturating_duration_since(paused_at);
            for timer in inner.entries.values_mut() {
                timer.due += paused;
            }
        }
    }

    /// The VM these timers call into is gone or stopped.
    pub(super) fn clear(&self) {
        self.lock().entries.clear();
    }
}

/// Waits until `due`; forever when nothing is due.
pub(super) async fn wait(due: Option<Instant>) {
    match due {
        Some(due) => sleep_until(due).await,
        None => pending().await,
    }
}

/// Sets `app.after`, `app.every` and `app.cancel` on `app`.
pub(super) fn install(lua: &Lua, app: &Table, shared: &Arc<Shared>) -> mlua::Result<()> {
    lua.set_named_registry_value(FUNCTIONS_KEY, lua.create_table()?)?;
    app.raw_set("after", adder(lua, shared, "app.after", false)?)?;
    app.raw_set("every", adder(lua, shared, "app.every", true)?)?;
    let canceller = shared.clone();
    let cancel = function(lua, move |lua, timer: Value| {
        let id = timer_id(&timer)?;
        canceller.timers.cancel(id);
        functions(lua)?.raw_set(id, Value::Nil)
    })?;
    app.raw_set("cancel", cancel)
}

fn adder(
    lua: &Lua,
    shared: &Arc<Shared>,
    what: &'static str,
    repeats: bool,
) -> mlua::Result<Function> {
    let shared = shared.clone();
    function(lua, move |lua, (seconds, handler): (Value, Value)| {
        let seconds = finite(&seconds, "seconds")?;
        let Value::Function(handler) = handler else {
            return Err(raise(format!("INVALID_ARGUMENT: {what} takes a function")));
        };
        let delay = delay(what, seconds, repeats)?;
        let Some(id) = shared.timers.add(delay, repeats.then_some(delay)) else {
            return Err(raise(format!(
                "PLUGIN_LIMIT: at most {VIEW_MAX_TIMERS} timers"
            )));
        };
        if let Err(error) = functions(lua)?.raw_set(id, handler) {
            shared.timers.cancel(id);
            return Err(error);
        }
        Ok(id)
    })
}

/// `seconds` as a delay, checked against the limits; it's finite already.
fn delay(what: &str, seconds: f64, repeats: bool) -> mlua::Result<Duration> {
    let shortest = if repeats {
        VIEW_TIMER_MIN_EVERY.as_secs_f64()
    } else {
        0.0
    };
    let longest = VIEW_TIMER_MAX.as_secs_f64();
    if seconds < shortest {
        return Err(raise(format!(
            "INVALID_ARGUMENT: {what}'s seconds must be at least {shortest}"
        )));
    }
    if seconds > longest {
        return Err(raise(format!(
            "INVALID_ARGUMENT: {what}'s seconds must be at most {longest}"
        )));
    }
    Ok(Duration::from_secs_f64(seconds))
}

fn timer_id(timer: &Value) -> mlua::Result<u64> {
    let number = match timer {
        Value::Integer(number) => *number as f64,
        Value::Number(number) => *number,
        _ => f64::NAN,
    };
    if (1.0..=MAX_ID).contains(&number) && number.fract() == 0.0 {
        Ok(number as u64)
    } else {
        Err(raise("INVALID_ARGUMENT: app.cancel takes a timer"))
    }
}

fn functions(lua: &Lua) -> mlua::Result<Table> {
    lua.named_registry_value(FUNCTIONS_KEY)
}

/// The function of a timer that fired; `None` if the script cancelled it.
/// A one-shot's function leaves here. (Not `raw_remove`: on a number key
/// that shifts the entries after it.)
pub(super) fn handler(lua: &Lua, fired: &Fired) -> mlua::Result<Option<Function>> {
    let table = functions(lua)?;
    let handler = table.raw_get::<Option<Function>>(fired.id)?;
    if !fired.repeating {
        table.raw_set(fired.id, Value::Nil)?;
    }
    Ok(handler)
}
