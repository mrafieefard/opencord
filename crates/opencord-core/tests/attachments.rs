//! Files in messages, upload settings and storage, against a real server
//! (media uploads plan, milestone A4).

use std::time::Duration;

use opencord_common::address::format_fingerprint;
use opencord_core::api::types::{
    CoreEvent, CoreEventPayload, ReadySnapshot, RoleChanges, StorageKind,
};
use opencord_core::client::Client;
use opencord_core::identity::Identity;
use opencord_server::config::Config;
use opencord_server::server::{self, ServerHandle};
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
