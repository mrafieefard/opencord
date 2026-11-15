//! The state a reload carries into the new VM (§11.6, Claude's call 8), as
//! JSON text: plain data only.

use mlua::{Table, Value};
use opencord_plugins::limits::VIEW_MEMORY_BYTES;

/// How deeply the state carried over a reload may nest; under
/// `json.decode`'s own limit.
const STATE_MAX_DEPTH: usize = 100;
/// What one carried number, boolean or table costs of the state's bytes.
const STATE_VALUE_BYTES: usize = 8;

/// The state as JSON text. Functions, userdata, threads, buffers and
/// numbers JSON can't hold are dropped; a table with keys exactly 1 to n is
/// a list, any other a map whose number keys become text.
pub(super) fn carry_state(state: &Table) -> Result<String, String> {
    let mut bytes = 0;
    let value = plain_table(state, 0, &mut bytes)?;
    serde_json::to_string(&value).map_err(|error| error.to_string())
}

fn plain_table(
    table: &Table,
    depth: usize,
    bytes: &mut usize,
) -> Result<serde_json::Value, String> {
    if depth >= STATE_MAX_DEPTH {
        return Err(format!(
            "it nests deeper than {STATE_MAX_DEPTH} tables, or contains itself"
        ));
    }
    let mut pairs = Vec::new();
    for pair in table.pairs::<Value, Value>() {
        pairs.push(pair.map_err(|error| error.to_string())?);
    }
    let length = table.raw_len();
    let is_list = length > 0
        && pairs.len() == length
        && pairs.iter().all(|(key, _)| match key {
            Value::Integer(index) => {
                usize::try_from(*index).is_ok_and(|i| (1..=length).contains(&i))
            }
            Value::Number(index) => {
                index.fract() == 0.0 && *index >= 1.0 && *index <= length as f64
            }
            _ => false,
        });
    if is_list {
        let mut list = Vec::with_capacity(length);
        for index in 1..=length {
            let item: Value = table.raw_get(index).map_err(|error| error.to_string())?;
            list.push(plain(&item, depth, bytes)?.unwrap_or(serde_json::Value::Null));
        }
        return Ok(serde_json::Value::Array(list));
    }
    let mut map = serde_json::Map::new();
    for (key, item) in pairs {
        let key = match key {
            Value::String(key) => key.to_string_lossy(),
            Value::Integer(number) => number.to_string(),
            Value::Number(number) if number.is_finite() => number.to_string(),
            _ => continue,
        };
        *bytes += key.len();
        if let Some(item) = plain(&item, depth, bytes)? {
            map.insert(key, item);
        }
    }
    Ok(serde_json::Value::Object(map))
}

/// One value of the state, or `None` when it isn't plain data.
fn plain(
    value: &Value,
    depth: usize,
    bytes: &mut usize,
) -> Result<Option<serde_json::Value>, String> {
    *bytes += STATE_VALUE_BYTES;
    let value = match value {
        Value::Nil => Some(serde_json::Value::Null),
        Value::LightUserData(data) if data.0.is_null() => Some(serde_json::Value::Null),
        Value::Boolean(flag) => Some(serde_json::Value::Bool(*flag)),
        Value::Integer(number) => Some(serde_json::Value::from(*number)),
        Value::Number(number) => {
            serde_json::Number::from_f64(*number).map(serde_json::Value::Number)
        }
        Value::String(text) => {
            *bytes += text.as_bytes().len();
            Some(serde_json::Value::String(text.to_string_lossy()))
        }
        Value::Table(table) => Some(plain_table(table, depth + 1, bytes)?),
        _ => None,
    };
    if *bytes > VIEW_MEMORY_BYTES {
        return Err("it is larger than a view's memory".to_owned());
    }
    Ok(value)
}
