//! The plugin runtime seen from members (plan Task 7).

use std::time::{Duration, Instant};

use opencord_proto::v1 as proto;
use proto::request::Kind;

use crate::common::{TestServer, channel_id, invite, join, media_token, owner};
use crate::support::{install, manifest, package, wait_for_log};

// Catches: running Lua on the server's runtime, where a busy plugin holds
// up everyone's messages.
#[tokio::test]
async fn an_endless_loop_does_not_delay_chat() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let zip = package(&[
        ("manifest.lua", &manifest("looper", &["read_messages"])),
        (
            "server/main.luau",
            "opencord.on('message_create', function() while true do end end)",
        ),
    ]);
    install(
        &server,
        &mut owner,
        &media_token(&ready),
        zip,
        &["read_messages"],
    )
    .await;
    let code = invite(&mut owner, None).await;
    let (mut alice, _) = join(&server, 2, &code).await;
    let general = channel_id(&ready, "general");

    for index in 0..20 {
        let content = format!("message {index}");
        // Past the burst, messages are rate limited: wait that out, and
        // time only the attempt that was accepted.
        let sent = loop {
            let sent = Instant::now();
            match owner
                .request(Kind::SendMessage(proto::SendMessage {
                    channel_id: general,
                    content: content.clone(),
                    ..Default::default()
                }))
                .await
            {
                proto::response::Result::Error(error)
                    if error.code == proto::ErrorCode::RateLimited as i32 =>
                {
                    let wait = error.retry_after_ms.unwrap_or(100);
                    tokio::time::sleep(Duration::from_millis(wait)).await;
                }
                proto::response::Result::Error(error) => panic!("not sent: {error:?}"),
                _ => break sent,
            }
        };
        alice
            .wait_for(|event| match event {
                proto::event::Kind::MessageCreate(created)
                    if created
                        .message
                        .as_ref()
                        .is_some_and(|message| message.content == content) =>
                {
                    Some(())
                }
                _ => None,
            })
            .await;
        assert!(
            sent.elapsed() < Duration::from_secs(1),
            "message {index} took {:?}",
            sent.elapsed()
        );
    }
    wait_for_log(
        &mut owner,
        "looper",
        "message_create handler failed: the handler ran longer than 100 ms",
    )
    .await;
}

#[tokio::test]
async fn a_restart_clears_timers_hooks_and_bus() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let zip = package(&[
        (
            "manifest.lua",
            &manifest(
                "crasher",
                &[
                    "receive_web_requests",
                    "talk_to_plugins",
                    "register_commands",
                ],
            ),
        ),
        (
            "server/main.luau",
            r#"
            hooks.on("ping", function(req)
                return { status = 200, body = "pong" }
            end)
            timer.every(10, function() end)
            commands.register({ name = "ping", description = "pong" }, function() end)
            plugins.on("other", "ev", function() end)
            log.info("hook_registered")
            "#,
        ),
    ]);
    install(
        &server,
        &mut owner,
        &media_token(&ready),
        zip,
        &[
            "receive_web_requests",
            "talk_to_plugins",
            "register_commands",
        ],
    )
    .await;

    wait_for_log(&mut owner, "crasher", "hook_registered").await;

    // Verify hook works
    let (status, body) = server
        .send_http("GET", "/plugins/crasher/hooks/ping", &[], b"")
        .await;
    assert_eq!(status, 200);
    assert_eq!(body, b"pong");

    // Stop the plugin
    owner
        .ok(Kind::SetPluginRunning(proto::SetPluginRunning {
            plugin_id: "crasher".to_owned(),
            running: false,
        }))
        .await;

    // Verify hook returns 503 when stopped
    let (status, _) = server
        .send_http("GET", "/plugins/crasher/hooks/ping", &[], b"")
        .await;
    assert_eq!(status, 503);

    // Uninstall the plugin
    owner
        .ok(Kind::UninstallPlugin(proto::UninstallPlugin {
            plugin_id: "crasher".to_owned(),
            delete_data: false,
        }))
        .await;

    // Verify hook is now gone (404)
    let (status, _) = server
        .send_http("GET", "/plugins/crasher/hooks/ping", &[], b"")
        .await;
    assert_eq!(status, 404);
}
