//! The plugin runtime's tests (plan Task 7).

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ed25519_dalek::SigningKey;
use opencord_common::snowflake::SnowflakeGenerator;
use opencord_plugins::limits::LOG_LINES;
use opencord_plugins::manifest;
use opencord_plugins::package::Package;
use opencord_proto::v1 as proto;

use super::Clock;
use crate::bootstrap;
use crate::config::Config;
use crate::db::test_pool;
use crate::guild::{Guild, Member, User};
use crate::plugins::{
    InstalledPlugin, PluginEvent, PluginHandle, PluginPermission, Plugins, identity,
};
use crate::state::AppState;
use crate::voice::nodes::VoiceNodes;

const OWNER: i64 = 1;

/// A bootstrapped server owned by OWNER, keeping its files in a temporary
/// folder, with small plugin limits.
async fn state() -> (Arc<AppState>, tempfile::TempDir) {
    let (pool, dir) = test_pool().await;
    let ids = SnowflakeGenerator::new(0).unwrap();
    let bootstrap = bootstrap::initialize(&pool, "Test", &ids, 0).await.unwrap();
    let mut conn = pool.acquire().await.unwrap();
    let mut guild = Guild::load(&mut conn, bootstrap.meta).await.unwrap();
    drop(conn);
    guild.meta.owner_id = Some(OWNER);
    guild.members.insert(
        OWNER,
        Member {
            user: User {
                id: OWNER,
                public_key: vec![1; 32],
                display_name: "owner".to_owned(),
                avatar_hash: None,
            },
            nickname: None,
            role_ids: Vec::new(),
            joined_at: 0,
        },
    );
    let mut config = Config::default();
    config.server.data_dir = dir.path().to_path_buf();
    config.plugins.handler_ms = 50;
    config.plugins.memory_mb = 16;
    let state = AppState::new(
        config,
        pool,
        ids,
        [0; 32],
        guild,
        SigningKey::from_bytes(&[1; 32]),
        VoiceNodes::new(None, [0; 32]),
    );
    (Arc::new(state), dir)
}

/// A plugin whose manifest runs `scripts` in order, with `files` in its
/// package.
fn installed(id: &str, user_id: i64, scripts: &[&str], files: &[(&str, &str)]) -> InstalledPlugin {
    let listed: Vec<String> = scripts.iter().map(|script| format!("{script:?}")).collect();
    let source = format!(
        r#"return {{
            id = "{id}", name = "Test plugin", version = "1.2.3", author = "Tester",
            license = "MIT", api = 1, server = {{ {} }},
        }}"#,
        listed.join(", ")
    );
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("manifest.lua"), &source).unwrap();
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    let package = Package::from_dir(dir.path()).unwrap();
    let manifest = manifest::evaluate(&source).unwrap();
    manifest::check_files(&manifest, &package).unwrap();
    InstalledPlugin {
        id: id.to_owned(),
        user_id,
        folder: dir.path().to_path_buf(),
        manifest,
        package,
        granted: HashSet::new(),
    }
}

/// A plugin with one script, `server/main.luau`.
fn one_script(id: &str, source: &str) -> InstalledPlugin {
    installed(
        id,
        77,
        &["server/main.luau"],
        &[("server/main.luau", source)],
    )
}

fn event(name: &str) -> PluginEvent {
    PluginEvent {
        name: name.to_owned(),
        payload: serde_json::json!({}),
        reply: None,
    }
}

fn logged(handle: &PluginHandle) -> Vec<proto::PluginLogLine> {
    handle.log().lines(None, LOG_LINES)
}

/// The first line containing `text`, waiting up to 5 s for it.
async fn wait_for_log(handle: &PluginHandle, text: &str) -> proto::PluginLogLine {
    let began = Instant::now();
    loop {
        if let Some(line) = logged(handle)
            .into_iter()
            .find(|line| line.text.contains(text))
        {
            return line;
        }
        assert!(
            began.elapsed() < Duration::from_secs(5),
            "no {text:?} in {:#?}",
            logged(handle)
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// A clock whose sleeps end at once, moving it forward.
struct FakeClock {
    base: Instant,
    offset_ns: AtomicU64,
    sleeps: Mutex<Vec<Duration>>,
}

impl FakeClock {
    fn new() -> Arc<FakeClock> {
        Arc::new(FakeClock {
            base: Instant::now(),
            offset_ns: AtomicU64::new(0),
            sleeps: Mutex::new(Vec::new()),
        })
    }

    fn sleeps(&self) -> Vec<Duration> {
        self.sleeps.lock().unwrap().clone()
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Instant {
        self.base + Duration::from_nanos(self.offset_ns.load(Ordering::SeqCst))
    }

    fn sleep(&self, wait: Duration) -> super::Sleep {
        self.sleeps.lock().unwrap().push(wait);
        let nanos = u64::try_from(wait.as_nanos()).unwrap();
        self.offset_ns.fetch_add(nanos, Ordering::SeqCst);
        Box::pin(std::future::ready(()))
    }
}

#[tokio::test]
async fn scripts_run_in_order_then_start_fires() {
    let (state, _dir) = state().await;
    let plugin = installed(
        "ordered",
        77,
        &["server/a.luau", "server/b.luau"],
        &[
            ("server/a.luau", "greeting = 'hello'"),
            (
                "server/b.luau",
                "greeting ..= ' world'\n\
                 opencord.on('start', function() log.info(greeting, opencord.id, opencord.version) end)",
            ),
        ],
    );

    state.plugins.start(&state, plugin).await.unwrap();

    let handle = state.plugins.get("ordered").unwrap();
    assert_eq!(handle.state(), proto::PluginState::Running);
    let line = wait_for_log(&handle, "hello").await;
    assert_eq!(line.text, "hello world\tordered\t1.2.3");
    assert_eq!(line.level, proto::PluginLogLevel::Info as i32);
}

#[tokio::test]
async fn a_failing_start_fails_start_with_its_error() {
    let (state, _dir) = state().await;
    let plugin = one_script(
        "broken",
        "opencord.on('start', function() error('no config') end)",
    );

    let error = state.plugins.start(&state, plugin).await.unwrap_err();

    assert_eq!(error.to_string(), "server/main.luau:1: no config");
    let handle = state.plugins.get("broken").unwrap();
    assert_eq!(handle.state(), proto::PluginState::Stopped);
    assert_eq!(
        handle.status_reason().as_deref(),
        Some("server/main.luau:1: no config")
    );
}

#[tokio::test]
async fn a_failing_handler_is_logged_with_its_traceback() {
    let (state, _dir) = state().await;
    let plugin = one_script(
        "failing",
        "local function explode()\n    error('boom')\nend\n\
         opencord.on('ping', function() explode() end)",
    );
    state.plugins.start(&state, plugin).await.unwrap();
    let handle = state.plugins.get("failing").unwrap();

    state.plugins.deliver("failing", event("ping"));

    let line = wait_for_log(&handle, "ping handler failed").await;
    assert_eq!(line.level, proto::PluginLogLevel::Error as i32);
    assert!(
        line.text
            .starts_with("ping handler failed: server/main.luau:2: boom\n"),
        "{}",
        line.text
    );
    // The traceback names the function that failed.
    assert!(line.text.contains("explode"), "{}", line.text);
    assert_eq!(handle.state(), proto::PluginState::Running);
}

#[tokio::test]
async fn an_endless_handler_stops_at_its_budget() {
    let (state, _dir) = state().await;
    let plugin = one_script(
        "looping",
        "opencord.on('ping', function() while true do end end)",
    );
    state.plugins.start(&state, plugin).await.unwrap();
    let handle = state.plugins.get("looping").unwrap();

    state.plugins.deliver("looping", event("ping"));

    let line = wait_for_log(&handle, "ping handler failed").await;
    assert_eq!(
        line.text,
        "ping handler failed: the handler ran longer than 50 ms"
    );
}

// Catches: counting failures without ever acting on them, or restarting
// forever.
#[tokio::test]
async fn a_plugin_that_keeps_failing_is_turned_off() {
    let (state, _dir) = state().await;
    let id = identity::ensure(&state, "crashy", "Crashy", None, OWNER, &HashSet::new())
        .await
        .unwrap();
    let plugin = installed(
        "crashy",
        id.user_id,
        &["server/main.luau"],
        &[(
            "server/main.luau",
            "opencord.on('ping', function() error('always') end)",
        )],
    );
    sqlx::query(
        "INSERT INTO plugins (id, user_id, role_id, version, folder, manifest, signature,
                              running, installed_at, updated_at)
         VALUES ('crashy', ?, ?, '1.2.3', 'v1', ?, 'unsigned', 1, 0, 0)",
    )
    .bind(id.user_id)
    .bind(id.role_id)
    .bind(serde_json::to_string(&plugin.manifest).unwrap())
    .execute(&state.db)
    .await
    .unwrap();
    let clock = FakeClock::new();
    let plugins = Plugins::with_clock(clock.clone());
    plugins.start(&state, plugin).await.unwrap();
    let handle = plugins.get("crashy").unwrap();

    let began = Instant::now();
    while handle.state() != proto::PluginState::Crashed {
        assert!(
            began.elapsed() < Duration::from_secs(20),
            "still {:?}",
            handle.state()
        );
        plugins.deliver("crashy", event("ping"));
        tokio::time::sleep(Duration::from_millis(2)).await;
    }

    let reason = "turned off after failing 5 times in an hour: \
                  ping handler failed: server/main.luau:1: always";
    assert_eq!(handle.status_reason().as_deref(), Some(reason));
    let secs = Duration::from_secs;
    assert_eq!(clock.sleeps(), [secs(1), secs(2), secs(4), secs(8)]);
    let saved: Option<String> =
        sqlx::query_scalar("SELECT crashed_reason FROM plugins WHERE id = 'crashy'")
            .fetch_one(&state.db)
            .await
            .unwrap();
    assert_eq!(saved.as_deref(), Some(reason));
    let started = logged(&handle)
        .iter()
        .filter(|line| line.text.starts_with("restarting"))
        .count();
    assert_eq!(started, 4);

    // Starting it again clears both.
    let again = installed(
        "crashy",
        id.user_id,
        &["server/main.luau"],
        &[("server/main.luau", "")],
    );
    plugins.start(&state, again).await.unwrap();
    let handle = plugins.get("crashy").unwrap();
    assert_eq!(handle.state(), proto::PluginState::Running);
    assert_eq!(handle.status_reason(), None);
    let saved: Option<String> =
        sqlx::query_scalar("SELECT crashed_reason FROM plugins WHERE id = 'crashy'")
            .fetch_one(&state.db)
            .await
            .unwrap();
    assert_eq!(saved, None);
}

#[tokio::test]
async fn out_of_memory_restarts_the_plugin() {
    let (state, _dir) = state().await;
    let plugins = Plugins::with_clock(FakeClock::new());
    let plugin = one_script(
        "hungry",
        "opencord.on('start', function() log.info('started') end)\n\
         opencord.on('ping', function() local s = string.rep('x', 64 * 1024 * 1024) end)",
    );
    plugins.start(&state, plugin).await.unwrap();
    let handle = plugins.get("hungry").unwrap();

    plugins.deliver("hungry", event("ping"));

    wait_for_log(&handle, "ping handler failed: the plugin ran out of memory").await;
    let began = Instant::now();
    // The restart logs "started" again, then the supervisor marks it
    // running.
    while logged(&handle)
        .iter()
        .filter(|line| line.text == "started")
        .count()
        < 2
        || handle.state() != proto::PluginState::Running
    {
        assert!(began.elapsed() < Duration::from_secs(5), "no restart");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn require_loads_the_plugins_own_files_only() {
    let (state, _dir) = state().await;
    let main = r#"
        local util = require("util")
        local again = require("lib/util")
        log.info(util.double(21), util == require("util"), loads, again.name)
        for _, name in { "../x", "client/x", "server/util", "util.luau", "", 5 } do
            local ok, err = pcall(require, name)
            log.info("require", name, ok, err)
        end
    "#;
    let plugin = installed(
        "modular",
        77,
        &["server/main.luau"],
        &[
            ("server/main.luau", main),
            (
                "server/util.luau",
                "loads = (loads or 0) + 1\nreturn { double = function(n) return n * 2 end }",
            ),
            ("server/lib/util.lua", "return { name = 'lib' }"),
            ("client/x.luau", "return 1"),
        ],
    );

    state.plugins.start(&state, plugin).await.unwrap();

    let handle = state.plugins.get("modular").unwrap();
    let texts: Vec<String> = logged(&handle).into_iter().map(|line| line.text).collect();
    let bad_name = "INVALID_ARGUMENT: require takes a module name such as \"util\" or \"lib/util\"";
    assert_eq!(
        texts,
        [
            "42\ttrue\t1\tlib".to_owned(),
            format!("require\t../x\tfalse\t{bad_name}"),
            "require\tclient/x\tfalse\tNOT_FOUND: the plugin has no \
             server/client/x.luau or server/client/x.lua"
                .to_owned(),
            "require\tserver/util\tfalse\tNOT_FOUND: the plugin has no \
             server/server/util.luau or server/server/util.lua"
                .to_owned(),
            format!("require\tutil.luau\tfalse\t{bad_name}"),
            format!("require\t\tfalse\t{bad_name}"),
            format!("require\t5\tfalse\t{bad_name}"),
        ]
    );
}

#[tokio::test]
async fn permission_errors_are_plain_strings() {
    let (state, _dir) = state().await;
    let plugin = one_script(
        "asking",
        r#"
        log.info(opencord.has_permission("send_messages"))
        log.info(pcall(opencord.has_permission, "fly"))
        log.info(pcall(opencord.on, "start", 5))
        log.info(pcall(function() opencord.id = "other" end))
        "#,
    );

    state.plugins.start(&state, plugin).await.unwrap();

    let handle = state.plugins.get("asking").unwrap();
    let texts: Vec<String> = logged(&handle).into_iter().map(|line| line.text).collect();
    assert_eq!(texts[0], "false");
    assert_eq!(
        texts[1],
        "false\tINVALID_ARGUMENT: there's no permission named fly"
    );
    assert_eq!(
        texts[2],
        "false\tINVALID_ARGUMENT: opencord.on takes a function"
    );
    assert!(texts[3].starts_with("false\t"), "{}", texts[3]);
}

#[tokio::test]
async fn argument_errors_are_plain_strings() {
    let (state, _dir) = state().await;
    let mut plugin = one_script(
        "arg-checker",
        r#"
        local calls = {
            function() timer.after("not_num", function() end) end,
            function() timer.after(1, "not_func") end,
            function() timer.every("not_num", function() end) end,
            function() timer.cron(123, function() end) end,
            function() timer.cancel("not_id") end,
            function() commands.unregister(123) end,
            function() members.kick("1", {}) end,
            function() members.ban("1", {}) end,
            function() voice.mute("1", "not_bool") end,
            function() voice.deafen("1", "not_bool") end,
            function() config.get(123) end,
            function() plugins.emit(123, "data") end,
            function() plugins.on(123, "ev", function() end) end,
            function() commands.register({ name = "foo", description = "desc", options = "bad" }) end,
            function() commands.register({ name = "foo", description = "desc", options = { { name = "opt", description = "d", kind = "text", required = "bad" } } }) end,
        }
        for _, f in ipairs(calls) do
            local ok, err = pcall(f)
            assert(not ok, "expected call to fail")
            assert(type(err) == "string", "expected string error, got " .. type(err))
            assert(string.find(err, "^INVALID_ARGUMENT:"), "expected INVALID_ARGUMENT prefix: " .. tostring(err))
        end
        log.info("all_checked")
        "#,
    );
    plugin.granted = HashSet::from([
        PluginPermission::RegisterCommands,
        PluginPermission::KickMembers,
        PluginPermission::BanMembers,
        PluginPermission::MuteMembers,
        PluginPermission::DeafenMembers,
        PluginPermission::TalkToPlugins,
    ]);

    state.plugins.start(&state, plugin).await.unwrap();
    let handle = state.plugins.get("arg-checker").unwrap();
    let texts: Vec<String> = logged(&handle).into_iter().map(|line| line.text).collect();
    assert_eq!(texts.last().map(|s| s.as_str()), Some("all_checked"));
}

#[tokio::test]
async fn stop_delivers_stop_and_cancels_waits() {
    let (state, _dir) = state().await;
    let plugin = one_script(
        "waiting",
        "opencord.on('ping', function() log.info('waiting') wait(10) log.info('woke') end)\n\
         opencord.on('stop', function() log.info('stopping') end)",
    );
    state.plugins.start(&state, plugin).await.unwrap();
    let handle = state.plugins.get("waiting").unwrap();
    state.plugins.deliver("waiting", event("ping"));
    wait_for_log(&handle, "waiting").await;

    let began = Instant::now();
    state.plugins.stop("waiting").await;

    assert!(began.elapsed() < Duration::from_secs(2));
    let texts: Vec<String> = logged(&handle).into_iter().map(|line| line.text).collect();
    assert!(texts.contains(&"stopping".to_owned()), "{texts:?}");
    assert!(texts.contains(&"wait cancelled".to_owned()), "{texts:?}");
    assert!(!texts.contains(&"woke".to_owned()), "{texts:?}");
    assert_eq!(handle.state(), proto::PluginState::Stopped);

    // A stopped plugin takes no events.
    state.plugins.deliver("waiting", event("ping"));
    tokio::time::sleep(Duration::from_millis(100)).await;
    let waits = logged(&handle)
        .iter()
        .filter(|line| line.text == "waiting")
        .count();
    assert_eq!(waits, 1);
}

#[tokio::test]
async fn a_reply_goes_back_once() {
    let (state, _dir) = state().await;
    let plugin = one_script(
        "replying",
        "opencord.on('ask', function(event) log.info(event.question, event.reply('yes', { n = 1 }), event.reply('no')) end)",
    );
    state.plugins.start(&state, plugin).await.unwrap();
    let (sender, answer) = tokio::sync::oneshot::channel();

    state.plugins.deliver(
        "replying",
        PluginEvent {
            name: "ask".to_owned(),
            payload: serde_json::json!({ "question": "ok?" }),
            reply: Some(sender),
        },
    );

    let answer = tokio::time::timeout(Duration::from_secs(5), answer)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(answer, serde_json::json!(["yes", { "n": 1 }]));
    let handle = state.plugins.get("replying").unwrap();
    let line = wait_for_log(&handle, "ok?").await;
    assert_eq!(line.text, "ok?\ttrue\tfalse");
}
