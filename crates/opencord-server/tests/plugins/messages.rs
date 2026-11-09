// SPDX-License-Identifier: MPL-2.0

//! Plugin messages API (plan Task 13, design 2026-11-07 §10.3).

use opencord_common::permissions::Permissions;
use opencord_proto::v1 as proto;
use proto::request::Kind;

use crate::common::{
    TestClient, TestServer, channel_id, invite, join, media_token, owner, self_id,
};
use crate::support::{install, manifest, package, wait_for_log};

async fn send(client: &mut TestClient, channel_id: i64, content: &str) {
    client
        .ok(Kind::SendMessage(proto::SendMessage {
            channel_id,
            content: content.to_owned(),
            ..Default::default()
        }))
        .await;
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
async fn a_plugin_sends_and_members_see_it_from_the_plugin_user() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let code = invite(&mut owner, None).await;
    let (mut member, _member_ready) = join(&server, 2, &code).await;
    let ch = channel_id(&ready, "general");

    let p = plugin(
        &server,
        &mut owner,
        &ready,
        "sender",
        &["send_messages"],
        &format!(
            r#"local msg = messages.send("{ch}", "hello from plugin")
            log.info("sent " .. msg.id .. " by " .. msg.author.name .. " in " .. msg.channel_id)
            messages.send("{ch}", {{ content = "table send" }})
            log.info("sent table successfully")
            typing.start("{ch}")
            log.info("typing started")"#
        ),
    )
    .await;

    wait_for_log(&mut owner, "sender", "typing started").await;

    // Member sees message from plugin user
    let mut saw_plugin_msg = false;
    for _ in 0..5 {
        let event = member.next_event().await;
        if let proto::event::Kind::MessageCreate(proto::MessageCreate { message: Some(msg) }) =
            event
            && msg.content == "hello from plugin"
        {
            assert_eq!(msg.author_id, p.user_id);
            assert_eq!(msg.channel_id, ch);
            saw_plugin_msg = true;
            break;
        }
    }
    assert!(saw_plugin_msg, "member never received message from plugin");
}

#[tokio::test]
async fn sending_needs_the_permission() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");

    plugin(
        &server,
        &mut owner,
        &ready,
        "noperm",
        &["read_messages"],
        &format!(
            r#"local ok, err = pcall(function()
                messages.send("{ch}", "not allowed")
            end)
            if not ok then
                log.info("send failed: " .. tostring(err))
            end
            local tok, terr = pcall(function()
                typing.start("{ch}")
            end)
            if not tok then
                log.info("typing failed: " .. tostring(terr))
            end"#
        ),
    )
    .await;

    wait_for_log(
        &mut owner,
        "noperm",
        "send failed: PERMISSION_DENIED: the plugin doesn't have send_messages",
    )
    .await;
    wait_for_log(
        &mut owner,
        "noperm",
        "typing failed: PERMISSION_DENIED: the plugin doesn't have send_messages",
    )
    .await;
}

#[tokio::test]
async fn a_plugin_cannot_send_where_its_role_cannot() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");
    let code = invite(&mut owner, None).await;

    let p = plugin(
        &server,
        &mut owner,
        &ready,
        "cantsend",
        &["send_messages", "read_members"],
        &format!(
            r#"opencord.on("member_join", function()
                local ok, err = pcall(function()
                    messages.send("{ch}", "should fail")
                end)
                log.info("send result: " .. tostring(ok) .. " " .. tostring(err))
            end)"#
        ),
    )
    .await;

    // Deny SEND_MESSAGES for plugin role
    owner
        .ok(Kind::SetChannelOverwrite(proto::SetChannelOverwrite {
            channel_id: ch,
            overwrite: Some(proto::PermissionOverwrite {
                target_kind: proto::OverwriteTarget::Role as i32,
                target_id: p.role_id,
                allow: 0,
                deny: Permissions::SEND_MESSAGES.bits(),
            }),
        }))
        .await;

    let _member = join(&server, 2, &code).await;
    wait_for_log(&mut owner, "cantsend", "send result: false FORBIDDEN").await;
}

#[tokio::test]
async fn a_plugin_edits_only_its_own_messages() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");

    plugin(
        &server,
        &mut owner,
        &ready,
        "editor",
        &["send_messages", "read_messages"],
        &format!(
            r#"local my_msg = messages.send("{ch}", "original text")
            local edited = messages.edit(my_msg.id, "edited text")
            log.info("self edit: " .. edited.content .. " edited_at: " .. tostring(type(edited.edited_at_ms)))

            opencord.on("message_create", function(event)
                if event.message.author.id ~= "{owner_id}" then
                    return
                end
                local ok, err = pcall(function()
                    messages.edit(event.message.id, "hacked")
                end)
                log.info("other edit: " .. tostring(ok) .. " " .. tostring(err))
            end)"#,
            owner_id = self_id(&ready)
        ),
    )
    .await;

    wait_for_log(
        &mut owner,
        "editor",
        "self edit: edited text edited_at: number",
    )
    .await;

    send(&mut owner, ch, "owner message").await;
    wait_for_log(
        &mut owner,
        "editor",
        "other edit: false FORBIDDEN: you can only edit your own messages",
    )
    .await;
}

#[tokio::test]
async fn deleting_others_needs_manage_messages() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");

    // Plugin 1 without manage_messages
    plugin(
        &server,
        &mut owner,
        &ready,
        "deleter1",
        &["send_messages", "read_messages"],
        &format!(
            r#"local my_msg = messages.send("{ch}", "self message")
            local ok, err = pcall(function()
                messages.delete(my_msg.id)
            end)
            log.info("self delete: " .. tostring(ok) .. " " .. tostring(err))

            opencord.on("message_create", function(event)
                if event.message.author.id ~= "{owner_id}" then
                    return
                end
                local dok, derr = pcall(function()
                    messages.delete(event.message.id)
                end)
                log.info("other delete without perm: " .. tostring(dok) .. " " .. tostring(derr))
            end)"#,
            owner_id = self_id(&ready)
        ),
    )
    .await;

    wait_for_log(&mut owner, "deleter1", "self delete: true nil").await;

    send(&mut owner, ch, "owner message to delete").await;
    wait_for_log(
        &mut owner,
        "deleter1",
        "other delete without perm: false PERMISSION_DENIED: the plugin doesn't have manage_messages",
    )
    .await;

    // Plugin 2 with manage_messages can delete other's message
    plugin(
        &server,
        &mut owner,
        &ready,
        "deleter2",
        &["send_messages", "read_messages", "manage_messages"],
        &format!(
            r#"opencord.on("message_create", function(event)
                if event.message.author.id ~= "{owner_id}" then
                    return
                end
                local dok, derr = pcall(function()
                    messages.delete(event.message.id)
                end)
                log.info("other delete with perm: " .. tostring(dok) .. " " .. tostring(derr))
            end)"#,
            owner_id = self_id(&ready)
        ),
    )
    .await;

    send(&mut owner, ch, "owner message 2").await;
    wait_for_log(&mut owner, "deleter2", "other delete with perm: true nil").await;
}

#[tokio::test]
async fn fetch_returns_history() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");

    send(&mut owner, ch, "first message").await;
    send(&mut owner, ch, "second message").await;
    send(&mut owner, ch, "third message").await;

    plugin(
        &server,
        &mut owner,
        &ready,
        "fetcher",
        &["read_messages"],
        &format!(
            r#"local history = messages.fetch("{ch}", {{ limit = 2 }})
            log.info("count: " .. #history .. " newest: " .. history[1].content .. " older: " .. history[2].content)
            local older = messages.fetch("{ch}", {{ before = history[1].id, limit = 10 }})
            log.info("older count: " .. #older .. " first: " .. older[1].content .. " second: " .. older[2].content)"#
        ),
    )
    .await;

    wait_for_log(
        &mut owner,
        "fetcher",
        "count: 2 newest: third message older: second message",
    )
    .await;
    wait_for_log(
        &mut owner,
        "fetcher",
        "older count: 2 first: second message second: first message",
    )
    .await;
}

#[tokio::test]
async fn ids_must_be_strings() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");

    plugin(
        &server,
        &mut owner,
        &ready,
        "idchecker",
        &["send_messages", "read_messages"],
        &format!(
            r#"local ok1, err1 = pcall(function() messages.send(12345, "hello") end)
            log.info("num channel: " .. tostring(err1))

            local ok2, err2 = pcall(function() messages.send("{ch}", {{ content = "x", files = {{ 12345 }} }}) end)
            log.info("files err: " .. tostring(err2))

            local ok3, err3 = pcall(function() messages.edit(12345, "hello") end)
            log.info("num edit: " .. tostring(err3))

            local ok4, err4 = pcall(function() messages.delete(12345) end)
            log.info("num delete: " .. tostring(err4))

            local ok5, err5 = pcall(function() messages.fetch(12345) end)
            log.info("num fetch: " .. tostring(err5))

            local ok6, err6 = pcall(function() typing.start(12345) end)
            log.info("num typing: " .. tostring(err6))"#
        ),
    )
    .await;

    wait_for_log(
        &mut owner,
        "idchecker",
        "num channel: INVALID_ARGUMENT: channel_id must be an id string",
    )
    .await;
    wait_for_log(
        &mut owner,
        "idchecker",
        "files err: INVALID_ARGUMENT: files must be a table of strings",
    )
    .await;
    wait_for_log(
        &mut owner,
        "idchecker",
        "num edit: INVALID_ARGUMENT: message_id must be an id string",
    )
    .await;
    wait_for_log(
        &mut owner,
        "idchecker",
        "num delete: INVALID_ARGUMENT: message_id must be an id string",
    )
    .await;
    wait_for_log(
        &mut owner,
        "idchecker",
        "num fetch: INVALID_ARGUMENT: channel_id must be an id string",
    )
    .await;
    wait_for_log(
        &mut owner,
        "idchecker",
        "num typing: INVALID_ARGUMENT: channel_id must be an id string",
    )
    .await;
}

#[tokio::test]
async fn the_plugins_messages_are_rate_limited() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");

    plugin(
        &server,
        &mut owner,
        &ready,
        "spammer",
        &["send_messages"],
        &format!(
            r#"for i = 1, 6 do
                local ok, err = pcall(function()
                    messages.send("{ch}", "msg " .. i)
                end)
                if not ok then
                    log.info("rate limit hit at " .. i .. ": " .. tostring(err))
                    break
                end
            end"#
        ),
    )
    .await;

    wait_for_log(
        &mut owner,
        "spammer",
        "rate limit hit at 6: RATE_LIMITED: slow down",
    )
    .await;
}

#[tokio::test]
async fn a_plugin_sends_its_file_as_an_attachment() {
    // Fails if messages.send does not attach files or attachment cannot be downloaded.
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let code = invite(&mut owner, None).await;
    let (mut member, member_ready) = join(&server, 2, &code).await;
    let ch = channel_id(&ready, "general");

    let p = plugin(
        &server,
        &mut owner,
        &ready,
        "attach-sender",
        &["send_messages", "attach_files"],
        &format!(
            r#"files.put("doc.txt", "hello attachment world")
            local msg = messages.send("{ch}", {{ content = "file message", files = {{ "doc.txt" }} }})
            log.info("sent attachments: " .. tostring(#msg.attachments))"#
        ),
    )
    .await;

    wait_for_log(&mut owner, "attach-sender", "sent attachments: 1").await;

    // Member sees message with attachment
    let mut saw_msg = false;
    for _ in 0..5 {
        let event = member.next_event().await;
        if let proto::event::Kind::MessageCreate(proto::MessageCreate { message: Some(msg) }) =
            event
            && msg.content == "file message"
        {
            assert_eq!(msg.author_id, p.user_id);
            assert_eq!(msg.channel_id, ch);
            assert_eq!(msg.attachments.len(), 1);
            let att = &msg.attachments[0];
            assert_eq!(att.filename, "doc.txt");
            assert_eq!(att.size_bytes, 22);

            let auth = format!("Bearer {}", media_token(&member_ready));
            let (status, body) = server
                .send_http(
                    "GET",
                    &format!("/media/attachments/{}", att.id),
                    &[("Authorization", &auth)],
                    &[],
                )
                .await;
            assert_eq!(status, 200);
            assert_eq!(body, b"hello attachment world");
            saw_msg = true;
            break;
        }
    }
    assert!(saw_msg, "member never received message with attachment");
}

#[tokio::test]
async fn too_many_files_or_a_blocked_one_is_refused() {
    // Fails if >10 files, blocked extensions, or missing attach_files are not refused.
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");

    // Block .exe extensions
    owner
        .ok(Kind::UpdateUploadSettings(proto::UpdateUploadSettings {
            blocked_extensions: Some(proto::BlockedExtensions {
                extensions: vec!["exe".to_owned()],
            }),
        }))
        .await;

    plugin(
        &server,
        &mut owner,
        &ready,
        "checks-plugin",
        &["send_messages", "attach_files"],
        &format!(
            r#"
            -- 1. Too many files (11)
            local files_11 = {{}}
            for i = 1, 11 do
                table.insert(files_11, "f" .. i .. ".txt")
            end
            local ok, err = pcall(function()
                messages.send("{ch}", {{ files = files_11 }})
            end)
            log.info("11 files: " .. tostring(err))

            -- 2. Blocked extension
            files.put("bad.exe", "dangerous")
            local ok2, err2 = pcall(function()
                messages.send("{ch}", {{ files = {{ "bad.exe" }} }})
            end)
            log.info("blocked: " .. tostring(err2))
            "#
        ),
    )
    .await;

    wait_for_log(
        &mut owner,
        "checks-plugin",
        "11 files: INVALID_ARGUMENT: a message can have at most 10 files",
    )
    .await;
    wait_for_log(
        &mut owner,
        "checks-plugin",
        "blocked: FILE_TYPE_BLOCKED: exe files can't be uploaded here",
    )
    .await;

    // 3. Plugin without attach_files permission
    plugin(
        &server,
        &mut owner,
        &ready,
        "no-attach-perm",
        &["send_messages"],
        &format!(
            r#"files.put("sample.txt", "abc")
            local ok, err = pcall(function()
                messages.send("{ch}", {{ files = {{ "sample.txt" }} }})
            end)
            log.info("no perm: " .. tostring(err))"#
        ),
    )
    .await;

    wait_for_log(
        &mut owner,
        "no-attach-perm",
        "no perm: PERMISSION_DENIED: the plugin doesn't have attach_files",
    )
    .await;
}

#[tokio::test]
async fn a_missing_file_is_not_found() {
    // Fails if a missing plugin file does not raise NOT_FOUND: no plugin file at <path>.
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");

    plugin(
        &server,
        &mut owner,
        &ready,
        "missing-file-plugin",
        &["send_messages", "attach_files"],
        &format!(
            r#"local ok, err = pcall(function()
                messages.send("{ch}", {{ files = {{ "not_here.txt" }} }})
            end)
            log.info("missing: " .. tostring(err))"#
        ),
    )
    .await;

    wait_for_log(
        &mut owner,
        "missing-file-plugin",
        "missing: NOT_FOUND: no plugin file at not_here.txt",
    )
    .await;
}
