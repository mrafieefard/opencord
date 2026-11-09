// SPDX-License-Identifier: MPL-2.0

//! Plugin commands API (plan Task 14, design 2026-11-07 §10.3, §10.4, §17).

use std::time::{Duration, Instant};

use opencord_proto::v1 as proto;
use proto::event::Kind as Event;
use proto::request::Kind;
use proto::response::Result as Response;

use crate::common::music::{Room, room};
use crate::common::{
    TestClient, TestServer, channel_id, invite, join, media_token, owner, self_id,
};
use crate::support::{install, manifest, package, wait_for_log};

fn run(channel_id: i64, name: &str, options: &[(&str, &str)]) -> Kind {
    Kind::ExecuteCommand(proto::ExecuteCommand {
        source: String::new(),
        channel_id,
        name: name.to_owned(),
        options: options
            .iter()
            .map(|(name, value)| proto::CommandOptionValue {
                name: (*name).to_owned(),
                value: (*value).to_owned(),
            })
            .collect(),
        nonce: "n1".to_owned(),
    })
}

fn result(response: Response) -> proto::CommandResult {
    match response {
        Response::CommandResult(result) => result,
        other => panic!("expected a command result, got {other:?}"),
    }
}

async fn plugin(
    server: &TestServer,
    owner: &mut TestClient,
    ready: &proto::Ready,
    id: &str,
    permissions: &[&str],
    source: &str,
) -> proto::Plugin {
    let zip = package(&[
        ("manifest.lua", &manifest(id, permissions)),
        ("server/main.luau", source),
    ]);
    install(server, owner, &media_token(ready), zip, permissions).await
}

#[tokio::test]
async fn a_plugin_command_answers_publicly() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let code = invite(&mut owner, None).await;
    let (mut member, member_ready) = join(&server, 2, &code).await;
    let ch = channel_id(&ready, "general");
    let member_id = self_id(&member_ready);

    let p = plugin(
        &server,
        &mut owner,
        &ready,
        "greeter",
        &["register_commands", "send_messages"],
        r#"commands.register({
            name = "greet",
            description = "Greets someone",
            options = {
                { name = "name", description = "Who to greet", kind = "text", required = true }
            }
        }, function(cmd)
            cmd:reply("Hello, " .. cmd.options.name .. "!")
        end)"#,
    )
    .await;

    // Wait until command is registered and available in commands update
    member
        .wait_for(|event| match event {
            Event::CommandsUpdate(update) => update
                .commands
                .as_ref()
                .filter(|c| c.commands.iter().any(|spec| spec.name == "greet"))
                .cloned(),
            _ => None,
        })
        .await;

    let res = result(member.ok(run(ch, "greet", &[("name", "World")])).await);
    assert!(res.message_id.is_some() && !res.ephemeral);

    let msg = member
        .wait_for(|event| match event {
            Event::MessageCreate(create) => create
                .message
                .clone()
                .filter(|m| m.content == "Hello, World!"),
            _ => None,
        })
        .await;

    assert_eq!(msg.author_id, p.user_id);
    let interaction = msg.interaction.expect("message has interaction");
    assert_eq!(interaction.user_id, member_id);
    assert_eq!(interaction.command, "greet");
}

#[tokio::test]
async fn an_only_you_reply_reaches_only_the_session() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let code = invite(&mut owner, None).await;
    let (mut member, _member_ready) = join(&server, 2, &code).await;
    let ch = channel_id(&ready, "general");

    let _p = plugin(
        &server,
        &mut owner,
        &ready,
        "secret-agent",
        &["register_commands"],
        r#"commands.register({
            name = "secret",
            description = "Tells a secret",
        }, function(cmd)
            cmd:reply("psst this is a secret", { only_you = true })
            local ok, err = pcall(function() cmd:reply("again") end)
            log.info("second reply: " .. tostring(ok) .. " " .. tostring(err))
        end)"#,
    )
    .await;

    member
        .wait_for(|event| match event {
            Event::CommandsUpdate(update) => update
                .commands
                .as_ref()
                .filter(|c| c.commands.iter().any(|spec| spec.name == "secret"))
                .cloned(),
            _ => None,
        })
        .await;

    let res = result(member.ok(run(ch, "secret", &[])).await);
    assert!(res.message_id.is_none() && res.ephemeral);

    let ephemeral = member
        .wait_for(|event| match event {
            Event::EphemeralMessage(msg) => Some(msg.clone()),
            _ => None,
        })
        .await;
    assert_eq!(ephemeral.channel_id, ch);
    assert_eq!(ephemeral.nonce, "n1");
    assert_eq!(ephemeral.content, "psst this is a secret");

    wait_for_log(
        &mut owner,
        "secret-agent",
        "second reply: false INVALID_ARGUMENT: already answered",
    )
    .await;
}

#[tokio::test]
async fn a_plugin_that_does_not_answer_times_out_in_3_seconds() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let code = invite(&mut owner, None).await;
    let (mut member, _member_ready) = join(&server, 2, &code).await;
    let ch = channel_id(&ready, "general");

    let _p = plugin(
        &server,
        &mut owner,
        &ready,
        "sloth",
        &["register_commands"],
        r#"commands.register({
            name = "hang",
            description = "Never replies",
        }, function(cmd)
            -- do nothing
        end)"#,
    )
    .await;

    member
        .wait_for(|event| match event {
            Event::CommandsUpdate(update) => update
                .commands
                .as_ref()
                .filter(|c| c.commands.iter().any(|spec| spec.name == "hang"))
                .cloned(),
            _ => None,
        })
        .await;

    let started = Instant::now();
    let err = member.error(run(ch, "hang", &[])).await;
    assert_eq!(err.code, proto::ErrorCode::PluginUnavailable as i32);
    assert!(
        err.message.contains("didn't answer"),
        "expected didn't answer, got: {}",
        err.message
    );
    assert!(
        started.elapsed() >= Duration::from_secs(3),
        "should wait at least 3 seconds, elapsed: {:?}",
        started.elapsed()
    );
}

fn run_from(channel_id: i64, name: &str, source: &str) -> Kind {
    Kind::ExecuteCommand(proto::ExecuteCommand {
        source: source.to_owned(),
        channel_id,
        name: name.to_owned(),
        options: Vec::new(),
        nonce: "n1".to_owned(),
    })
}

/// A plugin with one command that answers `text` to whoever runs it.
async fn answering(
    server: &TestServer,
    owner: &mut TestClient,
    ready: &proto::Ready,
    id: &str,
    command: &str,
    text: &str,
) -> proto::Plugin {
    let source = format!(
        r#"commands.register({{ name = "{command}", description = "From {id}" }}, function(cmd)
            cmd:reply("{text}", {{ only_you = true }})
        end)
        log.info("registered")"#
    );
    let installed = plugin(server, owner, ready, id, &["register_commands"], &source).await;
    wait_for_log(owner, id, "registered").await;
    installed
}

async fn only_you_reply(client: &mut TestClient) -> String {
    client
        .wait_for(|event| match event {
            Event::EphemeralMessage(message) => Some(message.content.clone()),
            _ => None,
        })
        .await
}

fn turn_off(source: &str, name: &str) -> Kind {
    Kind::UpdateCommandSettings(proto::UpdateCommandSettings {
        source: source.to_owned(),
        name: name.to_owned(),
        enabled: Some(false),
        channel_ids: None,
        results_public: None,
    })
}

async fn command_page(owner: &mut TestClient) -> Vec<proto::CommandSpec> {
    match owner
        .ok(Kind::FetchCommandSettings(proto::FetchCommandSettings {}))
        .await
    {
        Response::Commands(list) => list.commands,
        other => panic!("not the commands: {other:?}"),
    }
}

fn enabled_in(page: &[proto::CommandSpec], feature: &str, name: &str) -> bool {
    page.iter()
        .find(|spec| spec.feature == feature && spec.name == name)
        .unwrap_or_else(|| panic!("{feature}/{name} isn't listed"))
        .enabled
}

#[tokio::test]
async fn two_plugins_may_use_one_name_and_each_runs_its_own() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");
    answering(&server, &mut owner, &ready, "dice", "roll", "from dice").await;
    answering(
        &server,
        &mut owner,
        &ready,
        "tickets",
        "roll",
        "from tickets",
    )
    .await;

    owner.ok(run_from(ch, "roll", "plugin:dice")).await;
    assert_eq!(only_you_reply(&mut owner).await, "from dice");

    owner.ok(run_from(ch, "roll", "plugin:tickets")).await;
    assert_eq!(only_you_reply(&mut owner).await, "from tickets");
}

#[tokio::test]
async fn a_plugin_may_use_a_builtin_name_and_no_source_runs_the_builtin() {
    let Room {
        server,
        mut owner,
        mut member,
        media_token,
        ..
    } = room().await;
    let mut ready_client = TestClient::connect(&server, crate::common::key(1)).await;
    let ready = ready_client.identify("Owner", None, None).await.unwrap();
    let ch = channel_id(&ready, "general");
    let source = r#"commands.register({ name = "skip", description = "A plugin's skip" }, function(cmd)
            cmd:reply("from the plugin", { only_you = true })
        end)
        log.info("registered")"#;
    let zip = package(&[
        ("manifest.lua", &manifest("skipper", &["register_commands"])),
        ("server/main.luau", source),
    ]);
    install(
        &server,
        &mut owner,
        &media_token,
        zip,
        &["register_commands"],
    )
    .await;
    wait_for_log(&mut owner, "skipper", "registered").await;

    member.ok(run_from(ch, "skip", "")).await;
    assert_eq!(
        only_you_reply(&mut member).await,
        "Join a voice channel to use music commands"
    );

    member.ok(run_from(ch, "skip", "plugin:skipper")).await;
    assert_eq!(only_you_reply(&mut member).await, "from the plugin");
}

#[tokio::test]
async fn no_source_runs_the_only_plugin_with_the_name() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");
    answering(&server, &mut owner, &ready, "dice", "roll", "from dice").await;

    owner.ok(run_from(ch, "roll", "")).await;
    assert_eq!(only_you_reply(&mut owner).await, "from dice");
}

#[tokio::test]
async fn no_source_with_two_plugins_asks_to_pick() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");
    answering(&server, &mut owner, &ready, "dice", "roll", "from dice").await;
    answering(
        &server,
        &mut owner,
        &ready,
        "tickets",
        "roll",
        "from tickets",
    )
    .await;

    let error = owner.error(run_from(ch, "roll", "")).await;
    assert_eq!(error.code, proto::ErrorCode::CommandUnknown as i32);
    assert_eq!(
        error.message,
        "Two commands are called /roll. Pick one from the list."
    );
}

#[tokio::test]
async fn an_unknown_source_is_command_unknown() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");
    answering(&server, &mut owner, &ready, "dice", "roll", "from dice").await;

    for source in ["plugin:nobody", "nonsense"] {
        let error = owner.error(run_from(ch, "roll", source)).await;
        assert_eq!(
            error.code,
            proto::ErrorCode::CommandUnknown as i32,
            "{source}"
        );
    }
}

#[tokio::test]
async fn no_source_prefers_a_plugin_when_the_builtin_feature_is_not_set_up() {
    let server = TestServer::start_with(|config| config.music.enabled = false).await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");
    answering(
        &server,
        &mut owner,
        &ready,
        "my-music",
        "play",
        "from the plugin",
    )
    .await;

    owner.ok(run_from(ch, "play", "")).await;
    assert_eq!(only_you_reply(&mut owner).await, "from the plugin");
}

#[tokio::test]
async fn settings_are_kept_per_source() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");
    answering(&server, &mut owner, &ready, "dice", "roll", "from dice").await;
    answering(
        &server,
        &mut owner,
        &ready,
        "tickets",
        "roll",
        "from tickets",
    )
    .await;

    owner.ok(turn_off("plugin:dice", "roll")).await;

    let error = owner.error(run_from(ch, "roll", "plugin:dice")).await;
    assert_eq!(error.code, proto::ErrorCode::CommandDisabled as i32);
    owner.ok(run_from(ch, "roll", "plugin:tickets")).await;
    assert_eq!(only_you_reply(&mut owner).await, "from tickets");

    let page = command_page(&mut owner).await;
    assert!(!enabled_in(&page, "plugin:dice", "roll"));
    assert!(enabled_in(&page, "plugin:tickets", "roll"));
}

#[tokio::test]
async fn uninstall_removes_only_that_plugins_settings() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    answering(&server, &mut owner, &ready, "dice", "roll", "from dice").await;
    answering(
        &server,
        &mut owner,
        &ready,
        "tickets",
        "roll",
        "from tickets",
    )
    .await;
    owner.ok(turn_off("plugin:dice", "roll")).await;
    owner.ok(turn_off("plugin:tickets", "roll")).await;

    owner
        .ok(Kind::UninstallPlugin(proto::UninstallPlugin {
            plugin_id: "dice".to_owned(),
            delete_data: true,
        }))
        .await;
    answering(&server, &mut owner, &ready, "dice", "roll", "from dice").await;

    let page = command_page(&mut owner).await;
    assert!(enabled_in(&page, "plugin:dice", "roll"));
    assert!(!enabled_in(&page, "plugin:tickets", "roll"));
}

#[tokio::test]
async fn stopping_keeps_them() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    answering(&server, &mut owner, &ready, "dice", "roll", "from dice").await;
    owner.ok(turn_off("plugin:dice", "roll")).await;

    for running in [false, true] {
        owner
            .ok(Kind::SetPluginRunning(proto::SetPluginRunning {
                plugin_id: "dice".to_owned(),
                running,
            }))
            .await;
    }
    wait_for_log(&mut owner, "dice", "registered").await;

    let page = command_page(&mut owner).await;
    assert!(!enabled_in(&page, "plugin:dice", "roll"));
}

#[tokio::test]
async fn plugins_are_listed_in_install_order() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    answering(&server, &mut owner, &ready, "zebra", "aaa", "z").await;
    answering(&server, &mut owner, &ready, "apple", "bbb", "a").await;

    let page = command_page(&mut owner).await;
    let sources: Vec<&str> = page
        .iter()
        .filter(|spec| spec.feature.starts_with("plugin:"))
        .map(|spec| spec.feature.as_str())
        .collect();
    assert_eq!(sources, ["plugin:zebra", "plugin:apple"]);
}

#[tokio::test]
async fn stopping_the_plugin_removes_its_commands() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let code = invite(&mut owner, None).await;
    let (mut member, _member_ready) = join(&server, 2, &code).await;
    let ch = channel_id(&ready, "general");

    let _p = plugin(
        &server,
        &mut owner,
        &ready,
        "temporary",
        &["register_commands"],
        r#"commands.register({
            name = "temp-cmd",
            description = "Will disappear",
        }, function() end)
        log.info("temp registered")"#,
    )
    .await;

    wait_for_log(&mut owner, "temporary", "temp registered").await;

    member
        .wait_for(|event| match event {
            Event::CommandsUpdate(update) => update
                .commands
                .as_ref()
                .filter(|c| c.commands.iter().any(|spec| spec.name == "temp-cmd"))
                .cloned(),
            _ => None,
        })
        .await;

    // Stop the plugin via SetPluginRunning
    owner
        .ok(Kind::SetPluginRunning(proto::SetPluginRunning {
            plugin_id: "temporary".to_owned(),
            running: false,
        }))
        .await;

    // Member gets CommandsUpdate without it
    member
        .wait_for(|event| match event {
            Event::CommandsUpdate(update) => update
                .commands
                .as_ref()
                .filter(|c| !c.commands.iter().any(|spec| spec.name == "temp-cmd"))
                .cloned(),
            _ => None,
        })
        .await;

    // Executing the command now fails with CommandUnknown
    let err = member.error(run(ch, "temp-cmd", &[])).await;
    assert_eq!(err.code, proto::ErrorCode::CommandUnknown as i32);
}

#[tokio::test]
async fn autocomplete_suggestions_arrive() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let code = invite(&mut owner, None).await;
    let (mut member, _member_ready) = join(&server, 2, &code).await;
    let ch = channel_id(&ready, "general");

    let _p = plugin(
        &server,
        &mut owner,
        &ready,
        "searcher",
        &["register_commands"],
        r#"commands.register({
            name = "tag",
            description = "Search tags",
            options = {
                { name = "name", description = "Tag name", kind = "text", autocomplete = function(ac)
                    ac:suggest({
                        { name = "Rust", value = "rust" },
                        { name = "Luau", value = "luau" },
                    })
                end }
            }
        }, function() end)"#,
    )
    .await;

    member
        .wait_for(|event| match event {
            Event::CommandsUpdate(update) => update
                .commands
                .as_ref()
                .filter(|c| c.commands.iter().any(|spec| spec.name == "tag"))
                .cloned(),
            _ => None,
        })
        .await;

    let res = member
        .ok(Kind::AutocompleteCommand(proto::AutocompleteCommand {
            source: String::new(),
            channel_id: ch,
            name: "tag".to_owned(),
            option: "name".to_owned(),
            value: "ru".to_owned(),
        }))
        .await;

    let choices = match res {
        Response::CommandChoices(choices) => choices.choices,
        other => panic!("expected CommandChoices, got {other:?}"),
    };
    assert_eq!(choices.len(), 2);
    assert_eq!(choices[0].name, "Rust");
    assert_eq!(choices[0].value, "rust");
    assert_eq!(choices[1].name, "Luau");
    assert_eq!(choices[1].value, "luau");
}

#[tokio::test]
async fn disabled_plugin_commands_are_refused() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let code = invite(&mut owner, None).await;
    let (mut member, _member_ready) = join(&server, 2, &code).await;
    let ch = channel_id(&ready, "general");

    let _p = plugin(
        &server,
        &mut owner,
        &ready,
        "custom-feature",
        &["register_commands"],
        r#"commands.register({
            name = "toggleable",
            description = "Can be turned off",
        }, function() end)
        log.info("toggleable registered")"#,
    )
    .await;

    wait_for_log(&mut owner, "custom-feature", "toggleable registered").await;

    // Disable the command via update command settings
    owner
        .ok(Kind::UpdateCommandSettings(proto::UpdateCommandSettings {
            source: "plugin:custom-feature".to_owned(),
            name: "toggleable".to_owned(),
            enabled: Some(false),
            channel_ids: None,
            results_public: None,
        }))
        .await;

    // Executing the disabled command fails with CommandDisabled
    let err = member.error(run(ch, "toggleable", &[])).await;
    assert_eq!(err.code, proto::ErrorCode::CommandDisabled as i32);
}

#[tokio::test]
async fn an_invalid_reply_raises_in_the_function() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let code = invite(&mut owner, None).await;
    let (mut member, _member_ready) = join(&server, 2, &code).await;
    let ch = channel_id(&ready, "general");

    let _p = plugin(
        &server,
        &mut owner,
        &ready,
        "oversized-bot",
        &["register_commands"],
        r#"commands.register({
            name = "huge",
            description = "answers huge reply",
        }, function(cmd)
            local ok, err = pcall(function() cmd:reply(string.rep("a", 4001)) end)
            log.info("big reply: " .. tostring(ok) .. " " .. tostring(err))
            cmd:reply("a smaller one")
        end)
        log.info("ready")"#,
    )
    .await;

    wait_for_log(&mut owner, "oversized-bot", "ready").await;

    let res = result(member.ok(run(ch, "huge", &[])).await);
    assert!(res.message_id.is_some() || res.ephemeral);
    wait_for_log(
        &mut owner,
        "oversized-bot",
        "big reply: false INVALID_ARGUMENT: the reply must be 1 to 4000 characters",
    )
    .await;
}

#[tokio::test]
async fn commands_cap_at_100() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;

    let _p = plugin(
        &server,
        &mut owner,
        &ready,
        "cmd-spammer",
        &["register_commands"],
        r#"
        local i = 1
        while i <= 100 do
            local ok, err = pcall(function()
                commands.register({ name = "cmd" .. i, description = "desc" }, function() end)
            end)
            if ok then
                i = i + 1
            else
                wait(0.2)
            end
        end
        local ok, err
        repeat
            ok, err = pcall(function()
                commands.register({ name = "cmd101", description = "desc" }, function() end)
            end)
            if not ok and tostring(err):find("RATE_LIMITED") then
                wait(0.2)
            end
        until ok or not tostring(err):find("RATE_LIMITED")
        log.info("101: " .. tostring(err))
        "#,
    )
    .await;

    wait_for_log(&mut owner, "cmd-spammer", "at most 100 commands").await;
}

#[tokio::test]
async fn revoking_register_commands_removes_them() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let code = invite(&mut owner, None).await;
    let (mut member, _member_ready) = join(&server, 2, &code).await;
    let ch = channel_id(&ready, "general");

    let p = plugin(
        &server,
        &mut owner,
        &ready,
        "revokable",
        &["register_commands"],
        r#"commands.register({
            name = "revokable",
            description = "will be revoked",
        }, function(cmd)
            cmd.reply("still here")
        end)
        log.info("ready")"#,
    )
    .await;

    wait_for_log(&mut owner, "revokable", "ready").await;

    // Command works initially
    let _res = result(owner.ok(run(ch, "revokable", &[])).await);

    // Revoke register_commands permission
    owner
        .ok(Kind::SetPluginPermissions(proto::SetPluginPermissions {
            plugin_id: p.id.clone(),
            granted: vec![],
        }))
        .await;

    // Command is now unknown
    let err = member.error(run(ch, "revokable", &[])).await;
    assert_eq!(err.code, proto::ErrorCode::CommandUnknown as i32);
}

/// Runs `lua` as a plugin allowed to register commands, and waits for it to
/// log `done`.
async fn scripted(
    server: &TestServer,
    owner: &mut TestClient,
    ready: &proto::Ready,
    id: &str,
    lua: &str,
) {
    let source = format!("{lua}\nlog.info(\"done\")");
    plugin(server, owner, ready, id, &["register_commands"], &source).await;
    wait_for_log(owner, id, "done").await;
}

#[tokio::test]
async fn a_command_runs_its_own_function() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");
    scripted(
        &server,
        &mut owner,
        &ready,
        "two-fns",
        r#"commands.register({ name = "one", description = "d" }, function(cmd)
            cmd:reply("ran one", { only_you = true })
        end)
        commands.register({ name = "two", description = "d" }, function(cmd)
            cmd:reply("ran two: " .. cmd.name, { only_you = true })
        end)"#,
    )
    .await;

    owner.ok(run(ch, "two", &[])).await;
    assert_eq!(only_you_reply(&mut owner).await, "ran two: two");
    owner.ok(run(ch, "one", &[])).await;
    assert_eq!(only_you_reply(&mut owner).await, "ran one");
}

#[tokio::test]
async fn register_without_a_function_raises() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    scripted(
        &server,
        &mut owner,
        &ready,
        "no-fn",
        r#"local ok, err = pcall(function()
            commands.register({ name = "x", description = "d" })
        end)
        log.info("no function: " .. tostring(err))"#,
    )
    .await;
    wait_for_log(
        &mut owner,
        "no-fn",
        "no function: INVALID_ARGUMENT: a command needs a function",
    )
    .await;
}

#[tokio::test]
async fn option_autocomplete_true_raises() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    scripted(
        &server,
        &mut owner,
        &ready,
        "old-form",
        r#"local ok, err = pcall(function()
            commands.register({ name = "x", description = "d", options = {
                { name = "a", description = "d", kind = "text", autocomplete = true },
            } }, function() end)
        end)
        log.info("old form: " .. tostring(err))"#,
    )
    .await;
    wait_for_log(
        &mut owner,
        "old-form",
        "old form: INVALID_ARGUMENT: option autocomplete must be a function",
    )
    .await;
}

#[tokio::test]
async fn registering_a_name_again_replaces_spec_and_function() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");
    scripted(
        &server,
        &mut owner,
        &ready,
        "again",
        r#"commands.register({ name = "x", description = "first" }, function(cmd)
            cmd:reply("first", { only_you = true })
        end)
        commands.register({ name = "x", description = "second" }, function(cmd)
            cmd:reply("second", { only_you = true })
        end)"#,
    )
    .await;

    owner.ok(run(ch, "x", &[])).await;
    assert_eq!(only_you_reply(&mut owner).await, "second");
    let page = command_page(&mut owner).await;
    let spec = page.iter().find(|spec| spec.name == "x").unwrap();
    assert_eq!(spec.description, "second");
}

#[tokio::test]
async fn an_option_autocomplete_function_gets_its_own_calls() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");
    scripted(
        &server,
        &mut owner,
        &ready,
        "ac-fns",
        r#"commands.register({ name = "x", description = "d", options = {
            { name = "a", description = "d", kind = "text", autocomplete = function(ac)
                ac:suggest({ { name = "from a: " .. ac.value, value = "1" } })
            end },
            { name = "b", description = "d", kind = "text", autocomplete = function(ac)
                ac:suggest({ { name = "from b: " .. ac.value, value = "2" } })
            end },
        } }, function() end)"#,
    )
    .await;

    let answer = owner
        .ok(Kind::AutocompleteCommand(proto::AutocompleteCommand {
            source: "plugin:ac-fns".to_owned(),
            channel_id: ch,
            name: "x".to_owned(),
            option: "b".to_owned(),
            value: "hi".to_owned(),
        }))
        .await;
    let Response::CommandChoices(choices) = answer else {
        panic!("expected choices, got {answer:?}");
    };
    assert_eq!(choices.choices[0].name, "from b: hi");
}

#[tokio::test]
async fn on_command_raises_no_event_called() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    scripted(
        &server,
        &mut owner,
        &ready,
        "old-event",
        r#"local ok, err = pcall(function() opencord.on("command", function() end) end)
        log.info("listen: " .. tostring(err))"#,
    )
    .await;
    wait_for_log(
        &mut owner,
        "old-event",
        "listen: INVALID_ARGUMENT: no event called command",
    )
    .await;
}

#[tokio::test]
async fn on_a_typo_raises_no_event_called() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    scripted(
        &server,
        &mut owner,
        &ready,
        "typo",
        r#"local ok, err = pcall(function() opencord.on("message_creat", function() end) end)
        log.info("listen: " .. tostring(err))"#,
    )
    .await;
    wait_for_log(
        &mut owner,
        "typo",
        "listen: INVALID_ARGUMENT: no event called message_creat",
    )
    .await;
}

#[tokio::test]
async fn every_event_in_the_design_table_is_accepted() {
    const EVENTS: &[&str] = &[
        "start",
        "stop",
        "update",
        "settings_change",
        "permission_change",
        "message_create",
        "message_update",
        "message_delete",
        "typing_start",
        "channel_create",
        "channel_update",
        "channel_delete",
        "plugin_channel_create",
        "plugin_channel_delete",
        "role_create",
        "role_update",
        "role_delete",
        "member_join",
        "member_leave",
        "member_update",
        "member_unban",
        "presence_update",
        "invite_create",
        "invite_revoke",
        "server_update",
        "voice_settings_update",
        "upload_settings_update",
        "voice_state_update",
        "stream_create",
        "stream_update",
        "stream_delete",
        "stream_viewers_update",
        "soundboard_play",
        "soundboard_sound_create",
        "soundboard_sound_update",
        "soundboard_sound_delete",
        "music_now_playing",
        "music_queue_update",
        "music_notice",
        "view_open",
        "view_close",
        "activity_start",
        "activity_end",
        "file_uploaded",
    ];
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let names: Vec<String> = EVENTS.iter().map(|name| format!("{name:?}")).collect();
    let lua = format!(
        r#"for _, name in {{ {} }} do
            local ok, err = pcall(function() opencord.on(name, function() end) end)
            if not ok then log.warn("refused " .. name .. ": " .. tostring(err)) end
        end
        log.info("listened")"#,
        names.join(", ")
    );
    scripted(&server, &mut owner, &ready, "all-events", &lua).await;
    wait_for_log(&mut owner, "all-events", "listened").await;
    let lines = crate::support::log_lines(&mut owner, "all-events").await;
    assert!(
        !lines.iter().any(|line| line.contains("refused")),
        "{lines:#?}"
    );
}

#[tokio::test]
async fn a_function_that_errors_answers_at_once() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");
    scripted(
        &server,
        &mut owner,
        &ready,
        "oops",
        r#"commands.register({ name = "boom", description = "d" }, function()
            error("the dice fell off the table")
        end)"#,
    )
    .await;

    let started = Instant::now();
    let error = owner.error(run(ch, "boom", &[])).await;

    assert!(
        started.elapsed() < Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(error.code, proto::ErrorCode::PluginUnavailable as i32);
    assert_eq!(error.message, "Test plugin ran into an error");
    wait_for_log(&mut owner, "oops", "the dice fell off the table").await;
}

/// A plugin that defers `/slow` the way `defer` says, then runs `after`.
async fn thinking_plugin(
    server: &TestServer,
    owner: &mut TestClient,
    ready: &proto::Ready,
    permissions: &[&str],
    defer: &str,
    after: &str,
) {
    let source = format!(
        r#"commands.register({{ name = "slow", description = "d" }}, function(cmd)
            {defer}
            {after}
        end)
        log.info("done")"#
    );
    plugin(server, owner, ready, "slowpoke", permissions, &source).await;
    wait_for_log(owner, "slowpoke", "done").await;
}

const CAN_POST: &[&str] = &["register_commands", "send_messages"];
const THINKING: i32 = proto::CommandState::Thinking as i32;
const ANSWERED: i32 = proto::CommandState::Answered as i32;
const NO_ANSWER: i32 = proto::CommandState::NoAnswer as i32;

fn state_of(message: &proto::Message) -> i32 {
    message
        .interaction
        .as_ref()
        .expect("a command's message")
        .state
}

async fn created_in_state(client: &mut TestClient, state: i32) -> proto::Message {
    client
        .wait_for(|event| match event {
            Event::MessageCreate(create) => create.message.clone().filter(|message| {
                message
                    .interaction
                    .as_ref()
                    .is_some_and(|i| i.state == state)
            }),
            _ => None,
        })
        .await
}

async fn updated_in_state(client: &mut TestClient, state: i32) -> proto::Message {
    client
        .wait_for(|event| match event {
            Event::MessageUpdate(update) => update.message.clone().filter(|message| {
                message
                    .interaction
                    .as_ref()
                    .is_some_and(|i| i.state == state)
            }),
            _ => None,
        })
        .await
}

async fn ephemeral_in_state(client: &mut TestClient, state: i32) -> proto::EphemeralMessage {
    client
        .wait_for(|event| match event {
            Event::EphemeralMessage(message) if message.state == state => Some(message.clone()),
            _ => None,
        })
        .await
}

#[tokio::test]
async fn defer_posts_a_thinking_message_and_the_reply_edits_it() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");
    thinking_plugin(
        &server,
        &mut owner,
        &ready,
        CAN_POST,
        "cmd:defer()",
        r#"timer.after(0.5, function() cmd:reply("done") end)"#,
    )
    .await;

    let started = Instant::now();
    let res = result(owner.ok(run(ch, "slow", &[])).await);
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "returns at once"
    );
    assert!(res.message_id.is_some() && !res.ephemeral);

    let thinking = created_in_state(&mut owner, THINKING).await;
    assert_eq!(thinking.content, "Test plugin is thinking…");
    assert_eq!(Some(thinking.id), res.message_id);

    let done = updated_in_state(&mut owner, ANSWERED).await;
    assert_eq!(done.id, thinking.id);
    assert_eq!(done.content, "done");
    assert_eq!(done.edited_at_ms, None);
}

#[tokio::test]
async fn defer_only_you_sends_thinking_then_replaces_it_by_nonce() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");
    thinking_plugin(
        &server,
        &mut owner,
        &ready,
        CAN_POST,
        "cmd:defer({ only_you = true })",
        r#"timer.after(0.2, function() cmd:reply("private") end)"#,
    )
    .await;

    let res = result(owner.ok(run(ch, "slow", &[])).await);
    assert!(res.ephemeral && res.message_id.is_none());

    let thinking = ephemeral_in_state(&mut owner, THINKING).await;
    assert_eq!(thinking.nonce, "n1");
    assert_eq!(thinking.content, "Test plugin is thinking…");

    let done = ephemeral_in_state(&mut owner, ANSWERED).await;
    assert_eq!(done.nonce, "n1");
    assert_eq!(done.content, "private");
}

#[tokio::test]
async fn defer_decides_where_the_reply_goes() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");
    thinking_plugin(
        &server,
        &mut owner,
        &ready,
        CAN_POST,
        "cmd:defer()",
        r#"cmd:reply("still public", { only_you = true })"#,
    )
    .await;

    owner.ok(run(ch, "slow", &[])).await;

    let done = updated_in_state(&mut owner, ANSWERED).await;
    assert_eq!(done.content, "still public");
}

#[tokio::test]
async fn defer_goes_to_the_member_when_the_plugin_cannot_post() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");
    thinking_plugin(
        &server,
        &mut owner,
        &ready,
        &["register_commands"],
        "cmd:defer()",
        "",
    )
    .await;

    let res = result(owner.ok(run(ch, "slow", &[])).await);

    assert!(res.ephemeral);
    ephemeral_in_state(&mut owner, THINKING).await;
}

#[tokio::test]
async fn defer_follows_results_private() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");
    thinking_plugin(&server, &mut owner, &ready, CAN_POST, "cmd:defer()", "").await;
    owner
        .ok(Kind::UpdateCommandSettings(proto::UpdateCommandSettings {
            source: String::new(),
            name: String::new(),
            enabled: None,
            channel_ids: None,
            results_public: Some(false),
        }))
        .await;

    let res = result(owner.ok(run(ch, "slow", &[])).await);

    assert!(res.ephemeral);
    ephemeral_in_state(&mut owner, THINKING).await;
}

#[tokio::test]
async fn a_second_defer_or_a_defer_after_reply_raises() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");
    scripted(
        &server,
        &mut owner,
        &ready,
        "twice",
        r#"commands.register({ name = "dd", description = "d" }, function(cmd)
            cmd:defer({ only_you = true })
            local ok, err = pcall(function() cmd:defer() end)
            log.info("second defer: " .. tostring(err))
            cmd:reply("x")
        end)
        commands.register({ name = "rd", description = "d" }, function(cmd)
            cmd:reply("x", { only_you = true })
            local ok, err = pcall(function() cmd:defer() end)
            log.info("defer after reply: " .. tostring(err))
        end)"#,
    )
    .await;

    owner.ok(run(ch, "dd", &[])).await;
    wait_for_log(
        &mut owner,
        "twice",
        "second defer: INVALID_ARGUMENT: already answered",
    )
    .await;
    owner.ok(run(ch, "rd", &[])).await;
    wait_for_log(
        &mut owner,
        "twice",
        "defer after reply: INVALID_ARGUMENT: already answered",
    )
    .await;
}

#[tokio::test]
async fn no_reply_in_time_becomes_no_answer() {
    let server = TestServer::start_with(|config| config.plugins.thinking_secs = 1).await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");
    thinking_plugin(&server, &mut owner, &ready, CAN_POST, "cmd:defer()", "").await;

    owner.ok(run(ch, "slow", &[])).await;

    let message = updated_in_state(&mut owner, NO_ANSWER).await;
    assert_eq!(message.content, "Test plugin didn't answer");
}

#[tokio::test]
async fn the_plugin_stopping_while_thinking_becomes_no_answer() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");
    thinking_plugin(&server, &mut owner, &ready, CAN_POST, "cmd:defer()", "").await;
    owner.ok(run(ch, "slow", &[])).await;
    created_in_state(&mut owner, THINKING).await;

    owner
        .ok(Kind::SetPluginRunning(proto::SetPluginRunning {
            plugin_id: "slowpoke".to_owned(),
            running: false,
        }))
        .await;

    let message = updated_in_state(&mut owner, NO_ANSWER).await;
    assert_eq!(message.content, "Test plugin didn't answer");
}

#[tokio::test]
async fn an_error_after_defer_says_ran_into_an_error() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");
    thinking_plugin(
        &server,
        &mut owner,
        &ready,
        CAN_POST,
        "cmd:defer()",
        r#"error("oops")"#,
    )
    .await;

    owner.ok(run(ch, "slow", &[])).await;

    let message = updated_in_state(&mut owner, NO_ANSWER).await;
    assert_eq!(message.content, "Test plugin ran into an error");
    wait_for_log(&mut owner, "slowpoke", "oops").await;
}

#[tokio::test]
async fn the_101st_thinking_command_raises_plugin_limit() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");
    thinking_plugin(
        &server,
        &mut owner,
        &ready,
        CAN_POST,
        r#"local ok, err = pcall(function() cmd:defer({ only_you = true }) end)
        if not ok then log.info("refused: " .. tostring(err)) error(err) end"#,
        "",
    )
    .await;

    for _ in 0..100 {
        owner.ok_paced(run(ch, "slow", &[])).await;
    }
    let refused = loop {
        match owner.request(run(ch, "slow", &[])).await {
            Response::Error(error) if error.code == proto::ErrorCode::RateLimited as i32 => {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            other => break other,
        }
    };

    assert!(matches!(refused, Response::Error(_)), "{refused:?}");
    wait_for_log(
        &mut owner,
        "slowpoke",
        "refused: PLUGIN_LIMIT: too many commands waiting",
    )
    .await;
}

#[tokio::test]
async fn a_reply_after_the_message_was_deleted_logs_and_does_not_raise() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");
    thinking_plugin(
        &server,
        &mut owner,
        &ready,
        CAN_POST,
        "cmd:defer()",
        r#"timer.after(0.5, function()
            local ok, err = pcall(function() cmd:reply("too late") end)
            log.info("the reply: " .. tostring(ok))
        end)"#,
    )
    .await;

    owner.ok(run(ch, "slow", &[])).await;
    let thinking = created_in_state(&mut owner, THINKING).await;
    owner
        .ok(Kind::DeleteMessage(proto::DeleteMessage {
            message_id: thinking.id,
        }))
        .await;

    wait_for_log(&mut owner, "slowpoke", "the reply: true").await;
    wait_for_log(
        &mut owner,
        "slowpoke",
        "/slow's message was deleted before the reply",
    )
    .await;
}

async fn history(client: &mut TestClient, channel_id: i64) -> Vec<proto::Message> {
    match client
        .ok(Kind::FetchMessages(proto::FetchMessages {
            channel_id,
            before: None,
            limit: 10,
        }))
        .await
    {
        Response::Messages(list) => list.messages,
        other => panic!("not messages: {other:?}"),
    }
}

#[tokio::test]
async fn history_returns_a_thinking_message_with_its_state() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let code = invite(&mut owner, None).await;
    let (mut member, member_ready) = join(&server, 2, &code).await;
    let ch = channel_id(&ready, "general");
    thinking_plugin(&server, &mut owner, &ready, CAN_POST, "cmd:defer()", "").await;

    member.ok(run(ch, "slow", &[])).await;
    created_in_state(&mut member, THINKING).await;

    let messages = history(&mut owner, ch).await;
    let thinking = messages
        .iter()
        .find(|message| message.interaction.is_some())
        .expect("the thinking message");
    assert_eq!(state_of(thinking), THINKING);
    assert_eq!(
        thinking.interaction.as_ref().unwrap().user_id,
        self_id(&member_ready)
    );
}

#[tokio::test]
async fn a_restart_turns_thinking_into_no_answer() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");
    thinking_plugin(&server, &mut owner, &ready, CAN_POST, "cmd:defer()", "").await;
    owner.ok(run(ch, "slow", &[])).await;
    created_in_state(&mut owner, THINKING).await;
    owner.close().await;

    let server = server.restart(|_| {}).await;
    let mut owner = TestClient::connect(&server, crate::common::key(1)).await;
    owner.identify("Owner", None, None).await.unwrap();

    let messages = history(&mut owner, ch).await;
    let swept = messages
        .iter()
        .find(|message| message.interaction.is_some())
        .expect("the message");
    assert_eq!(state_of(swept), NO_ANSWER);
    assert_eq!(swept.content, "Test plugin didn't answer");
}

#[tokio::test]
async fn a_reply_after_3_seconds_raises_and_logs() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");
    thinking_plugin(
        &server,
        &mut owner,
        &ready,
        CAN_POST,
        "",
        r#"timer.after(3.1, function()
            local ok, err = pcall(function() cmd:reply("late") end)
            log.info("late reply: " .. tostring(err))
        end)"#,
    )
    .await;

    let error = owner.error(run(ch, "slow", &[])).await;
    assert!(error.message.contains("didn't answer"), "{}", error.message);

    wait_for_log(
        &mut owner,
        "slowpoke",
        "late reply: TIMEOUT: too late to reply",
    )
    .await;
    let lines = crate::support::log_lines(&mut owner, "slowpoke").await;
    let warning = lines
        .iter()
        .find(|line| line.starts_with("/slow answered after"))
        .unwrap_or_else(|| panic!("no warning in {lines:#?}"));
    assert!(warning.ends_with(" s, too late"), "{warning}");
}

#[tokio::test]
async fn a_defer_after_3_seconds_raises_too() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");
    thinking_plugin(
        &server,
        &mut owner,
        &ready,
        CAN_POST,
        "",
        r#"timer.after(3.1, function()
            local ok, err = pcall(function() cmd:defer() end)
            log.info("late defer: " .. tostring(err))
        end)"#,
    )
    .await;

    owner.error(run(ch, "slow", &[])).await;

    wait_for_log(
        &mut owner,
        "slowpoke",
        "late defer: TIMEOUT: too late to reply",
    )
    .await;
}

#[tokio::test]
async fn a_reply_after_15_minutes_raises_and_logs() {
    let server = TestServer::start_with(|config| config.plugins.thinking_secs = 1).await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");
    thinking_plugin(
        &server,
        &mut owner,
        &ready,
        CAN_POST,
        "cmd:defer()",
        r#"timer.after(1.3, function()
            local ok, err = pcall(function() cmd:reply("late") end)
            log.info("late reply: " .. tostring(err))
        end)"#,
    )
    .await;

    owner.ok(run(ch, "slow", &[])).await;
    updated_in_state(&mut owner, NO_ANSWER).await;

    wait_for_log(
        &mut owner,
        "slowpoke",
        "late reply: TIMEOUT: too late to reply",
    )
    .await;
    wait_for_log(&mut owner, "slowpoke", "/slow answered after 1.").await;
}

#[tokio::test]
async fn a_late_suggest_returns_false_and_logs_nothing() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");
    scripted(
        &server,
        &mut owner,
        &ready,
        "late-ac",
        r#"commands.register({ name = "x", description = "d", options = {
            { name = "a", description = "d", kind = "text", autocomplete = function(ac)
                timer.after(1.2, function()
                    log.info("suggest said " .. tostring(ac:suggest({ { name = "n", value = "v" } })))
                end)
            end },
        } }, function() end)"#,
    )
    .await;

    let answer = owner
        .ok(Kind::AutocompleteCommand(proto::AutocompleteCommand {
            source: String::new(),
            channel_id: ch,
            name: "x".to_owned(),
            option: "a".to_owned(),
            value: "h".to_owned(),
        }))
        .await;
    let Response::CommandChoices(choices) = answer else {
        panic!("expected choices, got {answer:?}");
    };
    assert!(choices.choices.is_empty());

    wait_for_log(&mut owner, "late-ac", "suggest said false").await;
    let lines = crate::support::log_lines(&mut owner, "late-ac").await;
    assert!(
        !lines.iter().any(|line| line.contains("too late")),
        "{lines:#?}"
    );
}
