//! The views service with a fake server: what a real server can't be made
//! to send (it refuses packages with a newer `api` at install).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::future::BoxFuture;
use opencord_plugins::view::{Batch, Patch};
use opencord_proto::v1 as proto;
use proto::request::Kind as Request;
use proto::response::Result as Response;

use super::{ViewTransport, Views};
use crate::api::types::CoreError;
use crate::http::HttpResponse;
use crate::views::Viewer;

const CONTEXT: &str = r#"{"width":800,"height":600,"theme":"dark","input":"mouse",
    "safe_area":{"top":0,"right":0,"bottom":0,"left":0},"text_scale":1}"#;

/// Answers every `OpenView` with `opened`, and records what it was asked.
struct FakeServer {
    opened: proto::ViewOpened,
    asked: Mutex<Vec<Request>>,
    fetched: Mutex<Vec<String>>,
}

impl ViewTransport for FakeServer {
    fn request(
        &self,
        _key: &str,
        kind: Request,
    ) -> BoxFuture<'static, Result<Response, CoreError>> {
        self.asked.lock().unwrap().push(kind.clone());
        let opened = self.opened.clone();
        Box::pin(async move {
            Ok(match kind {
                Request::OpenView(_) => Response::ViewOpened(opened),
                _ => Response::Ack(proto::Ack {}),
            })
        })
    }

    fn get(
        &self,
        _key: &str,
        path: String,
        _limit: usize,
    ) -> BoxFuture<'static, Result<HttpResponse, CoreError>> {
        self.fetched.lock().unwrap().push(path);
        Box::pin(async {
            Ok(HttpResponse {
                status: 200,
                body: CHANGED.as_bytes().to_vec(),
            })
        })
    }
}

/// What the fake serves for every file: a script that would draw.
const CHANGED: &str = r#"function init() return {} end
function render() return ui.text({ text = "changed" }) end"#;

fn opened(api: u32, sha256: String) -> proto::ViewOpened {
    proto::ViewOpened {
        view_id: 5,
        plugin_id: "tickets".into(),
        api,
        entry: "client/main.luau".into(),
        files: vec![proto::PluginFileRef {
            path: "client/main.luau".into(),
            sha256,
            size_bytes: CHANGED.len() as u64,
        }],
        show_errors: true,
    }
}

fn views_with(server: Arc<FakeServer>, dir: &tempfile::TempDir) -> Views {
    let views = Views::new(tokio::runtime::Handle::current(), dir.path(), server);
    views.set_viewer(
        "s",
        Viewer {
            id: "1".into(),
            name: "Ada".into(),
            roles: Vec::new(),
        },
    );
    views
}

async fn closed_on_server(server: &FakeServer, view_id: i64) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !server
            .asked
            .lock()
            .unwrap()
            .iter()
            .any(|kind| matches!(kind, Request::CloseView(close) if close.view_id == view_id))
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the server's view was never closed");
}

// Fails if the core runs a fetched file without checking it against the
// SHA-256 the server listed (the changed script would draw "changed").
#[tokio::test]
async fn a_changed_file_with_an_old_hash_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let old_hash = "ab".repeat(32);
    let server = Arc::new(FakeServer {
        opened: opened(opencord_plugins::API_VERSION, old_hash.clone()),
        asked: Mutex::default(),
        fetched: Mutex::default(),
    });
    let views = views_with(server.clone(), &dir);
    let mut outputs = views.outputs();

    let id = views.open("s", 42, CONTEXT).await.unwrap();

    let output = tokio::time::timeout(Duration::from_secs(5), outputs.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(output.view_id, id);
    let batch: Batch = serde_json::from_str(&output.json).unwrap();
    let [Patch::Error { details, .. }] = batch.ops.as_slice() else {
        panic!("not an error op alone: {batch:?}");
    };
    let details = details.as_deref().unwrap_or_default();
    assert!(details.contains("doesn't match its hash"), "{details}");
    assert!(!dir.path().join("cache/plugins").join(&old_hash).exists());
    closed_on_server(&server, 5).await;
}

// Fails if `open` starts the VM (or fetches files) for a manifest `api`
// above `API_VERSION` instead of sending `too_old`, or leaves the view
// open on the server.
#[tokio::test]
async fn a_too_new_api_sends_too_old() {
    let dir = tempfile::tempdir().unwrap();
    let server = Arc::new(FakeServer {
        opened: opened(opencord_plugins::API_VERSION + 1, "0".repeat(64)),
        asked: Mutex::default(),
        fetched: Mutex::default(),
    });
    let views = views_with(server.clone(), &dir);
    let mut outputs = views.outputs();

    let id = views.open("s", 42, CONTEXT).await.unwrap();

    let output = tokio::time::timeout(Duration::from_secs(5), outputs.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(output.view_id, id);
    let batch: Batch = serde_json::from_str(&output.json).unwrap();
    assert_eq!(batch.ops, vec![Patch::TooOld]);
    assert!(server.fetched.lock().unwrap().is_empty());
    closed_on_server(&server, 5).await;
}
