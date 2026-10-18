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

    // Finish the upload (Task 10 check)
    let (status, _) = server
        .send_http(
            "POST",
            &format!("/media/uploads/{upload_id}/finish"),
            &[("Authorization", &auth)],
            &[],
        )
        .await;
    assert_eq!(status, 202);

    let att = member
        .wait_for(|kind| match kind {
            proto::event::Kind::UploadReady(ready) => ready.attachment.clone(),
            _ => None,
        })
        .await;

    // Send a message with the attachment
    member
        .ok(Req::SendMessage(proto::SendMessage {
            channel_id: text,
            content: "file message".into(),
            attachment_ids: vec![att.id],
            ..Default::default()
        }))
        .await;

    // Download equals the original
    let (status, downloaded) = server
        .send_http(
            "GET",
            &format!("/media/attachments/{upload_id}"),
            &[("Authorization", &auth)],
            &[],
        )
        .await;
    assert_eq!(status, 200);
    let mut expected = piece1;
    expected.extend_from_slice(&piece2);
    assert_eq!(downloaded, expected);
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
async fn a_zero_byte_file_finishes_without_a_piece() {
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
            filename: "empty.txt".into(),
            size_bytes: 0,
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

    // Straight to finish: no empty PUT first.
    let (status, _) = server
        .send_http(
            "POST",
            &format!("/media/uploads/{upload_id}/finish"),
            &[("Authorization", &format!("Bearer {member_token}"))],
            &[],
        )
        .await;
    assert_eq!(status, 202);

    let att = member
        .wait_for(|kind| match kind {
            proto::event::Kind::UploadReady(ready) => ready.attachment.clone(),
            _ => None,
        })
        .await;
    assert_eq!(att.id, upload_id);
    assert_eq!(att.size_bytes, 0);
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

#[tokio::test]
async fn an_image_becomes_ready_with_a_thumbnail() {
    let Room {
        server,
        mut member,
        member_token,
        text,
        ..
    } = room().await;

    let img = png(1200, 900);
    let attachment = upload(&mut member, &server, &member_token, text, "large.png", &img).await;
    assert_eq!(attachment.kind, proto::AttachmentKind::Image as i32);
    assert_eq!(attachment.width, Some(1200));
    assert_eq!(attachment.height, Some(900));
    assert!(attachment.has_thumbnail);
    assert_eq!(attachment.thumbnail_width, Some(800));
    assert_eq!(attachment.thumbnail_height, Some(600));
}

#[tokio::test]
async fn a_zip_stays_a_file() {
    let Room {
        server,
        mut member,
        member_token,
        text,
        ..
    } = room().await;

    let mut zip_bytes = vec![0x50, 0x4B, 0x03, 0x04];
    zip_bytes.resize(512, 0);
    let attachment = upload(
        &mut member,
        &server,
        &member_token,
        text,
        "data.zip",
        &zip_bytes,
    )
    .await;
    assert_eq!(attachment.kind, proto::AttachmentKind::File as i32);
    assert!(!attachment.has_thumbnail);
}

#[tokio::test]
async fn a_voice_message_gets_the_server_waveform() {
    let Room {
        server,
        mut member,
        member_token,
        text,
        ..
    } = room().await;

    let ogg = voice_ogg(1.0, 1.0);
    let res = member
        .ok(Req::CreateUpload(proto::CreateUpload {
            channel_id: text,
            filename: "my-recording.ogg".into(),
            size_bytes: ogg.len() as u64,
            kind: proto::AttachmentKind::VoiceMessage.into(),
            width: None,
            height: None,
            duration_ms: None,
        }))
        .await;
    let upload = match res {
        Res::Upload(u) => u,
        other => panic!("expected Upload, got {other:?}"),
    };

    let auth = format!("Bearer {member_token}");
    let (status, _) = server
        .send_http(
            "PUT",
            &format!("/media/uploads/{}", upload.upload_id),
            &[("Authorization", &auth), ("Upload-Offset", "0")],
            &ogg,
        )
        .await;
    assert_eq!(status, 200);

    let (status, _) = server
        .send_http(
            "POST",
            &format!("/media/uploads/{}/finish", upload.upload_id),
            &[("Authorization", &auth)],
            &[],
        )
        .await;
    assert_eq!(status, 202);

    let attachment = member
        .wait_for(|kind| match kind {
            proto::event::Kind::UploadReady(ready) => ready.attachment.clone(),
            _ => None,
        })
        .await;

    assert_eq!(attachment.kind, proto::AttachmentKind::VoiceMessage as i32);
    assert_eq!(attachment.filename, "voice-message.ogg");
    assert_eq!(attachment.waveform.len(), 128);
    let duration = attachment.duration_ms.expect("duration_ms");
    assert!((1960..=2040).contains(&duration), "duration was {duration}");
}

#[tokio::test]
async fn odd_images_never_crash_processing() {
    let Room {
        server,
        mut member,
        member_token,
        text,
        ..
    } = room().await;

    fn crc32(buf: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &b in buf {
            crc ^= b as u32;
            for _ in 0..8 {
                crc = if (crc & 1) != 0 {
                    (crc >> 1) ^ 0xEDB88320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    // 1. PNG claiming 20,000x20,000 -> FILE
    let mut ihdr_data = Vec::new();
    ihdr_data.extend_from_slice(b"IHDR");
    ihdr_data.extend_from_slice(&20_000u32.to_be_bytes());
    ihdr_data.extend_from_slice(&20_000u32.to_be_bytes());
    ihdr_data.extend_from_slice(&[8, 2, 0, 0, 0]);
    let crc = crc32(&ihdr_data);

    let mut big_dim = Vec::new();
    big_dim.extend_from_slice(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
    big_dim.extend_from_slice(&13u32.to_be_bytes());
    big_dim.extend_from_slice(&ihdr_data);
    big_dim.extend_from_slice(&crc.to_be_bytes());
    let att = upload(
        &mut member,
        &server,
        &member_token,
        text,
        "huge.png",
        &big_dim,
    )
    .await;
    assert_eq!(att.kind, proto::AttachmentKind::File as i32);
    assert!(!att.has_thumbnail);

    // 2. Truncated PNG -> FILE
    let valid = png(100, 100);
    let truncated = &valid[..25];
    let att = upload(
        &mut member,
        &server,
        &member_token,
        text,
        "trunc.png",
        truncated,
    )
    .await;
    assert_eq!(att.kind, proto::AttachmentKind::File as i32);
    assert!(!att.has_thumbnail);

    // 3. 16-bit grey PNG -> IMAGE
    let grey16: image::ImageBuffer<image::Luma<u16>, Vec<u16>> =
        image::ImageBuffer::from_pixel(50, 50, image::Luma([40_000u16]));
    let mut grey_bytes = Vec::new();
    grey16
        .write_to(
            &mut std::io::Cursor::new(&mut grey_bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
    let att = upload(
        &mut member,
        &server,
        &member_token,
        text,
        "grey16.png",
        &grey_bytes,
    )
    .await;
    assert_eq!(att.kind, proto::AttachmentKind::Image as i32);
    assert_eq!(att.width, Some(50));
    assert_eq!(att.height, Some(50));

    // 4. 2-frame GIF -> animated == true
    let mut gif_bytes = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut gif_bytes);
        encoder
            .set_repeat(image::codecs::gif::Repeat::Infinite)
            .unwrap();
        let frame1 = image::Frame::new(image::RgbaImage::from_pixel(
            20,
            20,
            image::Rgba([255, 0, 0, 255]),
        ));
        let frame2 = image::Frame::new(image::RgbaImage::from_pixel(
            20,
            20,
            image::Rgba([0, 255, 0, 255]),
        ));
        encoder.encode_frame(frame1).unwrap();
        encoder.encode_frame(frame2).unwrap();
    }
    let att = upload(
        &mut member,
        &server,
        &member_token,
        text,
        "anim.gif",
        &gif_bytes,
    )
    .await;
    assert_eq!(att.kind, proto::AttachmentKind::Image as i32);
    assert!(att.animated);
}

#[tokio::test]
async fn a_storage_failure_fails_the_upload_cleanly() {
    let temp_file_as_dir = tempfile::NamedTempFile::new().unwrap();
    let file_path = temp_file_as_dir.path().to_path_buf();

    let server = TestServer::start_with(|config| {
        config.uploads.local_dir = Some(file_path);
    })
    .await;

    let (mut owner, owner_ready) = owner(&server).await;
    let invite = invite(&mut owner, None).await;
    let (mut member, member_ready) = join(&server, 2, &invite).await;
    let member_token = media_token(&member_ready);
    let text = channel_id(&owner_ready, "general");

    let img = png(100, 100);
    let res = member
        .ok(Req::CreateUpload(proto::CreateUpload {
            channel_id: text,
            filename: "photo.png".into(),
            size_bytes: img.len() as u64,
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

    let (status, _) = server
        .send_http(
            "PUT",
            &format!("/media/uploads/{upload_id}"),
            &[("Authorization", &auth), ("Upload-Offset", "0")],
            &img,
        )
        .await;
    assert_eq!(status, 200);

    let (status, _) = server
        .send_http(
            "POST",
            &format!("/media/uploads/{upload_id}/finish"),
            &[("Authorization", &auth)],
            &[],
        )
        .await;
    assert_eq!(status, 202);

    let failed = member
        .wait_for(|kind| match kind {
            proto::event::Kind::UploadFailed(f) => Some(f.clone()),
            _ => None,
        })
        .await;
    assert_eq!(failed.upload_id, upload_id);
    assert_eq!(
        failed.error.as_ref().unwrap().code,
        proto::ErrorCode::StorageUnavailable as i32
    );

    let (status, bytes) = server
        .send_http(
            "GET",
            &format!("/media/uploads/{upload_id}"),
            &[("Authorization", &auth)],
            &[],
        )
        .await;
    assert_eq!(status, 200);
    match decode_response(&bytes) {
        Res::UploadStatus(s) => {
            assert_eq!(s.state, proto::UploadState::Failed as i32);
            assert_eq!(
                s.error.as_ref().unwrap().code,
                proto::ErrorCode::StorageUnavailable as i32
            );
        }
        other => panic!("expected UploadStatus, got {other:?}"),
    }

    let temp_dir = server.dir.path().join("data").join("uploads");
    if temp_dir.exists() {
        let mut entries = tokio::fs::read_dir(&temp_dir).await.unwrap();
        let mut count = 0;
        while entries.next_entry().await.unwrap().is_some() {
            count += 1;
        }
        assert_eq!(count, 0, "temp folder must be empty");
    }
}

#[tokio::test]
async fn processing_resumes_after_a_restart() {
    let Room {
        server,
        mut member,
        member_token,
        text,
        invite,
        ..
    } = room().await;

    server.handle.hold_processing(true);

    let img = png(100, 100);
    let res = member
        .ok(Req::CreateUpload(proto::CreateUpload {
            channel_id: text,
            filename: "photo.png".into(),
            size_bytes: img.len() as u64,
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

    let (status, _) = server
        .send_http(
            "PUT",
            &format!("/media/uploads/{upload_id}"),
            &[("Authorization", &auth), ("Upload-Offset", "0")],
            &img,
        )
        .await;
    assert_eq!(status, 200);

    let (status, _) = server
        .send_http(
            "POST",
            &format!("/media/uploads/{upload_id}/finish"),
            &[("Authorization", &auth)],
            &[],
        )
        .await;
    assert_eq!(status, 202);

    // Restart server.
    let server = server.restart(|_| {}).await;

    // Reconnect member.
    let (_member, member_ready) = join(&server, 2, &invite).await;
    let new_token = media_token(&member_ready);
    let new_auth = format!("Bearer {new_token}");

    // Poll GET /media/uploads/{upload_id} until ready (<= 10 s).
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut is_ready = false;
    while tokio::time::Instant::now() < deadline {
        let (status, bytes) = server
            .send_http(
                "GET",
                &format!("/media/uploads/{upload_id}"),
                &[("Authorization", &new_auth)],
                &[],
            )
            .await;
        if status == 200
            && let Res::UploadStatus(s) = decode_response(&bytes)
            && s.state == proto::UploadState::Ready as i32
        {
            is_ready = true;
            assert!(s.attachment.is_some());
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(is_ready, "upload should become ready after server restart");
}

#[tokio::test]
async fn downloads_return_exact_bytes_and_ranges() {
    let Room {
        server,
        mut member,
        member_token,
        text,
        ..
    } = room().await;

    let auth = format!("Bearer {member_token}");

    for size in [0, 65_536, 200_000] {
        let bytes: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
        let att = upload(
            &mut member,
            &server,
            &member_token,
            text,
            &format!("file_{size}.bin"),
            &bytes,
        )
        .await;

        member
            .ok(Req::SendMessage(proto::SendMessage {
                channel_id: text,
                content: format!("size {size}"),
                attachment_ids: vec![att.id],
                ..Default::default()
            }))
            .await;

        // Full GET equals the original
        let (status, dl) = server
            .send_http(
                "GET",
                &format!("/media/attachments/{}", att.id),
                &[("Authorization", &auth)],
                &[],
            )
            .await;
        assert_eq!(status, 200);
        assert_eq!(dl, bytes);

        if size == 200_000 {
            // bytes=65530-65540 equals the slice
            let (status, dl_range) = server
                .send_http(
                    "GET",
                    &format!("/media/attachments/{}", att.id),
                    &[("Authorization", &auth), ("Range", "bytes=65530-65540")],
                    &[],
                )
                .await;
            assert_eq!(status, 206);
            assert_eq!(dl_range, &bytes[65530..=65540]);

            // bytes=-10 on 200,000 equals the last 10
            let (status, dl_suffix) = server
                .send_http(
                    "GET",
                    &format!("/media/attachments/{}", att.id),
                    &[("Authorization", &auth), ("Range", "bytes=-10")],
                    &[],
                )
                .await;
            assert_eq!(status, 206);
            assert_eq!(dl_suffix, &bytes[200_000 - 10..]);
        }
    }
}

#[tokio::test]
async fn downloads_check_the_channel_at_request_time() {
    let Room {
        server,
        mut owner,
        mut member,
        member_token,
        everyone,
        text,
        ..
    } = room().await;

    let auth = format!("Bearer {member_token}");
    let bytes = b"channel permission test content".to_vec();
    let att = upload(
        &mut member,
        &server,
        &member_token,
        text,
        "check.txt",
        &bytes,
    )
    .await;

    let msg_res = member
        .ok(Req::SendMessage(proto::SendMessage {
            channel_id: text,
            content: "perm check".into(),
            attachment_ids: vec![att.id],
            ..Default::default()
        }))
        .await;
    let message_id = match msg_res {
        Res::Message(m) => m.id,
        other => panic!("expected Message, got {other:?}"),
    };

    // 1. Owner hides channel from @everyone with an overwrite -> 404
    owner
        .ok(Req::SetChannelOverwrite(proto::SetChannelOverwrite {
            channel_id: text,
            overwrite: Some(proto::PermissionOverwrite {
                target_kind: proto::OverwriteTarget::Role as i32,
                target_id: everyone,
                allow: 0,
                deny: opencord_common::permissions::Permissions::VIEW_CHANNEL.bits(),
            }),
        }))
        .await;

    let (status, _) = server
        .send_http(
            "GET",
            &format!("/media/attachments/{}", att.id),
            &[("Authorization", &auth)],
            &[],
        )
        .await;
    assert_eq!(status, 404);

    // 2. With READ_HISTORY denied -> 403
    owner
        .ok(Req::SetChannelOverwrite(proto::SetChannelOverwrite {
            channel_id: text,
            overwrite: Some(proto::PermissionOverwrite {
                target_kind: proto::OverwriteTarget::Role as i32,
                target_id: everyone,
                allow: opencord_common::permissions::Permissions::VIEW_CHANNEL.bits(),
                deny: opencord_common::permissions::Permissions::READ_HISTORY.bits(),
            }),
        }))
        .await;

    let (status, _) = server
        .send_http(
            "GET",
            &format!("/media/attachments/{}", att.id),
            &[("Authorization", &auth)],
            &[],
        )
        .await;
    assert_eq!(status, 403);

    // Restore channel permissions
    owner
        .ok(Req::DeleteChannelOverwrite(proto::DeleteChannelOverwrite {
            channel_id: text,
            target_kind: proto::OverwriteTarget::Role as i32,
            target_id: everyone,
        }))
        .await;

    // 3. A deleted message -> 404
    member
        .ok(Req::DeleteMessage(proto::DeleteMessage { message_id }))
        .await;

    let (status, _) = server
        .send_http(
            "GET",
            &format!("/media/attachments/{}", att.id),
            &[("Authorization", &auth)],
            &[],
        )
        .await;
    assert_eq!(status, 404);

    // 4. A bad token -> 401
    let (status, _) = server
        .send_http(
            "GET",
            &format!("/media/attachments/{}", att.id),
            &[("Authorization", "Bearer invalid-token")],
            &[],
        )
        .await;
    assert_eq!(status, 401);
}

#[tokio::test]
async fn a_ready_but_unsent_upload_cannot_be_downloaded_by_anyone() {
    let Room {
        server,
        mut member,
        member_token,
        owner_token,
        text,
        ..
    } = room().await;

    let bytes = b"unsent file content".to_vec();
    let att = upload(
        &mut member,
        &server,
        &member_token,
        text,
        "unsent.txt",
        &bytes,
    )
    .await;

    // Member gets 404
    let (status, _) = server
        .send_http(
            "GET",
            &format!("/media/attachments/{}", att.id),
            &[("Authorization", &format!("Bearer {member_token}"))],
            &[],
        )
        .await;
    assert_eq!(status, 404);

    // Owner gets 404
    let (status, _) = server
        .send_http(
            "GET",
            &format!("/media/attachments/{}", att.id),
            &[("Authorization", &format!("Bearer {owner_token}"))],
            &[],
        )
        .await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn headers_forbid_inline_use() {
    let Room {
        server,
        mut member,
        member_token,
        text,
        ..
    } = room().await;

    let img = png(200, 200);
    let att = upload(
        &mut member,
        &server,
        &member_token,
        text,
        "test_image.png",
        &img,
    )
    .await;

    member
        .ok(Req::SendMessage(proto::SendMessage {
            channel_id: text,
            content: "header check".into(),
            attachment_ids: vec![att.id],
            ..Default::default()
        }))
        .await;

    let auth = format!("Bearer {member_token}");

    // Check attachment headers
    let (status, headers, _) = server
        .send_http_full(
            "GET",
            &format!("/media/attachments/{}", att.id),
            &[("Authorization", &auth)],
            &[],
        )
        .await;
    assert_eq!(status, 200);

    let get_header = |headers: &[(String, String)], name: &str| -> Option<String> {
        headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.clone())
    };

    assert_eq!(
        get_header(&headers, "cache-control").as_deref(),
        Some("private, no-store")
    );
    assert_eq!(
        get_header(&headers, "x-content-type-options").as_deref(),
        Some("nosniff")
    );
    let disp = get_header(&headers, "content-disposition").expect("content-disposition header");
    assert!(
        disp.starts_with("attachment;"),
        "expected Content-Disposition to start with 'attachment;', got: {disp}"
    );

    // Check thumbnail headers
    let (status, thumb_headers, _) = server
        .send_http_full(
            "GET",
            &format!("/media/attachments/{}/thumbnail", att.id),
            &[("Authorization", &auth)],
            &[],
        )
        .await;
    assert_eq!(status, 200);

    assert_eq!(
        get_header(&thumb_headers, "cache-control").as_deref(),
        Some("private, no-store")
    );
    assert_eq!(
        get_header(&thumb_headers, "x-content-type-options").as_deref(),
        Some("nosniff")
    );
    let thumb_disp =
        get_header(&thumb_headers, "content-disposition").expect("content-disposition header");
    assert!(
        thumb_disp.starts_with("attachment;"),
        "expected thumbnail Content-Disposition to start with 'attachment;', got: {thumb_disp}"
    );
}

#[tokio::test]
async fn expired_uploads_are_swept() {
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
            filename: "expired.txt".into(),
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

    // Put a piece so temp file definitely exists and has received_bytes > 0
    let piece = vec![0u8; 100];
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
    assert!(temp_file.exists());

    let now_ms = opencord_server::state::now_ms();
    server
        .handle
        .sweep_attachments(now_ms + 24 * 3600 * 1000 + 1000)
        .await;

    assert!(!temp_file.exists(), "temp file must be deleted by sweep");

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
}

#[tokio::test]
async fn deleted_rows_delete_their_objects() {
    let Room {
        server,
        mut member,
        member_token,
        text,
        ..
    } = room().await;

    let img = png(400, 300);
    let att = upload(
        &mut member,
        &server,
        &member_token,
        text,
        "to_delete.png",
        &img,
    )
    .await;
    assert!(att.has_thumbnail);

    let shard = format!("{:02x}", (att.id >> 22) & 0xff);
    let att_file = server
        .dir
        .path()
        .join("data")
        .join("attachments")
        .join("a")
        .join(&shard)
        .join(att.id.to_string());
    let thumb_file = server
        .dir
        .path()
        .join("data")
        .join("attachments")
        .join("t")
        .join(&shard)
        .join(att.id.to_string());

    assert!(
        att_file.exists(),
        "attachment object file must exist before cancel"
    );
    assert!(
        thumb_file.exists(),
        "thumbnail object file must exist before cancel"
    );

    member
        .ok(Req::CancelUpload(proto::CancelUpload { upload_id: att.id }))
        .await;

    let now_ms = opencord_server::state::now_ms();
    server.handle.drain_blob_deletions(now_ms + 1000).await;

    assert!(
        !att_file.exists(),
        "attachment object file must be deleted by drain"
    );
    assert!(
        !thumb_file.exists(),
        "thumbnail object file must be deleted by drain"
    );
}

async fn upload_voice_test(
    client: &mut TestClient,
    server: &TestServer,
    token: &str,
    channel_id: i64,
) -> proto::Attachment {
    let ogg = voice_ogg(1.0, 1.0);
    let res = client
        .ok(Req::CreateUpload(proto::CreateUpload {
            channel_id,
            filename: "voice-message.ogg".into(),
            size_bytes: ogg.len() as u64,
            kind: proto::AttachmentKind::VoiceMessage.into(),
            width: None,
            height: None,
            duration_ms: None,
        }))
        .await;
    let upload = match res {
        Res::Upload(u) => u,
        other => panic!("expected Upload, got {other:?}"),
    };
    let auth = format!("Bearer {token}");
    let (status, _) = server
        .send_http(
            "PUT",
            &format!("/media/uploads/{}", upload.upload_id),
            &[("Authorization", &auth), ("Upload-Offset", "0")],
            &ogg,
        )
        .await;
    assert_eq!(status, 200);
    let (status, _) = server
        .send_http(
            "POST",
            &format!("/media/uploads/{}/finish", upload.upload_id),
            &[("Authorization", &auth)],
            &[],
        )
        .await;
    assert_eq!(status, 202);
    client
        .wait_for(|kind| match kind {
            proto::event::Kind::UploadReady(ready) => ready.attachment.clone(),
            _ => None,
        })
        .await
}

#[tokio::test]
async fn a_message_carries_its_files_in_order() {
    let Room {
        server,
        mut member,
        member_token,
        text,
        ..
    } = room().await;

    let a = upload(&mut member, &server, &member_token, text, "a.txt", b"aaa").await;
    let b = upload(&mut member, &server, &member_token, text, "b.txt", b"bbb").await;

    let res = member
        .ok(Req::SendMessage(proto::SendMessage {
            channel_id: text,
            content: "".into(),
            attachment_ids: vec![b.id, a.id],
            ..Default::default()
        }))
        .await;
    let msg = match res {
        Res::Message(m) => m,
        other => panic!("expected Message, got {other:?}"),
    };
    assert_eq!(msg.content, "");
    assert_eq!(msg.attachments.len(), 2);
    assert_eq!(msg.attachments[0].id, b.id);
    assert_eq!(msg.attachments[1].id, a.id);

    // MessageCreate event
    let created = member
        .wait_for(|kind| match kind {
            proto::event::Kind::MessageCreate(c)
                if c.message.as_ref().map(|m| m.id) == Some(msg.id) =>
            {
                c.message.clone()
            }
            _ => None,
        })
        .await;
    assert_eq!(created.attachments.len(), 2);
    assert_eq!(created.attachments[0].id, b.id);
    assert_eq!(created.attachments[1].id, a.id);

    // FetchMessages
    let fetch_res = member
        .ok(Req::FetchMessages(proto::FetchMessages {
            channel_id: text,
            limit: 10,
            before: None,
        }))
        .await;
    let list = match fetch_res {
        Res::Messages(m) => m.messages,
        other => panic!("expected Messages, got {other:?}"),
    };
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].attachments.len(), 2);
    assert_eq!(list[0].attachments[0].id, b.id);
    assert_eq!(list[0].attachments[1].id, a.id);
}

#[tokio::test]
async fn sending_checks_permissions_again_at_send_time() {
    let Room {
        server,
        mut owner,
        mut member,
        member_token,
        everyone,
        text,
        ..
    } = room().await;

    let att = upload(
        &mut member,
        &server,
        &member_token,
        text,
        "doc.txt",
        b"hello",
    )
    .await;

    // Owner removes ATTACH_FILES from @everyone
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

    let error = member
        .error(Req::SendMessage(proto::SendMessage {
            channel_id: text,
            content: "hello".into(),
            attachment_ids: vec![att.id],
            ..Default::default()
        }))
        .await;
    assert_eq!(error.code, proto::ErrorCode::Forbidden as i32);
}

#[tokio::test]
async fn someone_elses_or_unready_uploads_cannot_be_sent() {
    let Room {
        server,
        mut owner,
        owner_token,
        mut member,
        member_token,
        text,
        ..
    } = room().await;

    // 1. Another user's upload
    let owner_att = upload(
        &mut owner,
        &server,
        &owner_token,
        text,
        "owner.txt",
        b"owner",
    )
    .await;
    let err = member
        .error(Req::SendMessage(proto::SendMessage {
            channel_id: text,
            content: "take this".into(),
            attachment_ids: vec![owner_att.id],
            ..Default::default()
        }))
        .await;
    assert_eq!(err.code, proto::ErrorCode::NotFound as i32);

    // 2. One still uploading
    let unready_res = member
        .ok(Req::CreateUpload(proto::CreateUpload {
            channel_id: text,
            filename: "unready.txt".into(),
            size_bytes: 100,
            kind: proto::AttachmentKind::File.into(),
            width: None,
            height: None,
            duration_ms: None,
        }))
        .await;
    let unready_id = match unready_res {
        Res::Upload(u) => u.upload_id,
        other => panic!("expected Upload, got {other:?}"),
    };
    let err = member
        .error(Req::SendMessage(proto::SendMessage {
            channel_id: text,
            content: "unready".into(),
            attachment_ids: vec![unready_id],
            ..Default::default()
        }))
        .await;
    assert_eq!(err.code, proto::ErrorCode::NotFound as i32);

    // 3. One made in another channel
    let ch2_res = owner
        .ok(Req::CreateChannel(proto::CreateChannel {
            name: "other-channel".into(),
            kind: proto::ChannelKind::Text.into(),
            topic: None,
            parent_id: None,
        }))
        .await;
    let ch2_id = match ch2_res {
        Res::Channel(c) => c.id,
        other => panic!("expected Channel, got {other:?}"),
    };
    let other_channel_att = upload(
        &mut member,
        &server,
        &member_token,
        ch2_id,
        "other.txt",
        b"other",
    )
    .await;
    let err = member
        .error(Req::SendMessage(proto::SendMessage {
            channel_id: text,
            content: "wrong channel".into(),
            attachment_ids: vec![other_channel_att.id],
            ..Default::default()
        }))
        .await;
    assert_eq!(err.code, proto::ErrorCode::NotFound as i32);
}

#[tokio::test]
async fn voice_messages_stand_alone() {
    let Room {
        server,
        mut member,
        member_token,
        text,
        ..
    } = room().await;

    let voice_att = upload_voice_test(&mut member, &server, &member_token, text).await;
    let file_att = upload(&mut member, &server, &member_token, text, "doc.txt", b"doc").await;

    // voice + text -> INVALID_ARGUMENT
    let err = member
        .error(Req::SendMessage(proto::SendMessage {
            channel_id: text,
            content: "voice message with text".into(),
            attachment_ids: vec![voice_att.id],
            ..Default::default()
        }))
        .await;
    assert_eq!(err.code, proto::ErrorCode::InvalidArgument as i32);

    // voice + file -> INVALID_ARGUMENT
    let err = member
        .error(Req::SendMessage(proto::SendMessage {
            channel_id: text,
            content: "".into(),
            attachment_ids: vec![voice_att.id, file_att.id],
            ..Default::default()
        }))
        .await;
    assert_eq!(err.code, proto::ErrorCode::InvalidArgument as i32);

    // voice alone -> ok
    let res = member
        .ok(Req::SendMessage(proto::SendMessage {
            channel_id: text,
            content: "".into(),
            attachment_ids: vec![voice_att.id],
            ..Default::default()
        }))
        .await;
    let msg = match res {
        Res::Message(m) => m,
        other => panic!("expected Message, got {other:?}"),
    };
    assert_eq!(msg.attachments.len(), 1);
    assert_eq!(msg.attachments[0].id, voice_att.id);
    assert_eq!(
        msg.attachments[0].kind,
        proto::AttachmentKind::VoiceMessage as i32
    );
}

#[tokio::test]
async fn editing_keeps_files_and_allows_empty_text() {
    let Room {
        server,
        mut member,
        member_token,
        text,
        ..
    } = room().await;

    let file_att = upload(
        &mut member,
        &server,
        &member_token,
        text,
        "file.txt",
        b"file",
    )
    .await;
    let msg_res = member
        .ok(Req::SendMessage(proto::SendMessage {
            channel_id: text,
            content: "initial text".into(),
            attachment_ids: vec![file_att.id],
            ..Default::default()
        }))
        .await;
    let msg_id = match msg_res {
        Res::Message(m) => m.id,
        other => panic!("expected Message, got {other:?}"),
    };

    // a message with a file edited to "" -> ok, and its MessageUpdate carries the file
    let edit_res = member
        .ok(Req::EditMessage(proto::EditMessage {
            message_id: msg_id,
            content: "".into(),
        }))
        .await;
    let edited = match edit_res {
        Res::Message(m) => m,
        other => panic!("expected Message, got {other:?}"),
    };
    assert_eq!(edited.content, "");
    assert_eq!(edited.attachments.len(), 1);
    assert_eq!(edited.attachments[0].id, file_att.id);

    let update = member
        .wait_for(|kind| match kind {
            proto::event::Kind::MessageUpdate(u)
                if u.message.as_ref().map(|m| m.id) == Some(msg_id) =>
            {
                u.message.clone()
            }
            _ => None,
        })
        .await;
    assert_eq!(update.content, "");
    assert_eq!(update.attachments.len(), 1);
    assert_eq!(update.attachments[0].id, file_att.id);

    // a text-only message edited to "" -> INVALID_ARGUMENT
    let text_msg_res = member
        .ok(Req::SendMessage(proto::SendMessage {
            channel_id: text,
            content: "text only".into(),
            ..Default::default()
        }))
        .await;
    let text_msg_id = match text_msg_res {
        Res::Message(m) => m.id,
        other => panic!("expected Message, got {other:?}"),
    };
    let err = member
        .error(Req::EditMessage(proto::EditMessage {
            message_id: text_msg_id,
            content: "".into(),
        }))
        .await;
    assert_eq!(err.code, proto::ErrorCode::InvalidArgument as i32);

    // a voice message edited to "hi" -> INVALID_ARGUMENT
    let voice_att = upload_voice_test(&mut member, &server, &member_token, text).await;
    let voice_msg_res = member
        .ok(Req::SendMessage(proto::SendMessage {
            channel_id: text,
            content: "".into(),
            attachment_ids: vec![voice_att.id],
            ..Default::default()
        }))
        .await;
    let voice_msg_id = match voice_msg_res {
        Res::Message(m) => m.id,
        other => panic!("expected Message, got {other:?}"),
    };
    let err = member
        .error(Req::EditMessage(proto::EditMessage {
            message_id: voice_msg_id,
            content: "hi".into(),
        }))
        .await;
    assert_eq!(err.code, proto::ErrorCode::InvalidArgument as i32);
}

#[tokio::test]
async fn deleting_a_message_deletes_its_files() {
    let Room {
        server,
        mut member,
        member_token,
        text,
        ..
    } = room().await;

    let img = png(200, 200);
    let att = upload(&mut member, &server, &member_token, text, "image.png", &img).await;
    let msg_res = member
        .ok(Req::SendMessage(proto::SendMessage {
            channel_id: text,
            content: "pic".into(),
            attachment_ids: vec![att.id],
            ..Default::default()
        }))
        .await;
    let msg_id = match msg_res {
        Res::Message(m) => m.id,
        other => panic!("expected Message, got {other:?}"),
    };

    let shard = format!("{:02x}", (att.id >> 22) & 0xff);
    let att_file = server
        .dir
        .path()
        .join("data")
        .join("attachments")
        .join("a")
        .join(&shard)
        .join(att.id.to_string());
    assert!(att_file.exists());

    member
        .ok(Req::DeleteMessage(proto::DeleteMessage {
            message_id: msg_id,
        }))
        .await;

    let now_ms = opencord_server::state::now_ms();
    server.handle.drain_blob_deletions(now_ms + 1000).await;
    assert!(!att_file.exists());

    let (status, _) = server
        .send_http(
            "GET",
            &format!("/media/attachments/{}", att.id),
            &[("Authorization", &format!("Bearer {member_token}"))],
            &[],
        )
        .await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn deleting_a_channel_deletes_its_files() {
    let Room {
        server,
        mut owner,
        mut member,
        member_token,
        ..
    } = room().await;

    let ch_res = owner
        .ok(Req::CreateChannel(proto::CreateChannel {
            name: "temp-channel".into(),
            kind: proto::ChannelKind::Text.into(),
            topic: None,
            parent_id: None,
        }))
        .await;
    let ch_id = match ch_res {
        Res::Channel(c) => c.id,
        other => panic!("expected Channel, got {other:?}"),
    };

    let img = png(100, 100);
    let att = upload(&mut member, &server, &member_token, ch_id, "temp.png", &img).await;
    member
        .ok(Req::SendMessage(proto::SendMessage {
            channel_id: ch_id,
            content: "msg in doomed channel".into(),
            attachment_ids: vec![att.id],
            ..Default::default()
        }))
        .await;

    let shard = format!("{:02x}", (att.id >> 22) & 0xff);
    let att_file = server
        .dir
        .path()
        .join("data")
        .join("attachments")
        .join("a")
        .join(&shard)
        .join(att.id.to_string());
    assert!(att_file.exists());

    owner
        .ok(Req::DeleteChannel(proto::DeleteChannel {
            channel_id: ch_id,
        }))
        .await;

    let now_ms = opencord_server::state::now_ms();
    server.handle.drain_blob_deletions(now_ms + 1000).await;
    assert!(!att_file.exists());
}

#[tokio::test]
async fn an_upload_cannot_be_sent_twice() {
    let Room {
        server,
        mut member,
        member_token,
        text,
        ..
    } = room().await;

    let att = upload(
        &mut member,
        &server,
        &member_token,
        text,
        "once.txt",
        b"once",
    )
    .await;

    member
        .ok(Req::SendMessage(proto::SendMessage {
            channel_id: text,
            content: "first send".into(),
            attachment_ids: vec![att.id],
            ..Default::default()
        }))
        .await;

    let err = member
        .error(Req::SendMessage(proto::SendMessage {
            channel_id: text,
            content: "second send".into(),
            attachment_ids: vec![att.id],
            ..Default::default()
        }))
        .await;
    assert_eq!(err.code, proto::ErrorCode::NotFound as i32);

    let fetch_res = member
        .ok(Req::FetchMessages(proto::FetchMessages {
            channel_id: text,
            limit: 10,
            before: None,
        }))
        .await;
    let list = match fetch_res {
        Res::Messages(m) => m.messages,
        other => panic!("expected Messages, got {other:?}"),
    };
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].content, "first send");
}

#[tokio::test]
async fn upload_settings_are_shared_and_validated() {
    let Room {
        server,
        mut owner,
        mut member,
        invite,
        ..
    } = room().await;

    // A member without MANAGE_SERVER -> FORBIDDEN.
    let err = member
        .error(Req::UpdateUploadSettings(proto::UpdateUploadSettings {
            blocked_extensions: Some(proto::BlockedExtensions {
                extensions: vec!["exe".into()],
            }),
            voice_message_max_seconds: None,
        }))
        .await;
    assert_eq!(err.code, proto::ErrorCode::Forbidden as i32);

    // 29 seconds -> INVALID_ARGUMENT.
    let err = owner
        .error(Req::UpdateUploadSettings(proto::UpdateUploadSettings {
            blocked_extensions: None,
            voice_message_max_seconds: Some(29),
        }))
        .await;
    assert_eq!(err.code, proto::ErrorCode::InvalidArgument as i32);

    // ["ex e"] -> INVALID_ARGUMENT whose message contains "ex e".
    let err = owner
        .error(Req::UpdateUploadSettings(proto::UpdateUploadSettings {
            blocked_extensions: Some(proto::BlockedExtensions {
                extensions: vec!["ex e".into()],
            }),
            voice_message_max_seconds: None,
        }))
        .await;
    assert_eq!(err.code, proto::ErrorCode::InvalidArgument as i32);
    assert!(
        err.message.contains("ex e"),
        "expected error message to contain 'ex e', got: {}",
        err.message
    );

    // The owner sets ["EXE", ".tar.gz"] -> everyone gets an UploadSettingsUpdate with ["exe", "tar.gz"].
    let res = owner
        .ok(Req::UpdateUploadSettings(proto::UpdateUploadSettings {
            blocked_extensions: Some(proto::BlockedExtensions {
                extensions: vec!["EXE".into(), ".tar.gz".into()],
            }),
            voice_message_max_seconds: None,
        }))
        .await;
    let settings = match res {
        Res::UploadSettings(s) => s,
        other => panic!("expected UploadSettings, got {other:?}"),
    };
    assert_eq!(settings.blocked_extensions, vec!["exe", "tar.gz"]);

    // Owner gets the event:
    let owner_settings = owner
        .wait_for(|e| match e {
            proto::event::Kind::UploadSettingsUpdate(u) => u.settings.clone(),
            _ => None,
        })
        .await;
    assert_eq!(owner_settings.blocked_extensions, vec!["exe", "tar.gz"]);

    // Member gets the event:
    let member_settings = member
        .wait_for(|e| match e {
            proto::event::Kind::UploadSettingsUpdate(u) => u.settings.clone(),
            _ => None,
        })
        .await;
    assert_eq!(member_settings.blocked_extensions, vec!["exe", "tar.gz"]);

    // A new session's Ready carries the same settings.
    let (_new_client, new_ready) = join(&server, 3, &invite).await;
    let ready_settings = new_ready.upload_settings.expect("upload_settings in Ready");
    assert_eq!(ready_settings.blocked_extensions, vec!["exe", "tar.gz"]);
}

#[tokio::test]
async fn blocked_extensions_stop_uploads() {
    let Room {
        server: _server,
        mut owner,
        mut member,
        text,
        ..
    } = room().await;

    // After the owner blocks exe and tar.gz:
    owner
        .ok(Req::UpdateUploadSettings(proto::UpdateUploadSettings {
            blocked_extensions: Some(proto::BlockedExtensions {
                extensions: vec!["exe".into(), "tar.gz".into()],
            }),
            voice_message_max_seconds: None,
        }))
        .await;

    // CreateUpload setup.EXE -> FILE_TYPE_BLOCKED
    let err = member
        .error(Req::CreateUpload(proto::CreateUpload {
            channel_id: text,
            filename: "setup.EXE".into(),
            size_bytes: 1024,
            kind: proto::AttachmentKind::File.into(),
            width: None,
            height: None,
            duration_ms: None,
        }))
        .await;
    assert_eq!(err.code, proto::ErrorCode::FileTypeBlocked as i32);

    // archive.tar.gz -> FILE_TYPE_BLOCKED with tar.gz in the message
    let err = member
        .error(Req::CreateUpload(proto::CreateUpload {
            channel_id: text,
            filename: "archive.tar.gz".into(),
            size_bytes: 1024,
            kind: proto::AttachmentKind::File.into(),
            width: None,
            height: None,
            duration_ms: None,
        }))
        .await;
    assert_eq!(err.code, proto::ErrorCode::FileTypeBlocked as i32);
    assert!(
        err.message.contains("tar.gz"),
        "expected error message to contain 'tar.gz', got: {}",
        err.message
    );

    // notes.txt -> ok
    let res = member
        .ok(Req::CreateUpload(proto::CreateUpload {
            channel_id: text,
            filename: "notes.txt".into(),
            size_bytes: 1024,
            kind: proto::AttachmentKind::File.into(),
            width: None,
            height: None,
            duration_ms: None,
        }))
        .await;
    assert!(matches!(res, Res::Upload(_)));

    // a voice message is never blocked
    let res = member
        .ok(Req::CreateUpload(proto::CreateUpload {
            channel_id: text,
            filename: "ignored.exe".into(),
            size_bytes: 1024,
            kind: proto::AttachmentKind::VoiceMessage.into(),
            width: None,
            height: None,
            duration_ms: Some(1000),
        }))
        .await;
    assert!(matches!(res, Res::Upload(_)));
}

#[tokio::test]
async fn role_limits_raise_members_and_respect_the_editor() {
    let Room {
        server,
        mut owner,
        mut member,
        member_id,
        everyone,
        invite,
        text,
        ..
    } = room().await;

    // The owner gives role R 200 MiB, then a member with R creates a 150 MiB upload -> ok.
    let res = owner
        .ok(Req::CreateRole(proto::CreateRole {
            name: "R".into(),
            color: 0,
            permissions: 0,
            hoist: false,
            mentionable: false,
            max_upload_bytes: Some(200 * 1024 * 1024),
        }))
        .await;
    let role_r = match res {
        Res::Role(r) => r,
        other => panic!("expected Role, got {other:?}"),
    };
    assert_eq!(role_r.max_upload_bytes, Some(200 * 1024 * 1024));

    owner
        .ok(Req::AddMemberRole(proto::AddMemberRole {
            user_id: member_id,
            role_id: role_r.id,
        }))
        .await;

    let res = member
        .ok(Req::CreateUpload(proto::CreateUpload {
            channel_id: text,
            filename: "big.dat".into(),
            size_bytes: 150 * 1024 * 1024,
            kind: proto::AttachmentKind::File.into(),
            width: None,
            height: None,
            duration_ms: None,
        }))
        .await;
    assert!(matches!(res, Res::Upload(_)));

    // A moderator (MANAGE_ROLES, a role above the target, own limit 50 MiB)
    // sets a lower role to 100 MiB -> FORBIDDEN, and to 40 MiB -> ok.
    let (mut moderator, mod_ready) = join(&server, 3, &invite).await;
    let mod_id = self_id(&mod_ready);

    // Create moderator role first (will end up at higher position than target role).
    let res = owner
        .ok(Req::CreateRole(proto::CreateRole {
            name: "Moderator".into(),
            color: 0,
            permissions: opencord_common::permissions::Permissions::MANAGE_ROLES.bits(),
            hoist: false,
            mentionable: false,
            max_upload_bytes: Some(50 * 1024 * 1024),
        }))
        .await;
    let mod_role = match res {
        Res::Role(r) => r,
        other => panic!("expected Role, got {other:?}"),
    };

    owner
        .ok(Req::AddMemberRole(proto::AddMemberRole {
            user_id: mod_id,
            role_id: mod_role.id,
        }))
        .await;

    // Create target role (inserted at position 1; mod_role shifts up).
    let res = owner
        .ok(Req::CreateRole(proto::CreateRole {
            name: "Target".into(),
            color: 0,
            permissions: 0,
            hoist: false,
            mentionable: false,
            max_upload_bytes: None,
        }))
        .await;
    let target_role = match res {
        Res::Role(r) => r,
        other => panic!("expected Role, got {other:?}"),
    };

    // Moderator sets lower role to 100 MiB -> FORBIDDEN (own limit is 50 MiB).
    let err = moderator
        .error(Req::UpdateRole(proto::UpdateRole {
            role_id: target_role.id,
            max_upload_bytes: Some(100 * 1024 * 1024),
            ..Default::default()
        }))
        .await;
    assert_eq!(err.code, proto::ErrorCode::Forbidden as i32);

    // Moderator sets lower role to 40 MiB -> ok.
    let res = moderator
        .ok(Req::UpdateRole(proto::UpdateRole {
            role_id: target_role.id,
            max_upload_bytes: Some(40 * 1024 * 1024),
            ..Default::default()
        }))
        .await;
    let updated_role = match res {
        Res::Role(r) => r,
        other => panic!("expected Role, got {other:?}"),
    };
    assert_eq!(updated_role.max_upload_bytes, Some(40 * 1024 * 1024));
    let r = member
        .wait_for(|e| match e {
            proto::event::Kind::RoleUpdate(u)
                if u.role.as_ref().map(|r| r.id) == Some(target_role.id) =>
            {
                u.role.clone()
            }
            _ => None,
        })
        .await;
    assert_eq!(r.max_upload_bytes, Some(40 * 1024 * 1024));

    // @everyone Some(0) -> INVALID_ARGUMENT.
    let err = owner
        .error(Req::UpdateRole(proto::UpdateRole {
            role_id: everyone,
            max_upload_bytes: Some(0),
            ..Default::default()
        }))
        .await;
    assert_eq!(err.code, proto::ErrorCode::InvalidArgument as i32);

    // Another role Some(0) clears its limit, and its RoleUpdate has no max_upload_bytes.
    let res = owner
        .ok(Req::UpdateRole(proto::UpdateRole {
            role_id: target_role.id,
            max_upload_bytes: Some(0),
            ..Default::default()
        }))
        .await;
    let cleared = match res {
        Res::Role(r) => r,
        other => panic!("expected Role, got {other:?}"),
    };
    assert_eq!(cleared.max_upload_bytes, None);

    let r = member
        .wait_for(|e| match e {
            proto::event::Kind::RoleUpdate(u)
                if u.role.as_ref().map(|r| r.id) == Some(target_role.id) =>
            {
                u.role.clone()
            }
            _ => None,
        })
        .await;
    assert_eq!(r.max_upload_bytes, None);
}

#[tokio::test]
async fn the_owner_is_limited_only_by_the_ceiling() {
    let server = TestServer::start_with(|c| c.uploads.max_file_mb = 1).await;
    let (mut owner, owner_ready) = owner(&server).await;
    let general = channel_id(&owner_ready, "general");

    let err = owner
        .error(Req::CreateUpload(proto::CreateUpload {
            channel_id: general,
            filename: "big.bin".into(),
            size_bytes: 2 * 1024 * 1024,
            kind: proto::AttachmentKind::File.into(),
            width: None,
            height: None,
            duration_ms: None,
        }))
        .await;
    assert_eq!(err.code, proto::ErrorCode::FileTooLarge as i32);
}
