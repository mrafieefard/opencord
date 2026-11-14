//! `db.exec`, `db.query` and `db.transaction` (spec §10.3, §10.5): plain
//! SQL with `?` parameters on the plugin's own database
//! (`plugins::database`). They need no permission.
//!
//! Inside `db.transaction(fn)`, calls `fn` makes itself use the
//! transaction's connection. Calls from a coroutine `fn` starts don't: they
//! wait for the transaction like any other handler's.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use mlua::{Function, Lua, MultiValue, Table, Thread, Value, Variadic};
use opencord_plugins::sandbox::paused;

use super::{async_function, async_function_passing, raise, string_arg};
use crate::plugins::database::{Changes, DbError, PluginTx, Rows, SqlValue};
use crate::plugins::runtime::PluginEnv;

/// Whole numbers this far from zero go in as INTEGER and come out as
/// numbers: a Luau number holds them exactly.
const SAFE_INTEGER: u64 = 1 << 53;

/// The open transactions, by the Lua thread running each one's function.
/// An entry leaves when its transaction ends, finished or cancelled
/// ([`Open`]'s drop).
#[derive(Default)]
struct Transactions(Mutex<HashMap<usize, PluginTx>>);

impl Transactions {
    fn lock(&self) -> MutexGuard<'_, HashMap<usize, PluginTx>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The transaction `thread` runs in, if any.
    fn of(&self, thread: &Thread) -> Option<PluginTx> {
        self.lock().get(&key(thread)).cloned()
    }
}

fn key(thread: &Thread) -> usize {
    thread.to_pointer() as usize
}

/// A transaction's entry in [`Transactions`], removed when this drops.
struct Open {
    transactions: Arc<Transactions>,
    key: usize,
}

impl Open {
    fn new(transactions: &Arc<Transactions>, thread: &Thread, tx: PluginTx) -> Open {
        let key = key(thread);
        transactions.lock().insert(key, tx);
        Open {
            transactions: transactions.clone(),
            key,
        }
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        self.transactions.lock().remove(&self.key);
    }
}

pub fn install(lua: &Lua, env: &Arc<PluginEnv>) -> mlua::Result<()> {
    let db = lua.create_table()?;
    let transactions = Arc::new(Transactions::default());

    let (exec_env, exec_open) = (env.clone(), transactions.clone());
    db.raw_set(
        "exec",
        async_function(lua, move |lua, (sql, params): (Value, Variadic<Value>)| {
            let env = exec_env.clone();
            let tx = exec_open.of(&lua.current_thread());
            let statement = statement(sql, params);
            async move {
                let (sql, params) = statement?;
                let changes = paused(&env.budget, async {
                    match tx {
                        Some(tx) => tx.exec(sql, params).await,
                        None => env.db.exec(sql, params).await,
                    }
                })
                .await
                .map_err(raised)?;
                changes_table(&lua, changes)
            }
        })?,
    )?;

    let (query_env, query_open) = (env.clone(), transactions.clone());
    db.raw_set(
        "query",
        async_function(lua, move |lua, (sql, params): (Value, Variadic<Value>)| {
            let env = query_env.clone();
            let tx = query_open.of(&lua.current_thread());
            let statement = statement(sql, params);
            async move {
                let (sql, params) = statement?;
                let rows = paused(&env.budget, async {
                    match tx {
                        Some(tx) => tx.query(sql, params).await,
                        None => env.db.query(sql, params).await,
                    }
                })
                .await
                .map_err(raised)?;
                rows_table(&lua, rows)
            }
        })?,
    )?;

    let env = env.clone();
    // Sandboxed globals reach Luau's own through `__index`.
    let pcall: Function = lua.globals().get("pcall")?;
    db.raw_set(
        "transaction",
        async_function_passing(lua, move |lua, body: Value| {
            let (env, transactions, pcall) = (env.clone(), transactions.clone(), pcall.clone());
            let nested = transactions.lock().contains_key(&key(&lua.current_thread()));
            async move {
                let Value::Function(body) = body else {
                    return Err(raise("INVALID_ARGUMENT: db.transaction takes a function"));
                };
                if nested {
                    return Err(raise(
                        "INVALID_ARGUMENT: a transaction can't start inside another",
                    ));
                }
                let tx = paused(&env.budget, env.db.begin()).await.map_err(raised)?;
                // `body` runs as `pcall(body)` on a thread of its own, so its
                // calls can be told apart and any error comes back as it was
                // raised.
                let thread = lua.create_thread(pcall)?;
                let open = Open::new(&transactions, &thread, tx.clone());
                let mut results = thread.into_async::<MultiValue>(body)?.await?;
                let succeeded = matches!(results.pop_front(), Some(Value::Boolean(true)));
                let finished = paused(&env.budget, tx.finish(succeeded)).await;
                drop(open);
                if succeeded {
                    finished.map_err(raised)?;
                    return Ok(Ok(results));
                }
                if let Err(error) = finished {
                    // The plugin gets its own error, which made this rollback.
                    tracing::warn!(plugin = %env.plugin_id, %error, "a plugin's transaction didn't roll back");
                }
                Ok(Err(results.pop_front().unwrap_or(Value::Nil)))
            }
        })?,
    )?;

    db.set_readonly(true);
    lua.globals().raw_set("db", db)
}

/// The SQL and its parameters, checked.
fn statement(sql: Value, params: Variadic<Value>) -> mlua::Result<(String, Vec<SqlValue>)> {
    let sql = string_arg(sql, "sql")?;
    let params = params
        .into_iter()
        .enumerate()
        .map(|(index, value)| sql_value(value, index + 1))
        .collect::<mlua::Result<_>>()?;
    Ok((sql, params))
}

fn sql_value(value: Value, position: usize) -> mlua::Result<SqlValue> {
    let safe = |number: f64| number.fract() == 0.0 && number.abs() <= SAFE_INTEGER as f64;
    Ok(match value {
        Value::Nil => SqlValue::Null,
        Value::Boolean(on) => SqlValue::Integer(i64::from(on)),
        Value::Integer(number) if number.unsigned_abs() <= SAFE_INTEGER => {
            SqlValue::Integer(number)
        }
        Value::Integer(number) => SqlValue::Real(number as f64),
        Value::Number(number) if safe(number) => SqlValue::Integer(number as i64),
        Value::Number(number) => SqlValue::Real(number),
        Value::String(text) => SqlValue::Text(text.as_bytes().to_vec()),
        Value::Buffer(bytes) => SqlValue::Blob(bytes.to_vec()),
        other => {
            return Err(raise(format!(
                "INVALID_ARGUMENT: argument {position} can't go into SQL: it's a {}",
                other.type_name()
            )));
        }
    })
}

fn lua_value(lua: &Lua, value: SqlValue) -> mlua::Result<Value> {
    Ok(match value {
        SqlValue::Null => Value::Nil,
        SqlValue::Integer(number) if number.unsigned_abs() <= SAFE_INTEGER => {
            Value::Number(number as f64)
        }
        SqlValue::Integer(number) => Value::String(lua.create_string(number.to_string())?),
        SqlValue::Real(number) => Value::Number(number),
        SqlValue::Text(bytes) => Value::String(lua.create_string(bytes)?),
        SqlValue::Blob(bytes) => Value::Buffer(lua.create_buffer(bytes)?),
    })
}

/// `{ changes = n, last_id = "…" }`.
fn changes_table(lua: &Lua, changes: Changes) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.raw_set("changes", changes.changes as f64)?;
    table.raw_set("last_id", changes.last_id.to_string())?;
    Ok(table)
}

/// An array of rows, each a table by column name; NULLs leave their column
/// out.
fn rows_table(lua: &Lua, rows: Rows) -> mlua::Result<Table> {
    let table = lua.create_table_with_capacity(rows.rows.len(), 0)?;
    for (index, values) in rows.rows.into_iter().enumerate() {
        let row = lua.create_table_with_capacity(0, values.len())?;
        for (column, value) in rows.columns.iter().zip(values) {
            row.raw_set(column.as_str(), lua_value(lua, value)?)?;
        }
        table.raw_set(index + 1, row)?;
    }
    Ok(table)
}

fn raised(error: DbError) -> mlua::Error {
    raise(error.to_string())
}
