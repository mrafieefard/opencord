// SPDX-License-Identifier: MPL-2.0

//! Type definitions tests (plan Task 21, design 2026-11-07 §10.3).

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::time::Duration;

use opencord_plugins::sandbox::{Vm, VmLimits};
use opencord_server::plugins::api;
use opencord_server::plugins::database::PluginDb;
use opencord_server::plugins::logbook::PluginLog;
use opencord_server::plugins::net::{NetGuard, PluginNet};
use opencord_server::plugins::queue::EventQueue;
use opencord_server::plugins::runtime::PluginEnv;

#[tokio::test]
async fn definitions_match_the_api() {
    let vm = Vm::new(&VmLimits {
        memory_bytes: 64 * 1024 * 1024,
        handler_time: Duration::from_secs(5),
    })
    .unwrap();

    let env = Arc::new(PluginEnv {
        state: Weak::new(),
        plugin_id: "test".to_owned(),
        user_id: 1,
        version: "1.0.0".to_owned(),
        granted: Arc::new(RwLock::new(HashSet::new())),
        log: PluginLog::new("test", None),
        budget: vm.budget().clone(),
        server: tokio::runtime::Handle::current(),
        net: PluginNet::new(Arc::new(NetGuard::new(&[]))).unwrap(),
        db: PluginDb::unused(),
        files_dir: PathBuf::new(),
        files_bytes: 0,
        hooks: Arc::new(RwLock::new(HashSet::new())),
        queue: Arc::new(EventQueue::default()),
        timers: Arc::new(Mutex::new(HashMap::new())),
        next_timer_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
        config_values: Arc::new(RwLock::new(HashMap::new())),
    });

    vm.budget().start(Duration::from_secs(5));
    api::install(&vm, &env).unwrap();
    vm.budget().stop();

    // 1. Walk API globals for all function names
    let globals = vm.lua.globals();
    let api_tables = [
        "opencord",
        "log",
        "http",
        "db",
        "messages",
        "typing",
        "commands",
        "channels",
        "roles",
        "members",
        "bans",
        "invites",
        "server",
        "voice",
        "soundboard",
        "music",
        "files",
        "hooks",
        "timer",
        "plugins",
        "config",
    ];

    let mut api_functions = BTreeSet::new();
    for table_name in api_tables {
        let table: mlua::Table = globals
            .get(table_name)
            .unwrap_or_else(|_| panic!("global table '{table_name}' must exist"));
        for pair in table.pairs::<String, mlua::Value>() {
            let (fn_name, val) = pair.unwrap();
            if matches!(val, mlua::Value::Function(_)) {
                api_functions.insert(format!("{table_name}.{fn_name}"));
            }
        }
    }

    // 2. Read crates/opencord-plugins/api/opencord.d.luau
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let d_luau_path = manifest_dir
        .parent()
        .unwrap()
        .join("opencord-plugins")
        .join("api")
        .join("opencord.d.luau");

    let content = std::fs::read_to_string(&d_luau_path).unwrap_or_else(|err| {
        panic!(
            "could not read opencord.d.luau at {}: {err}",
            d_luau_path.display()
        )
    });

    // 3. Parse declare blocks for function names
    let mut declared_functions = BTreeSet::new();
    let mut current_table: Option<String> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("declare ") {
            if let Some((name, _)) = rest.split_once(':') {
                let name = name.trim();
                current_table = Some(name.to_owned());
            }
        } else if trimmed.starts_with('}') {
            current_table = None;
        } else if let Some(table_name) = &current_table {
            // Check for function declaration: `<fn_name>: (`
            if let Some((fn_name, rest)) = trimmed.split_once(':') {
                let fn_name = fn_name.trim();
                let rest = rest.trim();
                if rest.starts_with('(') && !fn_name.contains(' ') {
                    declared_functions.insert(format!("{table_name}.{fn_name}"));
                }
            }
        }
    }

    // Assert the two sets are equal
    let missing_in_d_luau: Vec<_> = api_functions.difference(&declared_functions).collect();
    let extra_in_d_luau: Vec<_> = declared_functions.difference(&api_functions).collect();

    assert!(
        missing_in_d_luau.is_empty() && extra_in_d_luau.is_empty(),
        "definitions mismatch!\nMissing in opencord.d.luau: {missing_in_d_luau:?}\nExtra in opencord.d.luau: {extra_in_d_luau:?}"
    );
}
