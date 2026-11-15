// SPDX-License-Identifier: MPL-2.0

//! Uploads from plugin views: a member's file reaching the plugin, which
//! keeps it with `files.put` or lets it go after an hour (plan M2 Task 12,
//! design 2026-11-07 §13.3).

use std::time::Duration;

use opencord_common::permissions::Permissions;
use opencord_common::upload::EVERYONE_DEFAULT_BYTES;
use opencord_proto::v1 as proto;
use proto::ErrorCode;
use proto::event::Kind as Event;
use proto::request::Kind;
use proto::response::Result as Reply;

use crate::common::{
    TestClient, TestServer, invite, media_token, owner, png, self_id, upload as message_upload,
};
use crate::support::{install, manifest_with, package, session, wait_for_log};

const FEATURE: &str = "plugin_channels";
const PLUGIN: &str = "drop";
const HOUR_MS: i64 = 3_600_000;
const SERVER: &str = r#"
ui.register_channel_type("box", { name = "Box" })
local heard = {}
opencord.on("view_open", function(event)
    log.info("open " .. event.view.id)
end)
opencord.on("file_uploaded", function(event)
    local file = event.file
    heard[file.id] = file
    log.info("uploaded " .. file.id .. " " .. file.name .. " " .. file.size .. " " .. file.content_type .. " by " .. event.view.user.id .. " in " .. event.view.id)
    if file.name == "later.bin" then
        return
    end
    files.put("kept/" .. file.name, file, { public = true })
    local _, again = pcall(files.put, "again/" .. file.name, file)
    log.info("again " .. tostring(again))
    event.view:send("kept", { id = file.id })
end)
ui.on("keep", function(view, data)
    local _, err = pcall(files.put, "late/" .. data.id, heard[data.id] or { id = data.id })
    log.info("keep " .. data.id .. " " .. tostring(err))
end)
opencord.on("start", function()
    local _, err = pcall(files.put, "forged.bin", { id = "1" })
    log.info("forged " .. tostring(err))
    log.info("started")
end)
"#;

/// A server with the drop plugin running, granted `granted`, a `box`
/// channel of it, the owner (`admin`, seed 1) and a member (seed 2), both
/// listing the feature.
struct World {
    server: TestServer,
    _owner: TestClient,
    admin: TestClient,
    admin_ready: proto::Ready,
    member: TestClient,
    member_ready: proto::Ready,
    channel_id: i64,
}

async fn world(granted: &[&str]) -> World {
    let server = TestServer::start().await;
    let (_owner, _) = owner(&server).await;
    let (mut admin, admin_ready) = session(&server, 1, None, &[FEATURE]).await;
    let zip = package(&[
        (
            "manifest.lua",
            &manifest_with(
                PLUGIN,
                "1.0.0",
                &["create_channel_types", "accept_uploads"],
                r#"client = "client/main.luau","#,
            ),
        ),
        ("server/main.luau", SERVER),
        (
            "client/main.luau",
            "return { init = function() return {} end }",
        ),
    ]);
    install(
        &server,
        &mut admin,
        &media_token(&admin_ready),
        zip,
        granted,
    )
    .await;
    wait_for_log(&mut admin, PLUGIN, "started").await;
    let channel_id = match admin
        .ok(Kind::CreateChannel(proto::CreateChannel {
            kind: proto::ChannelKind::Plugin as i32,
            name: "Box".to_owned(),
            plugin_id: Some(PLUGIN.to_owned()),
            type_key: Some("box".to_owned()),
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

const BOTH: [&str; 2] = ["create_channel_types", "accept_uploads"];

/// Opens a view of the channel, and waits until the plugin heard it open.
async fn open_view(w: &mut World, client_is_admin: bool) -> i64 {
    let request = Kind::OpenView(proto::OpenView {
        place: Some(proto::ViewPlace {
            kind: Some(proto::view_place::Kind::ChannelId(w.channel_id)),
        }),
        platform: "desktop".to_owned(),
    });
    let client = if client_is_admin {
        &mut w.admin
    } else {
        &mut w.member
    };
    let view_id = match client.ok(request).await {
        Reply::ViewOpened(opened) => opened.view_id,
        other => panic!("not opened: {other:?}"),
    };
    wait_for_log(&mut w.admin, PLUGIN, &format!("open {view_id}")).await;
    view_id
}

fn create(channel_id: i64, view_id: Option<i64>, name: &str, size: u64) -> Kind {
    Kind::CreateUpload(proto::CreateUpload {
        channel_id,
        filename: name.to_owned(),
        size_bytes: size,
        kind: proto::AttachmentKind::File.into(),
        width: None,
        height: None,
        duration_ms: None,
        view_id,
    })
}

/// Starts an upload for the view: its id, or why it was refused.
async fn view_upload(
    client: &mut TestClient,
    view_id: i64,
    name: &str,
    size: u64,
) -> Result<i64, proto::Error> {
    match client.request(create(0, Some(view_id), name, size)).await {
        Reply::Upload(upload) => Ok(upload.upload_id),
        Reply::Error(error) => Err(error),
        other => panic!("not an upload: {other:?}"),
    }
}

/// Sends every byte of the upload and finishes it.
async fn send_and_finish(server: &TestServer, token: &str, upload_id: i64, bytes: &[u8]) {
    let bearer = format!("Bearer {token}");
    let (status, _) = server
        .send_http(
            "PUT",
            &format!("/media/uploads/{upload_id}"),
            &[("Authorization", &bearer), ("Upload-Offset", "0")],
            bytes,
        )
        .await;
    assert_eq!(status, 200);
    let (status, _) = server
        .send_http(
            "POST",
            &format!("/media/uploads/{upload_id}/finish"),
            &[("Authorization", &bearer)],
            &[],
        )
        .await;
    assert_eq!(status, 202);
}

/// Waits up to 10 s for the upload to be ready.
async fn ready(client: &mut TestClient, upload_id: i64) {
    tokio::time::timeout(
        Duration::from_secs(10),
        client.wait_for(|event| match event {
            Event::UploadReady(ready) => ready
                .attachment
                .as_ref()
                .filter(|attachment| attachment.id == upload_id)
                .map(|_| ()),
            _ => None,
        }),
    )
    .await
    .expect("the upload never became ready");
}

/// The status `GET /media/uploads/{id}` answers with `token`.
async fn upload_status(server: &TestServer, token: &str, upload_id: i64) -> u16 {
    server
        .send_http(
            "GET",
            &format!("/media/uploads/{upload_id}"),
            &[("Authorization", &format!("Bearer {token}"))],
            &[],
        )
        .await
        .0
}

fn keep(view_id: i64, upload_id: i64) -> Kind {
    Kind::SendViewEvent(proto::SendViewEvent {
        view_id,
        name: "keep".to_owned(),
        data_json: format!(r#"{{"id":"{upload_id}"}}"#),
    })
}

fn code(error: &proto::Error) -> ErrorCode {
    ErrorCode::try_from(error.code).unwrap()
}

fn text_channel(ready: &proto::Ready) -> i64 {
    ready
        .channels
        .iter()
        .find(|channel| channel.kind == proto::ChannelKind::Text as i32)
        .expect("a text channel")
        .id
}

// Fails if a view upload isn't announced as `file_uploaded` with its view
// and file, if `files.put` copies other bytes than were sent (a read past
// the first batch of segments, a short last one), if it keeps one upload
// twice, or if the upload outlives being kept.
#[tokio::test]
async fn an_upload_reaches_the_plugin_and_can_be_kept() {
    let mut w = world(&BOTH).await;
    let token = media_token(&w.member_ready);
    let view = open_view(&mut w, false).await;
    // A picture with more after it than one read batch (16 segments).
    let mut bytes = png(8, 8);
    bytes.extend((0..1_300_000u32).map(|n| (n % 251) as u8));

    let upload_id = view_upload(&mut w.member, view, "photo.png", bytes.len() as u64)
        .await
        .unwrap();
    send_and_finish(&w.server, &token, upload_id, &bytes).await;
    let kept = tokio::time::timeout(
        Duration::from_secs(10),
        w.member.wait_for(|event| match event {
            Event::ViewMessage(message) if message.name == "kept" => Some(message.clone()),
            _ => None,
        }),
    )
    .await
    .expect("the plugin never kept it");
    assert_eq!(kept.view_id, view);
    assert_eq!(kept.data_json, format!(r#"{{"id":"{upload_id}"}}"#));

    let member = self_id(&w.member_ready);
    let size = bytes.len();
    wait_for_log(
        &mut w.admin,
        PLUGIN,
        &format!("uploaded {upload_id} photo.png {size} image/png by {member} in {view}"),
    )
    .await;
    let (status, _, served) = w
        .server
        .send_http_full(
            "GET",
            &format!("/plugins/{PLUGIN}/files/kept/photo.png"),
            &[("Authorization", &format!("Bearer {token}"))],
            b"",
        )
        .await;
    assert_eq!(status, 200);
    assert!(served == bytes, "the kept file differs from the upload");
    wait_for_log(
        &mut w.admin,
        PLUGIN,
        "again NOT_FOUND: no upload with that id",
    )
    .await;
    assert_eq!(upload_status(&w.server, &token, upload_id).await, 404);
}

// Fails if a view upload skips the blocked extensions, the member's size
// limit or the channel's ATTACH_FILES, or is counted as anyone's but the
// member's.
#[tokio::test]
async fn blocked_extensions_and_size_limits_apply() {
    let mut w = world(&BOTH).await;
    let view = open_view(&mut w, false).await;
    w.admin
        .ok(Kind::UpdateUploadSettings(proto::UpdateUploadSettings {
            blocked_extensions: Some(proto::BlockedExtensions {
                extensions: vec!["exe".to_owned()],
            }),
        }))
        .await;
    let blocked = view_upload(&mut w.member, view, "setup.exe", 10)
        .await
        .unwrap_err();
    assert_eq!(code(&blocked), ErrorCode::FileTypeBlocked, "{blocked:?}");

    let too_large = view_upload(&mut w.member, view, "big.bin", EVERYONE_DEFAULT_BYTES + 1)
        .await
        .unwrap_err();
    assert_eq!(code(&too_large), ErrorCode::FileTooLarge, "{too_large:?}");
    let at_limit = view_upload(&mut w.member, view, "big.bin", EVERYONE_DEFAULT_BYTES)
        .await
        .unwrap();
    // The member's own upload: theirs to read, nobody else's.
    assert_eq!(
        upload_status(&w.server, &media_token(&w.member_ready), at_limit).await,
        200
    );
    assert_eq!(
        upload_status(&w.server, &media_token(&w.admin_ready), at_limit).await,
        404
    );

    let everyone = w.admin_ready.server.as_ref().unwrap().everyone_role_id;
    w.admin
        .ok(Kind::SetChannelOverwrite(proto::SetChannelOverwrite {
            channel_id: w.channel_id,
            overwrite: Some(proto::PermissionOverwrite {
                target_kind: proto::OverwriteTarget::Role as i32,
                target_id: everyone,
                allow: 0,
                deny: Permissions::ATTACH_FILES.bits(),
            }),
        }))
        .await;
    let no_files = view_upload(&mut w.member, view, "note.txt", 10)
        .await
        .unwrap_err();
    assert_eq!(code(&no_files), ErrorCode::Forbidden, "{no_files:?}");
}

// Fails if a plugin without accept_uploads is sent uploads, or can keep
// one with files.put.
#[tokio::test]
async fn without_accept_uploads_it_is_refused() {
    let mut w = world(&["create_channel_types"]).await;
    let view = open_view(&mut w, false).await;
    let refused = view_upload(&mut w.member, view, "note.txt", 10)
        .await
        .unwrap_err();
    assert_eq!(code(&refused), ErrorCode::Forbidden, "{refused:?}");
    assert_eq!(refused.message, "this plugin doesn't take uploads");
    wait_for_log(
        &mut w.admin,
        PLUGIN,
        "forged PERMISSION_DENIED: the plugin doesn't have accept_uploads",
    )
    .await;
}

// Fails if an upload can name a view of another member, of the member's
// other session, or one that closed, or name a channel and a view at once.
#[tokio::test]
async fn another_sessions_view_cant_take_an_upload() {
    let mut w = world(&BOTH).await;
    let admins = open_view(&mut w, true).await;
    let theirs = view_upload(&mut w.member, admins, "note.txt", 10)
        .await
        .unwrap_err();
    assert_eq!(code(&theirs), ErrorCode::NotFound, "{theirs:?}");

    let own = open_view(&mut w, false).await;
    let (mut other_session, _) = session(&w.server, 2, None, &[FEATURE]).await;
    let other = view_upload(&mut other_session, own, "note.txt", 10)
        .await
        .unwrap_err();
    assert_eq!(code(&other), ErrorCode::NotFound, "{other:?}");

    let both = w
        .member
        .error(create(
            text_channel(&w.member_ready),
            Some(own),
            "a.txt",
            10,
        ))
        .await;
    assert_eq!(code(&both), ErrorCode::InvalidArgument, "{both:?}");

    view_upload(&mut w.member, own, "note.txt", 10)
        .await
        .unwrap();
    w.member
        .ok(Kind::CloseView(proto::CloseView { view_id: own }))
        .await;
    let closed = view_upload(&mut w.member, own, "note.txt", 10)
        .await
        .unwrap_err();
    assert_eq!(code(&closed), ErrorCode::NotFound, "{closed:?}");
}

// Fails if files.put keeps an upload that isn't a finished upload for one
// of this plugin's views: a member's unsent message file, a view upload
// still arriving, or an id the plugin made up.
#[tokio::test]
async fn only_its_own_view_uploads_can_be_kept() {
    let mut w = world(&BOTH).await;
    let token = media_token(&w.member_ready);
    wait_for_log(
        &mut w.admin,
        PLUGIN,
        "forged NOT_FOUND: no upload with that id",
    )
    .await;
    let view = open_view(&mut w, false).await;
    let text = text_channel(&w.member_ready);
    let message_file = message_upload(&mut w.member, &w.server, &token, text, "a.txt", b"hi")
        .await
        .id;
    w.member.ok(keep(view, message_file)).await;
    wait_for_log(
        &mut w.admin,
        PLUGIN,
        &format!("keep {message_file} NOT_FOUND: no upload with that id"),
    )
    .await;
    assert_eq!(upload_status(&w.server, &token, message_file).await, 200);

    let arriving = view_upload(&mut w.member, view, "later.bin", 10)
        .await
        .unwrap();
    w.member.ok(keep(view, arriving)).await;
    wait_for_log(
        &mut w.admin,
        PLUGIN,
        &format!("keep {arriving} NOT_FOUND: no upload with that id"),
    )
    .await;
}

// Fails if an unkept view upload stays past an hour (the sweep's rule for
// ready uploads is a day), if the hour cuts message uploads short, or if
// files.put still keeps one that is over an hour old but not swept yet.
#[tokio::test]
async fn an_unkept_upload_is_deleted_after_an_hour() {
    let mut w = world(&BOTH).await;
    let token = media_token(&w.member_ready);
    let view = open_view(&mut w, false).await;
    let text = text_channel(&w.member_ready);

    let unkept = view_upload(&mut w.member, view, "later.bin", 4)
        .await
        .unwrap();
    send_and_finish(&w.server, &token, unkept, b"left").await;
    ready(&mut w.member, unkept).await;
    wait_for_log(
        &mut w.admin,
        PLUGIN,
        &format!("uploaded {unkept} later.bin"),
    )
    .await;
    let message_file = message_upload(&mut w.member, &w.server, &token, text, "a.txt", b"hi")
        .await
        .id;

    let now = opencord_server::state::now_ms();
    w.server
        .handle
        .sweep_attachments(now + HOUR_MS - 60_000)
        .await;
    assert_eq!(upload_status(&w.server, &token, unkept).await, 200);
    w.server
        .handle
        .sweep_attachments(now + HOUR_MS + 1_000)
        .await;
    assert_eq!(upload_status(&w.server, &token, unkept).await, 404);
    assert_eq!(upload_status(&w.server, &token, message_file).await, 200);
    w.member.ok(keep(view, unkept)).await;
    wait_for_log(
        &mut w.admin,
        PLUGIN,
        &format!("keep {unkept} NOT_FOUND: no upload with that id"),
    )
    .await;

    // Over an hour old, before the sweep comes by.
    let aged = view_upload(&mut w.member, view, "later.bin", 4)
        .await
        .unwrap();
    send_and_finish(&w.server, &token, aged, b"aged").await;
    ready(&mut w.member, aged).await;
    wait_for_log(&mut w.admin, PLUGIN, &format!("uploaded {aged} later.bin")).await;
    sqlx::query("UPDATE attachments SET updated_at = updated_at - $1 WHERE id = $2")
        .bind(HOUR_MS + 1_000)
        .bind(aged)
        .execute(&w.server.state().db)
        .await
        .unwrap();
    w.member.ok(keep(view, aged)).await;
    wait_for_log(
        &mut w.admin,
        PLUGIN,
        &format!("keep {aged} NOT_FOUND: no upload with that id"),
    )
    .await;
}
