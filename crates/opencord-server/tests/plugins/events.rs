//! Server events reaching plugins (plan Task 9, design 2026-11-07 §10.4).

use std::time::Duration;

use opencord_common::permissions::Permissions;
use opencord_proto::v1 as proto;
use opencord_server::plugins::permissions::PluginPermission;
use proto::request::Kind;

use crate::common::{
    TestClient, TestDevice, TestServer, channel_id, invite, join, key, media_token, owner, self_id,
    slip,
};
use crate::support::{install, log_lines, manifest, package, wait_for_log};

async fn send(client: &mut TestClient, channel_id: i64, content: &str) {
    client
        .ok(Kind::SendMessage(proto::SendMessage {
            channel_id,
            content: content.to_owned(),
            ..Default::default()
        }))
        .await;
}

/// Installs a plugin whose one script is `source`, asking for and granted
/// `permissions`.
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

async fn assert_never_logged(client: &mut TestClient, plugin_id: &str, text: &str) {
    let lines = log_lines(client, plugin_id).await;
    assert!(
        !lines.iter().any(|line| line.contains(text)),
        "{text:?} reached {plugin_id}: {lines:#?}"
    );
}

#[tokio::test]
async fn a_plugin_gets_messages_it_may_read() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    plugin(
        &server,
        &mut owner,
        &ready,
        "reader",
        &["read_messages"],
        r#"opencord.on("message_create", function(event)
            local message = event.message
            log.info("got " .. message.content .. " as " .. type(message.id)
                .. " in " .. type(message.channel_id) .. " from " .. message.author.name)
        end)"#,
    )
    .await;

    send(&mut owner, channel_id(&ready, "general"), "hello plugin").await;

    // Ids reach Lua as strings: snowflakes don't fit in a Luau number.
    wait_for_log(
        &mut owner,
        "reader",
        "got hello plugin as string in string from Owner",
    )
    .await;
}

#[tokio::test]
async fn without_read_messages_nothing_arrives() {
    // Catches: delivering an event to every running plugin, whatever it
    // was granted.
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    plugin(
        &server,
        &mut owner,
        &ready,
        "deaf",
        &["read_members"],
        r#"opencord.on("message_create", function(event)
            log.info("got " .. event.message.content)
        end)
        opencord.on("member_join", function(event)
            log.info("joined " .. event.member.name)
        end)"#,
    )
    .await;

    send(&mut owner, channel_id(&ready, "general"), "not for plugins").await;
    // The plugin's queue is in order: once the join arrives, the message
    // would have arrived before it.
    let code = invite(&mut owner, None).await;
    let _alice = join(&server, 2, &code).await;
    wait_for_log(&mut owner, "deaf", "joined user2").await;

    assert_never_logged(&mut owner, "deaf", "got ").await;
}

#[tokio::test]
async fn a_hidden_channel_sends_the_plugin_nothing() {
    // Catches: checking only the plugin permission, not whether the
    // plugin's role can view the channel.
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let reader = plugin(
        &server,
        &mut owner,
        &ready,
        "reader",
        &["read_messages"],
        r#"opencord.on("message_create", function(event)
            log.info("got " .. event.message.content)
        end)"#,
    )
    .await;
    let secret = match owner
        .ok(Kind::CreateChannel(proto::CreateChannel {
            kind: proto::ChannelKind::Text as i32,
            name: "secret".to_owned(),
            topic: None,
            parent_id: None,
        }))
        .await
    {
        proto::response::Result::Channel(channel) => channel.id,
        other => panic!("not a channel: {other:?}"),
    };
    owner
        .ok(Kind::SetChannelOverwrite(proto::SetChannelOverwrite {
            channel_id: secret,
            overwrite: Some(proto::PermissionOverwrite {
                target_kind: proto::OverwriteTarget::Role as i32,
                target_id: reader.role_id,
                allow: 0,
                deny: Permissions::VIEW_CHANNEL.bits(),
            }),
        }))
        .await;

    send(&mut owner, secret, "hidden words").await;
    send(&mut owner, channel_id(&ready, "general"), "open words").await;
    wait_for_log(&mut owner, "reader", "got open words").await;

    assert_never_logged(&mut owner, "reader", "got hidden words").await;
}

/// Names a careless router might give the events plugins must never get.
const OFF_LIMITS: &[&str] = &[
    "pending_event",
    "private_message",
    "friend_request",
    "private_typing_start",
    "private_settings_update",
    "network_status",
    "server_notice",
    "server_addresses_update",
    "storage_update",
    "upload_ready",
    "upload_failed",
    "voice_server_update",
    "ephemeral_message",
    "commands_update",
    "lavalink_node_status",
    "lavalink_node_event",
    "lavalink_test_progress",
    "plugin_update",
    "plugin_remove",
    "plugin_review_update",
    "plugin_review_remove",
    "plugin_log_append",
    "plugin_users_update",
];

#[tokio::test]
async fn off_limits_events_never_reach_a_plugin() {
    // Catches: a catch-all arm that routes, or routing private messages,
    // network and storage events to a plugin holding every permission.
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let everything: Vec<&str> = PluginPermission::ALL.iter().map(|p| p.name()).collect();
    let names: Vec<String> = OFF_LIMITS
        .iter()
        .chain(&["message_create"])
        .map(|name| format!("{name:?}"))
        .collect();
    let source = format!(
        r#"for _, name in ipairs({{ {} }}) do
            pcall(opencord.on, name, function() log.info("event: " .. name) end)
        end"#,
        names.join(", ")
    );
    plugin(&server, &mut owner, &ready, "nosy", &everything, &source).await;
    let code = invite(&mut owner, None).await;
    let (mut alice, alice_ready) = join(&server, 2, &code).await;
    let alice_id = self_id(&alice_ready);
    owner
        .register_device(&key(1), &TestDevice::new(10), 0)
        .await;
    alice
        .register_device(&key(2), &TestDevice::new(20), 0)
        .await;

    owner
        .ok(Kind::SendPrivateMessage(proto::SendPrivateMessage {
            recipient_id: alice_id,
            send_id: vec![1; 16],
            ciphertexts: vec![proto::DeviceCiphertext {
                user_id: alice_id,
                device_id: TestDevice::new(20).id.to_vec(),
                olm_type: 1,
                body: vec![1; 8],
            }],
            slip: Vec::new(),
            withheld: Vec::new(),
        }))
        .await;
    owner
        .ok(Kind::SendFriendRequest(proto::SendFriendRequest {
            user_id: alice_id,
            slip: slip(&key(1), &key(2).verifying_key().to_bytes()),
        }))
        .await;
    owner
        .ok(Kind::StartStorageMove(proto::StartStorageMove {}))
        .await;
    owner
        .wait_for(|kind| match kind {
            proto::event::Kind::StorageUpdate(update) => update
                .storage
                .as_ref()?
                .r#move
                .as_ref()
                .filter(|moving| !moving.running)
                .map(|_| ()),
            _ => None,
        })
        .await;
    owner
        .ok(Kind::WatchNetwork(proto::WatchNetwork { watch: true }))
        .await;
    tokio::time::sleep(Duration::from_millis(200)).await;

    send(&mut owner, channel_id(&ready, "general"), "fence").await;
    wait_for_log(&mut owner, "nosy", "event: message_create").await;

    for name in OFF_LIMITS {
        assert_never_logged(&mut owner, "nosy", &format!("event: {name}")).await;
    }
}
