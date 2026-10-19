//! Files in messages, upload settings and storage, against a real server
//! (media uploads plan, milestone A4).

use std::time::Duration;

use opencord_common::address::format_fingerprint;
use opencord_core::api::types::{
    AttachmentVariant, CoreError, CoreEvent, CoreEventPayload, ErrorCode, OutgoingFile,
    ReadySnapshot, RoleChanges, StorageKind, UploadPhase,
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
    assert_eq!(ready.upload_settings.voice_message_max_seconds, 300);
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
        .update_upload_settings(&key, Some(vec!["psd".to_owned()]), Some(120))
        .await
        .unwrap();

    assert_eq!(settings.blocked_extensions, ["psd"]);
    assert_eq!(settings.voice_message_max_seconds, 120);
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
        .update_upload_settings(&key, Some(vec!["exe".to_owned()]), None)
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
