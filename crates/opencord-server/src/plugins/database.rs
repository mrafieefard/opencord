//! Each plugin's own SQLite database (spec §10.5, plan Task 11):
//! `data/plugins/<id>/data.sqlite`, which reaches nothing but itself.
//!
//! Every connection is locked down before the plugin's first statement:
//! no attached databases (`SQLITE_LIMIT_ATTACHED` 0), an authorizer that
//! refuses `ATTACH`, `DETACH`, temporary tables, indexes, views and
//! triggers, the `load_extension` function and all but a few `PRAGMA`s,
//! `max_page_count` from the quota, and SQLite's defensive mode. Each
//! statement gets a progress handler that stops it after
//! [`DB_STATEMENT_TIME`].
//!
//! The pool lives on the runtime that opened it, the server's, and so do
//! its connections: each call runs as a task there, and a transaction is a
//! task there owning its connection. So nothing the plugin's VM holds, and
//! drops when it stops, holds a connection.

use std::ffi::{CStr, c_char, c_int, c_void};
use std::path::Path;
use std::time::Instant;

use futures_util::TryStreamExt as _;
use libsqlite3_sys as ffi;
use opencord_plugins::limits::{DB_MAX_ROWS, DB_STATEMENT_TIME};
use sqlx::sqlite::{
    Sqlite, SqliteArguments, SqliteConnectOptions, SqliteConnection, SqliteJournalMode, SqlitePool,
    SqlitePoolOptions, SqliteRow,
};
use sqlx::{AssertSqlSafe, Column as _, Decode, Row as _, TypeInfo as _, ValueRef as _};
use tokio::sync::{mpsc, oneshot};

/// Connections one plugin's database has open at most.
const MAX_CONNECTIONS: u32 = 2;
/// How long a statement waits for another connection's write lock.
const BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
/// SQLite's virtual machine steps between two looks at the clock.
const PROGRESS_STEPS: c_int = 1_000;

/// The pragmas a plugin may read or set.
const PRAGMAS_SET: [&str; 2] = ["user_version", "foreign_keys"];
/// The pragmas a plugin may read, given a table or index name.
const PRAGMAS_READ: [&str; 5] = [
    "table_info",
    "table_xinfo",
    "index_info",
    "index_list",
    "foreign_key_list",
];

/// A value going into or coming out of SQL.
#[derive(Debug, Clone, PartialEq)]
pub enum SqlValue {
    Null,
    Integer(i64),
    Real(f64),
    /// UTF-8 going in; whatever SQLite holds coming out.
    Text(Vec<u8>),
    Blob(Vec<u8>),
}

/// What `exec` changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Changes {
    pub changes: u64,
    pub last_id: i64,
}

/// What `query` found: each row's values in `columns`' order.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Rows {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<SqlValue>>,
}

/// Why a call failed, as the plugin sees it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DbError {
    #[error("PLUGIN_LIMIT: the plugin's database is full")]
    Full,
    #[error("PLUGIN_LIMIT: a query can return at most {DB_MAX_ROWS} rows")]
    TooManyRows,
    #[error("PLUGIN_LIMIT: a query's result is too large")]
    ResultTooLarge,
    #[error(
        "PLUGIN_LIMIT: a statement can run for at most {} seconds",
        DB_STATEMENT_TIME.as_secs()
    )]
    TooLong,
    /// The authorizer refused the statement.
    #[error("FORBIDDEN: {0}")]
    Refused(String),
    /// A constraint, or another connection holding the database.
    #[error("CONFLICT: {0}")]
    Conflict(String),
    #[error("INVALID_ARGUMENT: {0}")]
    Invalid(String),
    /// SQLite ended the transaction (a full database can), or the plugin's
    /// own `COMMIT` or `ROLLBACK` did.
    #[error(
        "CONFLICT: the transaction was already over: SQLite rolled it back, or the SQL ended it"
    )]
    Ended,
    #[error("STOPPING: the plugin's database is closed")]
    Closed,
    #[error("INTERNAL: the plugin's database failed")]
    Internal,
}

impl From<sqlx::Error> for DbError {
    fn from(error: sqlx::Error) -> Self {
        let sqlx::Error::Database(database) = &error else {
            return match error {
                sqlx::Error::PoolClosed | sqlx::Error::WorkerCrashed => DbError::Closed,
                sqlx::Error::PoolTimedOut => {
                    DbError::Conflict("the plugin's database is busy".to_owned())
                }
                other => {
                    tracing::warn!(error = %other, "a plugin's database call failed");
                    DbError::Internal
                }
            };
        };
        let message = database.message().to_owned();
        let code = database
            .code()
            .and_then(|code| code.parse::<c_int>().ok())
            .unwrap_or(ffi::SQLITE_ERROR);
        // A function the authorizer refused comes back as SQLITE_ERROR.
        if message.starts_with("not authorized") {
            return DbError::Refused(message);
        }
        match code & 0xff {
            ffi::SQLITE_FULL => DbError::Full,
            ffi::SQLITE_INTERRUPT => DbError::TooLong,
            ffi::SQLITE_AUTH => DbError::Refused(message),
            ffi::SQLITE_CONSTRAINT | ffi::SQLITE_BUSY | ffi::SQLITE_LOCKED => {
                DbError::Conflict(message)
            }
            ffi::SQLITE_IOERR
            | ffi::SQLITE_CORRUPT
            | ffi::SQLITE_CANTOPEN
            | ffi::SQLITE_NOMEM
            | ffi::SQLITE_NOTADB
            | ffi::SQLITE_READONLY => {
                tracing::warn!(code, "a plugin's database failed");
                DbError::Internal
            }
            _ => DbError::Invalid(message),
        }
    }
}

/// One plugin's database. Clones share the pool.
#[derive(Debug, Clone)]
pub struct PluginDb {
    pool: SqlitePool,
    /// Where the pool's connections are used.
    runtime: tokio::runtime::Handle,
    memory_cap: usize,
}

/// Opens (creating it if needed) the database at `path`, at most
/// `max_bytes` large. Call it on the runtime its calls should run on.
pub async fn open(path: &Path, max_bytes: u64, memory_cap: usize) -> Result<PluginDb, sqlx::Error> {
    if let Some(folder) = path.parent() {
        tokio::fs::create_dir_all(folder).await?;
    }
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(BUSY_TIMEOUT);
    let max_length = i32::try_from(memory_cap / 8).unwrap_or(i32::MAX);
    let pool = SqlitePoolOptions::new()
        .max_connections(MAX_CONNECTIONS)
        .after_connect(move |conn, _| Box::pin(lock_down(conn, max_bytes, max_length)))
        .connect_with(options)
        .await?;
    Ok(PluginDb {
        pool,
        runtime: tokio::runtime::Handle::current(),
        memory_cap,
    })
}

impl PluginDb {
    /// Runs `sql`, which may hold several statements, with `params`.
    pub async fn exec(&self, sql: String, params: Vec<SqlValue>) -> Result<Changes, DbError> {
        let pool = self.pool.clone();
        self.spawn(async move {
            let conn = &mut *pool.acquire().await?;
            exec_on(conn, &sql, params, &mut TxState::None).await
        })
        .await
    }

    /// The rows `sql` returns, at most [`DB_MAX_ROWS`].
    pub async fn query(&self, sql: String, params: Vec<SqlValue>) -> Result<Rows, DbError> {
        let pool = self.pool.clone();
        let memory_cap = self.memory_cap;
        self.spawn(async move {
            let conn = &mut *pool.acquire().await?;
            query_on(conn, &sql, params, &mut TxState::None, memory_cap).await
        })
        .await
    }

    /// Starts a `BEGIN IMMEDIATE` transaction. It ends with
    /// [`PluginTx::finish`], or rolls back once every copy is dropped.
    pub async fn begin(&self) -> Result<PluginTx, DbError> {
        let pool = self.pool.clone();
        let runtime = self.runtime.clone();
        let memory_cap = self.memory_cap;
        self.spawn(async move {
            let tx = pool.begin_with("BEGIN IMMEDIATE").await?;
            let (jobs, receiver) = mpsc::unbounded_channel();
            runtime.spawn(transaction(tx, receiver, memory_cap));
            Ok(PluginTx { jobs })
        })
        .await
    }

    /// Refuses new calls and closes each connection once it is free.
    pub async fn close(&self) {
        self.pool.close().await;
    }

    async fn spawn<T: Send + 'static>(
        &self,
        work: impl Future<Output = Result<T, DbError>> + Send + 'static,
    ) -> Result<T, DbError> {
        match self.runtime.spawn(work).await {
            Ok(result) => result,
            Err(error) if error.is_panic() => {
                tracing::error!("a plugin's database call panicked");
                Err(DbError::Internal)
            }
            Err(_) => Err(DbError::Closed),
        }
    }
}

/// An open transaction: a task owning its connection, doing one job at a
/// time.
#[derive(Debug, Clone)]
pub struct PluginTx {
    jobs: mpsc::UnboundedSender<TxJob>,
}

#[derive(Debug)]
enum TxJob {
    Exec {
        sql: String,
        params: Vec<SqlValue>,
        reply: oneshot::Sender<Result<Changes, DbError>>,
    },
    Query {
        sql: String,
        params: Vec<SqlValue>,
        reply: oneshot::Sender<Result<Rows, DbError>>,
    },
    Finish {
        commit: bool,
        reply: oneshot::Sender<Result<(), DbError>>,
    },
}

impl PluginTx {
    pub async fn exec(&self, sql: String, params: Vec<SqlValue>) -> Result<Changes, DbError> {
        self.ask(|reply| TxJob::Exec { sql, params, reply }).await
    }

    pub async fn query(&self, sql: String, params: Vec<SqlValue>) -> Result<Rows, DbError> {
        self.ask(|reply| TxJob::Query { sql, params, reply }).await
    }

    /// `COMMIT` or `ROLLBACK`; the transaction ends either way.
    pub async fn finish(&self, commit: bool) -> Result<(), DbError> {
        self.ask(|reply| TxJob::Finish { commit, reply }).await
    }

    async fn ask<T>(
        &self,
        job: impl FnOnce(oneshot::Sender<Result<T, DbError>>) -> TxJob,
    ) -> Result<T, DbError> {
        let (reply, answer) = oneshot::channel();
        self.jobs.send(job(reply)).map_err(|_| DbError::Closed)?;
        answer.await.map_err(|_| DbError::Closed)?
    }
}

async fn transaction(
    mut tx: sqlx::Transaction<'static, Sqlite>,
    mut jobs: mpsc::UnboundedReceiver<TxJob>,
    memory_cap: usize,
) {
    let mut state = TxState::Open;
    // A reply nobody waits for belongs to a handler that was cancelled; the
    // job is done either way, so dropping its answer is right.
    while let Some(job) = jobs.recv().await {
        match job {
            TxJob::Exec { sql, params, reply } => {
                let done = match state {
                    TxState::Ended => Err(DbError::Ended),
                    _ => exec_on(&mut tx, &sql, params, &mut state).await,
                };
                reply.send(done).ok();
            }
            TxJob::Query { sql, params, reply } => {
                let done = match state {
                    TxState::Ended => Err(DbError::Ended),
                    _ => query_on(&mut tx, &sql, params, &mut state, memory_cap).await,
                };
                reply.send(done).ok();
            }
            TxJob::Finish { commit, reply } => {
                let finished = match (commit, state) {
                    (true, TxState::Open) => tx.commit().await.map_err(DbError::from),
                    (true, _) => match tx.rollback().await {
                        Ok(()) => Err(DbError::Ended),
                        Err(error) => Err(error.into()),
                    },
                    (false, _) => tx.rollback().await.map_err(DbError::from),
                };
                reply.send(finished).ok();
                return;
            }
        }
    }
    // Abandoned: the handler that opened it is gone.
    if let Err(error) = tx.rollback().await {
        tracing::warn!(%error, "could not roll back a plugin's abandoned transaction");
    }
}

/// Whether a statement runs inside a plugin's transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TxState {
    None,
    Open,
    /// Over before `db.transaction` ended it; see [`DbError::Ended`].
    Ended,
}

async fn exec_on(
    conn: &mut SqliteConnection,
    sql: &str,
    params: Vec<SqlValue>,
    state: &mut TxState,
) -> Result<Changes, DbError> {
    let arguments = arguments(params)?;
    let deadline = Deadline::set(conn).await?;
    let done = sqlx::query_with(AssertSqlSafe(sql), arguments)
        .execute(&mut *conn)
        .await;
    settle(conn, deadline, state).await?;
    let done = done?;
    Ok(Changes {
        changes: done.rows_affected(),
        last_id: done.last_insert_rowid(),
    })
}

async fn query_on(
    conn: &mut SqliteConnection,
    sql: &str,
    params: Vec<SqlValue>,
    state: &mut TxState,
    memory_cap: usize,
) -> Result<Rows, DbError> {
    let arguments = arguments(params)?;
    let deadline = Deadline::set(conn).await?;
    let rows = {
        let mut stream = sqlx::query_with(AssertSqlSafe(sql), arguments).fetch(&mut *conn);
        collect(&mut stream, memory_cap).await
    };
    settle(conn, deadline, state).await?;
    rows
}

/// After a plugin's statement: its time limit goes, and the connection's
/// transaction must be as it was, or sqlx's own `COMMIT` and `ROLLBACK`
/// would no longer match it.
async fn settle(
    conn: &mut SqliteConnection,
    deadline: Deadline,
    state: &mut TxState,
) -> Result<(), DbError> {
    let in_transaction = deadline.clear(conn).await?;
    match (*state, in_transaction) {
        (TxState::None, true) => {
            sqlx::query("ROLLBACK").execute(&mut *conn).await?;
            Err(DbError::Invalid(
                "a statement can't leave a transaction open: use db.transaction".to_owned(),
            ))
        }
        (TxState::Open, false) => {
            // A new one for sqlx's ROLLBACK to end; nothing runs in it.
            sqlx::query("BEGIN").execute(&mut *conn).await?;
            *state = TxState::Ended;
            Ok(())
        }
        _ => Ok(()),
    }
}

async fn collect(
    stream: &mut (impl futures_util::Stream<Item = Result<SqliteRow, sqlx::Error>> + Unpin),
    memory_cap: usize,
) -> Result<Rows, DbError> {
    let mut found = Rows::default();
    let mut total_bytes = 0usize;
    while let Some(row) = stream.try_next().await? {
        if found.rows.len() == DB_MAX_ROWS {
            return Err(DbError::TooManyRows);
        }
        if found.rows.is_empty() {
            found.columns = row
                .columns()
                .iter()
                .map(|column| column.name().to_owned())
                .collect();
        }
        let (row_vals, row_bytes) = values(&row)?;
        total_bytes = total_bytes.saturating_add(row_bytes);
        if total_bytes > memory_cap {
            return Err(DbError::ResultTooLarge);
        }
        found.rows.push(row_vals);
    }
    Ok(found)
}

/// A row's values by the type each one has, not its column's.
fn values(row: &SqliteRow) -> Result<(Vec<SqlValue>, usize), DbError> {
    fn decode<'r, T: Decode<'r, Sqlite>>(
        raw: sqlx::sqlite::SqliteValueRef<'r>,
    ) -> Result<T, DbError> {
        T::decode(raw).map_err(|error| {
            tracing::warn!(%error, "a value in a plugin's database couldn't be read");
            DbError::Internal
        })
    }
    let mut row_bytes = 0usize;
    let mut vals = Vec::with_capacity(row.len());
    for index in 0..row.len() {
        let raw = row.try_get_raw(index)?;
        if raw.is_null() {
            vals.push(SqlValue::Null);
            continue;
        }
        let kind = raw.type_info().name().to_owned();
        match kind.as_str() {
            "INTEGER" => vals.push(SqlValue::Integer(decode(raw)?)),
            "REAL" => vals.push(SqlValue::Real(decode(raw)?)),
            "TEXT" => {
                let bytes: Vec<u8> = decode(raw)?;
                row_bytes = row_bytes.saturating_add(bytes.len());
                vals.push(SqlValue::Text(bytes));
            }
            _ => {
                let bytes: Vec<u8> = decode(raw)?;
                row_bytes = row_bytes.saturating_add(bytes.len());
                vals.push(SqlValue::Blob(bytes));
            }
        }
    }
    Ok((vals, row_bytes))
}

fn arguments(params: Vec<SqlValue>) -> Result<SqliteArguments, DbError> {
    use sqlx::Arguments as _;
    let mut arguments = SqliteArguments::default();
    for (index, value) in params.into_iter().enumerate() {
        let added = match value {
            SqlValue::Null => arguments.add(None::<i64>),
            SqlValue::Integer(number) => arguments.add(number),
            SqlValue::Real(number) => arguments.add(number),
            SqlValue::Text(bytes) => {
                let text = String::from_utf8(bytes).map_err(|_| {
                    DbError::Invalid(format!(
                        "argument {} isn't UTF-8 text; pass bytes as a buffer",
                        index + 1
                    ))
                })?;
                arguments.add(text)
            }
            SqlValue::Blob(bytes) => arguments.add(bytes),
        };
        added.map_err(|error| DbError::Invalid(error.to_string()))?;
    }
    Ok(arguments)
}

/// The progress handler that stops one statement after
/// [`DB_STATEMENT_TIME`]. Cleared after the statement, so it never stops
/// the pool's own statements later.
struct Deadline;

impl Deadline {
    async fn set(conn: &mut SqliteConnection) -> Result<Deadline, DbError> {
        let deadline = Instant::now() + DB_STATEMENT_TIME;
        conn.lock_handle()
            .await?
            .set_progress_handler(PROGRESS_STEPS, move || Instant::now() < deadline);
        Ok(Deadline)
    }

    /// Clears it; whether the connection is inside a transaction now.
    async fn clear(self, conn: &mut SqliteConnection) -> Result<bool, DbError> {
        let mut handle = conn.lock_handle().await?;
        handle.remove_progress_handler();
        // SAFETY: the locked handle is open, and sqlx's worker doesn't use
        // it until the lock drops.
        let autocommit = unsafe { ffi::sqlite3_get_autocommit(handle.as_raw_handle().as_ptr()) };
        Ok(autocommit == 0)
    }
}

/// What every connection gets before the plugin's first statement.
async fn lock_down(
    conn: &mut SqliteConnection,
    max_bytes: u64,
    max_length: i32,
) -> Result<(), sqlx::Error> {
    let page_size: i64 = sqlx::query_scalar("PRAGMA page_size")
        .fetch_one(&mut *conn)
        .await?;
    let pages = max_bytes / u64::try_from(page_size).unwrap_or(1).max(1);
    sqlx::query(AssertSqlSafe(format!("PRAGMA max_page_count = {pages}")))
        .execute(&mut *conn)
        .await?;
    let mut handle = conn.lock_handle().await?;
    let db = handle.as_raw_handle().as_ptr();
    // SAFETY: `db` is the connection's open handle, and the
    // `LockedSqliteHandle` keeps sqlx's worker from using it until it drops,
    // after these calls. The authorizer is a plain function without state,
    // so its null user data stays valid for the connection's life.
    let configured = unsafe {
        ffi::sqlite3_limit(db, ffi::SQLITE_LIMIT_ATTACHED, 0);
        ffi::sqlite3_limit(db, ffi::SQLITE_LIMIT_COLUMN, 64);
        ffi::sqlite3_limit(db, ffi::SQLITE_LIMIT_LENGTH, max_length);
        let mut defensive: c_int = 0;
        let defended = ffi::sqlite3_db_config(
            db,
            ffi::SQLITE_DBCONFIG_DEFENSIVE,
            1 as c_int,
            &mut defensive as *mut c_int,
        );
        let authorized = ffi::sqlite3_set_authorizer(db, Some(authorize), std::ptr::null_mut());
        (defended == ffi::SQLITE_OK && defensive == 1, authorized)
    };
    match configured {
        (true, ffi::SQLITE_OK) => Ok(()),
        _ => Err(sqlx::Error::Protocol(
            "could not lock down a plugin's database".to_owned(),
        )),
    }
}

/// SQLite's authorizer: what a plugin's statements may do.
extern "C" fn authorize(
    _: *mut c_void,
    action: c_int,
    first: *const c_char,
    second: *const c_char,
    database: *const c_char,
    _: *const c_char,
) -> c_int {
    // SAFETY: SQLite passes null or a NUL-terminated string that lives
    // through this call.
    let text = |pointer: *const c_char| {
        (!pointer.is_null()).then(|| unsafe { CStr::from_ptr(pointer) }.to_bytes())
    };
    if allowed(action, text(first), text(second), text(database)) {
        ffi::SQLITE_OK
    } else {
        ffi::SQLITE_DENY
    }
}

fn allowed(
    action: c_int,
    first: Option<&[u8]>,
    second: Option<&[u8]>,
    database: Option<&[u8]>,
) -> bool {
    let named = |names: &[&str], name: Option<&[u8]>| {
        name.is_some_and(|name| {
            names
                .iter()
                .any(|n| n.as_bytes().eq_ignore_ascii_case(name))
        })
    };
    match action {
        // Another file, through the server's own process.
        ffi::SQLITE_ATTACH | ffi::SQLITE_DETACH => false,
        // The temporary database is outside the quota: `CREATE TEMP …`, and
        // `CREATE … temp.name`, which SQLite reports as the plain kind.
        ffi::SQLITE_CREATE_TEMP_INDEX
        | ffi::SQLITE_CREATE_TEMP_TABLE
        | ffi::SQLITE_CREATE_TEMP_TRIGGER
        | ffi::SQLITE_CREATE_TEMP_VIEW => false,
        ffi::SQLITE_CREATE_INDEX
        | ffi::SQLITE_CREATE_TABLE
        | ffi::SQLITE_CREATE_TRIGGER
        | ffi::SQLITE_CREATE_VIEW
        | ffi::SQLITE_CREATE_VTABLE => !named(&["temp"], database),
        // The pragma's name, and its argument when one is given.
        ffi::SQLITE_PRAGMA => named(&PRAGMAS_SET, first) || named(&PRAGMAS_READ, first),
        // The function's name is the second.
        ffi::SQLITE_FUNCTION => !named(&["load_extension"], second),
        _ => true,
    }
}

#[cfg(any(test, debug_assertions))]
impl PluginDb {
    /// For tests whose plugin never uses its database: nothing opens until
    /// a call, and then an empty one in memory.
    pub fn unused() -> PluginDb {
        let options: SqliteConnectOptions = "sqlite::memory:".parse().unwrap();
        PluginDb {
            pool: SqlitePoolOptions::new().connect_lazy_with(options),
            runtime: tokio::runtime::Handle::current(),
            memory_cap: usize::MAX,
        }
    }
}

#[cfg(test)]
#[path = "database_tests.rs"]
mod tests;
