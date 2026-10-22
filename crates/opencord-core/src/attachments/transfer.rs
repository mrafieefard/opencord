//! HTTPS to a server's upload and download endpoints, checked like its
//! gateway (media uploads plan, Task 18). Unlike `http.rs`, 8 MiB pieces go
//! up and whole files come down, so each server gets one client whose
//! connections are kept, and answers are read as they arrive.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use bytes::Bytes;
use opencord_common::address::format_fingerprint;
use opencord_proto::v1 as proto;
use prost::Message as _;
use reqwest::StatusCode;

use crate::api::types::CoreError;
use crate::client::Client;
use crate::connection::{MediaTokens, Route, Routes};
use crate::convert;
use crate::http::{media_token, renew_media_token};
use crate::tofu::{TlsTrust, Verdict};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// The longest wait for more of an answer.
const READ_TIMEOUT: Duration = Duration::from_secs(60);
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);

/// One request to a media endpoint.
pub(crate) struct Call {
    pub method: reqwest::Method,
    /// From the root, like `/media/uploads/1`.
    pub path: String,
    pub headers: Vec<(&'static str, String)>,
    pub body: Bytes,
    /// For the whole request; `None` for downloads, which may take hours.
    pub timeout: Option<Duration>,
}

impl Call {
    pub fn get(path: String, timeout: Option<Duration>) -> Self {
        Self {
            method: reqwest::Method::GET,
            path,
            headers: Vec::new(),
            body: Bytes::new(),
            timeout,
        }
    }
}

pub(crate) struct Transfers {
    routes: Routes,
    tokens: MediaTokens,
    /// Each server's client, built for the route it was last reached by.
    /// An entry leaves when its server has no route any more.
    clients: Mutex<HashMap<String, Pooled>>,
}

#[derive(Clone)]
struct Pooled {
    route: Route,
    http: reqwest::Client,
    trust: Arc<TlsTrust>,
}

impl Transfers {
    pub fn new(routes: Routes, tokens: MediaTokens) -> Self {
        Self {
            routes,
            tokens,
            clients: Mutex::default(),
        }
    }

    /// Sends `call` to the server at `key` with this device's media token,
    /// renewing an expired token once. Any answer comes back, whatever its
    /// status; only failing to reach the server is an error.
    pub async fn send(
        &self,
        client: &Client,
        key: &str,
        call: Call,
    ) -> Result<reqwest::Response, CoreError> {
        let pooled = self.pooled(key)?;
        let url = format!("https://{}{}", pooled.route.address, call.path);
        let mut renewed = false;
        loop {
            let token = media_token(&self.tokens, key).ok_or(CoreError::NotConnected)?;
            let mut request = pooled
                .http
                .request(call.method.clone(), &url)
                .bearer_auth(token)
                .body(call.body.clone());
            for (name, value) in &call.headers {
                request = request.header(*name, value);
            }
            if let Some(timeout) = call.timeout {
                request = request.timeout(timeout);
            }
            let response = request
                .send()
                .await
                .map_err(|error| failure(&error, &pooled.trust))?;
            if response.status() != StatusCode::UNAUTHORIZED || renewed {
                return Ok(response);
            }
            renew_media_token(client, &self.tokens, key).await?;
            renewed = true;
        }
    }

    /// The client for the server at `key`, built again when the server
    /// was reached another way.
    fn pooled(&self, key: &str) -> Result<Pooled, CoreError> {
        let route = self
            .routes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(key)
            .cloned();
        let mut clients = self.lock_clients();
        let Some(route) = route else {
            clients.remove(key);
            return Err(CoreError::NotConnected);
        };
        if let Some(pooled) = clients.get(key)
            && pooled.route == route
        {
            return Ok(pooled.clone());
        }
        let trust = Arc::new(TlsTrust::new(route.pin));
        let http = reqwest::Client::builder()
            .tls_backend_preconfigured((*trust.config).clone())
            .connect_timeout(CONNECT_TIMEOUT)
            .read_timeout(READ_TIMEOUT)
            .pool_idle_timeout(IDLE_TIMEOUT)
            .build()
            .map_err(|error| network(&error))?;
        let pooled = Pooled { route, http, trust };
        clients.insert(key.to_owned(), pooled.clone());
        Ok(pooled)
    }

    fn lock_clients(&self) -> MutexGuard<'_, HashMap<String, Pooled>> {
        self.clients.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Why a request didn't reach the server: its certificate changed, or the
/// network.
fn failure(error: &reqwest::Error, trust: &TlsTrust) -> CoreError {
    match trust.verdict() {
        Some(Verdict::Mismatch {
            expected,
            presented,
        }) => CoreError::FingerprintMismatch {
            expected: format_fingerprint(&expected),
            presented: format_fingerprint(&presented),
        },
        _ => network(error),
    }
}

/// A network failure, said by its first cause: reqwest's own text only
/// names the request.
pub(crate) fn network(error: &reqwest::Error) -> CoreError {
    if error.is_timeout() {
        return CoreError::Connection {
            message: "the server stopped answering".to_owned(),
        };
    }
    let mut cause: &dyn std::error::Error = error;
    while let Some(inner) = cause.source() {
        cause = inner;
    }
    CoreError::Connection {
        message: cause.to_string(),
    }
}

/// A media endpoint's protobuf answer (they are small), or the error it
/// holds.
async fn answer(
    response: reqwest::Response,
) -> Result<(StatusCode, proto::response::Result), CoreError> {
    let status = response.status();
    let body = response.bytes().await.map_err(|error| network(&error))?;
    let result = proto::Response::decode(body)
        .ok()
        .and_then(|response| response.result)
        .ok_or_else(|| unexpected(status))?;
    match result {
        proto::response::Result::Error(error) => Err(convert::error(error)),
        result => Ok((status, result)),
    }
}

/// The upload status an upload endpoint answered with.
pub(crate) async fn upload_status(
    response: reqwest::Response,
) -> Result<(StatusCode, proto::UploadStatus), CoreError> {
    match answer(response).await? {
        (status, proto::response::Result::UploadStatus(upload)) => Ok((status, upload)),
        (status, _) => Err(unexpected(status)),
    }
}

/// Why a download was refused.
pub(crate) async fn refusal(response: reqwest::Response) -> CoreError {
    match answer(response).await {
        Ok((status, _)) => unexpected(status),
        Err(error) => error,
    }
}

fn unexpected(status: StatusCode) -> CoreError {
    CoreError::Connection {
        message: format!("the server answered {status}"),
    }
}
