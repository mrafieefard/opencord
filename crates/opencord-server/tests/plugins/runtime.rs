//! The plugin runtime seen from members (plan Task 7).

use std::time::{Duration, Instant};

use opencord_proto::v1 as proto;
use proto::request::Kind;

use crate::common::{TestClient, TestServer, channel_id, invite, join, owner};
use crate::support::{manifest, package};

/// Installs `zip` as the owner, granting `granted`.
async fn install_plugin(
    _server: &TestServer,
    _owner: &mut TestClient,
    _zip: Vec<u8>,
    _granted: &[&str],
) {
    todo!("Task 8: install with support::install")
}

/// Waits up to 5 s for `text` in the plugin's log.
async fn wait_for_log(_owner: &mut TestClient, _plugin_id: &str, _text: &str) {
    todo!("Task 8: support::wait_for_log")
}

// Catches: running Lua on the server's runtime, where a busy plugin holds
// up everyone's messages.
#[tokio::test]
#[ignore = "needs Task 8"]
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
    install_plugin(&server, &mut owner, zip, &["read_messages"]).await;
    let code = invite(&mut owner, None).await;
    let (mut alice, _) = join(&server, 2, &code).await;
    let general = channel_id(&ready, "general");

    for index in 0..20 {
        let content = format!("message {index}");
        let sent = Instant::now();
        owner
            .ok(Kind::SendMessage(proto::SendMessage {
                channel_id: general,
                content: content.clone(),
                ..Default::default()
            }))
            .await;
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
