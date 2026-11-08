// SPDX-License-Identifier: MPL-2.0

//! Plugin bus tests (plan Task 20, design 2026-11-07 §10.3).

use opencord_proto::v1 as proto;

use crate::common::{TestClient, TestServer, media_token, owner};
use crate::support::{install, manifest, package, wait_for_log};

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
async fn the_bus_needs_the_permission_on_both_sides() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;

    // 1. Sender without permission fails
    let _sender_unauth = plugin(
        &server,
        &mut owner,
        &ready,
        "bus-sender-no-perm",
        &[],
        r#"
        local ok, err = pcall(function()
            plugins.emit("hello", { greeting = "world" })
        end)
        if not ok then
            log.info("emit_denied: " .. tostring(err))
        end
        "#,
    )
    .await;
    wait_for_log(&mut owner, "bus-sender-no-perm", "emit_denied:").await;

    // 2. Receiver without permission fails
    let _receiver_unauth = plugin(
        &server,
        &mut owner,
        &ready,
        "bus-recv-no-perm",
        &[],
        r#"
        local ok, err = pcall(function()
            plugins.on("bus-sender", "hello", function(data) end)
        end)
        if not ok then
            log.info("listen_denied: " .. tostring(err))
        end
        "#,
    )
    .await;
    wait_for_log(&mut owner, "bus-recv-no-perm", "listen_denied:").await;

    // 3. Both with talk_to_plugins: successful communication
    let _receiver = plugin(
        &server,
        &mut owner,
        &ready,
        "bus-receiver",
        &["talk_to_plugins"],
        r#"
        plugins.on("bus-sender", "ping", function(data)
            log.info("bus_received: " .. data.msg)
        end)
        log.info("receiver_ready")
        "#,
    )
    .await;
    wait_for_log(&mut owner, "bus-receiver", "receiver_ready").await;

    let _sender = plugin(
        &server,
        &mut owner,
        &ready,
        "bus-sender",
        &["talk_to_plugins"],
        r#"
        -- Test oversized payload > 64 KiB
        local big_data = { payload = string.rep("A", 66000) }
        local ok, err = pcall(function()
            plugins.emit("oversized", big_data)
        end)
        if not ok then
            log.info("oversized_refused: " .. tostring(err))
        end

        -- Normal emit
        plugins.emit("ping", { msg = "pong" })
        "#,
    )
    .await;

    wait_for_log(&mut owner, "bus-sender", "oversized_refused:").await;
    wait_for_log(&mut owner, "bus-receiver", "bus_received: pong").await;
}
