//! The plugin runtime seen from members (plan Task 7).

use std::time::{Duration, Instant};

use opencord_proto::v1 as proto;
use proto::request::Kind;

use crate::common::{TestClient, TestServer, channel_id, invite, join, media_token, owner};
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
    let server = TestServer::start_with(|c| c.plugins.memory_mb = 16).await;
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
            local marker = files.get("marker")
            if not marker then
                files.put("marker", "1")
                hooks.on("old", function(req)
                    return { status = 200, body = "pong" }
                end)
                commands.register({ name = "ping", description = "pong" }, function() end)
                log.info("first_run")
                timer.after(0.05, function()
                    local s = string.rep("x", 64 * 1024 * 1024)
                end)
            else
                log.info("second_run")
            end
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

    wait_for_log(&mut owner, "crasher", "first_run").await;

    // Verify hook works on first run
    let (status, body) = server
        .send_http("GET", "/plugins/crasher/hooks/old", &[], b"")
        .await;
    assert_eq!(status, 200);
    assert_eq!(body, b"pong");

    // Verify command /ping is registered
    assert!(
        server
            .state()
            .commands
            .find_plugin("crasher", "ping")
            .is_some()
    );

    // Wait for OOM restart and second run
    wait_for_log(&mut owner, "crasher", "second_run").await;

    // Verify hook is now gone (404)
    let (status, _) = server
        .send_http("GET", "/plugins/crasher/hooks/old", &[], b"")
        .await;
    assert_eq!(status, 404);

    // Verify command /ping is gone
    assert!(
        server
            .state()
            .commands
            .find_plugin("crasher", "ping")
            .is_none()
    );
    // The bus entry left by a restart can't be seen from outside: the new
    // VM has no Lua listener for it.
}

/// Spins as hard as the handler budget lets it, for as long as it runs.
const SPINNER: &str = r#"
local function spin()
    local x = 0
    for i = 1, 3000000 do x = x + i end
    timer.after(0, spin)
end
timer.after(0, spin)
log.info("spinning")
"#;

async fn cpu_of(client: &mut TestClient, id: &str) -> u32 {
    crate::support::plugins(client)
        .await
        .into_iter()
        .find(|plugin| plugin.id == id)
        .unwrap_or_else(|| panic!("{id} isn't installed"))
        .cpu_permille
}

#[tokio::test]
async fn a_plugin_spinning_in_handlers_stays_near_its_share() {
    let server = TestServer::start_with(|config| config.plugins.handler_ms = 5_000).await;
    let (mut owner, ready) = owner(&server).await;
    let zip = package(&[
        ("manifest.lua", &manifest("spinner", &[])),
        ("server/main.luau", SPINNER),
    ]);
    install(&server, &mut owner, &media_token(&ready), zip, &[]).await;
    wait_for_log(&mut owner, "spinner", "spinning").await;

    tokio::time::sleep(Duration::from_secs(2)).await;
    let free = cpu_of(&mut owner, "spinner").await;
    assert!(free > 300, "it should run freely at first: {free}");

    tokio::time::sleep(Duration::from_secs(7)).await;
    let held = cpu_of(&mut owner, "spinner").await;
    assert!(
        held < 450,
        "it should be held near a quarter of a core: {held}"
    );
}

#[tokio::test]
async fn commands_still_answer_while_a_neighbour_is_held() {
    let server = TestServer::start_with(|config| config.plugins.handler_ms = 5_000).await;
    let (mut owner, ready) = owner(&server).await;
    let token = media_token(&ready);
    let spinner = package(&[
        ("manifest.lua", &manifest("spinner", &[])),
        ("server/main.luau", SPINNER),
    ]);
    install(&server, &mut owner, &token, spinner, &[]).await;
    wait_for_log(&mut owner, "spinner", "spinning").await;
    let quiet = package(&[
        ("manifest.lua", &manifest("quiet", &["register_commands"])),
        (
            "server/main.luau",
            r#"commands.register({ name = "hello", description = "d" }, function(cmd)
                cmd:reply("hi", { only_you = true })
            end)
            log.info("ready")"#,
        ),
    ]);
    install(&server, &mut owner, &token, quiet, &["register_commands"]).await;
    wait_for_log(&mut owner, "quiet", "ready").await;
    tokio::time::sleep(Duration::from_secs(4)).await;

    let started = Instant::now();
    owner
        .ok(Kind::ExecuteCommand(proto::ExecuteCommand {
            source: "plugin:quiet".to_owned(),
            channel_id: channel_id(&ready, "general"),
            name: "hello".to_owned(),
            options: Vec::new(),
            nonce: "n".to_owned(),
        }))
        .await;

    assert!(
        started.elapsed() < Duration::from_secs(1),
        "{:?}",
        started.elapsed()
    );
}
