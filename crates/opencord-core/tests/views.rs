//! Plugin views through the core's client, against a real server and the
//! real client VM (plan M2 Task 15).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use opencord_common::address::format_fingerprint;
use opencord_core::api::types::{CoreError, CoreEvent, CoreEventPayload, ReadySnapshot};
use opencord_core::client::Client;
use opencord_core::identity::Identity;
use opencord_core::views::client::ViewOutput;
use opencord_plugins::view::{Batch, CloseReason, Patch};
use opencord_server::config::Config;
use opencord_server::server::{self, ServerHandle};
use serde_json::json;
use tokio::sync::mpsc::UnboundedReceiver;

const WAIT: Duration = Duration::from_secs(15);
const FEATURES: [&str; 2] = ["plugin_users", "plugin_channels"];
const CONTEXT: &str = r#"{"width":800,"height":600,"theme":"dark","input":"mouse",
    "safe_area":{"top":0,"right":0,"bottom":0,"left":0},"text_scale":1}"#;

/// Each view open makes the server send `opened`; each `press` is echoed
/// as `count`, which the client adds up, so a press delivered twice shows.
const SERVER: &str = r#"
ui.register_channel_type("desk", { name = "Desk" })
opencord.on("view_open", function(event)
    log.info("view_open " .. event.view.id)
    event.view:send("opened", {})
end)
ui.on("press", function(view)
    view:send("count", {})
end)
"#;

fn client_script(label: &str) -> String {
    format!(
        r#"
function init() return {{ count = 0, opened = 0 }} end
app.on("opened", function() app.set(function(s) s.opened += 1 end) end)
app.on("count", function() app.set(function(s) s.count += 1 end) end)
function render(s)
    return ui.column({{}}, {{
        ui.text({{ text = "{label} count " .. s.count .. " opened " .. s.opened }}),
        ui.button({{ text = "add", on_press = function() app.send("press", {{}}) end }}),
    }})
end
"#
    )
}

fn package(version: &str, label: &str) -> Vec<u8> {
    let manifest = format!(
        r#"return {{ id = "tickets", name = "Tickets", version = "{version}", author = "A",
            license = "MIT", api = 1, permissions = {{ "create_channel_types" }},
            server = {{ "server/main.luau" }}, client = "client/main.luau" }}"#
    );
    let dir = tempfile::tempdir().unwrap();
    for (path, text) in [
        ("manifest.lua", manifest),
        ("server/main.luau", SERVER.to_owned()),
        ("client/main.luau", client_script(label)),
    ] {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    opencord_plugins::package::Package::from_dir(dir.path())
        .unwrap()
        .write_zip()
}

struct TestServer {
    handle: ServerHandle,
    dir: tempfile::TempDir,
}

impl TestServer {
    async fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let handle = server::start(Self::config(&dir, 0)).await.unwrap();
        Self { handle, dir }
    }

    /// Stops the server and starts it again on the same port and data:
    /// every session is gone.
    async fn restart(self) -> Self {
        let port = self.handle.local_addr.port();
        self.handle.shutdown().await;
        let deadline = Instant::now() + WAIT;
        let handle = loop {
            match server::start(Self::config(&self.dir, port)).await {
                Ok(handle) => break handle,
                Err(_) if Instant::now() < deadline => {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                Err(error) => panic!("could not restart the test server: {error:?}"),
            }
        };
        Self {
            handle,
            dir: self.dir,
        }
    }

    fn config(dir: &tempfile::TempDir, port: u16) -> Config {
        let mut config = Config::default();
        config.server.bind = format!("127.0.0.1:{port}").parse().unwrap();
        config.server.public_host = "127.0.0.1".to_owned();
        config.server.data_dir = dir.path().join("data");
        config.server.name = "Test Server".to_owned();
        config.plugins.enabled = true;
        config.voice.udp_port = 0;
        config.music.audio_udp_port = 0;
        config.network.ip_lookup_v4 = String::new();
        config.network.ip_lookup_v6 = String::new();
        config
    }

    fn key(&self) -> String {
        format!("127.0.0.1:{}", self.handle.local_addr.port())
    }

    /// The installed `client/main.luau` on the server's disk.
    fn installed_client_script(&self) -> PathBuf {
        let plugin = self.dir.path().join("data/plugins/tickets");
        let version = std::fs::read_dir(&plugin)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| path.join("client/main.luau").exists())
            .expect("an installed version");
        version.join("client/main.luau")
    }
}

struct App {
    client: Client,
    events: UnboundedReceiver<CoreEvent>,
    outputs: UnboundedReceiver<ViewOutput>,
    batches: Vec<Batch>,
    _dir: tempfile::TempDir,
}

impl App {
    fn new(name: &str, features: Option<&[&str]>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let (client, events) = Client::new(dir.path(), tokio::runtime::Handle::current()).unwrap();
        client.set_identity(Identity::generate(), name.to_owned());
        if let Some(features) = features {
            let features = features.iter().map(|f| (*f).to_owned()).collect();
            client.set_plugin_features(features).unwrap();
        }
        let outputs = client.view_outputs();
        Self {
            client,
            events,
            outputs,
            batches: Vec::new(),
            _dir: dir,
        }
    }

    async fn ready(&mut self) -> ReadySnapshot {
        let found = tokio::time::timeout(WAIT, async {
            loop {
                let event = self.events.recv().await.expect("the event stream ended");
                if let CoreEventPayload::Ready(ready) = event.payload {
                    return ready;
                }
            }
        });
        found.await.expect("no Ready")
    }

    /// Reads batches of `view` until `done` holds for them.
    async fn until(&mut self, view: &str, what: &str, done: impl Fn(&[Batch]) -> bool) {
        let deadline = Instant::now() + WAIT;
        while !done(&self.batches) {
            let left = deadline.saturating_duration_since(Instant::now());
            let output = tokio::time::timeout(left, self.outputs.recv()).await;
            let output = output
                .unwrap_or_else(|_| panic!("timed out waiting for {what}: {:#?}", self.batches))
                .expect("the output stream ended");
            if output.view_id == view {
                self.batches
                    .push(serde_json::from_str(&output.json).unwrap());
            }
        }
    }

    async fn shows(&mut self, view: &str, text: &str) {
        let what = format!("the text {text:?}");
        self.until(view, &what, |batches| {
            shown_text(batches) == Some(text.to_owned())
        })
        .await;
    }

    fn press(&self, view: &str, node: u64) {
        let input = json!({ "kind": "event", "node": node, "event": "press", "data": {} });
        self.client.view_input(view, &input.to_string());
    }
}

fn ops(batches: &[Batch]) -> impl Iterator<Item = &Patch> {
    batches.iter().flat_map(|batch| &batch.ops)
}

/// The text block's text as the last op set it.
fn shown_text(batches: &[Batch]) -> Option<String> {
    let mut text_node = None;
    let mut shown = None;
    for op in ops(batches) {
        match op {
            Patch::Create {
                id, block, props, ..
            } if block == "text" => {
                text_node = Some(*id);
                shown = props
                    .get("text")
                    .and_then(|t| t.as_str())
                    .map(str::to_owned);
            }
            Patch::Props { id, set, .. } if Some(*id) == text_node => {
                if let Some(text) = set.get("text").and_then(|t| t.as_str()) {
                    shown = Some(text.to_owned());
                }
            }
            _ => {}
        }
    }
    shown
}

fn button(batches: &[Batch]) -> u64 {
    ops(batches)
        .filter_map(|op| match op {
            Patch::Create { id, block, .. } if block == "button" => Some(*id),
            _ => None,
        })
        .last()
        .expect("a button")
}

/// A server with the tickets plugin installed by its owner, and a `desk`
/// channel of it.
async fn world() -> (TestServer, App, i64) {
    let server = TestServer::start().await;
    let mut owner = App::new("Owner", Some(&FEATURES));
    let key = server.key();
    owner
        .client
        .trust_fingerprint(&key, &format_fingerprint(&server.handle.fingerprint))
        .unwrap();
    owner
        .client
        .add_server(&key, server.handle.claim_token.clone())
        .await
        .unwrap();
    owner.ready().await;
    install(&owner, &key, "1.0.0", "v1").await;
    let deadline = Instant::now() + WAIT;
    let channel = loop {
        // The type is registered once the plugin's start runs.
        let created = owner
            .client
            .create_plugin_channel(
                &key,
                "tickets".into(),
                "desk".into(),
                "desk".into(),
                None,
                None,
            )
            .await;
        match created {
            Ok(channel) => break channel,
            Err(_) if Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(error) => panic!("no plugin channel: {error:?}"),
        }
    };
    (server, owner, channel.id)
}

async fn install(app: &App, key: &str, version: &str, label: &str) {
    let review = app
        .client
        .upload_plugin_package(key, package(version, label))
        .await
        .unwrap();
    let granted = vec!["create_channel_types".to_owned()];
    app.client
        .confirm_plugin_review(key, review.review_id, granted, vec![], false)
        .await
        .unwrap();
}

/// Opens the channel, retrying while the plugin's start hasn't finished.
async fn open(app: &App, key: &str, channel_id: i64) -> String {
    let deadline = Instant::now() + WAIT;
    loop {
        match app.client.view_open(key, channel_id, CONTEXT).await {
            Ok(view) => return view,
            Err(CoreError::Server { .. }) if Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(error) => panic!("the view didn't open: {error:?}"),
        }
    }
}

// Fails if `Identify` lists "plugin_channels" by default (the phone would
// get plugin channels), or if `set_plugin_features` isn't what it sends.
#[tokio::test(flavor = "multi_thread")]
async fn only_an_app_that_asks_gets_plugin_channels() {
    let (server, owner, channel_id) = world().await;
    let key = server.key();
    let invite = owner
        .client
        .create_invite(&key, None, None, None)
        .await
        .unwrap();

    let mut plain = App::new("Plain", None);
    plain.client.add_server(&invite.link, None).await.unwrap();
    let plain_ready = plain.ready().await;
    let mut asking = App::new("Asking", Some(&FEATURES));
    asking.client.add_server(&invite.link, None).await.unwrap();
    let asking_ready = asking.ready().await;

    assert!(!plain_ready.channels.iter().any(|c| c.id == channel_id));
    assert!(asking_ready.channels.iter().any(|c| c.id == channel_id));
}

// Fails if the core doesn't load the client files from the assets route,
// start the VM, or pass its batches to `view_outputs`.
#[tokio::test(flavor = "multi_thread")]
async fn opening_a_plugin_channel_draws_its_first_tree() {
    let (server, mut owner, channel_id) = world().await;

    let view = open(&owner, &server.key(), channel_id).await;

    owner.shows(&view, "v1 count 0 opened 1").await;
    button(&owner.batches);
}

// Fails if `app.send` never becomes `SendViewEvent`, or a `ViewMessage`
// never reaches the VM's `app.on`.
#[tokio::test(flavor = "multi_thread")]
async fn a_press_reaches_the_server_and_its_answer_redraws() {
    let (server, mut owner, channel_id) = world().await;
    let view = open(&owner, &server.key(), channel_id).await;
    owner.shows(&view, "v1 count 0 opened 1").await;

    owner.press(&view, button(&owner.batches));

    owner.shows(&view, "v1 count 1 opened 1").await;
}

// Fails if a second open fetches the files again instead of using the
// cache by hash: the server's copy has changed under the same hash, so a
// fetch would be refused.
#[tokio::test(flavor = "multi_thread")]
async fn client_files_load_once_by_hash() {
    let (server, mut owner, channel_id) = world().await;
    let key = server.key();
    let first = open(&owner, &key, channel_id).await;
    owner.shows(&first, "v1 count 0 opened 1").await;
    owner.client.view_close(&first);
    std::fs::write(server.installed_client_script(), client_script("changed")).unwrap();

    let second = open(&owner, &key, channel_id).await;

    owner.shows(&second, "v1 count 0 opened 1").await;
}

// Review Focus 4. Fails if a new session doesn't open the view again (no
// second `opened`), if reopening starts a new VM (the count goes back to
// 0), or if an event is delivered twice after it (the count jumps by 2).
#[tokio::test(flavor = "multi_thread")]
async fn a_new_session_reopens_open_views_keeping_state() {
    let (server, mut owner, channel_id) = world().await;
    let key = server.key();
    let view = open(&owner, &key, channel_id).await;
    owner.shows(&view, "v1 count 0 opened 1").await;
    owner.press(&view, button(&owner.batches));
    owner.shows(&view, "v1 count 1 opened 1").await;

    // A restarted server forgets every session: the core identifies anew.
    let _server = server.restart().await;
    owner.shows(&view, "v1 count 1 opened 2").await;
    owner.press(&view, button(&owner.batches));

    owner.shows(&view, "v1 count 2 opened 2").await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    while let Ok(output) = owner.outputs.try_recv() {
        owner
            .batches
            .push(serde_json::from_str(&output.json).unwrap());
    }
    assert_eq!(
        shown_text(&owner.batches).as_deref(),
        Some("v1 count 2 opened 2")
    );
}

// Review Focus 2, the client's half. Fails if `ViewScriptsUpdate` isn't
// followed by loading the new files and a reload, or if the reload loses
// the state (the count goes back to 0). The new files arrive through an
// update installed by review; a dev push sends the same event.
#[tokio::test(flavor = "multi_thread")]
async fn a_dev_push_reloads_open_views_keeping_state() {
    let (server, mut owner, channel_id) = world().await;
    let key = server.key();
    let view = open(&owner, &key, channel_id).await;
    owner.shows(&view, "v1 count 0 opened 1").await;
    owner.press(&view, button(&owner.batches));
    owner.shows(&view, "v1 count 1 opened 1").await;

    install(&owner, &key, "1.0.1", "v2").await;

    // The updated plugin's new VM hears `view_open` too (Claude's call 5).
    owner.shows(&view, "v2 count 1 opened 2").await;
}

// Fails if `ViewClosed` doesn't reach the renderer as a `closed` op with
// its reason.
#[tokio::test(flavor = "multi_thread")]
async fn server_closing_the_view_sends_closed() {
    let (server, mut owner, channel_id) = world().await;
    let key = server.key();
    let view = open(&owner, &key, channel_id).await;
    owner.shows(&view, "v1 count 0 opened 1").await;

    owner.client.delete_channel(&key, channel_id).await.unwrap();

    owner
        .until(&view, "a closed op", |batches| {
            ops(batches).any(|op| {
                *op == Patch::Closed {
                    reason: CloseReason::ChannelDeleted,
                }
            })
        })
        .await;
}
