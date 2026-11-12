//! The call an activity's view is open in (§11.4, §11.5): `app.call` and
//! what the renderer's `call` input changes in it.
//!
//! The app already tracks who is in the call and who speaks, so the core
//! keeps no copy of its own: the renderer sends both lists, and this keeps
//! the last ones it sent for the view's script. A channel's view has no
//! call and ignores the input.

use mlua::{Lua, Table};
use opencord_plugins::limits::CALL_INPUT_MAX_IDS;

use super::Place;

/// The call as the last `call` input gave it. Kept by the view, not the
/// VM, so a reload's new VM gets it too.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Call {
    channel_id: String,
    members: Vec<String>,
    speaking: Vec<String>,
}

/// What a `call` input changed, for the listeners: their event's data, as
/// JSON, in the order they're told.
#[derive(Debug, Default, PartialEq)]
pub(super) struct Changes {
    pub(super) members: Option<String>,
    pub(super) speaking: Option<String>,
}

impl Call {
    /// An activity's view starts with both lists empty; a channel's has no
    /// call.
    pub(super) fn of(place: &Place) -> Option<Call> {
        match place {
            Place::Channel { .. } => None,
            Place::Activity { channel_id, .. } => Some(Call {
                channel_id: channel_id.clone(),
                members: Vec::new(),
                speaking: Vec::new(),
            }),
        }
    }

    /// Takes the lists of a `call` input that [`checked`] passed, and says
    /// what the listeners are told.
    pub(super) fn apply(&mut self, members: Vec<String>, speaking: Vec<String>) -> Changes {
        let mut changes = Changes::default();
        if members != self.members {
            let joined = not_in(&members, &self.members);
            let left = not_in(&self.members, &members);
            let change = serde_json::json!({ "members": members, "joined": joined, "left": left });
            changes.members = Some(change.to_string());
            self.members = members;
        }
        if speaking != self.speaking {
            changes.speaking = Some(serde_json::json!({ "speaking": speaking }).to_string());
            self.speaking = speaking;
        }
        changes
    }
}

/// The ids of `list` that `other` doesn't hold, in `list`'s order.
fn not_in<'a>(list: &'a [String], other: &[String]) -> Vec<&'a str> {
    let missing = list.iter().filter(|id| !other.contains(id));
    missing.map(String::as_str).collect()
}

/// A `call` input's two lists, if each holds at most
/// [`CALL_INPUT_MAX_IDS`] ids and every one is an id string.
pub(super) fn checked(
    members: &[serde_json::Value],
    speaking: &[serde_json::Value],
) -> Option<(Vec<String>, Vec<String>)> {
    Some((ids(members)?, ids(speaking)?))
}

fn ids(list: &[serde_json::Value]) -> Option<Vec<String>> {
    if list.len() > CALL_INPUT_MAX_IDS {
        return None;
    }
    let read = list.iter().map(|id| match id {
        serde_json::Value::String(id) if is_id(id) => Some(id.clone()),
        _ => None,
    });
    read.collect()
}

/// Whether `text` is an id as Lua sees one: a decimal string that fits a
/// `u64`.
pub(super) fn is_id(text: &str) -> bool {
    text.bytes().all(|b| b.is_ascii_digit()) && text.parse::<u64>().is_ok()
}

/// Sets `app.call`: read-only, its lists read-only arrays. Runs no Lua.
pub(super) fn install(lua: &Lua, app: &Table, call: &Call) -> mlua::Result<()> {
    let table = lua.create_table()?;
    table.raw_set("channel_id", call.channel_id.as_str())?;
    table.raw_set("members", id_list(lua, &call.members)?)?;
    table.raw_set("speaking", id_list(lua, &call.speaking)?)?;
    table.set_readonly(true);
    super::context::writing(app, || app.raw_set("call", table))
}

fn id_list(lua: &Lua, ids: &[String]) -> mlua::Result<Table> {
    let list = lua.create_sequence_from(ids.iter().map(String::as_str))?;
    list.set_readonly(true);
    Ok(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fails if a string that isn't all digits, or doesn't fit a u64, passes.
    #[test]
    fn ids_are_decimal_strings_that_fit() {
        assert!(is_id("0") && is_id("18446744073709551615"));
        for bad in ["", "x1", "+1", "-1", " 1", "1.0", "18446744073709551616"] {
            assert!(!is_id(bad), "{bad:?}");
        }
    }
}
