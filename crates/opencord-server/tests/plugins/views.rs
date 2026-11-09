// SPDX-License-Identifier: MPL-2.0

//! Views on the server: opening, events both ways, their limits, and the
//! server closing them (plan M2 Task 10, design 2026-11-07 §10.3, §10.4,
//! §13.1 and §14.3).

use std::time::Duration;

use opencord_proto::v1 as proto;
use proto::envelope::Payload;
use proto::event::Kind as Event;
use proto::request::Kind;
use proto::response::Result as Reply;
use proto::{ErrorCode, ViewCloseReason};
use sha2::{Digest, Sha256};

use crate::common::{Received, TestClient, TestServer, invite, media_token, owner, self_id};
use crate::support::{install, log_lines, manifest_with, package, session, wait_for_log};

const FEATURE: &str = "plugin_channels";
const PLUGIN: &str = "tickets";
const CLIENT: &str = "return { init = function() return {} end }";
const LOGO: &str = "not really a png";
const SERVER: &str = r#"
ui.register_channel_type("desk", { name = "Desk" })
local seen = {}
opencord.on("view_open", function(event)
    local view = event.view
    seen[view.id] = view
    log.info("open " .. view.id .. " by " .. view.user.id .. " in " .. view.place.channel_id .. " on " .. view.platform)
end)
opencord.on("view_close", function(event)
    log.info("close " .. event.view.id .. " " .. event.reason)
end)
ui.on("ping", function(view, data)
    view:send("pong", { n = data.n })
end)
ui.on("poke", function(view, data)
    log.info("poked " .. data.view .. " sent " .. tostring(seen[data.view]:send("late", {})))
end)
ui.on("burst", function(view, data)
    local sent, ok, err = 0, true, nil
    for _ = 1, data.count do
        ok, err = pcall(view.send, view, "b", data.text or {})
        if not ok then break end
        sent += 1
    end
    log.info("burst sent " .. sent .. " then " .. tostring(err))
end)
ui.on("big", function(view)
    local _, err = pcall(view.send, view, "big", string.rep("x", 70000))
    log.info("big " .. tostring(err))
end)
ui.on("role", function(view, data)
    local reached = ui.send_role(data.role, "hi", {})
    local all = ui.send_all("all", {})
    local place = ui.send_place(view.place, "place", {})
    log.info("role " .. reached .. " all " .. all .. " place " .. place .. " views " .. #ui.views() .. " here " .. #ui.views(view.place))
end)
ui.on("token", function(view)
    view:send("token", { t = files.token("secret.png", view) })
    local _, err = pcall(files.token, "missing.png", view)
    log.info("missing " .. tostring(err))
end)
opencord.on("start", function()
    files.put("secret.png", "private bytes")
    log.info("started")
end)
"#;

/// A server with the tickets plugin running, a `desk` channel of it, the
/// owner (`admin`, seed 1) and a member (seed 2), both listing the feature.
struct World {
    server: TestServer,
    _owner: TestClient,
    admin: TestClient,
    admin_ready: proto::Ready,
    member: TestClient,
    member_ready: proto::Ready,
    channel_id: i64,
}

async fn world() -> World {
    world_with(SERVER, Some("client/main.luau")).await
}

async fn world_with(source: &str, entry: Option<&str>) -> World {
    let server = TestServer::start().await;
    let (_owner, _) = owner(&server).await;
    let (mut admin, admin_ready) = session(&server, 1, None, &[FEATURE]).await;
    let extra = entry.map_or(String::new(), |entry| format!("client = {entry:?},"));
    let zip = package(&[
        (
            "manifest.lua",
            &manifest_with(PLUGIN, "1.0.0", &["create_channel_types"], &extra),
        ),
        ("server/main.luau", source),
        ("client/main.luau", CLIENT),
        ("assets/logo.png", LOGO),
    ]);
    install(
        &server,
        &mut admin,
        &media_token(&admin_ready),
        zip,
        &["create_channel_types"],
    )
    .await;
    wait_for_log(&mut admin, PLUGIN, "started").await;
    let channel_id = match admin
        .ok(Kind::CreateChannel(proto::CreateChannel {
            kind: proto::ChannelKind::Plugin as i32,
            name: "Desk".to_owned(),
            plugin_id: Some(PLUGIN.to_owned()),
            type_key: Some("desk".to_owned()),
            ..Default::default()
        }))
        .await
    {
        Reply::Channel(channel) => channel.id,
        other => panic!("not a channel: {other:?}"),
    };
    let code = invite(&mut admin, None).await;
    let (member, member_ready) = session(&server, 2, Some(&code), &[FEATURE]).await;
    World {
        server,
        _owner,
        admin,
        admin_ready,
        member,
        member_ready,
        channel_id,
    }
}

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

fn code(error: &proto::Error) -> ErrorCode {
    ErrorCode::try_from(error.code).unwrap()
}

/// Waits up to 5 s for an event `pick` takes.
async fn within<T>(client: &mut TestClient, pick: impl FnMut(&Event) -> Option<T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), client.wait_for(pick))
        .await
        .expect("the event never came")
}

/// The next `ViewClosed` the client is sent.
async fn closed(client: &mut TestClient) -> proto::ViewClosed {
    within(client, |event| match event {
        Event::ViewClosed(closed) => Some(*closed),
        _ => None,
    })
    .await
}

/// The next `ViewMessage` named `name`, failing on a `ViewClosed` first.
async fn message(client: &mut TestClient, name: &str) -> proto::ViewMessage {
    within(client, |event| match event {
        Event::ViewClosed(closed) => panic!("the view closed: {closed:?}"),
        Event::ViewMessage(message) if message.name == name => Some(message.clone()),
        _ => None,
    })
    .await
}

/// Sends every request at once, then reads their answers in order.
async fn all_at_once(client: &mut TestClient, kinds: Vec<Kind>) -> Vec<Reply> {
    const FIRST: u64 = 1_000_000;
    for (index, kind) in kinds.iter().enumerate() {
        client
            .send_envelope(proto::Envelope {
                seq: 0,
                request_id: FIRST + index as u64,
                payload: Some(Payload::Request(proto::Request {
                    kind: Some(kind.clone()),
                })),
            })
            .await;
    }
    let mut replies: Vec<Option<Reply>> = vec![None; kinds.len()];
    while replies.iter().any(Option::is_none) {
        match client.recv().await {
            Received::Envelope(envelope) => {
                if let Some(Payload::Response(response)) = envelope.payload {
                    replies[(envelope.request_id - FIRST) as usize] = response.result;
                }
            }
            Received::Closed(code) => panic!("closed ({code:?})"),
        }
    }
    replies.into_iter().map(Option::unwrap).collect()
}

/// Waits up to 5 s until `text` is in exactly `count` of the plugin's lines.
async fn wait_for_count(admin: &mut TestClient, text: &str, count: usize) {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        let lines = log_lines(admin, PLUGIN).await;
        let found = lines.iter().filter(|line| line.contains(text)).count();
        if found == count {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "{text:?} is in {found} lines, not {count}: {lines:#?}"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

// Fails if ViewOpened lists the wrong files, entry or api, shows errors to
// a member who doesn't manage plugins, or the plugin doesn't hear view_open
// with the view's user, place and platform.
#[tokio::test]
async fn opening_a_view_answers_its_files_and_tells_the_plugin() {
    let mut w = world().await;
    let opened = open_view(&mut w.member, w.channel_id).await;
    assert_eq!(opened.plugin_id, PLUGIN);
    assert_eq!(opened.api, 1);
    assert_eq!(opened.entry, "client/main.luau");
    assert!(!opened.show_errors);
    let files: Vec<(String, String, u64)> = opened
        .files
        .iter()
        .map(|file| (file.path.clone(), file.sha256.clone(), file.size_bytes))
        .collect();
    let expected: Vec<(String, String, u64)> =
        [("assets/logo.png", LOGO), ("client/main.luau", CLIENT)]
            .iter()
            .map(|(path, text)| {
                let sha = hex::encode(Sha256::digest(text.as_bytes()));
                ((*path).to_owned(), sha, text.len() as u64)
            })
            .collect();
    assert_eq!(files, expected);
    let user = self_id(&w.member_ready);
    let line = format!(
        "open {} by {user} in {} on desktop",
        opened.view_id, w.channel_id
    );
    wait_for_log(&mut w.admin, PLUGIN, &line).await;

    assert!(open_view(&mut w.admin, w.channel_id).await.show_errors);
}

// Fails if a hidden channel, a stopped plugin's channel, a text channel, a
// plugin without a screen, or a session without the feature opens a view.
#[tokio::test]
async fn an_unavailable_or_hidden_channel_cant_be_opened() {
    let mut w = world().await;
    let everyone = w.admin_ready.server.as_ref().unwrap().everyone_role_id;
    let hidden = |channel_id| {
        Kind::SetChannelOverwrite(proto::SetChannelOverwrite {
            channel_id,
            overwrite: Some(proto::PermissionOverwrite {
                target_kind: proto::OverwriteTarget::Role as i32,
                target_id: everyone,
                allow: 0,
                deny: opencord_common::permissions::Permissions::VIEW_CHANNEL.bits(),
            }),
        })
    };
    w.admin.ok(hidden(w.channel_id)).await;
    let error = w.member.error(open(w.channel_id)).await;
    assert_eq!(code(&error), ErrorCode::NotFound, "{error:?}");

    let text = proto::Channel::default();
    let text_id = w
        .member_ready
        .channels
        .iter()
        .find(|channel| channel.kind == proto::ChannelKind::Text as i32)
        .unwrap_or(&text)
        .id;
    let error = w.member.error(open(text_id)).await;
    assert_eq!(code(&error), ErrorCode::InvalidArgument, "{error:?}");

    let (mut old_app, _) = session(&w.server, 1, None, &[]).await;
    let error = old_app.error(open(w.channel_id)).await;
    assert_eq!(code(&error), ErrorCode::NotFound, "{error:?}");

    w.admin
        .ok(Kind::SetPluginRunning(proto::SetPluginRunning {
            plugin_id: PLUGIN.to_owned(),
            running: false,
        }))
        .await;
    let error = w.admin.error(open(w.channel_id)).await;
    assert_eq!(code(&error), ErrorCode::PluginUnavailable, "{error:?}");

    let mut screenless = world_with(SERVER, None).await;
    let error = screenless.member.error(open(screenless.channel_id)).await;
    assert_eq!(code(&error), ErrorCode::PluginUnavailable);
    assert_eq!(error.message, "this plugin has no screen");
}

// Fails if a session can hold more than 4 views, if closing one doesn't
// free its place, or if the limit counts another session's views.
#[tokio::test]
async fn a_fifth_view_on_a_session_is_refused() {
    let mut w = world().await;
    let mut views = Vec::new();
    for _ in 0..4 {
        views.push(open_view(&mut w.member, w.channel_id).await.view_id);
    }
    let error = w.member.error(open(w.channel_id)).await;
    assert_eq!(code(&error), ErrorCode::PluginLimit);
    assert_eq!(error.message, "too many open views");

    let (mut second, _) = session(&w.server, 2, None, &[FEATURE]).await;
    open_view(&mut second, w.channel_id).await;
    w.member
        .ok(Kind::CloseView(proto::CloseView { view_id: views[0] }))
        .await;
    open_view(&mut w.member, w.channel_id).await;
}

// Fails if a client event doesn't reach `ui.on` with its data, or
// `view:send` doesn't reach the view's session as a ViewMessage.
#[tokio::test]
async fn events_go_both_ways() {
    let mut w = world().await;
    let view = open_view(&mut w.member, w.channel_id).await.view_id;
    w.member.ok(send(view, "ping", r#"{"n":7}"#)).await;
    let pong = message(&mut w.member, "pong").await;
    assert_eq!(pong.view_id, view);
    let data: serde_json::Value = serde_json::from_str(&pong.data_json).unwrap();
    assert_eq!(data, serde_json::json!({ "n": 7 }));
}

// Fails if another member or another session of the same member can send
// to, or close, a view that isn't theirs.
#[tokio::test]
async fn views_of_another_session_cant_be_used() {
    let mut w = world().await;
    let view = open_view(&mut w.member, w.channel_id).await.view_id;
    let (mut same_user, _) = session(&w.server, 2, None, &[FEATURE]).await;
    for client in [&mut w.admin, &mut same_user] {
        let error = client.error(send(view, "ping", r#"{"n":1}"#)).await;
        assert_eq!(code(&error), ErrorCode::NotFound);
        let error = client
            .error(Kind::CloseView(proto::CloseView { view_id: view }))
            .await;
        assert_eq!(code(&error), ErrorCode::NotFound);
    }
    w.member.ok(send(view, "ping", r#"{"n":2}"#)).await;
    message(&mut w.member, "pong").await;
}

// Review Focus 1. Fails if `close_views` skips lost access, or if a view
// table the plugin kept still reaches the member after it closed.
#[tokio::test]
async fn losing_access_closes_the_view() {
    let mut w = world().await;
    let view = open_view(&mut w.member, w.channel_id).await.view_id;
    let everyone = w.admin_ready.server.as_ref().unwrap().everyone_role_id;
    w.admin
        .ok(Kind::SetChannelOverwrite(proto::SetChannelOverwrite {
            channel_id: w.channel_id,
            overwrite: Some(proto::PermissionOverwrite {
                target_kind: proto::OverwriteTarget::Role as i32,
                target_id: everyone,
                allow: 0,
                deny: opencord_common::permissions::Permissions::VIEW_CHANNEL.bits(),
            }),
        }))
        .await;
    let gone = closed(&mut w.member).await;
    assert_eq!(gone.view_id, view);
    assert_eq!(gone.reason, ViewCloseReason::LostAccess as i32);
    wait_for_log(&mut w.admin, PLUGIN, &format!("close {view} lost_access")).await;

    let own = open_view(&mut w.admin, w.channel_id).await.view_id;
    w.admin
        .ok(send(own, "poke", &format!(r#"{{"view":"{view}"}}"#)))
        .await;
    wait_for_log(&mut w.admin, PLUGIN, &format!("poked {view} sent false")).await;
    w.member
        .assert_no_event(Duration::from_millis(300), |event| {
            matches!(event, Event::ViewMessage(_))
        })
        .await;
}

// Fails if deleting a plugin channel leaves its views open, or closes them
// with another reason.
#[tokio::test]
async fn deleting_the_channel_closes_its_views() {
    let mut w = world().await;
    let view = open_view(&mut w.member, w.channel_id).await.view_id;
    w.admin
        .ok(Kind::DeleteChannel(proto::DeleteChannel {
            channel_id: w.channel_id,
        }))
        .await;
    let gone = closed(&mut w.member).await;
    assert_eq!(
        (gone.view_id, gone.reason),
        (view, ViewCloseReason::ChannelDeleted as i32)
    );
    wait_for_log(
        &mut w.admin,
        PLUGIN,
        &format!("close {view} channel_deleted"),
    )
    .await;
}

// Review Focus 2. Fails if a restart closes views, if the new VM doesn't
// hear view_open for them, if a view is announced twice to one VM, or if
// events stop reaching the new VM's handlers.
#[tokio::test]
async fn views_survive_a_restart_and_the_new_vm_hears_them() {
    let mut w = world().await;
    let view = open_view(&mut w.member, w.channel_id).await.view_id;
    let opened = format!("open {view} by");
    wait_for_count(&mut w.admin, &opened, 1).await;
    w.admin
        .ok(Kind::RestartPlugin(proto::RestartPlugin {
            plugin_id: PLUGIN.to_owned(),
        }))
        .await;
    wait_for_count(&mut w.admin, "started", 2).await;
    wait_for_count(&mut w.admin, &opened, 2).await;
    w.member.ok(send(view, "ping", r#"{"n":3}"#)).await;
    message(&mut w.member, "pong").await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    wait_for_count(&mut w.admin, &opened, 2).await;
    wait_for_count(&mut w.admin, "close ", 0).await;
}

// Review Focus 2. Fails if turning the plugin off leaves views open, or
// the plugin doesn't hear them close as it stops.
#[tokio::test]
async fn stopping_closes_views() {
    let mut w = world().await;
    let view = open_view(&mut w.member, w.channel_id).await.view_id;
    w.admin
        .ok(Kind::SetPluginRunning(proto::SetPluginRunning {
            plugin_id: PLUGIN.to_owned(),
            running: false,
        }))
        .await;
    let gone = closed(&mut w.member).await;
    assert_eq!(
        (gone.view_id, gone.reason),
        (view, ViewCloseReason::PluginStopped as i32)
    );
    wait_for_log(
        &mut w.admin,
        PLUGIN,
        &format!("close {view} plugin_stopping"),
    )
    .await;
    let error = w.member.error(send(view, "ping", "{}")).await;
    assert_eq!(code(&error), ErrorCode::NotFound);
}

// Fails if a view takes more than 30 events a second from its client, or
// a plugin can send it more than 60.
#[tokio::test]
async fn the_31st_event_in_a_second_is_refused() {
    let mut w = world().await;
    let view = open_view(&mut w.member, w.channel_id).await.view_id;
    let replies = all_at_once(
        &mut w.member,
        (0..31)
            .map(|_| send(view, "nobody_listens", "{}"))
            .collect(),
    )
    .await;
    for reply in &replies[..30] {
        assert!(matches!(reply, Reply::Ack(_)), "{reply:?}");
    }
    let Reply::Error(error) = &replies[30] else {
        panic!("the 31st went through: {:?}", replies[30]);
    };
    assert_eq!(code(error), ErrorCode::RateLimited);

    let other = open_view(&mut w.member, w.channel_id).await.view_id;
    w.member.ok(send(other, "burst", r#"{"count":61}"#)).await;
    wait_for_log(
        &mut w.admin,
        PLUGIN,
        "burst sent 60 then RATE_LIMITED: too many events to this view",
    )
    .await;
}

// Fails if a view takes more than 128 KiB a second from its client or one
// event over 64 KiB, or a plugin can send it more than 512 KiB a second or
// one event over 64 KiB.
#[tokio::test]
async fn bytes_over_the_limit_are_refused() {
    let mut w = world().await;
    let view = open_view(&mut w.member, w.channel_id).await.view_id;
    let text = format!("\"{}\"", "x".repeat(60_000));
    let big = format!("\"{}\"", "x".repeat(70_000));
    let error = w.member.error(send(view, "nobody_listens", &big)).await;
    assert_eq!(code(&error), ErrorCode::InvalidArgument, "{error:?}");
    let replies = all_at_once(
        &mut w.member,
        (0..3)
            .map(|_| send(view, "nobody_listens", &text))
            .collect(),
    )
    .await;
    assert!(matches!(replies[0], Reply::Ack(_)), "{:?}", replies[0]);
    assert!(matches!(replies[1], Reply::Ack(_)), "{:?}", replies[1]);
    let Reply::Error(error) = &replies[2] else {
        panic!("the third went through");
    };
    assert_eq!(code(error), ErrorCode::RateLimited);

    let other = open_view(&mut w.member, w.channel_id).await.view_id;
    let burst = format!(r#"{{"count":20,"text":"{}"}}"#, "x".repeat(60_000));
    w.member.ok(send(other, "burst", &burst)).await;
    wait_for_log(
        &mut w.admin,
        PLUGIN,
        "burst sent 8 then RATE_LIMITED: too many events to this view",
    )
    .await;
    let third = open_view(&mut w.member, w.channel_id).await.view_id;
    w.member.ok(send(third, "big", "{}")).await;
    wait_for_log(
        &mut w.admin,
        PLUGIN,
        "big PLUGIN_LIMIT: an event is at most 64 KiB",
    )
    .await;
}

// Fails if send_role reaches a view whose user lacks the role, or the
// counts send_role, send_all, send_place and ui.views give are wrong.
#[tokio::test]
async fn send_role_reaches_only_holders() {
    let mut w = world().await;
    let role_id = match w
        .admin
        .ok(Kind::CreateRole(proto::CreateRole {
            name: "Helpers".to_owned(),
            ..Default::default()
        }))
        .await
    {
        Reply::Role(role) => role.id,
        other => panic!("not a role: {other:?}"),
    };
    w.admin
        .ok(Kind::AddMemberRole(proto::AddMemberRole {
            user_id: self_id(&w.member_ready),
            role_id,
        }))
        .await;
    let code = invite(&mut w.admin, None).await;
    let (mut other, _) = session(&w.server, 3, Some(&code), &[FEATURE]).await;
    open_view(&mut w.member, w.channel_id).await;
    open_view(&mut other, w.channel_id).await;
    let own = open_view(&mut w.admin, w.channel_id).await.view_id;
    w.admin
        .ok(send(own, "role", &format!(r#"{{"role":"{role_id}"}}"#)))
        .await;
    wait_for_log(&mut w.admin, PLUGIN, "role 1 all 3 place 3 views 3 here 3").await;
    message(&mut w.member, "hi").await;
    let names = within(&mut other, {
        let mut names = Vec::new();
        move |event| match event {
            Event::ViewMessage(message) => {
                names.push(message.name.clone());
                (message.name == "place").then(|| names.clone())
            }
            _ => None,
        }
    })
    .await;
    assert_eq!(names, ["all", "place"]);
}

// Fails if a session that ends (not one that only lost its connection)
// leaves its views open.
#[tokio::test]
async fn a_session_ending_closes_its_views() {
    let mut w = world().await;
    let view = open_view(&mut w.member, w.channel_id).await.view_id;
    wait_for_log(&mut w.admin, PLUGIN, &format!("open {view}")).await;
    w.server
        .handle
        .drop_user_connections(self_id(&w.member_ready));
    w.server.handle.reap_sessions_after(Duration::from_secs(1));
    tokio::time::sleep(Duration::from_millis(300)).await;
    wait_for_count(&mut w.admin, "close ", 0).await;
    w.server.handle.reap_sessions_after(Duration::from_secs(61));
    wait_for_log(&mut w.admin, PLUGIN, &format!("close {view} disconnected")).await;
}

// Fails if a file token is issued for a missing file, or works for a
// member other than the view's user.
#[tokio::test]
async fn a_file_token_works_for_its_views_user_only() {
    let mut w = world().await;
    let view = open_view(&mut w.member, w.channel_id).await.view_id;
    w.member.ok(send(view, "token", "{}")).await;
    let token = message(&mut w.member, "token").await;
    let data: serde_json::Value = serde_json::from_str(&token.data_json).unwrap();
    let path = format!("/plugins/{PLUGIN}/files/t/{}", data["t"].as_str().unwrap());
    let bearer = |ready: &proto::Ready| format!("Bearer {}", media_token(ready));
    let (status, body) = w
        .server
        .send_http(
            "GET",
            &path,
            &[("Authorization", &bearer(&w.member_ready))],
            b"",
        )
        .await;
    assert_eq!(
        (status, body.as_slice()),
        (200, b"private bytes".as_slice())
    );
    let (status, _) = w
        .server
        .send_http(
            "GET",
            &path,
            &[("Authorization", &bearer(&w.admin_ready))],
            b"",
        )
        .await;
    assert_eq!(status, 404);
    wait_for_log(
        &mut w.admin,
        PLUGIN,
        "missing NOT_FOUND: no plugin file at missing.png",
    )
    .await;
}

#[tokio::test]
async fn an_update_tells_open_views() {
    // Fails if updating a plugin does not send ViewScriptsUpdate to sessions with an open view.
    let mut world = world().await;
    let _view = open_view(&mut world.member, world.channel_id).await;

    // Admin updates plugin with client v2
    let zip = package(&[
        (
            "manifest.lua",
            &manifest_with(
                PLUGIN,
                "1.0.1",
                &["create_channel_types"],
                "client = \"client/v2.luau\",",
            ),
        ),
        ("server/main.luau", SERVER),
        (
            "client/v2.luau",
            "return { init = function() return { updated = true } end }",
        ),
        ("assets/logo.png", LOGO),
    ]);
    install(
        &world.server,
        &mut world.admin,
        &media_token(&world.admin_ready),
        zip,
        &["create_channel_types"],
    )
    .await;

    // Member has the view open -> gets ViewScriptsUpdate
    let mut saw_update = false;
    for _ in 0..10 {
        let event = world.member.next_event().await;
        if let Event::ViewScriptsUpdate(update) = event {
            assert_eq!(update.plugin_id, PLUGIN);
            assert_eq!(update.entry, "client/v2.luau");
            assert!(update.files.iter().any(|f| f.path == "client/v2.luau"));
            saw_update = true;
            break;
        }
    }
    assert!(saw_update, "open view never received ViewScriptsUpdate");

    // Admin has no view open -> no ViewScriptsUpdate
    let admin_events = world.admin.collect_for(Duration::from_millis(200)).await;
    assert!(
        !admin_events
            .iter()
            .any(|e| matches!(e, Event::ViewScriptsUpdate(_))),
        "admin received ViewScriptsUpdate: {admin_events:?}"
    );
}

#[tokio::test]
async fn a_view_error_reaches_the_log() {
    // Fails if ReportViewError does not log "view error for {user name}: {text}" to the plugin log.
    let mut world = world().await;
    let view = open_view(&mut world.member, world.channel_id).await;

    world
        .member
        .ok(Kind::ReportViewError(proto::ReportViewError {
            view_id: view.view_id,
            text: "test view error at line 10".to_owned(),
        }))
        .await;

    let member_name = world
        .member_ready
        .self_user
        .as_ref()
        .map(|u| u.display_name.clone())
        .unwrap();

    let expected = format!("view error for {member_name}: test view error at line 10");
    wait_for_log(&mut world.admin, PLUGIN, &expected).await;

    // Check cutting at 2000 characters
    let long_text = "x".repeat(2500);
    let view2 = open_view(&mut world.member, world.channel_id).await;
    world
        .member
        .ok(Kind::ReportViewError(proto::ReportViewError {
            view_id: view2.view_id,
            text: long_text.clone(),
        }))
        .await;
    let expected_cut: String = format!("view error for {member_name}: {long_text}")
        .chars()
        .take(2000)
        .collect();
    wait_for_log(&mut world.admin, PLUGIN, &expected_cut).await;
}

#[tokio::test]
async fn a_second_report_within_10_seconds_is_dropped() {
    // Fails if a second ReportViewError within 10s is not dropped or returns an error.
    let mut world = world().await;
    let view = open_view(&mut world.member, world.channel_id).await;

    world
        .member
        .ok(Kind::ReportViewError(proto::ReportViewError {
            view_id: view.view_id,
            text: "first error".to_owned(),
        }))
        .await;
    wait_for_log(&mut world.admin, PLUGIN, "first error").await;

    // Second error on the same view immediately
    world
        .member
        .ok(Kind::ReportViewError(proto::ReportViewError {
            view_id: view.view_id,
            text: "second error".to_owned(),
        }))
        .await;

    tokio::time::sleep(Duration::from_millis(300)).await;
    let lines = log_lines(&mut world.admin, PLUGIN).await;
    assert!(!lines.iter().any(|l| l.contains("second error")));
}

#[tokio::test]
async fn another_sessions_view_cant_report() {
    // Fails if ReportViewError for another session's view does not return NotFound.
    let mut world = world().await;
    let view = open_view(&mut world.member, world.channel_id).await;

    let err = world
        .admin
        .error(Kind::ReportViewError(proto::ReportViewError {
            view_id: view.view_id,
            text: "rogue report".to_owned(),
        }))
        .await;
    assert_eq!(err.code, ErrorCode::NotFound as i32);
    assert_eq!(err.message, "view not found");
}
