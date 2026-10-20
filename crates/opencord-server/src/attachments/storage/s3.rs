//! An S3-compatible bucket. Requests are presigned with `rusty-s3` and sent
//! with the server's own HTTP client. Signed URLs carry credentials, so they
//! are never logged or returned.

use std::path::Path;
use std::time::Duration;

use bytes::Bytes;
use opencord_proto::v1 as proto;
use reqwest::StatusCode;
use reqwest::header::{CONTENT_LENGTH, ETAG, RANGE};
use rusty_s3::actions::CreateMultipartUpload;
use rusty_s3::{Bucket, Credentials, S3Action, UrlStyle};
use tokio::io::AsyncReadExt;

use super::StorageError;

const MIB: u64 = 1024 * 1024;
/// How long a presigned request stays valid.
const SIGNED_FOR: Duration = Duration::from_secs(15 * 60);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);
/// Larger files go up in parts.
const SINGLE_PUT_MAX: u64 = 64 * MIB;
const PART_BYTES: u64 = 16 * MIB;
/// S3's smallest part, used by the bucket test.
const TEST_PART_BYTES: u64 = 5 * MIB;

/// A bucket's settings. No `Debug`: it holds the secret.
#[derive(Clone)]
pub struct S3Settings {
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub key_prefix: String,
    pub path_style: bool,
    pub allow_http: bool,
    pub access_key_id: String,
    pub secret_access_key: String,
}

/// `Credentials` keeps the secret out of `Debug`.
#[derive(Debug)]
pub struct S3Storage {
    bucket: Bucket,
    credentials: Credentials,
    key_prefix: String,
    http: reqwest::Client,
}

impl S3Storage {
    /// Errors are messages for the person who typed the settings.
    pub fn new(settings: &S3Settings, http: reqwest::Client) -> Result<Self, String> {
        let endpoint = reqwest::Url::parse(settings.endpoint.trim())
            .map_err(|_| "the endpoint isn't a web address".to_owned())?;
        match endpoint.scheme() {
            "https" => {}
            "http" if settings.allow_http => {}
            "http" => return Err("a plain http:// endpoint needs \"Allow plain HTTP\"".to_owned()),
            _ => return Err("the endpoint must start with https://".to_owned()),
        }
        let style = if settings.path_style {
            UrlStyle::Path
        } else {
            UrlStyle::VirtualHost
        };
        let bucket = Bucket::new(
            endpoint,
            style,
            settings.bucket.clone(),
            settings.region.clone(),
        )
        .map_err(|error| format!("the bucket settings don't work: {error}"))?;
        Ok(Self {
            bucket,
            credentials: Credentials::new(
                settings.access_key_id.clone(),
                settings.secret_access_key.clone(),
            ),
            key_prefix: settings.key_prefix.clone(),
            http,
        })
    }

    fn object(&self, key: &str) -> String {
        format!("{}{key}", self.key_prefix)
    }

    async fn send(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<reqwest::Response, StorageError> {
        request
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(network_error)
    }

    /// Small files in one request, larger ones in parts. The file at `path`
    /// stays where it is; the caller removes it.
    pub async fn put_file(&self, key: &str, path: &Path) -> Result<(), StorageError> {
        let size = tokio::fs::metadata(path).await?.len();
        if size <= SINGLE_PUT_MAX {
            let bytes = tokio::fs::read(path).await?;
            return self.put_bytes(key, bytes).await;
        }
        self.put_parts(key, path, PART_BYTES).await
    }

    pub async fn put_bytes(&self, key: &str, bytes: Vec<u8>) -> Result<(), StorageError> {
        let object = self.object(key);
        let url = self
            .bucket
            .put_object(Some(&self.credentials), &object)
            .sign(SIGNED_FOR);
        let response = self.send(self.http.put(url).body(bytes)).await?;
        if !response.status().is_success() {
            return Err(failure(response).await);
        }
        Ok(())
    }

    pub async fn get_range(&self, key: &str, start: u64, end: u64) -> Result<Bytes, StorageError> {
        if end <= start {
            return Ok(Bytes::new());
        }
        let object = self.object(key);
        let url = self
            .bucket
            .get_object(Some(&self.credentials), &object)
            .sign(SIGNED_FOR);
        let range = format!("bytes={start}-{}", end - 1);
        let response = self.send(self.http.get(url).header(RANGE, range)).await?;
        match response.status() {
            StatusCode::PARTIAL_CONTENT => {}
            // Some servers answer a range covering the whole object in full.
            StatusCode::OK if start == 0 => {}
            StatusCode::NOT_FOUND => return Err(StorageError::NotFound),
            _ => return Err(failure(response).await),
        }
        let bytes = response.bytes().await.map_err(network_error)?;
        if bytes.len() as u64 != end - start {
            return Err(StorageError::Unavailable(format!(
                "asked for {} bytes and got {}",
                end - start,
                bytes.len()
            )));
        }
        Ok(bytes)
    }

    pub async fn size(&self, key: &str) -> Result<Option<u64>, StorageError> {
        let object = self.object(key);
        let url = self
            .bucket
            .head_object(Some(&self.credentials), &object)
            .sign(SIGNED_FOR);
        let response = self.send(self.http.head(url)).await?;
        match response.status() {
            StatusCode::NOT_FOUND => Ok(None),
            status if status.is_success() => {
                // Read the header itself: a HEAD answer has no body to count.
                let length = response
                    .headers()
                    .get(CONTENT_LENGTH)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse().ok())
                    .ok_or_else(|| {
                        StorageError::Unavailable("the storage didn't give the file's size".into())
                    })?;
                Ok(Some(length))
            }
            _ => Err(failure(response).await),
        }
    }

    pub async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let object = self.object(key);
        let url = self
            .bucket
            .delete_object(Some(&self.credentials), &object)
            .sign(SIGNED_FOR);
        let response = self.send(self.http.delete(url)).await?;
        if response.status().is_success() || response.status() == StatusCode::NOT_FOUND {
            Ok(())
        } else {
            Err(failure(response).await)
        }
    }

    /// A multipart upload in parts of `part_bytes`, aborted on any failure.
    async fn put_parts(&self, key: &str, path: &Path, part_bytes: u64) -> Result<(), StorageError> {
        let object = self.object(key);
        let url = self
            .bucket
            .create_multipart_upload(Some(&self.credentials), &object)
            .sign(SIGNED_FOR);
        let response = self.send(self.http.post(url)).await?;
        if !response.status().is_success() {
            return Err(failure(response).await);
        }
        let text = response.text().await.map_err(network_error)?;
        let created = CreateMultipartUpload::parse_response(&text).map_err(|error| {
            StorageError::Unavailable(format!("the storage's answer couldn't be read: {error}"))
        })?;
        let upload_id = created.upload_id();
        let result = self.send_parts(&object, upload_id, path, part_bytes).await;
        if result.is_err() {
            let url = self
                .bucket
                .abort_multipart_upload(Some(&self.credentials), &object, upload_id)
                .sign(SIGNED_FOR);
            if let Err(error) = self.send(self.http.delete(url)).await {
                tracing::warn!(%error, "an unfinished multipart upload couldn't be aborted");
            }
        }
        result
    }

    async fn send_parts(
        &self,
        object: &str,
        upload_id: &str,
        path: &Path,
        part_bytes: u64,
    ) -> Result<(), StorageError> {
        let mut file = tokio::fs::File::open(path).await?;
        let mut etags = Vec::new();
        loop {
            let mut part = Vec::new();
            (&mut file).take(part_bytes).read_to_end(&mut part).await?;
            if part.is_empty() {
                break;
            }
            let number = u16::try_from(etags.len() + 1)
                .map_err(|_| StorageError::Unavailable("the file has too many parts".into()))?;
            let url = self
                .bucket
                .upload_part(Some(&self.credentials), object, number, upload_id)
                .sign(SIGNED_FOR);
            let response = self.send(self.http.put(url).body(part)).await?;
            if !response.status().is_success() {
                return Err(failure(response).await);
            }
            let etag = response
                .headers()
                .get(ETAG)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| {
                    StorageError::Unavailable("the storage gave a part no ETag".into())
                })?;
            etags.push(etag.to_owned());
        }
        let action = self.bucket.complete_multipart_upload(
            Some(&self.credentials),
            object,
            upload_id,
            etags.iter().map(String::as_str),
        );
        // Sign first: `body` consumes the action.
        let url = action.sign(SIGNED_FOR);
        let response = self.send(self.http.post(url).body(action.body())).await?;
        if !response.status().is_success() {
            return Err(failure(response).await);
        }
        // S3 can answer 200 and still report an error in the body.
        let text = response.text().await.map_err(network_error)?;
        if text.contains("<Error>") {
            return Err(StorageError::Unavailable(error_message(
                StatusCode::OK,
                &text,
            )));
        }
        Ok(())
    }

    /// Checks that the bucket takes writes, ranged reads, multipart uploads
    /// and deletes. Stops at the first failing step.
    pub async fn test(&self, temp_dir: &Path) -> proto::StorageTest {
        let run = format!("opencord-test/{}", hex::encode(crate::random::bytes::<8>()));
        let small = format!("{run}/small");
        let parts = format!("{run}/parts");
        let mut steps = Vec::new();
        let ok = self.run_test(&small, &parts, temp_dir, &mut steps).await;
        if !ok {
            // A failed step may leave a test file behind.
            for key in [&small, &parts] {
                if let Err(error) = self.delete(key).await {
                    tracing::debug!(%error, "a storage test file couldn't be deleted");
                }
            }
        }
        proto::StorageTest { steps, ok }
    }

    async fn run_test(
        &self,
        small: &str,
        parts: &str,
        temp_dir: &Path,
        steps: &mut Vec<proto::StorageTestStep>,
    ) -> bool {
        let data = crate::random::bytes::<1024>().to_vec();
        let written = self.put_bytes(small, data.clone()).await;
        if !record(steps, "Write a small file", written) {
            return false;
        }
        let read = match self.get_range(small, 100, 200).await {
            Ok(bytes) if bytes[..] == data[100..200] => Ok(()),
            Ok(_) => Err(StorageError::Unavailable(
                "the bytes read back differ".into(),
            )),
            Err(error) => Err(error),
        };
        if !record(steps, "Read part of it back", read) {
            return false;
        }
        let uploaded = self.test_parts(parts, temp_dir).await;
        if !record(steps, "Upload in parts", uploaded) {
            return false;
        }
        let deleted = match self.delete(small).await {
            Ok(()) => self.delete(parts).await,
            Err(error) => Err(error),
        };
        record(steps, "Delete the test files", deleted)
    }

    /// Two parts, 5 MiB + 1 KiB, through a temp file.
    async fn test_parts(&self, key: &str, temp_dir: &Path) -> Result<(), StorageError> {
        const SIZE: u64 = TEST_PART_BYTES + 1024;
        let name = format!(
            "storage-test-{}.tmp",
            hex::encode(crate::random::bytes::<8>())
        );
        let path = temp_dir.join(name);
        let result = async {
            tokio::fs::write(&path, vec![0x5a; SIZE as usize]).await?;
            self.put_parts(key, &path, TEST_PART_BYTES).await?;
            match self.size(key).await? {
                Some(SIZE) => Ok(()),
                other => Err(StorageError::Unavailable(format!(
                    "the uploaded file has the wrong size ({other:?} bytes)"
                ))),
            }
        }
        .await;
        if let Err(error) = tokio::fs::remove_file(&path).await
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(%error, "a storage test's temp file couldn't be deleted");
        }
        result
    }
}

fn record(
    steps: &mut Vec<proto::StorageTestStep>,
    name: &str,
    result: Result<(), StorageError>,
) -> bool {
    let detail = match &result {
        Ok(()) => String::new(),
        Err(StorageError::Unavailable(message)) => message.clone(),
        Err(error) => error.to_string(),
    };
    steps.push(proto::StorageTestStep {
        name: name.to_owned(),
        ok: result.is_ok(),
        detail,
    });
    result.is_ok()
}

/// A network error without its URL: the URL carries the signature.
fn network_error(error: reqwest::Error) -> StorageError {
    StorageError::Unavailable(error.without_url().to_string())
}

async fn failure(response: reqwest::Response) -> StorageError {
    let status = response.status();
    // The body only improves the message; without it the status still says
    // what happened.
    let body = response.text().await.unwrap_or_default();
    StorageError::Unavailable(error_message(status, &body))
}

/// "Code: Message" from an S3 error body, or the HTTP status. At most 200
/// characters.
fn error_message(status: StatusCode, body: &str) -> String {
    let field = |name: &str| {
        let open = format!("<{name}>");
        let start = body.find(&open)? + open.len();
        let end = start + body[start..].find(&format!("</{name}>"))?;
        Some(body[start..end].trim().to_owned())
    };
    let message = match (field("Code"), field("Message")) {
        (Some(code), Some(message)) => format!("{code}: {message}"),
        (Some(code), None) => code,
        _ => format!("HTTP {status}"),
    };
    message.chars().take(200).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(endpoint: &str, allow_http: bool) -> S3Settings {
        S3Settings {
            endpoint: endpoint.into(),
            region: "us-east-1".into(),
            bucket: "files".into(),
            key_prefix: String::new(),
            path_style: true,
            allow_http,
            access_key_id: "AKIAEXAMPLE".into(),
            secret_access_key: "a-very-secret-value".into(),
        }
    }

    #[test]
    fn http_endpoints_need_allow_http() {
        let http = crate::tls::http_client().unwrap();
        assert!(S3Storage::new(&settings("http://127.0.0.1:9000", false), http.clone()).is_err());
        assert!(S3Storage::new(&settings("http://127.0.0.1:9000", true), http.clone()).is_ok());
        assert!(S3Storage::new(&settings("https://s3.example.com", false), http.clone()).is_ok());
        assert!(S3Storage::new(&settings("ftp://s3.example.com", true), http.clone()).is_err());
        assert!(S3Storage::new(&settings("not a url", true), http).is_err());
    }

    #[test]
    fn debug_output_hides_the_secret() {
        let storage = S3Storage::new(
            &settings("https://s3.example.com", false),
            crate::tls::http_client().unwrap(),
        )
        .unwrap();
        assert!(!format!("{storage:?}").contains("a-very-secret-value"));
    }

    #[test]
    fn error_bodies_become_short_messages() {
        let body = "<?xml version=\"1.0\"?><Error><Code>NoSuchBucket</Code>\
                    <Message>The specified bucket does not exist</Message></Error>";
        assert_eq!(
            error_message(StatusCode::NOT_FOUND, body),
            "NoSuchBucket: The specified bucket does not exist"
        );
        assert_eq!(
            error_message(StatusCode::BAD_GATEWAY, "<html>proxy error</html>"),
            "HTTP 502 Bad Gateway"
        );
        let long = format!(
            "<Error><Code>X</Code><Message>{}</Message></Error>",
            "m".repeat(500)
        );
        assert_eq!(
            error_message(StatusCode::BAD_REQUEST, &long)
                .chars()
                .count(),
            200
        );
    }
}
