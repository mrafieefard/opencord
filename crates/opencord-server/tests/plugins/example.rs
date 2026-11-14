// SPDX-License-Identifier: MPL-2.0

//! End-to-end test of the example welcome bot (plan Task 28).

use std::path::Path;

use opencord_plugins::package::Package;
use opencord_proto::v1 as proto;
use proto::event::Kind as Event;
use proto::request::Kind;

use crate::common::{TestServer, channel_id, invite, join, media_token, owner, self_id};
use crate::support::{install_with_config, wait_for_log};

#[tokio::test]
async fn the_welcome_bot_works_end_to_end() {
    let example_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/plugins/welcome-bot");
    let package = Package::from_dir(&example_path).expect("welcome-bot folder packs");
    let zip = package.write_zip();

    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let general_id = channel_id(&ready, "general");

    let plugin = install_with_config(
        &server,
        &mut owner,
        &media_token(&ready),
        zip,
        &[
            "read_members",
            "send_messages",
            "read_messages",
            "manage_messages",
            "register_commands",
        ],
        vec![
            proto::PluginConfigValue {
                key: "welcome_channel".to_owned(),
                value: general_id.to_string(),
                has_value: true,
            },
            proto::PluginConfigValue {
                key: "banned_words".to_owned(),
                value: "heck,badword".to_owned(),
                has_value: true,
            },
        ],
    )
    .await;

    // Wait until the bot starts
    wait_for_log(&mut owner, &plugin.id, "welcome bot started").await;

    // 1. Check /roll command works
    let invite_code = invite(&mut owner, None).await;
    let (mut member, member_ready) = join(&server, 2, &invite_code).await;
    let member_id = self_id(&member_ready);

    let commands = member_ready.commands.expect("commands in ready");
    assert!(commands.commands.iter().any(|c| c.name == "roll"));

    let roll_reply = member
        .ok(Kind::ExecuteCommand(proto::ExecuteCommand {
            source: String::new(),
            channel_id: general_id,
            name: "roll".to_owned(),
            options: vec![proto::CommandOptionValue {
                name: "sides".to_owned(),
                value: "20".to_owned(),
            }],
            nonce: "roll-1".to_owned(),
        }))
        .await;

    let proto::response::Result::CommandResult(res) = roll_reply else {
        panic!("expected CommandResult, got {roll_reply:?}");
    };
    assert!(!res.ephemeral);
    assert!(res.message_id.is_some());

    let roll_msg = member
        .wait_for(|event| match event {
            Event::MessageCreate(create) => create
                .message
                .clone()
                .filter(|m| m.content.starts_with("You rolled ")),
            _ => None,
        })
        .await;
    assert_eq!(roll_msg.author_id, plugin.user_id);
    let interaction = roll_msg.interaction.expect("has interaction");
    assert_eq!(interaction.user_id, member_id);
    assert_eq!(interaction.command, "roll");

    // 2. Check banned words: sends message with banned word -> gets deleted and logged
    member
        .ok(Kind::SendMessage(proto::SendMessage {
            channel_id: general_id,
            content: "what the heck is this".to_owned(),
            nonce: "msg-banned".to_owned(),
            ..Default::default()
        }))
        .await;

    // Wait for the deletion event
    member
        .wait_for(|event| match event {
            Event::MessageDelete(_) => Some(()),
            _ => None,
        })
        .await;

    wait_for_log(
        &mut owner,
        &plugin.id,
        "deleted message containing banned word",
    )
    .await;

    // 3. Check greeting: new member joins -> bot greets in welcome_channel and counts in database
    let (mut member3, member3_ready) = join(&server, 3, &invite_code).await;
    let member3_id = self_id(&member3_ready);

    let greeting_msg = member3
        .wait_for(|event| match event {
            Event::MessageCreate(create) => create
                .message
                .clone()
                .filter(|m| m.content.contains(&format!("<@{member3_id}>"))),
            _ => None,
        })
        .await;
    assert_eq!(greeting_msg.author_id, plugin.user_id);

    wait_for_log(&mut owner, &plugin.id, "greeted new member").await;
}
