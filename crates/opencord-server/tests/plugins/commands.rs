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
        })
        opencord.on("command", function(cmd)
            if cmd.name == "greet" then
                cmd:reply("Hello, " .. cmd.options.name .. "!")
            end
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
        })
        opencord.on("command", function(cmd)
            if cmd.name == "secret" then
                cmd:reply("psst this is a secret", { only_you = true })
                local ok, err = pcall(function() cmd:reply("again") end)
                log.info("second reply: " .. tostring(ok) .. " " .. tostring(err))
            end
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
        "second reply: false INVALID_ARGUMENT: already replied",
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
        })
        opencord.on("command", function(cmd)
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
        r#"commands.register({{ name = "{command}", description = "From {id}" }})
        opencord.on("command", function(cmd)
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
    let source = r#"commands.register({ name = "skip", description = "A plugin's skip" })
        opencord.on("command", function(cmd) cmd:reply("from the plugin", { only_you = true }) end)
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
        })
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
                { name = "name", description = "Tag name", kind = "text", autocomplete = true }
            }
        })
        opencord.on("command_autocomplete", function(ac)
            if ac.name == "tag" and ac.option == "name" then
                ac:suggest({
                    { name = "Rust", value = "rust" },
                    { name = "Luau", value = "luau" },
                })
            end
        end)"#,
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
        })
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
async fn an_oversized_reply_is_refused() {
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
        })
        opencord.on("command", function(cmd)
            local big = string.rep("a", 4001)
            cmd.reply(big)
        end)
        log.info("ready")"#,
    )
    .await;

    wait_for_log(&mut owner, "oversized-bot", "ready").await;

    let err = member.error(run(ch, "huge", &[])).await;
    assert_eq!(err.code, proto::ErrorCode::PluginUnavailable as i32);
    assert!(err.message.contains("sent an invalid reply"));
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
                commands.register({ name = "cmd" .. i, description = "desc" })
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
                commands.register({ name = "cmd101", description = "desc" })
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
        })
        opencord.on("command", function(cmd)
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
