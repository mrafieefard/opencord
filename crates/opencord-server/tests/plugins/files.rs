// SPDX-License-Identifier: MPL-2.0

//! Plugin files tests (plan Task 18, design 2026-11-07 §10.3, §13.1).

use opencord_proto::v1 as proto;

use crate::common::{TestClient, TestServer, channel_id, media_token, owner};
use crate::support::{install, manifest, package, wait_for_log};

async fn send_msg(client: &mut TestClient, channel_id: i64, content: &str) {
    client
        .ok(proto::request::Kind::SendMessage(proto::SendMessage {
            channel_id,
            content: content.to_owned(),
            nonce: String::new(),
            attachment_ids: vec![],
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
async fn files_round_trip() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");

    let _p = plugin(
        &server,
        &mut owner,
        &ready,
        "files-rt",
        &["read_messages"],
        r#"
        opencord.on("message_create", function(event)
            if event.message.content == "run" then
                -- Put string
                files.put("hello.txt", "world", { public = true })
                local b = files.get("hello.txt")
                assert(b ~= nil, "file should exist")
                assert(buffer.tostring(b) == "world", "content mismatch")

                -- Put buffer
                local buf = buffer.create(4)
                buffer.writeu8(buf, 0, 65)
                buffer.writeu8(buf, 1, 66)
                buffer.writeu8(buf, 2, 67)
                buffer.writeu8(buf, 3, 68)
                files.put("nested/abc.bin", buf)

                local b2 = files.get("nested/abc.bin")
                assert(b2 ~= nil, "nested file should exist")
                assert(buffer.tostring(b2) == "ABCD", "buffer content mismatch")

                -- Delete
                files.delete("hello.txt")
                local b3 = files.get("hello.txt")
                assert(b3 == nil, "deleted file should be nil")

                log.info("round trip ok")
            end
        end)
        log.info("ready")
        "#,
    )
    .await;

    wait_for_log(&mut owner, "files-rt", "ready").await;
    send_msg(&mut owner, ch, "run").await;
    wait_for_log(&mut owner, "files-rt", "round trip ok").await;
}

#[tokio::test]
async fn bad_paths_are_refused() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");

    let _p = plugin(
        &server,
        &mut owner,
        &ready,
        "files-bad-paths",
        &["read_messages"],
        r#"
        opencord.on("message_create", function(event)
            if event.message.content == "test" then
                local bad = {
                    "../x",
                    "/x",
                    "a\\b",
                    "foo//bar",
                    ".dot",
                    "bar/./baz",
                    "",
                }
                for _, path in ipairs(bad) do
                    local ok, err = pcall(function()
                        files.put(path, "data")
                    end)
                    assert(not ok, "expected error for path: " .. path)
                    assert(string.find(err, "INVALID_ARGUMENT"), "expected INVALID_ARGUMENT for " .. path .. " got: " .. tostring(err))

                    local ok2, err2 = pcall(function()
                        files.get(path)
                    end)
                    assert(not ok2, "expected error on get for: " .. path)

                    local ok3, err3 = pcall(function()
                        files.delete(path)
                    end)
                    assert(not ok3, "expected error on delete for: " .. path)
                end
                log.info("bad paths rejected")
            end
        end)
        log.info("ready")
        "#,
    )
    .await;

    wait_for_log(&mut owner, "files-bad-paths", "ready").await;
    send_msg(&mut owner, ch, "test").await;
    wait_for_log(&mut owner, "files-bad-paths", "bad paths rejected").await;
}

#[tokio::test]
async fn the_quota_is_enforced() {
    let server = TestServer::start_with(|config| {
        config.plugins.files_mb = 16;
    })
    .await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");

    let _p = plugin(
        &server,
        &mut owner,
        &ready,
        "files-quota",
        &["read_messages"],
        r#"
        opencord.on("message_create", function(event)
            if event.message.content == "quota" then
                local ok, err = pcall(function()
                    files.put("large.bin", buffer.create(17 * 1024 * 1024))
                end)
                assert(not ok, "expected quota error")
                assert(string.find(err, "PLUGIN_LIMIT"), "expected PLUGIN_LIMIT, got: " .. tostring(err))
                log.info("quota enforced")
            end
        end)
        log.info("ready")
        "#,
    )
    .await;

    wait_for_log(&mut owner, "files-quota", "ready").await;
    send_msg(&mut owner, ch, "quota").await;
    wait_for_log(&mut owner, "files-quota", "quota enforced").await;
}

#[tokio::test]
async fn list_filters_by_prefix() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let ch = channel_id(&ready, "general");

    let _p = plugin(
        &server,
        &mut owner,
        &ready,
        "files-list",
        &["read_messages"],
        r#"
        opencord.on("message_create", function(event)
            if event.message.content == "list" then
                files.put("docs/readme.txt", "hello", { public = true })
                files.put("docs/guide.md", "content")
                files.put("images/logo.png", "binary")

                local all = files.list()
                assert(#all == 3, "expected 3 files, got " .. #all)

                local docs = files.list("docs/")
                assert(#docs == 2, "expected 2 docs, got " .. #docs)
                assert(docs[1].path == "docs/guide.md" or docs[2].path == "docs/guide.md")

                for _, item in ipairs(all) do
                    assert(type(item.path) == "string", "path must be string")
                    assert(type(item.size) == "number", "size must be number")
                    assert(type(item.public) == "boolean", "public must be bool")
                    assert(type(item.updated_at_ms) == "number", "updated_at_ms must be number")
                end

                log.info("list ok")
            end
        end)
        log.info("ready")
        "#,
    )
    .await;

    wait_for_log(&mut owner, "files-list", "ready").await;
    send_msg(&mut owner, ch, "list").await;
    wait_for_log(&mut owner, "files-list", "list ok").await;
}

#[tokio::test]
async fn files_survive_a_restart() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let _ch = channel_id(&ready, "general");

    let _p = plugin(
        &server,
        &mut owner,
        &ready,
        "files-restart",
        &["read_messages"],
        r#"
        opencord.on("start", function()
            local b = files.get("persisted.txt")
            if b then
                log.info("FOUND: " .. buffer.tostring(b))
            else
                files.put("persisted.txt", "saved_data")
                log.info("SAVED")
            end
        end)
        "#,
    )
    .await;

    wait_for_log(&mut owner, "files-restart", "SAVED").await;

    // Restart the plugin
    owner
        .ok(proto::request::Kind::RestartPlugin(proto::RestartPlugin {
            plugin_id: "files-restart".to_owned(),
        }))
        .await;

    wait_for_log(&mut owner, "files-restart", "FOUND: saved_data").await;
}
