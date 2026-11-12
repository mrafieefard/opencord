//! The views service with a fake server: what a real server can't be made
//! to send (it refuses packages with a newer `api` at install).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::future::BoxFuture;
use opencord_plugins::limits::IMAGE_MAX_RAW_BYTES;
use opencord_plugins::view::{Batch, Patch};
use opencord_proto::v1 as proto;
use proto::request::Kind as Request;
use proto::response::Result as Response;
use sha2::Digest as _;

use serde_json::json;

use super::{ViewTransport, Views};
use crate::api::types::{CoreError, ErrorCode};
use crate::http::HttpResponse;
use crate::views::Viewer;

const CONTEXT: &str = r#"{"width":800,"height":600,"theme":"dark","input":"mouse",
    "safe_area":{"top":0,"right":0,"bottom":0,"left":0},"text_scale":1}"#;

/// Answers every `OpenView` with `opened`, and records what it was asked.
/// Every `GET` gets `answer`, or [`CHANGED`] when it's `None`. The first
/// `OpenView` runs `before_answer` first: what the server sends while its
/// answer is on the way.
struct FakeServer {
    opened: proto::ViewOpened,
    asked: Mutex<Vec<Request>>,
    fetched: Mutex<Vec<String>>,
    answer: Mutex<Option<HttpResponse>>,
    before_answer: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl FakeServer {
    fn new(opened: proto::ViewOpened) -> Arc<Self> {
        Arc::new(Self {
            opened,
            asked: Mutex::default(),
            fetched: Mutex::default(),
            answer: Mutex::default(),
            before_answer: Mutex::default(),
        })
    }
}

impl ViewTransport for FakeServer {
    fn request(
        &self,
        _key: &str,
        kind: Request,
    ) -> BoxFuture<'static, Result<Response, CoreError>> {
        self.asked.lock().unwrap().push(kind.clone());
        if matches!(kind, Request::OpenView(_))
            && let Some(before) = self.before_answer.lock().unwrap().take()
        {
            before();
        }
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
        let answer = self.answer.lock().unwrap().clone();
        Box::pin(async {
            Ok(answer.unwrap_or_else(|| HttpResponse {
                status: 200,
                body: CHANGED.as_bytes().to_vec(),
            }))
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
    let server = FakeServer::new(opened(opencord_plugins::API_VERSION, old_hash.clone()));
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
    let server = FakeServer::new(opened(opencord_plugins::API_VERSION + 1, "0".repeat(64)));
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

fn image_views() -> (tempfile::TempDir, Arc<FakeServer>, Views) {
    let dir = tempfile::tempdir().unwrap();
    let server = FakeServer::new(opened(opencord_plugins::API_VERSION, "0".repeat(64)));
    let views = views_with(server.clone(), &dir);
    (dir, server, views)
}

/// Every file under `dir`, however deep.
fn files_under(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .map(|entry| entry.unwrap().path())
        .flat_map(|path| {
            if path.is_dir() {
                files_under(&path)
            } else {
                vec![path]
            }
        })
        .collect()
}

// Fails if a source asks a route other than its own, or a URL goes into
// the query unencoded (its own `?` and `&` would reach the server as the
// route's).
#[tokio::test]
async fn each_image_source_asks_its_route() {
    let (_dir, server, views) = image_views();

    for source in [
        json!({ "file": "tickets/42.png" }),
        json!({ "file_token": "AbC_-9" }),
        json!({ "url": "https://example.com/a b.png?x=1&y=2" }),
    ] {
        let path = views
            .image_file("s", "tickets", &source.to_string())
            .await
            .unwrap();
        assert_eq!(std::fs::read(path).unwrap(), CHANGED.as_bytes());
    }

    assert_eq!(
        *server.fetched.lock().unwrap(),
        vec![
            "/plugins/tickets/files/tickets/42.png",
            "/plugins/tickets/files/t/AbC_-9",
            "/plugins/tickets/images?url=https%3A%2F%2Fexample.com%2Fa%20b.png%3Fx%3D1%26y%3D2",
        ]
    );
}

// Fails if an image is fetched again while its version stays (every
// redraw would fetch it), isn't fetched again when the version rises
// (§13.2), or if two versions share a path (the renderer, which caches by
// path, would keep showing the old one).
#[tokio::test]
async fn an_image_is_fetched_again_only_for_a_new_version() {
    let (_dir, server, views) = image_views();
    let image = |version: u32| {
        let views = &views;
        async move {
            let source = json!({ "file": "a.png", "version": version }).to_string();
            views.image_file("s", "tickets", &source).await.unwrap()
        }
    };

    let first = image(1).await;
    let same = image(1).await;
    let newer = image(2).await;

    assert_eq!(same, first);
    assert_ne!(newer, first);
    assert_eq!(server.fetched.lock().unwrap().len(), 2);
}

// Fails if a refused fetch loses the server's reason (the plugin's
// `on_error` gets it) or writes the error's body out as the image.
#[tokio::test]
async fn a_refused_image_carries_the_servers_reason() {
    let (dir, server, views) = image_views();
    let refusal = proto::Response {
        result: Some(Response::Error(proto::Error {
            code: proto::ErrorCode::NotFound as i32,
            message: "no plugin file at a.png".into(),
            ..Default::default()
        })),
    };
    *server.answer.lock().unwrap() = Some(HttpResponse {
        status: 404,
        body: prost::Message::encode_to_vec(&refusal),
    });

    let result = views
        .image_file("s", "tickets", r#"{"file":"a.png"}"#)
        .await;

    let Err(CoreError::Server { code, message, .. }) = result else {
        panic!("not the server's refusal: {result:?}");
    };
    assert_eq!(code, ErrorCode::NotFound);
    assert_eq!(message, "no plugin file at a.png");
    assert_eq!(files_under(dir.path()), Vec::<std::path::PathBuf>::new());
}

// Fails if a source the plugin's script makes up reaches a URL or the disk
// unchecked, or an `attachment` is written out instead of refused.
#[tokio::test]
async fn bad_image_sources_are_refused_before_any_fetch() {
    let (_dir, server, views) = image_views();
    let too_big = data_encoding::BASE64.encode(&vec![0; IMAGE_MAX_RAW_BYTES + 1]);

    for source in [
        json!("a.png"),
        json!({}),
        json!({ "file": "a.png", "url": "https://example.com/a.png" }),
        json!({ "file": "../secret" }),
        json!({ "file": "a/../../b" }),
        json!({ "file": "a.png?x=1" }),
        json!({ "file_token": "a/b" }),
        json!({ "url": "http://example.com/a.png" }),
        json!({ "url": 5 }),
        json!({ "asset": "client/main.luau" }),
        json!({ "asset": "assets/../manifest.lua" }),
        json!({ "attachment": 1 }),
        json!({ "bytes": "not base64!" }),
        json!({ "bytes": too_big }),
        json!({ "file": "a.png", "version": -1 }),
    ] {
        let result = views.image_file("s", "tickets", &source.to_string()).await;
        assert!(
            matches!(result, Err(CoreError::InvalidInput { .. })),
            "{source}: {result:?}"
        );
    }
    let result = views.image_file("s", "../x", r#"{"file":"a.png"}"#).await;
    assert!(
        matches!(result, Err(CoreError::InvalidInput { .. })),
        "{result:?}"
    );
    assert!(server.fetched.lock().unwrap().is_empty());
}

// Fails if what the server sends a view before its `ViewOpened` answer
// arrives is dropped: the server queues `view_open` for the plugin before
// it answers, so a plugin that sends from `view_open` can get there first.
#[tokio::test]
async fn a_message_before_the_open_answer_is_kept_for_the_view() {
    const SCRIPT: &str = r#"function init() return { heard = "nothing" } end
app.on("hello", function(data) app.set(function(s) s.heard = data.word end) end)
function render(s) return ui.text({ text = s.heard }) end"#;
    let dir = tempfile::tempdir().unwrap();
    let mut opened = opened(opencord_plugins::API_VERSION, String::new());
    opened.files[0].sha256 = hex::encode(sha2::Sha256::digest(SCRIPT));
    opened.files[0].size_bytes = SCRIPT.len() as u64;
    let server = FakeServer::new(opened);
    *server.answer.lock().unwrap() = Some(HttpResponse {
        status: 200,
        body: SCRIPT.as_bytes().to_vec(),
    });
    let views = views_with(server.clone(), &dir);
    let early = views.clone();
    *server.before_answer.lock().unwrap() = Some(Box::new(move || {
        early.on_message(
            "s",
            proto::ViewMessage {
                view_id: 5,
                name: "hello".into(),
                data_json: r#"{"word":"early"}"#.into(),
            },
        );
        // The connection, as the answer comes in.
        early.expect("s", 5);
    }));
    let mut outputs = views.outputs();

    let id = views.open("s", 42, CONTEXT).await.unwrap();

    let mut shown = Vec::new();
    let heard = tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(output) = outputs.recv().await {
            assert_eq!(output.view_id, id);
            let batch: Batch = serde_json::from_str(&output.json).unwrap();
            for op in batch.ops {
                let text = match op {
                    Patch::Create { props, .. } => props.get("text").cloned(),
                    Patch::Props { set, .. } => set.get("text").cloned(),
                    _ => None,
                };
                if let Some(text) = text.and_then(|text| text.as_str().map(str::to_owned)) {
                    if text == "early" {
                        return;
                    }
                    shown.push(text);
                }
            }
        }
    });
    heard
        .await
        .unwrap_or_else(|_| panic!("the early message never arrived; shown: {shown:?}"));
}
