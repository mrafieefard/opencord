// SPDX-License-Identifier: MPL-2.0

//! Plugin config and log watching tests (plan Task 20, design 2026-11-07 §10.3, §17).

use opencord_proto::v1 as proto;
use proto::event::Kind as EventKind;

use crate::common::{TestServer, channel_id, media_token, owner};
use crate::support::{install_with_config, manifest_with, package, wait_for_log};

#[tokio::test]
async fn config_values_have_their_kinds() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;

    let ch = channel_id(&ready, "general");
    let r = ready.roles[0].id;
    let u = ready.self_user.as_ref().unwrap().id;

    let extra_manifest = r#"
        config = {
            { key = "num", label = "Number", kind = "number" },
            { key = "bool", label = "Boolean", kind = "boolean" },
            { key = "txt", label = "Text", kind = "text" },
            { key = "sec", label = "Secret", kind = "secret" },
            { key = "ch", label = "Channel", kind = "channel" },
            { key = "ro", label = "Role", kind = "role" },
            { key = "usr", label = "User", kind = "user" },
            { key = "choice", label = "Choice", kind = "choice", choices = { "apple", "banana" } },
        },
    "#;

    let lua_source = format!(
        r#"
        local num = config.get("num")
        local bool = config.get("bool")
        local txt = config.get("txt")
        local sec = config.get("sec")
        local ch = config.get("ch")
        local ro = config.get("ro")
        local usr = config.get("usr")
        local choice = config.get("choice")
        local unset = config.get("non_existent")

        assert(type(num) == "number" and num == 42, "num failed: " .. tostring(num))
        assert(type(bool) == "boolean" and bool == true, "bool failed: " .. tostring(bool))
        assert(type(txt) == "string" and txt == "hello", "txt failed: " .. tostring(txt))
        assert(type(sec) == "string" and sec == "topsecret", "sec failed: " .. tostring(sec))
        assert(type(ch) == "string" and ch == "{ch}", "ch failed: " .. tostring(ch))
        assert(type(ro) == "string" and ro == "{r}", "ro failed: " .. tostring(ro))
        assert(type(usr) == "string" and usr == "{u}", "usr failed: " .. tostring(usr))
        assert(type(choice) == "string" and choice == "apple", "choice failed: " .. tostring(choice))
        assert(unset == nil, "unset failed: " .. tostring(unset))

        log.info("all_kinds_verified")
        "#
    );

    let zip = package(&[
        (
            "manifest.lua",
            &manifest_with("config-kinds", "1.0.0", &[], extra_manifest),
        ),
        ("server/main.luau", &lua_source),
    ]);

    let submitted_config = vec![
        proto::PluginConfigValue {
            key: "num".into(),
            value: "42".into(),
            has_value: true,
        },
        proto::PluginConfigValue {
            key: "bool".into(),
            value: "true".into(),
            has_value: true,
        },
        proto::PluginConfigValue {
            key: "txt".into(),
            value: "hello".into(),
            has_value: true,
        },
        proto::PluginConfigValue {
            key: "sec".into(),
            value: "topsecret".into(),
            has_value: true,
        },
        proto::PluginConfigValue {
            key: "ch".into(),
            value: ch.to_string(),
            has_value: true,
        },
        proto::PluginConfigValue {
            key: "ro".into(),
            value: r.to_string(),
            has_value: true,
        },
        proto::PluginConfigValue {
            key: "usr".into(),
            value: u.to_string(),
            has_value: true,
        },
        proto::PluginConfigValue {
            key: "choice".into(),
            value: "apple".into(),
            has_value: true,
        },
    ];

    let _p = install_with_config(
        &server,
        &mut owner,
        &media_token(&ready),
        zip,
        &[],
        submitted_config,
    )
    .await;

    wait_for_log(&mut owner, "config-kinds", "all_kinds_verified").await;
}

#[tokio::test]
async fn a_secret_is_readable_by_the_plugin_but_never_sent_to_apps() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;

    let extra_manifest = r#"
        config = {
            { key = "api_key", label = "API Key", kind = "secret" },
        },
    "#;

    let lua_source = r#"
        local secret = config.get("api_key")
        log.info("secret_read: " .. tostring(secret))
    "#;

    let zip = package(&[
        (
            "manifest.lua",
            &manifest_with("config-secret", "1.0.0", &[], extra_manifest),
        ),
        ("server/main.luau", lua_source),
    ]);

    let submitted_config = vec![proto::PluginConfigValue {
        key: "api_key".into(),
        value: "my-secret-vault-token-42".into(),
        has_value: true,
    }];

    let _p = install_with_config(
        &server,
        &mut owner,
        &media_token(&ready),
        zip,
        &[],
        submitted_config,
    )
    .await;

    // 1. Secret is readable by the plugin
    wait_for_log(
        &mut owner,
        "config-secret",
        "secret_read: my-secret-vault-token-42",
    )
    .await;

    // 2. Secret is never sent to apps in FetchPlugins
    let res = owner
        .ok(proto::request::Kind::FetchPlugins(proto::FetchPlugins {}))
        .await;
    let proto::response::Result::Plugins(plugins_list) = res else {
        panic!("expected Plugins");
    };
    let plugin = plugins_list
        .plugins
        .iter()
        .find(|p| p.id == "config-secret")
        .expect("found plugin");
    let secret_conf = plugin
        .config
        .iter()
        .find(|c| c.key == "api_key")
        .expect("found api_key config");
    assert!(secret_conf.has_value, "secret must have has_value == true");
    assert_eq!(
        secret_conf.value, "",
        "secret value must be empty string in FetchPlugins"
    );
}

#[tokio::test]
async fn log_watchers_get_new_lines() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;

    let zip = package(&[
        (
            "manifest.lua",
            &manifest_with("log-watch-p", "1.0.0", &["read_server"], ""),
        ),
        (
            "server/main.luau",
            r#"
            opencord.on("start", function()
                log.info("initial_line")
            end)
            opencord.on("server_update", function()
                log.info("live_stream_line")
            end)
            "#,
        ),
    ]);

    let _p = install_with_config(
        &server,
        &mut owner,
        &media_token(&ready),
        zip,
        &["read_server"],
        vec![],
    )
    .await;

    wait_for_log(&mut owner, "log-watch-p", "initial_line").await;

    // Start watching log
    owner
        .ok(proto::request::Kind::WatchPluginLog(
            proto::WatchPluginLog {
                plugin_id: "log-watch-p".to_owned(),
                watch: true,
            },
        ))
        .await;

    // Trigger server update which logs a line
    owner
        .ok(proto::request::Kind::UpdateServer(proto::UpdateServer {
            name: Some("Renamed Server".to_owned()),
            description: None,
            open_join: None,
        }))
        .await;

    // Verify client receives PluginLogAppend event
    let event = tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let ev = owner.next_event().await;
            if let EventKind::PluginLogAppend(append) = ev
                && append.plugin_id == "log-watch-p"
            {
                return append;
            }
        }
    })
    .await
    .expect("did not receive PluginLogAppend in time");

    assert!(
        event
            .lines
            .iter()
            .any(|l| l.text.contains("live_stream_line")),
        "expected live_stream_line in PluginLogAppend"
    );
}

#[tokio::test]
async fn a_watch_ends_with_its_session() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;

    let zip = package(&[
        (
            "manifest.lua",
            &manifest_with("log-watch-end", "1.0.0", &["read_server"], ""),
        ),
        (
            "server/main.luau",
            r#"
            opencord.on("server_update", function()
                log.info("line_after_unwatch")
            end)
            "#,
        ),
    ]);

    let _p = install_with_config(
        &server,
        &mut owner,
        &media_token(&ready),
        zip,
        &["read_server"],
        vec![],
    )
    .await;

    // Watch
    owner
        .ok(proto::request::Kind::WatchPluginLog(
            proto::WatchPluginLog {
                plugin_id: "log-watch-end".to_owned(),
                watch: true,
            },
        ))
        .await;

    // Unwatch
    owner
        .ok(proto::request::Kind::WatchPluginLog(
            proto::WatchPluginLog {
                plugin_id: "log-watch-end".to_owned(),
                watch: false,
            },
        ))
        .await;

    // Trigger server update
    owner
        .ok(proto::request::Kind::UpdateServer(proto::UpdateServer {
            name: Some("Renamed Again".to_owned()),
            description: None,
            open_join: None,
        }))
        .await;

    // Ensure we do not receive PluginLogAppend
    let received_append = tokio::time::timeout(std::time::Duration::from_millis(500), async {
        loop {
            let ev = owner.next_event().await;
            if let EventKind::PluginLogAppend(append) = ev
                && append.plugin_id == "log-watch-end"
            {
                return true;
            }
        }
    })
    .await;

    assert!(
        received_append.is_err(),
        "unwatched session should not receive PluginLogAppend"
    );
}
