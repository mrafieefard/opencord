//! The plugin's database (plan Task 11): what SQL can't reach, its limits,
//! and how values and transactions look from Lua.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::time::{Duration, Instant};

use opencord_plugins::sandbox::{Vm, VmLimits};

use super::*;
use crate::plugins::api;
use crate::plugins::logbook::PluginLog;
use crate::plugins::net::{NetGuard, PluginNet};
use crate::plugins::runtime::PluginEnv;

const MIB: u64 = 1024 * 1024;

/// A database in its own folder, at most `max_bytes` large.
async fn scratch(max_bytes: u64) -> (PluginDb, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db = open(
        &dir.path().join("data.sqlite"),
        max_bytes,
        64 * MIB as usize,
    )
    .await
    .unwrap();
    (db, dir)
}

fn text(value: &str) -> SqlValue {
    SqlValue::Text(value.as_bytes().to_vec())
}

async fn exec(db: &PluginDb, sql: &str) -> Result<Changes, DbError> {
    db.exec(sql.to_owned(), Vec::new()).await
}

async fn query(db: &PluginDb, sql: &str) -> Result<Rows, DbError> {
    db.query(sql.to_owned(), Vec::new()).await
}

fn refused(result: Result<impl std::fmt::Debug, DbError>, what: &str) {
    match result {
        Err(DbError::Refused(_)) => {}
        other => panic!("{what}: {other:?}"),
    }
}

/// A VM with the whole API and a plugin holding no permissions.
async fn plugin() -> (Vm, Arc<PluginEnv>, tempfile::TempDir) {
    let (db, dir) = scratch(64 * MIB).await;
    let vm = Vm::new(&VmLimits {
        memory_bytes: 64 * MIB as usize,
        handler_time: Duration::from_secs(5),
    })
    .unwrap();
    let env = Arc::new(PluginEnv {
        state: Weak::new(),
        plugin_id: "test".to_owned(),
        user_id: 7,
        version: "1.0.0".to_owned(),
        plugin_name: "Test".to_owned(),
        assets: Arc::new(Default::default()),
        granted: Arc::new(RwLock::new(Default::default())),
        log: PluginLog::new("test", None),
        budget: vm.budget().clone(),
        server: tokio::runtime::Handle::current(),
        net: PluginNet::new(Arc::new(NetGuard::new(&[]))).unwrap(),
        db,
        files_dir: dir.path().join("files"),
        files_bytes: 5120 * 1024 * 1024,
        hooks: Arc::new(RwLock::new(std::collections::HashSet::new())),
        queue: Arc::new(crate::plugins::queue::EventQueue::default()),
        timers: Arc::new(Mutex::new(HashMap::new())),
        next_timer_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
        config_values: Arc::new(RwLock::new(HashMap::new())),
        files_lock: Arc::new(tokio::sync::Mutex::new(())),
        files_reads: Arc::new(tokio::sync::Semaphore::new(10)),
        views_heard: Default::default(),
        views_skip_logged: Default::default(),
    });
    vm.budget().start(Duration::from_secs(5));
    api::install(&vm, &env).unwrap();
    vm.budget().stop();
    (vm, env, dir)
}

/// Runs `source` within a budget; it returns a string.
async fn run(vm: &Vm, source: &str) -> String {
    vm.budget().start(Duration::from_secs(30));
    let result = vm
        .load("test", source)
        .unwrap()
        .call_async::<String>(())
        .await;
    vm.budget().stop();
    result.unwrap()
}

// Catches: a connection that skipped the lock-down, such as the pool's
// second one, opened while the first holds a transaction.
#[tokio::test]
async fn attach_is_refused() {
    let (db, dir) = scratch(16 * MIB).await;
    let other = dir.path().join("other.sqlite");
    let attach = || {
        db.exec(
            "ATTACH ? AS other".to_owned(),
            vec![text(other.to_str().unwrap())],
        )
    };
    refused(attach().await, "ATTACH");
    let tx = db.begin().await.unwrap();
    refused(attach().await, "ATTACH on the second connection");
    refused(
        tx.exec(
            "ATTACH ? AS other".to_owned(),
            vec![text(other.to_str().unwrap())],
        )
        .await,
        "ATTACH in a transaction",
    );
    tx.finish(false).await.unwrap();
    refused(exec(&db, "DETACH main").await, "DETACH");
    assert!(!other.exists());
}

// Catches: VACUUM INTO, which attaches its target, getting past the guard.
#[tokio::test]
async fn vacuum_into_is_refused_and_writes_nothing() {
    let (db, dir) = scratch(16 * MIB).await;
    exec(&db, "CREATE TABLE t (x)").await.unwrap();
    exec(&db, "INSERT INTO t VALUES (randomblob(1000))")
        .await
        .unwrap();
    let copy = dir.path().join("copy.sqlite");
    let result = db
        .exec(
            "VACUUM INTO ?".to_owned(),
            vec![text(copy.to_str().unwrap())],
        )
        .await;
    assert!(result.is_err(), "{result:?}");
    assert!(!copy.exists());
}

// Catches: an authorizer that lets the function through. SQLite's own
// switch is off too; the authorizer refuses it first, as FORBIDDEN.
#[tokio::test]
async fn load_extension_is_refused() {
    let (db, _dir) = scratch(16 * MIB).await;
    refused(
        query(&db, "SELECT load_extension('/nowhere/evil')").await,
        "load_extension",
    );
    refused(
        query(&db, "SELECT LOAD_EXTENSION('/nowhere/evil', 'entry')").await,
        "LOAD_EXTENSION",
    );
}

// Catches: the quota set as a pragma the plugin can change, in either of
// SQL's two ways of running one.
#[tokio::test]
async fn pragma_max_page_count_cannot_be_raised() {
    let (db, _dir) = scratch(MIB).await;
    refused(
        exec(&db, "PRAGMA max_page_count = 1000000000").await,
        "PRAGMA max_page_count",
    );
    let through_table = query(&db, "SELECT * FROM pragma_max_page_count(1000000000)").await;
    assert!(through_table.is_err(), "{through_table:?}");
    exec(&db, "CREATE TABLE t (x)").await.unwrap();
    let filled = exec(&db, "INSERT INTO t VALUES (randomblob(2000000))").await;
    assert_eq!(filled, Err(DbError::Full));
}

// Catches: an allow-list that is a deny-list, and allowed pragmas that
// stopped working.
#[tokio::test]
async fn other_pragmas_are_refused() {
    let (db, _dir) = scratch(16 * MIB).await;
    for pragma in [
        "PRAGMA journal_mode = DELETE",
        "PRAGMA writable_schema = ON",
        "PRAGMA page_size = 65536",
        "PRAGMA temp_store = MEMORY",
        "PRAGMA cache_size = -1000000",
        "PRAGMA mmap_size = 1000000000",
        "PRAGMA query_only = ON",
        "PRAGMA database_list",
        "PRAGMA compile_options",
        "PRAGMA incremental_vacuum",
        "PRAGMA main.max_page_count",
    ] {
        refused(exec(&db, pragma).await, pragma);
    }
    let through_table = query(&db, "SELECT * FROM pragma_database_list").await;
    assert!(through_table.is_err(), "{through_table:?}");

    exec(
        &db,
        "CREATE TABLE parent (id INTEGER PRIMARY KEY);
         CREATE TABLE child (id INTEGER, parent INTEGER REFERENCES parent (id));
         CREATE INDEX child_parent ON child (parent);
         PRAGMA user_version = 3;
         PRAGMA foreign_keys = ON;",
    )
    .await
    .unwrap();
    let version = query(&db, "PRAGMA user_version").await.unwrap();
    assert_eq!(version.rows, [[SqlValue::Integer(3)]]);
    for allowed in [
        "PRAGMA foreign_keys",
        "PRAGMA table_info(child)",
        "PRAGMA table_xinfo(child)",
        "PRAGMA index_list(child)",
        "PRAGMA index_info(child_parent)",
        "PRAGMA foreign_key_list(child)",
        "SELECT name FROM pragma_table_info('child')",
    ] {
        let rows = query(&db, allowed).await;
        assert!(rows.is_ok_and(|rows| !rows.rows.is_empty()), "{allowed}");
    }
}

// Catches: temporary tables, which live outside the database's quota.
#[tokio::test]
async fn temporary_tables_are_refused() {
    let (db, _dir) = scratch(16 * MIB).await;
    exec(&db, "CREATE TABLE t (x)").await.unwrap();
    for statement in [
        "CREATE TEMP TABLE scratch (x)",
        "CREATE TABLE temp.scratch (x)",
        "CREATE TEMP VIEW v AS SELECT 1",
        "CREATE TEMP TRIGGER trigger_t AFTER INSERT ON t BEGIN SELECT 1; END",
    ] {
        refused(exec(&db, statement).await, statement);
    }
}

// Catches: a quota that isn't set on every connection, or a full database
// reported as some other error.
#[tokio::test]
async fn the_quota_stops_inserts() {
    let (db, _dir) = scratch(MIB).await;
    exec(&db, "CREATE TABLE t (x)").await.unwrap();
    exec(&db, "INSERT INTO t VALUES (randomblob(100000))")
        .await
        .unwrap();
    let tx = db.begin().await.unwrap();
    let full = exec(&db, "INSERT INTO t VALUES (randomblob(2000000))").await;
    assert!(full.is_err());
    let full = tx
        .exec(
            "INSERT INTO t VALUES (randomblob(2000000))".to_owned(),
            Vec::new(),
        )
        .await;
    assert_eq!(full, Err(DbError::Full));
    assert_eq!(
        DbError::Full.to_string(),
        "PLUGIN_LIMIT: the plugin's database is full"
    );
    // SQLite may have rolled the transaction back: nothing more runs in
    // it, rather than on its own outside it.
    let after = tx
        .exec("INSERT INTO t VALUES (1)".to_owned(), Vec::new())
        .await;
    if after != Err(DbError::Ended) {
        after.unwrap();
    }
    tx.finish(false).await.unwrap();
    let rows = query(&db, "SELECT count(*) FROM t").await.unwrap();
    assert_eq!(rows.rows, [[SqlValue::Integer(1)]]);
    let full = exec(&db, "INSERT INTO t VALUES (randomblob(2000000))").await;
    assert_eq!(full, Err(DbError::Full));
}

// Catches: the plugin's own BEGIN or COMMIT putting the pool's connections
// and sqlx's transactions out of step.
#[tokio::test]
async fn transactions_in_sql_stay_in_their_statement() {
    let (db, _dir) = scratch(16 * MIB).await;
    exec(&db, "CREATE TABLE t (x)").await.unwrap();
    exec(&db, "BEGIN; INSERT INTO t VALUES (1); COMMIT")
        .await
        .unwrap();
    let left_open = exec(&db, "BEGIN; INSERT INTO t VALUES (2)").await;
    assert_eq!(
        left_open,
        Err(DbError::Invalid(
            "a statement can't leave a transaction open: use db.transaction".to_owned()
        ))
    );
    let tx = db.begin().await.unwrap();
    tx.exec("INSERT INTO t VALUES (3); COMMIT".to_owned(), Vec::new())
        .await
        .unwrap();
    let after = tx
        .exec("INSERT INTO t VALUES (4)".to_owned(), Vec::new())
        .await;
    assert_eq!(after, Err(DbError::Ended));
    assert_eq!(tx.finish(true).await, Err(DbError::Ended));
    let rows = query(&db, "SELECT x FROM t ORDER BY x").await.unwrap();
    assert_eq!(rows.rows, [[SqlValue::Integer(1)], [SqlValue::Integer(3)]]);
    // Both connections take a transaction again.
    let first = db.begin().await.unwrap();
    first.finish(false).await.unwrap();
    let second = db.begin().await.unwrap();
    second.finish(true).await.unwrap();
}

// Catches: a lost type on the way in or out (a boolean as text, a whole
// float as REAL, a buffer as TEXT, NULL as a value).
#[tokio::test]
async fn values_round_trip() {
    let (vm, _env, _dir) = plugin().await;
    let source = r#"
        db.exec("CREATE TABLE v (a, b, c, d, e, f, g, h, i, j)")
        local bytes = buffer.fromstring("\0\1\2z")
        db.exec("INSERT INTO v VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            nil, true, false, 42, 3.0, 1.5, "text ✓", bytes, 2^53, 2^53 + 2)
        local row = db.query([[SELECT *, typeof(a) ta, typeof(b) tb, typeof(c) tc,
            typeof(d) td, typeof(e) te, typeof(f) tf, typeof(g) tg, typeof(h) th,
            typeof(i) ti, typeof(j) tj FROM v]])[1]
        local seen = {}
        for _, name in { "a", "b", "c", "d", "e", "f", "g", "h", "i", "j" } do
            local value = row[name]
            local shown = if typeof(value) == "buffer" then buffer.tostring(value) else tostring(value)
            if name == "i" then shown = tostring(value == 2^53) end
            if name == "j" then shown = tostring(value == 2^53 + 2) end
            table.insert(seen, name .. "=" .. typeof(value) .. ":" .. shown .. ":" .. row["t" .. name])
        end
        return table.concat(seen, " ")
    "#;
    assert_eq!(
        run(&vm, source).await,
        "a=nil:nil:null b=number:1:integer c=number:0:integer d=number:42:integer \
         e=number:3:integer f=number:1.5:real g=string:text ✓:text \
         h=buffer:\0\u{1}\u{2}z:blob i=number:true:integer j=number:true:real",
    );
}

// Catches: integers past 2^53 rounded into a double on the way out.
#[tokio::test]
async fn big_integers_come_back_as_strings() {
    let (vm, _env, _dir) = plugin().await;
    let source = r#"
        local row = db.query([[SELECT 9007199254740993 AS big, -9007199254740993 AS small,
            9007199254740992 AS edge, -9007199254740992 AS low_edge]])[1]
        db.exec("CREATE TABLE t (x)")
        local inserted = db.exec("INSERT INTO t (rowid, x) VALUES (9007199254740993, 1)")
        return table.concat({
            typeof(row.big), row.big, typeof(row.small), row.small,
            typeof(row.edge), tostring(row.edge == 2^53),
            typeof(row.low_edge), tostring(row.low_edge == -2^53),
            inserted.last_id, tostring(inserted.changes),
        }, " ")
    "#;
    assert_eq!(
        run(&vm, source).await,
        "string 9007199254740993 string -9007199254740993 number true \
         number true 9007199254740993 1"
    );
}

// Catches: statements inside the transaction sent to the other connection
// (it can't see the uncommitted row), no rollback on an error, the error
// changed on its way out, and nesting let through.
#[tokio::test]
async fn a_failing_transaction_rolls_back() {
    let (vm, _env, _dir) = plugin().await;
    let source = r#"
        db.exec("CREATE TABLE t (x)")
        local seen
        local ok, failure = pcall(db.transaction, function()
            db.exec("INSERT INTO t VALUES (1)")
            seen = db.query("SELECT count(*) AS n FROM t")[1].n
            error({ code = 7 })
        end)
        local after_failure = db.query("SELECT count(*) AS n FROM t")[1].n
        local _, message = pcall(db.transaction, function()
            error("boom", 0)
        end)
        local result = db.transaction(function()
            db.exec("INSERT INTO t VALUES (2)")
            return "done", 5
        end)
        local after_success = db.query("SELECT count(*) AS n FROM t")[1].n
        local nested_ok, nested = pcall(db.transaction, function()
            db.transaction(function() end)
        end)
        local _, not_a_function = pcall(db.transaction, 5)
        return table.concat({
            tostring(ok), tostring(failure.code), seen, after_failure, message, result,
            after_success, tostring(nested_ok), nested, not_a_function,
        }, "|")
    "#;
    assert_eq!(
        run(&vm, source).await,
        "false|7|1|0|boom|done|1|false|\
         INVALID_ARGUMENT: a transaction can't start inside another|\
         INVALID_ARGUMENT: db.transaction takes a function"
    );
}

// Catches: a transaction whose handler was cancelled (its plugin stopped)
// keeping the database's write lock and its row.
#[tokio::test]
async fn an_abandoned_transaction_rolls_back() {
    let (vm, env, _dir) = plugin().await;
    assert_eq!(
        run(&vm, "db.exec('CREATE TABLE t (x)') return 'ok'").await,
        "ok"
    );
    vm.budget().start(Duration::from_secs(30));
    let source = "db.transaction(function() db.exec('INSERT INTO t VALUES (1)') wait(60) end)";
    let call = vm.load("test", source).unwrap().call_async::<()>(());
    let cut = tokio::time::timeout(Duration::from_millis(300), call).await;
    vm.budget().stop();
    assert!(cut.is_err(), "the transaction ended by itself");
    // What the cancelled call held goes with Lua's next collection.
    vm.lua.gc_collect().unwrap();
    let began = Instant::now();
    env.db
        .exec("INSERT INTO t VALUES (2)".to_owned(), Vec::new())
        .await
        .unwrap();
    assert!(
        began.elapsed() < Duration::from_secs(2),
        "{:?}",
        began.elapsed()
    );
    let rows = query(&env.db, "SELECT x FROM t").await.unwrap();
    assert_eq!(rows.rows, [[SqlValue::Integer(2)]]);
}

// Catches: no time limit, and a limit left on the connection that stops
// the statements after it.
#[tokio::test]
async fn a_long_statement_is_interrupted() {
    let (db, _dir) = scratch(16 * MIB).await;
    let counting = "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c \
                    WHERE x < 1000000000000) SELECT count(*) FROM c";
    let began = Instant::now();
    let result = query(&db, counting).await;
    let took = began.elapsed();
    assert_eq!(result, Err(DbError::TooLong));
    assert!(
        took >= DB_STATEMENT_TIME && took < DB_STATEMENT_TIME + Duration::from_secs(3),
        "{took:?}"
    );
    assert_eq!(
        DbError::TooLong.to_string(),
        "PLUGIN_LIMIT: a statement can run for at most 5 seconds"
    );
    let short = "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c \
                 WHERE x < 100000) SELECT count(*) AS n FROM c";
    for _ in 0..4 {
        let rows = query(&db, short).await.unwrap();
        assert_eq!(rows.rows, [[SqlValue::Integer(100_000)]]);
    }
}

// Catches: the row limit counted off by one, or not at all.
#[tokio::test]
async fn a_query_returns_at_most_10000_rows() {
    let (db, _dir) = scratch(16 * MIB).await;
    let rows = |count: usize| {
        format!(
            "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c \
             WHERE x < {count}) SELECT x FROM c"
        )
    };
    let found = db.query(rows(DB_MAX_ROWS), Vec::new()).await.unwrap();
    assert_eq!(found.rows.len(), DB_MAX_ROWS);
    assert_eq!(found.columns, ["x"]);
    let too_many = db.query(rows(DB_MAX_ROWS + 1), Vec::new()).await;
    assert_eq!(too_many, Err(DbError::TooManyRows));
    assert_eq!(
        DbError::TooManyRows.to_string(),
        "PLUGIN_LIMIT: a query can return at most 10000 rows"
    );
}

// Catches: db calls asking for a plugin permission.
#[tokio::test]
async fn queries_need_no_permission() {
    let (vm, env, _dir) = plugin().await;
    assert!(env.granted.read().unwrap().is_empty());
    let source = r#"
        local made = db.exec("CREATE TABLE notes (body TEXT)")
        local added = db.exec("INSERT INTO notes VALUES (?), (?)", "one", "two")
        local rows = db.query("SELECT body FROM notes ORDER BY body")
        local _, bad = pcall(db.exec, "INSERT INTO notes VALUES (?)", {})
        local _, wrong = pcall(db.query, 5)
        return table.concat({
            tostring(made.changes), tostring(added.changes), added.last_id,
            rows[1].body, rows[2].body, bad, wrong,
        }, " ")
    "#;
    assert_eq!(
        run(&vm, source).await,
        "0 2 2 one two INVALID_ARGUMENT: argument 1 can't go into SQL: it's a table \
         INVALID_ARGUMENT: sql must be a string"
    );
}

// Catches: SELECT zeroblob exceeding the VM's memory cap.
#[tokio::test]
async fn a_huge_blob_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(&dir.path().join("data.sqlite"), 10 * MIB, MIB as usize)
        .await
        .unwrap();
    let result = db
        .query("SELECT zeroblob(5000000)".to_owned(), Vec::new())
        .await;
    assert!(
        result.is_err(),
        "expected huge blob to be refused by length limit"
    );
}

#[tokio::test]
async fn a_result_over_the_memory_cap_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let db = open(&dir.path().join("data.sqlite"), 10 * MIB, MIB as usize)
        .await
        .unwrap();
    let sql = "WITH RECURSIVE cnt(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM cnt WHERE x < 12) SELECT zeroblob(100000) FROM cnt".to_owned();
    let result = db.query(sql, Vec::new()).await;
    assert_eq!(result.unwrap_err(), DbError::ResultTooLarge);
}
