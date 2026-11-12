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
        plugin_name: "Test".to_owned(),
        assets: Arc::new(HashSet::new()),
        granted: Arc::new(RwLock::new(HashSet::new())),
        log: PluginLog::new("test", None),
        budget: vm.budget().clone(),
        server: tokio::runtime::Handle::current(),
        net: PluginNet::new(Arc::new(NetGuard::new(&[]))).unwrap(),
        db: PluginDb::unused(),
        files_dir: PathBuf::new(),
        files_bytes: 0,
        files_lock: Arc::new(tokio::sync::Mutex::new(())),
        files_reads: Arc::new(tokio::sync::Semaphore::new(10)),
        views_heard: Default::default(),
        sessions_heard: Default::default(),
        views_skip_logged: Default::default(),
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
        "ui",
        "activity",
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
    let mut in_client_only = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "--- Client only" {
            in_client_only = true;
        }
        if let Some(rest) = trimmed.strip_prefix("declare ") {
            if let Some((name, _)) = rest.split_once(':') {
                let name = name.trim();
                current_table = Some(name.to_owned());
                if name == "ui" {
                    in_client_only = false;
                }
            }
        } else if trimmed.starts_with('}') {
            current_table = None;
        } else if let Some(table_name) = &current_table {
            if trimmed.contains("Client only") {
                in_client_only = true;
                continue;
            } else if trimmed.contains("Server only") {
                in_client_only = false;
                continue;
            }
            // Check for function declaration: `<fn_name>: (`
            // or `["<fn_name>"]: (` for a name that is a keyword.
            if let Some((fn_name, rest)) = trimmed.split_once(':') {
                let fn_name = fn_name.trim();
                let fn_name = fn_name
                    .strip_prefix("[\"")
                    .and_then(|name| name.strip_suffix("\"]"))
                    .unwrap_or(fn_name);
                let rest = rest.trim();
                if rest.starts_with('(')
                    && !fn_name.contains(' ')
                    && !in_client_only
                    && table_name != "app"
                {
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

// Fails if client definitions in opencord.d.luau don't match BLOCKS or if docs/plugins.md misses blocks or limits.
#[test]
fn client_definitions_match_blocks_and_docs() {
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
    let content = content.replace("\r\n", "\n");

    let client_section = content
        .split_once("\n--- Client only\n")
        .expect("opencord.d.luau must contain a '--- Client only' section")
        .1;

    // 1. All 43 icons must be in IconName union in client definitions
    const ICONS: &[&str] = &[
        "add",
        "arrow_back",
        "arrow_forward",
        "arrow_up",
        "arrow_down",
        "check",
        "close",
        "delete",
        "edit",
        "search",
        "settings",
        "info",
        "warning",
        "error",
        "star",
        "person",
        "group",
        "chat",
        "send",
        "image",
        "link",
        "lock",
        "lock_open",
        "refresh",
        "play",
        "pause",
        "stop",
        "music",
        "mic",
        "volume",
        "schedule",
        "history",
        "pin",
        "tag",
        "copy",
        "download",
        "upload",
        "more",
        "menu",
        "visibility",
        "visibility_off",
        "palette",
        "extension",
    ];
    for &icon in ICONS {
        assert!(
            client_section.contains(&format!("\"{icon}\"")),
            "IconName missing icon '{icon}'"
        );
    }

    // 2. All 35 blocks in BLOCKS must have constructors in `declare ui: { ... }`
    for spec in opencord_plugins::blocks::BLOCKS {
        let constructor = format!("{}: (", spec.name);
        assert!(
            client_section.contains(&constructor),
            "client definitions missing ui.{} constructor",
            spec.name
        );
    }

    // 3. Every block's specific props and events must be present in client definitions
    for spec in opencord_plugins::blocks::BLOCKS {
        for &(prop, _) in spec.props {
            assert!(
                client_section.contains(&format!("{prop}:")),
                "prop '{prop}' of block '{}' missing in client definitions",
                spec.name
            );
        }
        for &event in spec.events {
            assert!(
                client_section.contains(&format!("on_{event}:")),
                "event 'on_{event}' of block '{}' missing in client definitions",
                spec.name
            );
        }
    }

    // 4. Common props and container props
    for &(prop, _) in opencord_plugins::blocks::COMMON_PROPS {
        assert!(
            client_section.contains(&format!("{prop}:")),
            "common prop '{prop}' missing in client definitions"
        );
    }
    for &(prop, _) in opencord_plugins::blocks::CONTAINER_PROPS {
        assert!(
            client_section.contains(&format!("{prop}:")),
            "container prop '{prop}' missing in client definitions"
        );
    }
    for &(prop, _) in opencord_plugins::blocks::STACK_CHILD_PROPS {
        assert!(
            client_section.contains(&format!("{prop}:")),
            "stack child prop '{prop}' missing in client definitions"
        );
    }
    for &event in opencord_plugins::blocks::COMMON_EVENTS {
        assert!(
            client_section.contains(&format!("on_{event}:")),
            "common event 'on_{event}' missing in client definitions"
        );
    }

    // 5. Globals: declare function init, render, migrate; declare app
    assert!(
        client_section.contains("declare function init(")
            || client_section.contains("declare init:"),
        "init declaration missing"
    );
    assert!(
        client_section.contains("declare function render(")
            || client_section.contains("declare render:"),
        "render declaration missing"
    );
    assert!(
        client_section.contains("declare function migrate(")
            || client_section.contains("declare migrate:"),
        "migrate declaration missing"
    );
    assert!(
        client_section.contains("declare app:"),
        "app declaration missing"
    );

    // 6. Verify docs/plugins.md has all blocks, corrected limits, and icons
    let docs_path = manifest_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("docs")
        .join("plugins.md");
    let docs_content = std::fs::read_to_string(&docs_path).unwrap_or_else(|err| {
        panic!(
            "could not read docs/plugins.md at {}: {err}",
            docs_path.display()
        )
    });
    let docs_content = docs_content.replace("\r\n", "\n");

    for spec in opencord_plugins::blocks::BLOCKS {
        assert!(
            docs_content.contains(&format!("`{}`", spec.name)),
            "docs/plugins.md missing block `{}`",
            spec.name
        );
    }

    // Corrected limits table rows
    assert!(
        docs_content.contains("2,000"),
        "docs/plugins.md limits table must specify 2,000 files in a zip"
    );
    assert!(
        docs_content.contains("1 GB"),
        "docs/plugins.md limits table must specify 1 GB database"
    );
    assert!(
        docs_content.contains("5 GB"),
        "docs/plugins.md limits table must specify 5 GB files"
    );
    assert!(
        docs_content.contains("10 s") || docs_content.contains("10 seconds"),
        "docs/plugins.md limits table must specify 10 s HTTP timeout"
    );
    assert!(
        docs_content.contains("1,000"),
        "docs/plugins.md limits table must specify 1,000 timers"
    );
    assert!(
        docs_content.contains("100 ms"),
        "docs/plugins.md limits table must specify 100 ms handler time"
    );
}
