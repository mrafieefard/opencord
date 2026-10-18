mod common;

use common::*;
use opencord_proto::v1 as proto;
use proto::request::Kind as Req;
use proto::response::Result as Res;

#[allow(dead_code)]
struct Room {
    server: TestServer,
    owner: TestClient,
    owner_token: String,
    member: TestClient,
    member_id: i64,
    member_token: String,
    text: i64,
    everyone: i64,
    invite: String,
}

async fn room() -> Room {
    let server = TestServer::start().await;
    let (mut owner, owner_ready) = owner(&server).await;
    let invite = invite(&mut owner, None).await;
    let (member, member_ready) = join(&server, 2, &invite).await;
    Room {
        text: channel_id(&owner_ready, "general"),
        everyone: owner_ready.server.as_ref().unwrap().everyone_role_id,
        owner_token: media_token(&owner_ready),
        member_id: self_id(&member_ready),
        member_token: media_token(&member_ready),
        server,
        owner,
        member,
        invite,
    }
}

fn media_token(ready: &proto::Ready) -> String {
    ready.media_token.as_ref().unwrap().token.clone()
}

#[tokio::test]
async fn create_upload_checks_permissions_and_size() {
    let Room {
        mut owner,
        mut member,
        everyone,
        text,
        ..
    } = room().await;

    // Owner removes ATTACH_FILES from @everyone.
    owner
        .ok(Req::UpdateRole(proto::UpdateRole {
            role_id: everyone,
            permissions: Some(
                (opencord_common::permissions::Permissions::DEFAULT_EVERYONE
                    - opencord_common::permissions::Permissions::ATTACH_FILES)
                    .bits(),
            ),
            ..Default::default()
        }))
        .await;

    // A member without ATTACH_FILES gets FORBIDDEN.
    let error = member
        .error(Req::CreateUpload(proto::CreateUpload {
            channel_id: text,
            filename: "photo.png".into(),
            size_bytes: 1024,
            kind: proto::AttachmentKind::File.into(),
            width: None,
            height: None,
            duration_ms: None,
        }))
        .await;
    assert_eq!(error.code, proto::ErrorCode::Forbidden as i32);

    // Restore ATTACH_FILES to @everyone.
    owner
        .ok(Req::UpdateRole(proto::UpdateRole {
            role_id: everyone,
            permissions: Some(opencord_common::permissions::Permissions::DEFAULT_EVERYONE.bits()),
            ..Default::default()
        }))
        .await;

    // With it: 60 MiB -> FILE_TOO_LARGE with "50 MB" in the message.
    let error = member
        .error(Req::CreateUpload(proto::CreateUpload {
            channel_id: text,
            filename: "big.zip".into(),
            size_bytes: 60 * 1024 * 1024,
            kind: proto::AttachmentKind::File.into(),
            width: None,
            height: None,
            duration_ms: None,
        }))
        .await;
    assert_eq!(error.code, proto::ErrorCode::FileTooLarge as i32);
    assert!(
        error.message.contains("50 MB"),
        "message was: {}",
        error.message
    );

    // 10 unfinished uploads succeed (consumes 10 upload rate limit tokens).
    let mut ids = Vec::new();
    for i in 0..10 {
        let res = member
            .ok(Req::CreateUpload(proto::CreateUpload {
                channel_id: text,
                filename: format!("file{i}.txt"),
                size_bytes: 1024,
                kind: proto::AttachmentKind::File.into(),
                width: None,
                height: None,
                duration_ms: None,
            }))
            .await;
        match res {
            Res::Upload(u) => ids.push(u.upload_id),
            other => panic!("expected Upload, got {other:?}"),
        }
    }

    // An 11th unfinished upload -> CONFLICT (consumes the 11th upload rate limit token).
    let error = member
        .error(Req::CreateUpload(proto::CreateUpload {
            channel_id: text,
            filename: "file11.txt".into(),
            size_bytes: 1024,
            kind: proto::AttachmentKind::File.into(),
            width: None,
            height: None,
            duration_ms: None,
        }))
        .await;
    assert_eq!(error.code, proto::ErrorCode::Conflict as i32);

    // Cancel all 10 unfinished uploads so count_unfinished drops to 0.
    for upload_id in ids {
        member
            .ok(Req::CancelUpload(proto::CancelUpload { upload_id }))
            .await;
    }

    // Now test rate limit: 20 per minute.
    // 11 creates have already consumed tokens. Create and cancel 9 more to reach 20:
    for i in 11..20 {
        let res = member
            .ok(Req::CreateUpload(proto::CreateUpload {
                channel_id: text,
                filename: format!("file{i}.txt"),
                size_bytes: 1024,
                kind: proto::AttachmentKind::File.into(),
                width: None,
                height: None,
                duration_ms: None,
            }))
            .await;
        let upload_id = match res {
            Res::Upload(u) => u.upload_id,
            other => panic!("expected Upload, got {other:?}"),
        };
        member
            .ok(Req::CancelUpload(proto::CancelUpload { upload_id }))
            .await;
    }

    // A 21st create within a minute -> RATE_LIMITED.
    let error = member
        .error(Req::CreateUpload(proto::CreateUpload {
            channel_id: text,
            filename: "file21.txt".into(),
            size_bytes: 1024,
            kind: proto::AttachmentKind::File.into(),
            width: None,
            height: None,
            duration_ms: None,
        }))
        .await;
    assert_eq!(error.code, proto::ErrorCode::RateLimited as i32);
}

#[tokio::test]
async fn pieces_must_arrive_in_order_and_have_the_right_size() {
    let Room {
        server,
        mut member,
        member_token,
        text,
        ..
    } = room().await;

    let total_size = 9 * 1024 * 1024; // 9 MiB
    let res = member
        .ok(Req::CreateUpload(proto::CreateUpload {
            channel_id: text,
            filename: "video.mp4".into(),
            size_bytes: total_size,
            kind: proto::AttachmentKind::File.into(),
            width: None,
            height: None,
            duration_ms: None,
        }))
        .await;
    let upload = match res {
        Res::Upload(u) => u,
        other => panic!("expected Upload, got {other:?}"),
    };
    let upload_id = upload.upload_id;
    let auth = format!("Bearer {member_token}");

    // PUT offset 8 MiB first -> 409 with received_bytes == 0
    let piece2 = vec![0xaa; 1024 * 1024]; // 1 MiB
    let (status, bytes) = server
        .send_http(
            "PUT",
            &format!("/media/uploads/{upload_id}"),
            &[("Authorization", &auth), ("Upload-Offset", "8388608")],
            &piece2,
        )
        .await;
    assert_eq!(status, 409);
    match decode_response(&bytes) {
        Res::UploadStatus(s) => {
            assert_eq!(s.received_bytes, 0);
            assert_eq!(s.state, proto::UploadState::Uploading as i32);
        }
        other => panic!("expected UploadStatus, got {other:?}"),
    }

    // A 1-byte-short first piece -> 400
    let short_piece = vec![0xbb; 8 * 1024 * 1024 - 1];
    let (status, _) = server
        .send_http(
            "PUT",
            &format!("/media/uploads/{upload_id}"),
            &[("Authorization", &auth), ("Upload-Offset", "0")],
            &short_piece,
        )
        .await;
    assert_eq!(status, 400);

    // The correct pieces -> 200 with offsets 8 MiB then 9 MiB.
    let piece1 = vec![0x11; 8 * 1024 * 1024];
    let (status, bytes) = server
        .send_http(
            "PUT",
            &format!("/media/uploads/{upload_id}"),
            &[("Authorization", &auth), ("Upload-Offset", "0")],
            &piece1,
        )
        .await;
    assert_eq!(status, 200);
    match decode_response(&bytes) {
        Res::UploadStatus(s) => assert_eq!(s.received_bytes, 8 * 1024 * 1024),
        other => panic!("expected UploadStatus, got {other:?}"),
    }

    let (status, bytes) = server
        .send_http(
            "PUT",
            &format!("/media/uploads/{upload_id}"),
            &[("Authorization", &auth), ("Upload-Offset", "8388608")],
            &piece2,
        )
        .await;
    assert_eq!(status, 200);
    match decode_response(&bytes) {
        Res::UploadStatus(s) => assert_eq!(s.received_bytes, 9 * 1024 * 1024),
        other => panic!("expected UploadStatus, got {other:?}"),
    }
}

#[tokio::test]
async fn a_repeated_piece_gets_a_conflict_and_the_upload_continues() {
    let Room {
        server,
        mut member,
        member_token,
        text,
        ..
    } = room().await;

    let total_size = 9 * 1024 * 1024;
    let res = member
        .ok(Req::CreateUpload(proto::CreateUpload {
            channel_id: text,
            filename: "file.bin".into(),
            size_bytes: total_size,
            kind: proto::AttachmentKind::File.into(),
            width: None,
            height: None,
            duration_ms: None,
        }))
        .await;
    let upload_id = match res {
        Res::Upload(u) => u.upload_id,
        other => panic!("expected Upload, got {other:?}"),
    };
    let auth = format!("Bearer {member_token}");

    let piece1 = vec![0x42; 8 * 1024 * 1024];
    let piece2 = vec![0x43; 1024 * 1024];

    // PUT piece 1 first time -> 200
    let (status, _) = server
        .send_http(
            "PUT",
            &format!("/media/uploads/{upload_id}"),
            &[("Authorization", &auth), ("Upload-Offset", "0")],
            &piece1,
        )
        .await;
    assert_eq!(status, 200);

    // PUT piece 1 twice -> second is 409 with offset 8 MiB
    let (status, bytes) = server
        .send_http(
            "PUT",
            &format!("/media/uploads/{upload_id}"),
            &[("Authorization", &auth), ("Upload-Offset", "0")],
            &piece1,
        )
        .await;
    assert_eq!(status, 409);
    match decode_response(&bytes) {
        Res::UploadStatus(s) => assert_eq!(s.received_bytes, 8 * 1024 * 1024),
        other => panic!("expected UploadStatus, got {other:?}"),
    }

    // PUT piece 2 -> 200
    let (status, bytes) = server
        .send_http(
            "PUT",
            &format!("/media/uploads/{upload_id}"),
            &[("Authorization", &auth), ("Upload-Offset", "8388608")],
            &piece2,
        )
        .await;
    assert_eq!(status, 200);
    match decode_response(&bytes) {
        Res::UploadStatus(s) => assert_eq!(s.received_bytes, 9 * 1024 * 1024),
        other => panic!("expected UploadStatus, got {other:?}"),
    }
}

#[tokio::test]
async fn another_users_upload_is_not_found() {
    let Room {
        server,
        mut owner,
        owner_token,
        member_token,
        text,
        ..
    } = room().await;

    let res = owner
        .ok(Req::CreateUpload(proto::CreateUpload {
            channel_id: text,
            filename: "secret.txt".into(),
            size_bytes: 100,
            kind: proto::AttachmentKind::File.into(),
            width: None,
            height: None,
            duration_ms: None,
        }))
        .await;
    let upload_id = match res {
        Res::Upload(u) => u.upload_id,
        other => panic!("expected Upload, got {other:?}"),
    };

    // Member tries to GET /media/uploads/{upload_id} -> 404
    let (status, _) = server
        .send_http(
            "GET",
            &format!("/media/uploads/{upload_id}"),
            &[("Authorization", &format!("Bearer {member_token}"))],
            &[],
        )
        .await;
    assert_eq!(status, 404);

    // Member tries to PUT /media/uploads/{upload_id} -> 404
    let (status, _) = server
        .send_http(
            "PUT",
            &format!("/media/uploads/{upload_id}"),
            &[
                ("Authorization", &format!("Bearer {member_token}")),
                ("Upload-Offset", "0"),
            ],
            &[0u8; 100],
        )
        .await;
    assert_eq!(status, 404);

    // Owner can GET it -> 200
    let (status, _) = server
        .send_http(
            "GET",
            &format!("/media/uploads/{upload_id}"),
            &[("Authorization", &format!("Bearer {owner_token}"))],
            &[],
        )
        .await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn cancel_removes_the_row_and_temp_file() {
    let Room {
        server,
        mut member,
        member_token,
        text,
        ..
    } = room().await;

    let res = member
        .ok(Req::CreateUpload(proto::CreateUpload {
            channel_id: text,
            filename: "temp.txt".into(),
            size_bytes: 100,
            kind: proto::AttachmentKind::File.into(),
            width: None,
            height: None,
            duration_ms: None,
        }))
        .await;
    let upload_id = match res {
        Res::Upload(u) => u.upload_id,
        other => panic!("expected Upload, got {other:?}"),
    };

    let temp_file = server
        .dir
        .path()
        .join("data")
        .join("uploads")
        .join(format!("{upload_id}.part"));
    assert!(temp_file.exists(), "temp file must exist after create");

    // Member cancels the upload
    member
        .ok(Req::CancelUpload(proto::CancelUpload { upload_id }))
        .await;

    assert!(
        !temp_file.exists(),
        "temp file must be deleted after cancel"
    );

    // GET /media/uploads/{id} -> 404
    let (status, _) = server
        .send_http(
            "GET",
            &format!("/media/uploads/{upload_id}"),
            &[("Authorization", &format!("Bearer {member_token}"))],
            &[],
        )
        .await;
    assert_eq!(status, 404);

    // Canceling again -> NOT_FOUND
    let error = member
        .error(Req::CancelUpload(proto::CancelUpload { upload_id }))
        .await;
    assert_eq!(error.code, proto::ErrorCode::NotFound as i32);
}

#[tokio::test]
async fn nothing_plain_reaches_the_temp_folder() {
    let Room {
        server,
        mut member,
        member_token,
        text,
        ..
    } = room().await;

    let marker = "SUPER_SECRET_PLAINTEXT_MARKER_1234567890";
    let piece = marker.repeat(100).into_bytes();

    let res = member
        .ok(Req::CreateUpload(proto::CreateUpload {
            channel_id: text,
            filename: "plain.txt".into(),
            size_bytes: piece.len() as u64,
            kind: proto::AttachmentKind::File.into(),
            width: None,
            height: None,
            duration_ms: None,
        }))
        .await;
    let upload_id = match res {
        Res::Upload(u) => u.upload_id,
        other => panic!("expected Upload, got {other:?}"),
    };

    let (status, _) = server
        .send_http(
            "PUT",
            &format!("/media/uploads/{upload_id}"),
            &[
                ("Authorization", &format!("Bearer {member_token}")),
                ("Upload-Offset", "0"),
            ],
            &piece,
        )
        .await;
    assert_eq!(status, 200);

    let temp_file = server
        .dir
        .path()
        .join("data")
        .join("uploads")
        .join(format!("{upload_id}.part"));
    let disk_bytes = tokio::fs::read(&temp_file).await.unwrap();

    assert!(
        !disk_bytes
            .windows(marker.len())
            .any(|w| w == marker.as_bytes()),
        "temp file on disk must not contain the plaintext marker"
    );
}
