//! Tests against a real S3-compatible server. They are ignored by default.
//!
//! ```bash
//! docker run -d --rm --name opencord-s3 -p 9000:9000 \
//!   -e RUSTFS_ACCESS_KEY=opencord -e RUSTFS_SECRET_KEY=opencord-secret \
//!   rustfs/rustfs:1.0.3 /data
//! cargo test -p opencord-server --test s3 -- --ignored --test-threads=1
//! docker stop opencord-s3
//! ```
//!
//! `OPENCORD_TEST_S3_ENDPOINT` overrides the endpoint (default
//! `http://127.0.0.1:9000`).

mod common;

use std::time::Duration;

use common::*;
use opencord_proto::v1 as proto;
use opencord_server::attachments::storage::{S3Settings, S3Storage};
use proto::request::Kind as Request;
use proto::response::Result as Response;
use rusty_s3::{Bucket, Credentials, S3Action, UrlStyle};

const ACCESS_KEY: &str = "opencord";
const SECRET_KEY: &str = "opencord-secret";

fn endpoint() -> String {
    std::env::var("OPENCORD_TEST_S3_ENDPOINT").unwrap_or_else(|_| "http://127.0.0.1:9000".into())
}

fn code(error: &proto::Error) -> proto::ErrorCode {
    proto::ErrorCode::try_from(error.code).unwrap()
}

fn get_storage(res: Response) -> proto::Storage {
    match res {
        Response::Storage(storage) => storage,
        other => panic!("expected Storage, got {other:?}"),
    }
}

/// A new, empty bucket for one test. `drop_bucket` removes it.
struct TestBucket {
    name: String,
    http: reqwest::Client,
}

impl TestBucket {
    async fn create() -> Self {
        let name = format!("opencord-test-{}", hex::encode(rand_bytes()));
        let http = opencord_server::tls::http_client().unwrap();
        let bucket = Bucket::new(
            endpoint().parse().unwrap(),
            UrlStyle::Path,
            name.clone(),
            "us-east-1",
        )
        .unwrap();
        let credentials = Credentials::new(ACCESS_KEY, SECRET_KEY);
        let url = bucket
            .create_bucket(&credentials)
            .sign(Duration::from_secs(60));
        let response = http.put(url).send().await.unwrap();
        assert!(
            response.status().is_success(),
            "{}",
            response.text().await.unwrap()
        );
        Self { name, http }
    }

    fn settings(&self, secret: &str) -> S3Settings {
        S3Settings {
            endpoint: endpoint(),
            region: "us-east-1".into(),
            bucket: self.name.clone(),
            key_prefix: "files/".into(),
            path_style: true,
            allow_http: true,
            access_key_id: ACCESS_KEY.into(),
            secret_access_key: secret.into(),
        }
    }

    fn storage(&self) -> S3Storage {
        S3Storage::new(&self.settings(SECRET_KEY), self.http.clone()).unwrap()
    }

    fn proto_config(&self) -> proto::S3Config {
        proto::S3Config {
            endpoint: endpoint(),
            region: "us-east-1".into(),
            bucket: self.name.clone(),
            key_prefix: "files/".into(),
            path_style: true,
            allow_http: true,
            access_key_id: ACCESS_KEY.into(),
            secret_access_key: SECRET_KEY.into(),
        }
    }

    async fn drop_bucket(self) {
        let bucket = Bucket::new(
            endpoint().parse().unwrap(),
            UrlStyle::Path,
            self.name.clone(),
            "us-east-1",
        )
        .unwrap();
        let credentials = Credentials::new(ACCESS_KEY, SECRET_KEY);
        let url = bucket
            .delete_bucket(&credentials)
            .sign(Duration::from_secs(60));
        let response = self.http.delete(url).send().await.unwrap();
        assert!(
            response.status().is_success(),
            "{}",
            response.text().await.unwrap()
        );
    }
}

fn rand_bytes() -> [u8; 6] {
    opencord_server::random::bytes::<6>()
}

#[tokio::test]
#[ignore = "needs an S3 server; see the file header"]
async fn round_trip_ranges_and_delete() {
    let bucket = TestBucket::create().await;
    let storage = bucket.storage();
    let data: Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();
    storage.put_bytes("a/1", data.clone()).await.unwrap();
    assert_eq!(storage.size("a/1").await.unwrap(), Some(1000));
    assert_eq!(
        storage.get_range("a/1", 10, 20).await.unwrap(),
        &data[10..20]
    );
    assert_eq!(storage.get_range("a/1", 0, 1000).await.unwrap(), &data[..]);
    storage.delete("a/1").await.unwrap();
    storage.delete("a/1").await.unwrap();
    assert_eq!(storage.size("a/1").await.unwrap(), None);
    bucket.drop_bucket().await;
}

#[tokio::test]
#[ignore = "needs an S3 server; see the file header"]
async fn multipart_put_file_of_70_mib() {
    let bucket = TestBucket::create().await;
    let storage = bucket.storage();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big");
    let data: Vec<u8> = (0..70 * 1024 * 1024u32).map(|i| (i % 251) as u8).collect();
    tokio::fs::write(&path, &data).await.unwrap();
    storage.put_file("a/2", &path).await.unwrap();
    assert!(path.exists(), "S3 leaves the source file for the caller");
    assert_eq!(storage.size("a/2").await.unwrap(), Some(data.len() as u64));
    let boundary = 16 * 1024 * 1024;
    assert_eq!(
        storage
            .get_range("a/2", boundary - 10, boundary + 10)
            .await
            .unwrap(),
        &data[boundary as usize - 10..boundary as usize + 10]
    );
    storage.delete("a/2").await.unwrap();
    bucket.drop_bucket().await;
}

#[tokio::test]
#[ignore = "needs an S3 server; see the file header"]
async fn the_bucket_test_passes_on_s3_and_fails_on_a_wrong_secret() {
    let bucket = TestBucket::create().await;
    let dir = tempfile::tempdir().unwrap();
    let passed = bucket.storage().test(dir.path()).await;
    assert!(passed.ok, "{passed:?}");
    assert_eq!(passed.steps.len(), 4);

    let wrong = S3Storage::new(&bucket.settings("wrong-secret"), bucket.http.clone()).unwrap();
    let failed = wrong.test(dir.path()).await;
    assert!(!failed.ok);
    assert_eq!(failed.steps.len(), 1);
    assert_eq!(failed.steps[0].name, "Write a small file");
    assert!(!failed.steps[0].detail.is_empty());
    println!("wrong secret: {}", failed.steps[0].detail);
    bucket.drop_bucket().await;
}

#[tokio::test]
#[ignore = "needs an S3 server; see the file header"]
async fn the_secret_never_comes_back() {
    let bucket = TestBucket::create().await;
    let server = TestServer::start().await;
    let (mut client, _ready) = owner(&server).await;

    let res = client
        .ok(Request::AddStorage(proto::AddStorage {
            name: "S3 Primary".into(),
            s3: Some(bucket.proto_config()),
        }))
        .await;
    let added = get_storage(res);
    let s3_backend = added
        .backends
        .iter()
        .find(|b| b.name == "S3 Primary")
        .unwrap();
    let s3_conf = s3_backend.s3.as_ref().unwrap();
    assert_eq!(s3_conf.secret_access_key, "");
    assert!(s3_backend.has_secret);
    assert_eq!(s3_conf.access_key_id, "••••cord");

    let fetched = get_storage(client.ok(Request::GetStorage(proto::GetStorage {})).await);
    let s3_fetched = fetched
        .backends
        .iter()
        .find(|b| b.name == "S3 Primary")
        .unwrap();
    let s3_fetched_conf = s3_fetched.s3.as_ref().unwrap();
    assert_eq!(s3_fetched_conf.secret_access_key, "");
    assert!(s3_fetched.has_secret);
    assert_eq!(s3_fetched_conf.access_key_id, "••••cord");

    bucket.drop_bucket().await;
}

#[tokio::test]
#[ignore = "needs an S3 server; see the file header"]
async fn activating_s3_sends_new_uploads_there_and_survives_a_restart() {
    let bucket = TestBucket::create().await;
    let server = TestServer::start().await;
    let (mut client, ready) = owner(&server).await;
    let token = media_token(&ready);
    let channel_id = channel_id(&ready, "general");

    let res = client
        .ok(Request::AddStorage(proto::AddStorage {
            name: "S3 Primary".into(),
            s3: Some(bucket.proto_config()),
        }))
        .await;
    let s3_id = get_storage(res)
        .backends
        .iter()
        .find(|b| b.name == "S3 Primary")
        .unwrap()
        .id;
    client
        .ok(Request::ActivateStorage(proto::ActivateStorage {
            storage_id: s3_id,
        }))
        .await;

    let content = b"hello s3 storage upload";
    let att = upload(
        &mut client,
        &server,
        &token,
        channel_id,
        "s3_file.txt",
        content,
    )
    .await;
    client
        .ok(Request::SendMessage(proto::SendMessage {
            channel_id,
            content: "sending to s3".into(),
            attachment_ids: vec![att.id],
            ..Default::default()
        }))
        .await;

    let object_key = format!("a/{}", att.id);
    assert!(bucket.storage().size(&object_key).await.unwrap().is_some());

    let auth = format!("Bearer {token}");
    let (status, dl) = server
        .send_http(
            "GET",
            &format!("/media/attachments/{}", att.id),
            &[("Authorization", &auth)],
            &[],
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(dl, content);

    let server = server.restart(|_| {}).await;
    let (mut client, ready) = owner_again(&server).await;
    let token = media_token(&ready);
    let auth = format!("Bearer {token}");

    let storage = get_storage(client.ok(Request::GetStorage(proto::GetStorage {})).await);
    let s3 = storage.backends.iter().find(|b| b.id == s3_id).unwrap();
    assert!(s3.active);

    let (status, dl) = server
        .send_http(
            "GET",
            &format!("/media/attachments/{}", att.id),
            &[("Authorization", &auth)],
            &[],
        )
        .await;
    assert_eq!(status, 200);
    assert_eq!(dl, content);

    let content2 = b"second upload to s3 after restart";
    let att2 = upload(
        &mut client,
        &server,
        &token,
        channel_id,
        "s3_file2.txt",
        content2,
    )
    .await;
    let object_key2 = format!("a/{}", att2.id);
    assert!(bucket.storage().size(&object_key2).await.unwrap().is_some());

    let err = client
        .error(Request::RemoveStorage(proto::RemoveStorage {
            storage_id: s3_id,
        }))
        .await;
    assert_eq!(code(&err), proto::ErrorCode::Conflict);

    bucket.storage().delete(&object_key).await.unwrap();
    bucket.storage().delete(&object_key2).await.unwrap();
    bucket.drop_bucket().await;
}

#[tokio::test]
#[ignore = "needs an S3 server; see the file header"]
async fn a_storage_holding_files_keeps_its_bucket() {
    let bucket = TestBucket::create().await;
    let server = TestServer::start().await;
    let (mut client, ready) = owner(&server).await;
    let token = media_token(&ready);
    let channel_id = channel_id(&ready, "general");

    let res = client
        .ok(Request::AddStorage(proto::AddStorage {
            name: "S3 Primary".into(),
            s3: Some(bucket.proto_config()),
        }))
        .await;
    let s3_id = get_storage(res)
        .backends
        .iter()
        .find(|b| b.name == "S3 Primary")
        .unwrap()
        .id;
    client
        .ok(Request::ActivateStorage(proto::ActivateStorage {
            storage_id: s3_id,
        }))
        .await;

    let content = b"holding file";
    let att = upload(
        &mut client,
        &server,
        &token,
        channel_id,
        "file.txt",
        content,
    )
    .await;
    client
        .ok(Request::SendMessage(proto::SendMessage {
            channel_id,
            content: "sending".into(),
            attachment_ids: vec![att.id],
            ..Default::default()
        }))
        .await;

    let mut other_conf = bucket.proto_config();
    other_conf.bucket = "different-bucket".into();
    let err = client
        .error(Request::UpdateStorage(proto::UpdateStorage {
            storage_id: s3_id,
            name: None,
            s3: Some(other_conf),
        }))
        .await;
    assert_eq!(code(&err), proto::ErrorCode::Conflict);

    client
        .ok(Request::UpdateStorage(proto::UpdateStorage {
            storage_id: s3_id,
            name: Some("Renamed S3".into()),
            s3: None,
        }))
        .await;

    let storage = get_storage(client.ok(Request::GetStorage(proto::GetStorage {})).await);
    let s3 = storage.backends.iter().find(|b| b.id == s3_id).unwrap();
    assert_eq!(s3.name, "Renamed S3");

    bucket
        .storage()
        .delete(&format!("a/{}", att.id))
        .await
        .unwrap();
    bucket.drop_bucket().await;
}
