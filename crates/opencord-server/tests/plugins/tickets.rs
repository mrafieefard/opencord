// SPDX-License-Identifier: MPL-2.0

//! End-to-end tests for the ticket system example plugin (plan M2 Task 26).

use std::path::Path;
use std::time::Duration;

use opencord_plugins::package::Package;
use opencord_proto::v1 as proto;
use proto::event::Kind as Event;
use proto::request::Kind;
use proto::response::Result as Reply;

use crate::common::{TestClient, TestServer, channel_id, invite, media_token, owner, self_id};
use crate::support::{install_with_config, session, wait_for_log};

const FEATURE: &str = "plugin_channels";

fn open(channel_id: i64) -> Kind {
    Kind::OpenView(proto::OpenView {
        place: Some(proto::ViewPlace {
            kind: Some(proto::view_place::Kind::ChannelId(channel_id)),
        }),
        platform: "desktop".to_owned(),
    })
}

async fn open_view(client: &mut TestClient, channel_id: i64) -> proto::ViewOpened {
    match client.ok(open(channel_id)).await {
        Reply::ViewOpened(opened) => opened,
        other => panic!("not opened: {other:?}"),
    }
}

fn send(view_id: i64, name: &str, data_json: &str) -> Kind {
    Kind::SendViewEvent(proto::SendViewEvent {
        view_id,
        name: name.to_owned(),
        data_json: data_json.to_owned(),
    })
}

async fn within<T>(client: &mut TestClient, pick: impl FnMut(&Event) -> Option<T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), client.wait_for(pick))
        .await
        .expect("the event never came")
}

async fn message(client: &mut TestClient, name: &str) -> proto::ViewMessage {
    within(client, |event| match event {
        Event::ViewClosed(closed) => panic!("the view closed: {closed:?}"),
        Event::ViewMessage(message) if message.name == name => Some(message.clone()),
        _ => None,
    })
    .await
}

// Fails if the ticket system plugin cannot be installed, if tickets channel cannot
// be opened, if support member does not receive ticket updates, if a regular member
// sees someone else's tickets, or if closing by a non-support member is not refused.
#[tokio::test]
async fn the_ticket_system_works_end_to_end() {
    let example_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/plugins/tickets");
    let package = Package::from_dir(&example_path).expect("tickets folder packs");
    let zip = package.write_zip();

    let server = TestServer::start().await;
    let (mut owner_client, _) = owner(&server).await;
    let (mut admin, admin_ready) = session(&server, 1, None, &[FEATURE]).await;
    let admin_id = self_id(&admin_ready);
    let general_id = channel_id(&admin_ready, "general");

    // 1. Create a "Support" role
    let support_role_id = match admin
        .ok(Kind::CreateRole(proto::CreateRole {
            name: "Support".to_owned(),
            ..Default::default()
        }))
        .await
    {
        Reply::Role(role) => role.id,
        other => panic!("not a role: {other:?}"),
    };

    // 2. Invite member 2 (support) and member 3 (regular)
    let code = invite(&mut admin, None).await;
    let (mut member2, member2_ready) = session(&server, 2, Some(&code), &[FEATURE]).await;
    let member2_id = self_id(&member2_ready);

    // Assign "Support" role to member 2
    admin
        .ok(Kind::AddMemberRole(proto::AddMemberRole {
            user_id: member2_id,
            role_id: support_role_id,
        }))
        .await;

    let (mut member3, _member3_ready) = session(&server, 3, Some(&code), &[FEATURE]).await;

    // 3. Install tickets plugin with config
    let plugin = install_with_config(
        &server,
        &mut admin,
        &media_token(&admin_ready),
        zip,
        &["create_channel_types", "send_messages", "read_roles"],
        vec![
            proto::PluginConfigValue {
                key: "support_role".to_owned(),
                value: support_role_id.to_string(),
                has_value: true,
            },
            proto::PluginConfigValue {
                key: "log_channel".to_owned(),
                value: general_id.to_string(),
                has_value: true,
            },
        ],
    )
    .await;

    wait_for_log(&mut admin, &plugin.id, "tickets plugin started").await;

    // 4. Create tickets channel
    let channel_id = match admin
        .ok(Kind::CreateChannel(proto::CreateChannel {
            kind: proto::ChannelKind::Plugin as i32,
            name: "support-desk".to_owned(),
            plugin_id: Some("tickets".to_owned()),
            type_key: Some("tickets".to_owned()),
            ..Default::default()
        }))
        .await
    {
        Reply::Channel(channel) => channel.id,
        other => panic!("not a channel: {other:?}"),
    };

    // 5. Open views as admin (member 1) and support member (member 2)
    let view1 = open_view(&mut admin, channel_id).await;
    let view2 = open_view(&mut member2, channel_id).await;

    // 6. Member 1 creates a ticket
    admin
        .ok(send(
            view1.view_id,
            "ticket.create",
            &serde_json::json!({
                "title": "Need assistance",
                "body": "Cannot access server dashboard",
                "priority": "high",
            })
            .to_string(),
        ))
        .await;

    // Support member gets the update
    let msg2 = message(&mut member2, "ticket.created").await;
    let created2: serde_json::Value =
        serde_json::from_str(&msg2.data_json).expect("valid ticket.created json");
    let ticket = &created2["ticket"];
    assert_eq!(ticket["title"], "Need assistance");
    assert_eq!(ticket["body"], "Cannot access server dashboard");
    assert_eq!(ticket["author_id"], admin_id.to_string());
    assert_eq!(ticket["priority"], "high");
    assert_eq!(ticket["state"], "open");
    let ticket_id = ticket["id"]
        .as_i64()
        .or_else(|| ticket["id"].as_str().and_then(|s| s.parse().ok()))
        .expect("ticket id");

    // Author (member 1) gets ticket.created as well
    let msg1 = message(&mut admin, "ticket.created").await;
    let created1: serde_json::Value =
        serde_json::from_str(&msg1.data_json).expect("valid ticket.created json");
    assert_eq!(created1["ticket"]["id"], ticket_id);

    // log_channel receives "Ticket #{id} opened by {name}: {title}"
    let log_msg = owner_client
        .wait_for(|event| match event {
            Event::MessageCreate(create) => create
                .message
                .clone()
                .filter(|m| m.channel_id == general_id && m.content.starts_with("Ticket #")),
            _ => None,
        })
        .await;
    assert!(log_msg.content.contains("opened by"));
    assert!(log_msg.content.contains("Need assistance"));

    // 7. Member 3 (other member) opens view and lists tickets: does not see someone else's ticket
    let view3 = open_view(&mut member3, channel_id).await;
    member3.ok(send(view3.view_id, "ticket.list", "{}")).await;
    let msg3 = message(&mut member3, "ticket.list").await;
    let list3: serde_json::Value =
        serde_json::from_str(&msg3.data_json).expect("valid ticket.list json");
    let is_empty = match &list3["tickets"] {
        serde_json::Value::Array(a) => a.is_empty(),
        serde_json::Value::Object(o) => o.is_empty(),
        _ => false,
    };
    assert!(
        is_empty,
        "other member must not see someone else's ticket: {list3:?}"
    );

    // 8. Closing by a non-support member (member 3) is refused
    member3
        .ok(send(
            view3.view_id,
            "ticket.close",
            &serde_json::json!({ "id": ticket_id }).to_string(),
        ))
        .await;
    let err3 = message(&mut member3, "ticket.error").await;
    assert!(
        err3.data_json.contains("support"),
        "non-support member close must be refused"
    );

    // 9. Closing by support member (member 2) succeeds
    member2
        .ok(send(
            view2.view_id,
            "ticket.close",
            &serde_json::json!({ "id": ticket_id }).to_string(),
        ))
        .await;

    let closed2 = message(&mut member2, "ticket.closed").await;
    let closed2_json: serde_json::Value =
        serde_json::from_str(&closed2.data_json).expect("valid ticket.closed json");
    assert_eq!(closed2_json["ticket"]["id"], ticket_id);
    assert_eq!(closed2_json["ticket"]["state"], "closed");

    // Author (member 1) also receives ticket.closed
    let closed1 = message(&mut admin, "ticket.closed").await;
    let closed1_json: serde_json::Value =
        serde_json::from_str(&closed1.data_json).expect("valid ticket.closed json");
    assert_eq!(closed1_json["ticket"]["id"], ticket_id);
    assert_eq!(closed1_json["ticket"]["state"], "closed");
}
