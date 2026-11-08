// SPDX-License-Identifier: MPL-2.0

//! Plugin commands API (plan Task 14, design 2026-11-07 §10.3, §10.4, §17).

use std::time::{Duration, Instant};

use opencord_proto::v1 as proto;
use proto::event::Kind as Event;
use proto::request::Kind;
use proto::response::Result as Response;

use crate::common::{
    TestClient, TestServer, channel_id, invite, join, media_token, owner, self_id,
};
use crate::support::{install, manifest, package, wait_for_log};

fn run(channel_id: i64, name: &str, options: &[(&str, &str)]) -> Kind {
    Kind::ExecuteCommand(proto::ExecuteCommand {
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

#[tokio::test]
async fn a_taken_name_is_refused() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;

    // 1. Collides with built-in command
    let _p1 = plugin(
        &server,
        &mut owner,
        &ready,
        "collider",
        &["register_commands"],
        r#"local ok, err = pcall(function()
            commands.register({ name = "play", description = "collides with play" })
        end)
        log.info("builtin collision: " .. tostring(ok) .. " " .. tostring(err))"#,
    )
    .await;

    wait_for_log(
        &mut owner,
        "collider",
        "builtin collision: false INVALID_ARGUMENT: /play is already taken",
    )
    .await;

    // 2. Collides with another plugin
    let _p2 = plugin(
        &server,
        &mut owner,
        &ready,
        "first-plugin",
        &["register_commands"],
        r#"commands.register({ name = "unique-cmd", description = "first" })
        log.info("first registered")"#,
    )
    .await;

    wait_for_log(&mut owner, "first-plugin", "first registered").await;

    let _p3 = plugin(
        &server,
        &mut owner,
        &ready,
        "second-plugin",
        &["register_commands"],
        r#"local ok, err = pcall(function()
            commands.register({ name = "unique-cmd", description = "second" })
        end)
        log.info("plugin collision: " .. tostring(ok) .. " " .. tostring(err))"#,
    )
    .await;

    wait_for_log(
        &mut owner,
        "second-plugin",
        "plugin collision: false INVALID_ARGUMENT: /unique-cmd is already taken",
    )
    .await;
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
