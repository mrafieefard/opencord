//! Quick dialogs (§11.4): `app.confirm`, `app.alert` and `app.prompt`, and
//! the handlers waiting on them or on `app.modal.ask`.
//!
//! A waiting handler's coroutine is paused in an async call that awaits a
//! oneshot; the answer (a `dialog` input, or the modal closing) sends on
//! it. Only plain JSON crosses into Rust, never a Lua value. An entry
//! leaves [`Waits`] when it's answered, or when its call is dropped: the
//! handler ended with the VM (a reload, the view closing).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use mlua::{Function, Lua, Table, Value};
use opencord_plugins::limits::{DIALOG_MAX_CHARS, VIEW_MAX_TEXT_CHARS, VIEW_MAX_WAITING};
use opencord_plugins::lua_api::{async_function, raise};
use opencord_plugins::view::{DialogKind, Patch};
use tokio::sync::oneshot;

use super::api_app::Shared;
use super::cut;

type Json = serde_json::Value;

/// The handlers waiting on a dialog or a modal, by dialog id (the `dialog`
/// op's) or ask ticket; one count for both.
#[derive(Default)]
pub(super) struct Waits {
    inner: Mutex<WaitsInner>,
}

#[derive(Default)]
struct WaitsInner {
    last: u64,
    pending: HashMap<u64, Pending>,
}

struct Pending {
    /// The modal's serial for `app.modal.ask`; `None` for a dialog.
    modal: Option<u64>,
    answer: oneshot::Sender<Json>,
}

impl Waits {
    fn lock(&self) -> MutexGuard<'_, WaitsInner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The `dialog` input: resumes the handler waiting on dialog `id`.
    /// `false` when none is (answered already, or its handler ended).
    pub(super) fn answer(&self, id: u64, result: Json) -> bool {
        let mut inner = self.lock();
        if !matches!(inner.pending.get(&id), Some(Pending { modal: None, .. })) {
            return false;
        }
        match inner.pending.remove(&id) {
            // The receiver is gone only when its handler ended meanwhile.
            Some(pending) => pending.answer.send(result).is_ok(),
            None => false,
        }
    }

    /// Modal `serial` closed: resumes every `app.modal.ask` waiting on it.
    pub(super) fn wake(&self, serial: u64) {
        let mut inner = self.lock();
        let tickets: Vec<u64> = inner
            .pending
            .iter()
            .filter(|(_, pending)| pending.modal == Some(serial))
            .map(|(ticket, _)| *ticket)
            .collect();
        for ticket in tickets {
            if let Some(pending) = inner.pending.remove(&ticket)
                && pending.answer.send(Json::Null).is_err()
            {
                // Its handler ended meanwhile: no one is left to resume.
                tracing::debug!(serial, "a modal closed after its asking handler ended");
            }
        }
    }

    fn forget(&self, id: u64) {
        self.lock().pending.remove(&id);
    }
}

/// Raises unless a handler may wait now: only a handler can pause (not
/// the script's start or `render`), and at most [`VIEW_MAX_WAITING`] wait.
pub(super) fn check_wait(shared: &Shared, what: &str) -> mlua::Result<()> {
    if !shared.in_handler() {
        return Err(raise(format!("FORBIDDEN: {what} works only in a handler")));
    }
    if shared.waits.lock().pending.len() >= VIEW_MAX_WAITING {
        return Err(raise(format!(
            "PLUGIN_LIMIT: at most {VIEW_MAX_WAITING} handlers can wait on a dialog or a modal"
        )));
    }
    Ok(())
}

/// One handler's wait. Dropping it (the call dropped with its handler)
/// removes its entry.
pub(super) struct Waiting {
    shared: Arc<Shared>,
    id: u64,
    answer: Option<oneshot::Receiver<Json>>,
}

impl Waiting {
    /// Starts a wait, for modal `modal`'s close or (`None`) a dialog.
    pub(super) fn begin(
        shared: &Arc<Shared>,
        what: &str,
        modal: Option<u64>,
    ) -> mlua::Result<Self> {
        check_wait(shared, what)?;
        let (sender, receiver) = oneshot::channel();
        let mut inner = shared.waits.lock();
        inner.last += 1;
        let id = inner.last;
        inner.pending.insert(
            id,
            Pending {
                modal,
                answer: sender,
            },
        );
        Ok(Waiting {
            shared: shared.clone(),
            id,
            answer: Some(receiver),
        })
    }

    pub(super) fn id(&self) -> u64 {
        self.id
    }

    /// The answer; `null` if the wait ended without one.
    pub(super) async fn answer(mut self) -> Json {
        match self.answer.take() {
            Some(answer) => answer.await.unwrap_or(Json::Null),
            None => Json::Null,
        }
    }
}

impl Drop for Waiting {
    fn drop(&mut self) {
        self.shared.waits.forget(self.id);
    }
}

/// Sets `app.confirm`, `app.alert` and `app.prompt` on `app`.
pub(super) fn install(lua: &Lua, app: &Table, shared: &Arc<Shared>) -> mlua::Result<()> {
    for (name, kind) in [
        ("confirm", DialogKind::Confirm),
        ("alert", DialogKind::Alert),
        ("prompt", DialogKind::Prompt),
    ] {
        app.raw_set(name, dialog(lua, shared, name, kind)?)?;
    }
    Ok(())
}

/// `app.<name>(text, default?)`: shows the dialog and pauses the handler
/// until the `dialog` input answers it.
fn dialog(
    lua: &Lua,
    shared: &Arc<Shared>,
    name: &'static str,
    kind: DialogKind,
) -> mlua::Result<Function> {
    let shared = shared.clone();
    async_function(lua, move |lua, (text, default): (Value, Value)| {
        let shared = shared.clone();
        async move {
            let text = dialog_text(&text, &format!("app.{name}'s text"))?;
            let default = match (kind, &default) {
                (DialogKind::Prompt, Value::Nil) | (DialogKind::Confirm | DialogKind::Alert, _) => {
                    None
                }
                (DialogKind::Prompt, default) => {
                    Some(dialog_text(default, "app.prompt's default")?)
                }
            };
            let waiting = Waiting::begin(&shared, &format!("app.{name}"), None)?;
            shared.push_effect(Patch::Dialog {
                id: waiting.id(),
                kind,
                text,
                default,
            });
            let answer = waiting.answer().await;
            Ok(match (kind, answer) {
                (DialogKind::Confirm, answer) => Value::Boolean(answer == Json::Bool(true)),
                (DialogKind::Alert, _) => Value::Nil,
                (DialogKind::Prompt, Json::String(text)) => {
                    Value::String(lua.create_string(cut(&text, VIEW_MAX_TEXT_CHARS))?)
                }
                (DialogKind::Prompt, _) => Value::Nil,
            })
        }
    })
}

/// A dialog's text: a UTF-8 string of at most [`DIALOG_MAX_CHARS`]
/// characters. `what` names it in the error.
fn dialog_text(value: &Value, what: &str) -> mlua::Result<String> {
    let Value::String(text) = value else {
        return Err(raise(format!("INVALID_ARGUMENT: {what} must be a string")));
    };
    let text = text
        .to_str()
        .map_err(|_| raise(format!("INVALID_ARGUMENT: {what} must be UTF-8 text")))?
        .to_owned();
    if text.chars().count() > DIALOG_MAX_CHARS {
        return Err(raise(format!(
            "INVALID_ARGUMENT: {what} is at most {DIALOG_MAX_CHARS} characters"
        )));
    }
    Ok(text)
}

#[cfg(test)]
#[path = "dialogs_tests.rs"]
mod tests;
