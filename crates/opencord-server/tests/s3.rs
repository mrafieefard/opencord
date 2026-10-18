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

use std::time::Duration;

use opencord_server::attachments::storage::{S3Settings, S3Storage};
use rusty_s3::{Bucket, Credentials, S3Action, UrlStyle};

const ACCESS_KEY: &str = "opencord";
const SECRET_KEY: &str = "opencord-secret";

fn endpoint() -> String {
    std::env::var("OPENCORD_TEST_S3_ENDPOINT").unwrap_or_else(|_| "http://127.0.0.1:9000".into())
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
