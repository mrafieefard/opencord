//! Files in messages, upload settings and storage, against a real server
//! (media uploads plan, milestone A4).

use std::time::Duration;

use opencord_common::address::format_fingerprint;
use opencord_core::api::types::{
    AttachmentKind, AttachmentVariant, CoreError, CoreEvent, CoreEventPayload, ErrorCode,
    OutgoingFile, ReadySnapshot, RoleChanges, StorageKind, UploadPhase,
};
use opencord_core::client::Client;
use opencord_core::identity::Identity;
use opencord_server::config::Config;
use opencord_server::server::{self, ServerHandle};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc::UnboundedReceiver;

const WAIT: Duration = Duration::from_secs(20);
const MIB: usize = 1024 * 1024;

struct TestServer {
    handle: ServerHandle,
    _dir: tempfile::TempDir,
}

impl TestServer {
    async fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.server.bind = "127.0.0.1:0".parse().unwrap();
        config.server.public_host = "127.0.0.1".to_owned();
        config.server.data_dir = dir.path().join("data");
        config.server.name = "Test Server".to_owned();
        config.voice.udp_port = 0;
        config.music.audio_udp_port = 0;
        config.network.ip_lookup_v4 = String::new();
        config.network.ip_lookup_v6 = String::new();
        let handle = server::start(config).await.unwrap();
        Self { handle, _dir: dir }
    }

    fn address(&self) -> String {
        format!("127.0.0.1:{}", self.handle.local_addr.port())
    }
}

struct TestClient {
    client: Client,
    events: UnboundedReceiver<CoreEvent>,
    _dir: tempfile::TempDir,
}

impl TestClient {
    /// The owner: trusts the certificate and claims the server.
    async fn owner(server: &TestServer) -> (Self, ReadySnapshot) {
        let dir = tempfile::tempdir().unwrap();
        let (client, events) = Client::new(dir.path(), tokio::runtime::Handle::current()).unwrap();
        client.set_identity(Identity::generate(), "Owner".to_owned());
        client
            .trust_fingerprint(
                &server.address(),
                &format_fingerprint(&server.handle.fingerprint),
            )
            .unwrap();
        client
            .add_server(&server.address(), server.handle.claim_token.clone())
            .await
            .unwrap();
        let mut owner = Self {
            client,
            events,
            _dir: dir,
        };
        let ready = owner
            .wait_for(|payload| match payload {
                CoreEventPayload::Ready(ready) => Some(ready.clone()),
                _ => None,
            })
            .await;
        (owner, ready)
    }

    /// Skips events until `pick` returns something.
    async fn wait_for<T>(&mut self, mut pick: impl FnMut(&CoreEventPayload) -> Option<T>) -> T {
        tokio::time::timeout(WAIT, async {
            loop {
                let event = self.events.recv().await.expect("the event stream ended");
                if let Some(found) = pick(&event.payload) {
                    return found;
                }
            }
        })
        .await
        .expect("timed out waiting for an event")
    }
}

fn general(ready: &ReadySnapshot) -> i64 {
    ready
        .channels
        .iter()
        .find(|channel| channel.name == "general")
        .unwrap()
        .id
}

/// Bytes that don't repeat within a piece.
fn data(len: usize) -> Vec<u8> {
    let mut state = 0x2545_F491_u32;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as u8
        })
        .collect()
}

fn bytes_file(name: &str, bytes: Vec<u8>) -> OutgoingFile {
    OutgoingFile {
        path: None,
        bytes: Some(bytes),
        filename: name.to_owned(),
        staged_id: None,
    }
}

fn sha256(bytes: &[u8]) -> Vec<u8> {
    Sha256::digest(bytes).to_vec()
}

async fn download(owner: &TestClient, key: &str, attachment_id: i64) -> Vec<u8> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("saved");
    owner
        .client
        .download_attachment(key, attachment_id, path.to_string_lossy().into_owned())
        .await
        .unwrap();
    std::fs::read(path).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn ready_carries_upload_settings_and_role_limits() {
    let server = TestServer::start().await;
    let (_owner, ready) = TestClient::owner(&server).await;

    assert_eq!(ready.upload_settings.max_attachments, 10);
    assert_eq!(ready.upload_settings.max_file_bytes, 4096 * MIB as u64);
    assert_eq!(ready.upload_settings.voice_message_max_seconds, 1_800);
    let everyone = ready
        .roles
        .iter()
        .find(|role| role.name == "@everyone")
        .unwrap();
    assert_eq!(everyone.max_upload_bytes, Some(50 * MIB as u64));
}

#[tokio::test(flavor = "multi_thread")]
async fn owner_updates_upload_settings_and_sees_the_event() {
    let server = TestServer::start().await;
    let (mut owner, _) = TestClient::owner(&server).await;
    let key = server.address();

    let settings = owner
        .client
        .update_upload_settings(&key, Some(vec!["psd".to_owned()]))
        .await
        .unwrap();

    assert_eq!(settings.blocked_extensions, ["psd"]);
    // Fixed at 30 minutes on every server.
    assert_eq!(settings.voice_message_max_seconds, 1_800);
    let event = owner
        .wait_for(|payload| match payload {
            CoreEventPayload::UploadSettingsUpdate(settings) => Some(settings.clone()),
            _ => None,
        })
        .await;
    assert_eq!(event, settings);
}

#[tokio::test(flavor = "multi_thread")]
async fn storage_get_lists_the_local_storage() {
    let server = TestServer::start().await;
    let (owner, _) = TestClient::owner(&server).await;

    let storage = owner.client.storage_get(&server.address()).await.unwrap();

    assert_eq!(storage.backends.len(), 1);
    assert_eq!(storage.backends[0].kind, StorageKind::Local);
    assert!(storage.backends[0].active);
    assert!(storage.move_status.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn role_limit_round_trips() {
    let server = TestServer::start().await;
    let (owner, _) = TestClient::owner(&server).await;
    let key = server.address();

    let role = owner
        .client
        .create_role(
            &key,
            "uploaders".to_owned(),
            0,
            0,
            false,
            false,
            Some(100 * MIB as u64),
        )
        .await
        .unwrap();
    assert_eq!(role.max_upload_bytes, Some(100 * MIB as u64));

    let changes = RoleChanges {
        max_upload_bytes: Some(0),
        ..RoleChanges::default()
    };
    let role = owner
        .client
        .update_role(&key, role.id, changes)
        .await
        .unwrap();
    assert_eq!(role.max_upload_bytes, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn send_message_with_files_uploads_and_sends() {
    let server = TestServer::start().await;
    let (mut owner, ready) = TestClient::owner(&server).await;
    let key = server.address();
    let dir = tempfile::tempdir().unwrap();
    let big = data(9 * MIB);
    let path = dir.path().join("big.bin");
    std::fs::write(&path, &big).unwrap();
    let small = data(1024);
    let files = vec![
        OutgoingFile {
            path: Some(path.to_string_lossy().into_owned()),
            bytes: None,
            filename: String::new(),
            staged_id: None,
        },
        bytes_file("small.bin", small.clone()),
    ];

    let message = owner
        .client
        .send_message_with_files(
            &key,
            general(&ready),
            "two files".to_owned(),
            "n1".to_owned(),
            files,
        )
        .await
        .unwrap();

    let names: Vec<_> = message
        .attachments
        .iter()
        .map(|a| a.filename.as_str())
        .collect();
    assert_eq!(names, ["big.bin", "small.bin"]);
    assert_eq!(message.attachments[0].size_bytes, big.len() as u64);
    let mut done = [false, false];
    while done != [true, true] {
        let (index, phase) = owner
            .wait_for(|payload| match payload {
                CoreEventPayload::UploadProgress {
                    file_index, phase, ..
                } => Some((*file_index as usize, phase.clone())),
                _ => None,
            })
            .await;
        if phase == UploadPhase::Done {
            done[index] = true;
        }
    }
    let saved = download(&owner, &key, message.attachments[0].id).await;
    assert_eq!(sha256(&saved), sha256(&big));
    assert_eq!(
        download(&owner, &key, message.attachments[1].id).await,
        small
    );
}

/// Review Focus 1: the server took a piece but its answer was lost. The
/// piece goes again, the server answers 409 with its count, and the upload
/// carries on.
#[tokio::test(flavor = "multi_thread")]
async fn uploads_resume_after_a_lost_answer() {
    let server = TestServer::start().await;
    let (owner, ready) = TestClient::owner(&server).await;
    let key = server.address();
    let file = data(20 * MIB);
    owner.client.lose_upload_answers(1);

    let message = owner
        .client
        .send_message_with_files(
            &key,
            general(&ready),
            String::new(),
            "n1".to_owned(),
            vec![bytes_file("file.bin", file.clone())],
        )
        .await
        .unwrap();

    let saved = download(&owner, &key, message.attachments[0].id).await;
    assert_eq!(sha256(&saved), sha256(&file));
}

#[tokio::test(flavor = "multi_thread")]
async fn cancel_send_cancels_server_uploads() {
    let server = TestServer::start().await;
    let (mut owner, ready) = TestClient::owner(&server).await;
    let key = server.address();
    server.handle.hold_processing(true);
    let client = owner.client.clone();
    let (send_key, channel_id) = (key.clone(), general(&ready));
    let send = tokio::spawn(async move {
        client
            .send_message_with_files(
                &send_key,
                channel_id,
                String::new(),
                "n1".to_owned(),
                vec![bytes_file("file.bin", data(1024))],
            )
            .await
    });
    owner
        .wait_for(|payload| match payload {
            CoreEventPayload::UploadProgress {
                phase: UploadPhase::Processing,
                ..
            } => Some(()),
            _ => None,
        })
        .await;

    owner.client.cancel_send(&key, "n1");

    assert!(matches!(send.await.unwrap(), Err(CoreError::Cancelled)));
    owner
        .wait_for(|payload| match payload {
            CoreEventPayload::UploadProgress {
                phase: UploadPhase::Cancelled,
                ..
            } => Some(()),
            _ => None,
        })
        .await;
    for state in ["uploading", "processing", "ready"] {
        assert_eq!(server.handle.uploads_in_state(state).await.unwrap(), 0);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_blocked_file_fails_with_the_servers_reason() {
    let server = TestServer::start().await;
    let (owner, ready) = TestClient::owner(&server).await;
    let key = server.address();
    // The list starts empty; the owner picks what to block.
    owner
        .client
        .update_upload_settings(&key, Some(vec!["exe".to_owned()]))
        .await
        .unwrap();

    let sent = owner
        .client
        .send_message_with_files(
            &key,
            general(&ready),
            String::new(),
            "n1".to_owned(),
            vec![bytes_file("setup.exe", data(1024))],
        )
        .await;

    assert!(matches!(
        sent,
        Err(CoreError::Server {
            code: ErrorCode::FileTypeBlocked,
            ..
        })
    ));
}

/// An expired media token is renewed once, then the transfer goes
/// ahead.
#[tokio::test(flavor = "multi_thread")]
async fn an_expired_media_token_is_renewed() {
    let server = TestServer::start().await;
    let (owner, ready) = TestClient::owner(&server).await;
    let key = server.address();
    owner.client.spoil_media_token(&key);

    let message = owner
        .client
        .send_message_with_files(
            &key,
            general(&ready),
            String::new(),
            "n1".to_owned(),
            vec![bytes_file("file.bin", data(1024))],
        )
        .await
        .unwrap();

    owner.client.spoil_media_token(&key);
    assert_eq!(
        download(&owner, &key, message.attachments[0].id).await,
        data(1024)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn download_attachment_writes_the_exact_file() {
    let server = TestServer::start().await;
    let (mut owner, ready) = TestClient::owner(&server).await;
    let key = server.address();
    let file = data(3 * MIB + 7);
    let message = owner
        .client
        .send_message_with_files(
            &key,
            general(&ready),
            String::new(),
            "n1".to_owned(),
            vec![bytes_file("file.bin", file.clone())],
        )
        .await
        .unwrap();
    let attachment_id = message.attachments[0].id;

    let saved = download(&owner, &key, attachment_id).await;

    assert_eq!(sha256(&saved), sha256(&file));
    let (received, total) = owner
        .wait_for(|payload| match payload {
            CoreEventPayload::DownloadProgress {
                received_bytes,
                total_bytes,
                done: true,
                ..
            } => Some((*received_bytes, *total_bytes)),
            _ => None,
        })
        .await;
    assert_eq!((received, total), (file.len() as u64, file.len() as u64));
}

#[tokio::test(flavor = "multi_thread")]
async fn attachment_bytes_uses_the_cache_second_time() {
    let server = TestServer::start().await;
    let (owner, ready) = TestClient::owner(&server).await;
    let key = server.address();
    owner.client.media_cache_init(&[9; 32]).unwrap();
    let mut png = Vec::new();
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(64, 64, image::Rgb([1, 2, 3])))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let message = owner
        .client
        .send_message_with_files(
            &key,
            general(&ready),
            String::new(),
            "n1".to_owned(),
            vec![bytes_file("picture.png", png.clone())],
        )
        .await
        .unwrap();
    let id = message.attachments[0].id;
    let original = owner
        .client
        .attachment_bytes(&key, id, AttachmentVariant::Original)
        .await
        .unwrap();
    let thumbnail = owner
        .client
        .attachment_bytes(&key, id, AttachmentVariant::Thumbnail)
        .await
        .unwrap();
    assert_eq!(original, png);
    assert!(owner.client.media_cache_size().await.unwrap() > 0);

    server.handle.shutdown().await;

    let cached = owner
        .client
        .attachment_bytes(&key, id, AttachmentVariant::Original)
        .await
        .unwrap();
    assert_eq!(cached, png);
    let cached = owner
        .client
        .attachment_bytes(&key, id, AttachmentVariant::Thumbnail)
        .await
        .unwrap();
    assert_eq!(cached, thumbnail);
    owner.client.media_cache_clear().await.unwrap();
    assert_eq!(owner.client.media_cache_size().await.unwrap(), 0);
}

const SECRET: &[u8] = b"GPS 52.5200N 13.4050E";

/// A JPEG whose Exif holds orientation 6 and a GPS position.
fn photo_with_location() -> Vec<u8> {
    let mut jpeg = Vec::new();
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
        32,
        16,
        image::Rgb([200, 80, 40]),
    ))
    .write_to(
        &mut std::io::Cursor::new(&mut jpeg),
        image::ImageFormat::Jpeg,
    )
    .unwrap();
    let mut exif = b"Exif\0\0II\x2A\x00\x08\x00\x00\x00\x01\x00\x12\x01\x03\x00\x01\x00\x00\x00\x06\x00\x00\x00\x00\x00\x00\x00".to_vec();
    exif.extend_from_slice(SECRET);
    let mut photo = jpeg[..2].to_vec();
    photo.extend_from_slice(&[0xFF, 0xE1]);
    photo.extend_from_slice(&u16::try_from(exif.len() + 2).unwrap().to_be_bytes());
    photo.extend_from_slice(&exif);
    photo.extend_from_slice(&jpeg[2..]);
    photo
}

/// Task 20: the photo's GPS position never reaches the server, and it still
/// shows the right way up.
#[tokio::test(flavor = "multi_thread")]
async fn a_photos_location_never_leaves_the_device() {
    let server = TestServer::start().await;
    let (owner, ready) = TestClient::owner(&server).await;
    let key = server.address();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("photo.jpg");
    std::fs::write(&path, photo_with_location()).unwrap();

    let message = owner
        .client
        .send_message_with_files(
            &key,
            general(&ready),
            String::new(),
            "n1".to_owned(),
            vec![OutgoingFile {
                path: Some(path.to_string_lossy().into_owned()),
                bytes: None,
                filename: String::new(),
                staged_id: None,
            }],
        )
        .await
        .unwrap();

    let attachment = &message.attachments[0];
    let sent = download(&owner, &key, attachment.id).await;
    assert!(!sent.windows(SECRET.len()).any(|window| window == SECRET));
    // Orientation 6 turns the 32×16 photo on its side.
    assert_eq!((attachment.width, attachment.height), (Some(16), Some(32)));
}

#[tokio::test(flavor = "multi_thread")]
async fn range_reader_reads_any_span() {
    use std::io::{Read, Seek, SeekFrom};

    let server = TestServer::start().await;
    let (owner, ready) = TestClient::owner(&server).await;
    let key = server.address();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test_file.bin");

    let source_data: Vec<u8> = (0..(2500 * 1024)).map(|i| (i % 251) as u8).collect();
    std::fs::write(&path, &source_data).unwrap();

    let message = owner
        .client
        .send_message_with_files(
            &key,
            general(&ready),
            String::new(),
            "n1".to_owned(),
            vec![OutgoingFile {
                path: Some(path.to_string_lossy().into_owned()),
                bytes: None,
                filename: "test_file.bin".to_owned(),
                staged_id: None,
            }],
        )
        .await
        .unwrap();

    let attachment = &message.attachments[0];
    let mut reader = opencord_core::attachments::range_reader::RangeReader::open(
        owner.client.clone(),
        key.clone(),
        attachment.id,
    )
    .await
    .unwrap();

    let waits = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let heard = waits.clone();
    reader.on_wait(move |waiting| heard.lock().unwrap().push(waiting));

    let total_size = reader.seek(SeekFrom::End(0)).unwrap();
    assert_eq!(total_size, source_data.len() as u64);

    // The first block waits for the network; the next was fetched ahead.
    let mut byte = [0u8; 1];
    reader.seek(SeekFrom::Start(0)).unwrap();
    reader.read_exact(&mut byte).unwrap();
    assert_eq!(*waits.lock().unwrap(), [true, false]);
    tokio::time::sleep(Duration::from_secs(1)).await;
    reader.seek(SeekFrom::Start(1024 * 1024 + 5)).unwrap();
    reader.read_exact(&mut byte).unwrap();
    assert_eq!(byte[0], source_data[1024 * 1024 + 5]);
    assert_eq!(
        *waits.lock().unwrap(),
        [true, false],
        "block 1 wasn't read ahead"
    );

    let mut rng_state = 123456789u64;
    let mut rand_u32 = || {
        rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1);
        (rng_state >> 32) as u32
    };

    for _ in 0..50 {
        let max_pos = (source_data.len() - 100) as u64;
        let pos = (rand_u32() as u64) % max_pos;
        let len = ((rand_u32() as usize) % (128 * 1024)) + 1;
        let len = len.min(source_data.len() - pos as usize);

        let new_pos = reader.seek(SeekFrom::Start(pos)).unwrap();
        assert_eq!(new_pos, pos);

        let mut buf = vec![0u8; len];
        reader.read_exact(&mut buf).unwrap();
        assert_eq!(&buf[..], &source_data[pos as usize..pos as usize + len]);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn video_uploads_get_hints_and_a_poster() {
    let server = TestServer::start().await;
    let (owner, ready) = TestClient::owner(&server).await;
    let key = server.address();

    let fixture_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../opencord-media/tests/fixtures/h264_aac.mp4");

    let message = owner
        .client
        .send_message_with_files(
            &key,
            general(&ready),
            String::new(),
            "n1".to_owned(),
            vec![OutgoingFile {
                path: Some(fixture_path.to_string_lossy().into_owned()),
                bytes: None,
                filename: "video.mp4".to_owned(),
                staged_id: None,
            }],
        )
        .await
        .unwrap();

    let attachment = &message.attachments[0];
    assert_eq!(attachment.width, Some(320));
    assert_eq!(attachment.height, Some(240));
    assert!(attachment.duration_ms.is_some());
    assert!(attachment.has_thumbnail);

    let thumbnail = owner
        .client
        .attachment_bytes(&key, attachment.id, AttachmentVariant::Thumbnail)
        .await
        .unwrap();
    assert!(!thumbnail.is_empty());
    assert_eq!(&thumbnail[..2], &[0xFF, 0xD8]);
}

/// An MP4 with sound and no picture is audio whatever its brand (`isom`
/// here, as phones and FFmpeg write them): the server reads its tracks.
/// Sent by path, the sender's probe adds its length.
#[tokio::test(flavor = "multi_thread")]
async fn an_mp4_without_a_picture_is_audio() {
    let server = TestServer::start().await;
    let (owner, ready) = TestClient::owner(&server).await;
    let key = server.address();
    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../opencord-media/tests/fixtures/aac_isom.m4a");
    let by_path = OutgoingFile {
        path: Some(fixture.to_string_lossy().into_owned()),
        bytes: None,
        filename: "song.m4a".to_owned(),
        staged_id: None,
    };
    let by_bytes = bytes_file("song.m4a", std::fs::read(&fixture).unwrap());

    for (nonce, file) in [("n1", by_path), ("n2", by_bytes)] {
        let message = owner
            .client
            .send_message_with_files(
                &key,
                general(&ready),
                String::new(),
                nonce.to_owned(),
                vec![file],
            )
            .await
            .unwrap();
        let attachment = &message.attachments[0];
        assert_eq!(attachment.kind, AttachmentKind::Audio, "{nonce}");
        assert_eq!(attachment.content_type, "audio/mp4", "{nonce}");
        assert_eq!(
            (attachment.width, attachment.height),
            (None, None),
            "{nonce}"
        );
        if nonce == "n1" {
            let duration = attachment.duration_ms.expect("the probe's length");
            assert!((950..=1100).contains(&duration), "{duration} ms");
        }
    }
}

/// A file staged in the composer uploads at once; sending takes the
/// finished upload over without reading the file again.
#[tokio::test(flavor = "multi_thread")]
async fn a_staged_file_is_sent_without_uploading_again() {
    let server = TestServer::start().await;
    let (mut owner, ready) = TestClient::owner(&server).await;
    let key = server.address();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("report.bin");
    let contents = data(3 * MIB);
    std::fs::write(&path, &contents).unwrap();
    let file = OutgoingFile {
        path: Some(path.to_string_lossy().into_owned()),
        bytes: None,
        filename: String::new(),
        staged_id: Some("s1".to_owned()),
    };

    owner
        .client
        .stage_upload(&key, general(&ready), "s1".to_owned(), file.clone())
        .await
        .unwrap();
    let done = owner
        .wait_for(|payload| match payload {
            CoreEventPayload::StagedUploadProgress {
                staged_id,
                phase: UploadPhase::Done,
                sent_bytes,
                ..
            } if staged_id == "s1" => Some(*sent_bytes),
            _ => None,
        })
        .await;
    assert_eq!(done, contents.len() as u64);

    // Gone from the disk: only the staged upload can be sent now.
    std::fs::remove_file(&path).unwrap();
    let message = owner
        .client
        .send_message_with_files(
            &key,
            general(&ready),
            String::new(),
            "n1".to_owned(),
            vec![file],
        )
        .await
        .unwrap();
    let saved = download(&owner, &key, message.attachments[0].id).await;
    assert_eq!(sha256(&saved), sha256(&contents));
}

/// Sent while still uploading, the message waits for the staged upload
/// and goes out with it.
#[tokio::test(flavor = "multi_thread")]
async fn a_message_sent_mid_upload_takes_the_upload_over() {
    let server = TestServer::start().await;
    let (mut owner, ready) = TestClient::owner(&server).await;
    let key = server.address();
    let contents = data(17 * MIB);
    let file = OutgoingFile {
        path: None,
        bytes: Some(contents.clone()),
        filename: "big.bin".to_owned(),
        staged_id: Some("s2".to_owned()),
    };
    let staging = {
        let client = owner.client.clone();
        let (key, file) = (key.clone(), file.clone());
        let channel = general(&ready);
        tokio::spawn(async move {
            client
                .stage_upload(&key, channel, "s2".to_owned(), file)
                .await
        })
    };
    owner
        .wait_for(|payload| match payload {
            CoreEventPayload::StagedUploadProgress {
                staged_id,
                phase: UploadPhase::Uploading,
                ..
            } if staged_id == "s2" => Some(()),
            _ => None,
        })
        .await;

    // The bytes sent with the message are not the staged ones: had it
    // uploaded them again, the saved copy would differ.
    let other = OutgoingFile {
        bytes: Some(data(1024)),
        ..file
    };
    let message = owner
        .client
        .send_message_with_files(
            &key,
            general(&ready),
            String::new(),
            "n2".to_owned(),
            vec![other],
        )
        .await
        .unwrap();
    staging.await.unwrap().unwrap();
    let saved = download(&owner, &key, message.attachments[0].id).await;
    assert_eq!(sha256(&saved), sha256(&contents));
}

/// A staged file removed before sending is gone: sending it uploads it
/// again, from the file.
#[tokio::test(flavor = "multi_thread")]
async fn a_discarded_staged_file_is_not_reused() {
    let server = TestServer::start().await;
    let (owner, ready) = TestClient::owner(&server).await;
    let key = server.address();
    let file = OutgoingFile {
        staged_id: Some("s3".to_owned()),
        ..bytes_file("old.bin", data(2048))
    };
    owner
        .client
        .stage_upload(&key, general(&ready), "s3".to_owned(), file.clone())
        .await
        .unwrap();
    owner.client.discard_staged_upload(&key, "s3").await;

    let fresh = data(4096);
    let message = owner
        .client
        .send_message_with_files(
            &key,
            general(&ready),
            String::new(),
            "n3".to_owned(),
            vec![OutgoingFile {
                bytes: Some(fresh.clone()),
                ..file
            }],
        )
        .await
        .unwrap();
    assert_eq!(
        download(&owner, &key, message.attachments[0].id).await,
        fresh
    );
}

/// A profile picture made on this device goes to each server, which names
/// it by the same hash; others fetch it by that hash. The server icon too.
#[tokio::test(flavor = "multi_thread")]
async fn a_profile_picture_and_the_server_icon_reach_the_server() {
    let server = TestServer::start().await;
    let (owner, _ready) = TestClient::owner(&server).await;
    let key = server.address();
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("me.png");
    image::RgbaImage::from_fn(500, 300, |x, y| image::Rgba([x as u8, y as u8, 60, 255]))
        .save(&source)
        .unwrap();
    let source = source.to_string_lossy().into_owned();

    let hash = owner
        .client
        .set_own_avatar(Some(&source))
        .unwrap()
        .expect("a picture");
    assert_eq!(owner.client.own_avatar_hash(), Some(hash.clone()));
    let user = owner.client.upload_avatar(&key).await.unwrap();
    assert_eq!(user.avatar_hash.as_deref(), Some(hash.as_str()));
    let path = owner.client.avatar_path(&key, &hash).await.unwrap();
    let png = std::fs::read(&path).unwrap();
    assert_eq!(opencord_common::avatar::hash(&png), hash);

    let info = owner
        .client
        .set_server_icon(&key, Some(source))
        .await
        .unwrap();
    assert_eq!(
        info.icon_hash,
        Some(hash),
        "the same picture, the same hash"
    );

    assert_eq!(owner.client.set_own_avatar(None).unwrap(), None);
    let user = owner.client.upload_avatar(&key).await.unwrap();
    assert_eq!(user.avatar_hash, None);
    let info = owner.client.set_server_icon(&key, None).await.unwrap();
    assert_eq!(info.icon_hash, None);
}
