//! Plugin views through the core's client, against a real server and the
//! real client VM (plan M2 Task 15).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use opencord_common::address::format_fingerprint;
use opencord_core::api::types::{
    ActivitySession, ChannelKind, CoreError, CoreEvent, CoreEventPayload, ErrorCode,
    PluginChannelState, PluginChannelType, ReadySnapshot,
};
use opencord_core::client::Client;
use opencord_core::identity::Identity;
use opencord_core::views::client::ViewOutput;
use opencord_plugins::view::{Batch, CloseReason, Patch};
use opencord_server::config::Config;
use opencord_server::server::{self, ServerHandle};
use serde_json::json;
use sha2::Digest as _;
use tokio::sync::mpsc::UnboundedReceiver;

const WAIT: Duration = Duration::from_secs(15);
const FEATURES: [&str; 2] = ["plugin_users", "plugin_channels"];
const ACTIVITY_FEATURES: [&str; 3] = ["plugin_users", "plugin_channels", "activities"];
const CONTEXT: &str = r#"{"width":800,"height":600,"theme":"dark","input":"mouse",
    "safe_area":{"top":0,"right":0,"bottom":0,"left":0},"text_scale":1}"#;

/// Each view open makes the server send `opened`; each `press` is echoed
/// as `count`, which the client adds up, so a press delivered twice shows.
/// At start it saves a public file for the image tests. `opened` carries
/// the activity session the server's view is in; an activity's
/// `paint_press` is answered to every view of its session.
const SERVER: &str = r#"
ui.register_channel_type("desk", { name = "Desk" })
ui.register_activity("paint", { name = "Paint" })
opencord.on("start", function()
    files.put("pub/a.png", "public bytes", { public = true })
end)
opencord.on("view_open", function(event)
    log.info("view_open " .. event.view.id)
    event.view:send("opened", { session = event.view.place.session_id })
end)
ui.on("paint_press", function(view)
    ui.send_place({ session_id = view.place.session_id }, "count", {})
end)
opencord.on("file_uploaded", function(event)
    log.info("uploaded " .. event.file.name .. " " .. event.file.size)
end)
ui.on("press", function(view)
    view:send("count", {})
end)
"#;

/// The plugin's `assets/logo.png`: not an image, but the core never looks.
const LOGO: &[u8] = b"logo bytes";

fn client_script(label: &str) -> String {
    format!(
        r#"
function init() return {{ count = 0, opened = 0 }} end
local function activity() return app.place.kind == "activity" end
app.on("opened", function(data)
    app.set(function(s)
        s.opened += 1
        s.heard = data.session
    end)
end)
app.on("count", function() app.set(function(s) s.count += 1 end) end)
local function uploaded(file)
    app.set(function(s) s.upload = file.name .. " " .. file.size end)
end
local function refused(problem)
    app.set(function(s) s.upload = "error " .. problem.message end)
end
function render(s)
    local upload = s.upload and (" upload " .. s.upload) or ""
    local place = activity() and ("activity " .. tostring(s.heard) .. " ") or ""
    local press = activity() and "paint_press" or "press"
    return ui.column({{}}, {{
        ui.text({{ text = place .. "{label} count " .. s.count .. " opened " .. s.opened .. upload }}),
        ui.button({{ text = "add", on_press = function() app.send(press, {{}}) end }}),
        ui.file_picker({{ text = "pick", on_upload = uploaded, on_upload_error = refused }}),
    }})
end
"#
    )
}

fn package(version: &str, label: &str) -> Vec<u8> {
    let manifest = format!(
        r#"return {{ id = "tickets", name = "Tickets", version = "{version}", author = "A",
            license = "MIT", api = 1,
            permissions = {{ "create_channel_types", "accept_uploads", "register_activities" }},
            server = {{ "server/main.luau" }}, client = "client/main.luau" }}"#
    );
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("assets")).unwrap();
    std::fs::write(dir.path().join("assets/logo.png"), LOGO).unwrap();
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
        self.installed("client/main.luau")
    }

    /// A file of the installed package on the server's disk.
    fn installed(&self, path: &str) -> PathBuf {
        let plugin = self.dir.path().join("data/plugins/tickets");
        let version = std::fs::read_dir(&plugin)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|version| version.join(path).exists())
            .expect("an installed version");
        version.join(path)
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

    /// Reads events until one makes `found` true.
    async fn hears(&mut self, what: &str, found: impl Fn(&CoreEventPayload) -> bool) {
        let heard = tokio::time::timeout(WAIT, async {
            loop {
                let event = self.events.recv().await.expect("the event stream ended");
                if found(&event.payload) {
                    return;
                }
            }
        });
        heard
            .await
            .unwrap_or_else(|_| panic!("timed out waiting for {what}"));
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
    node(batches, "button")
}

/// The last block of `kind` created.
fn node(batches: &[Batch], kind: &str) -> u64 {
    ops(batches)
        .filter_map(|op| match op {
            Patch::Create { id, block, .. } if block == kind => Some(*id),
            _ => None,
        })
        .last()
        .unwrap_or_else(|| panic!("no {kind}"))
}

/// A server with the tickets plugin installed by its owner, and a `desk`
/// channel of it.
async fn world() -> (TestServer, App, i64) {
    let (server, owner, channel_id, _) = world_with(&FEATURES).await;
    (server, owner, channel_id)
}

/// [`world`] for an owner whose app lists `features`, with the owner's
/// `Ready`.
async fn world_with(features: &[&str]) -> (TestServer, App, i64, ReadySnapshot) {
    let server = TestServer::start().await;
    let mut owner = App::new("Owner", Some(features));
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
    let ready = owner.ready().await;
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
    (server, owner, channel.id, ready)
}

async fn install(app: &App, key: &str, version: &str, label: &str) {
    let review = app
        .client
        .upload_plugin_package(key, package(version, label))
        .await
        .unwrap();
    let granted = vec![
        "create_channel_types".to_owned(),
        "accept_uploads".to_owned(),
        "register_activities".to_owned(),
    ];
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

// Fails if the core drops a plugin channel's plugin, type or state on the
// way from the wire, gives them to other channels, leaves
// `plugin_channel_types` out of `Ready`, or drops `PluginChannelTypesUpdate`
// (the app would offer the type of a plugin that stopped).
#[tokio::test(flavor = "multi_thread")]
async fn plugin_channels_and_their_types_reach_the_app() {
    let (server, owner, channel_id) = world().await;
    let key = server.key();
    let invite = owner
        .client
        .create_invite(&key, None, None, None)
        .await
        .unwrap();
    let mut asking = App::new("Asking", Some(&FEATURES));
    asking.client.add_server(&invite.link, None).await.unwrap();

    let ready = asking.ready().await;

    let channel = ready
        .channels
        .iter()
        .find(|c| c.id == channel_id)
        .expect("the plugin channel");
    assert_eq!(channel.kind, ChannelKind::Plugin);
    assert_eq!(channel.plugin_id.as_deref(), Some("tickets"));
    assert_eq!(channel.type_key.as_deref(), Some("desk"));
    assert_eq!(channel.plugin_state, Some(PluginChannelState::Available));
    let others: Vec<_> = ready
        .channels
        .iter()
        .filter(|c| c.kind != ChannelKind::Plugin)
        .collect();
    assert!(!others.is_empty());
    assert!(
        others
            .iter()
            .all(|c| c.plugin_id.is_none() && c.type_key.is_none() && c.plugin_state.is_none()),
        "{others:?}"
    );
    assert_eq!(
        ready.plugin_channel_types,
        vec![PluginChannelType {
            plugin_id: "tickets".into(),
            type_key: "desk".into(),
            name: "Desk".into(),
            description: String::new(),
            icon: String::new(),
            plugin_name: "Tickets".into(),
        }]
    );

    owner
        .client
        .set_plugin_running(&key, "tickets".into(), false)
        .await
        .unwrap();

    asking
        .hears("an empty PluginChannelTypesUpdate", |payload| {
            matches!(payload, CoreEventPayload::PluginChannelTypesUpdate(types) if types.is_empty())
        })
        .await;
}

// Fails if `plugin_asset_file` fetches a file it already has (the server's
// copy changed under the same hash, so a second fetch brings other bytes),
// returns a path to anything but the asset, or accepts bytes that don't
// match the hash it was given.
#[tokio::test(flavor = "multi_thread")]
async fn assets_load_once_and_match_their_hash() {
    let (server, owner, _) = world().await;
    let key = server.key();
    let sha256 = hex::encode(sha2::Sha256::digest(LOGO));

    let first = owner
        .client
        .plugin_asset_file(&key, "tickets", "assets/logo.png", "")
        .await
        .unwrap();
    std::fs::write(server.installed("assets/logo.png"), b"changed").unwrap();
    let again = owner
        .client
        .plugin_asset_file(&key, "tickets", "assets/logo.png", "")
        .await
        .unwrap();
    let by_hash = owner
        .client
        .plugin_asset_file(&key, "tickets", "assets/logo.png", &sha256)
        .await
        .unwrap();
    let wrong = owner
        .client
        .plugin_asset_file(&key, "tickets", "assets/logo.png", &"ab".repeat(32))
        .await;

    assert_eq!(std::fs::read(&first).unwrap(), LOGO);
    assert_eq!(again, first);
    assert_eq!(by_hash, first);
    assert!(wrong.is_err(), "{wrong:?}");
}

// Fails if an image source isn't fetched from its plugin route (`file`
// from the public files, `asset` from the package), if `bytes` aren't
// written out as they are, or if an `attachment` is written to disk
// instead of refused: attachments come from `attachment_bytes`, through
// the encrypted cache.
#[tokio::test(flavor = "multi_thread")]
async fn image_sources_become_files() {
    let (server, owner, _) = world().await;
    let key = server.key();
    let image = |source: serde_json::Value| {
        let client = &owner.client;
        let key = &key;
        async move {
            client
                .plugin_image_file(key, "tickets", &source.to_string())
                .await
        }
    };

    // The plugin saves the file when its start runs.
    let deadline = Instant::now() + WAIT;
    let file = loop {
        match image(json!({ "file": "pub/a.png" })).await {
            Ok(path) => break path,
            Err(_) if Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(error) => panic!("no public file: {error:?}"),
        }
    };
    let asset = image(json!({ "asset": "assets/logo.png" })).await.unwrap();
    let raw = data_encoding::BASE64.encode(b"raw image");
    let bytes = image(json!({ "bytes": raw })).await.unwrap();
    let attachment = image(json!({ "attachment": 1 })).await;

    assert_eq!(std::fs::read(file).unwrap(), b"public bytes");
    assert_eq!(std::fs::read(asset).unwrap(), LOGO);
    assert_eq!(std::fs::read(bytes).unwrap(), b"raw image");
    assert!(
        matches!(attachment, Err(CoreError::InvalidInput { .. })),
        "{attachment:?}"
    );
}

// Fails if `view_upload` doesn't upload through `CreateUpload` for the
// view (the server refuses one with neither a channel nor a view), or
// doesn't answer the picker with `upload`, the file's name and its size.
#[tokio::test(flavor = "multi_thread")]
async fn a_picked_file_uploads_for_the_view_and_answers_the_picker() {
    let (server, mut owner, channel_id) = world().await;
    let view = open(&owner, &server.key(), channel_id).await;
    owner.shows(&view, "v1 count 0 opened 1").await;
    let picker = node(&owner.batches, "file_picker");
    let path = owner._dir.path().join("notes.txt");
    std::fs::write(&path, "hello").unwrap();

    owner
        .client
        .view_upload(&view, picker, path.to_string_lossy().into_owned())
        .await
        .unwrap();

    owner
        .shows(&view, "v1 count 0 opened 1 upload notes.txt 5")
        .await;
}

// Fails if a refused upload only reaches the caller and the picker never
// hears `upload_error` (its spinner would turn forever).
#[tokio::test(flavor = "multi_thread")]
async fn a_refused_upload_answers_the_picker_with_upload_error() {
    let (server, mut owner, channel_id) = world().await;
    let key = server.key();
    owner
        .client
        .update_upload_settings(&key, Some(vec!["exe".to_owned()]))
        .await
        .unwrap();
    let view = open(&owner, &key, channel_id).await;
    owner.shows(&view, "v1 count 0 opened 1").await;
    let picker = node(&owner.batches, "file_picker");
    let path = owner._dir.path().join("run.exe");
    std::fs::write(&path, "MZ").unwrap();

    owner
        .client
        .view_upload(&view, picker, path.to_string_lossy().into_owned())
        .await
        .unwrap();

    owner
        .until(&view, "an upload error", |batches| {
            shown_text(batches)
                .is_some_and(|text| text.starts_with("v1 count 0 opened 1 upload error "))
        })
        .await;
}

// Activities (plan M3 Task 8). The tickets plugin registers `paint`; an
// activity's view shows "activity <the session id its server view is in>"
// before the label, and its button sends `paint_press`, which the plugin
// answers with `ui.send_place({ session_id })`.

/// A server with the tickets plugin, its owner's app listing "activities";
/// the owner's user id and the server's voice channel. Nobody is in it.
async fn call_world() -> (TestServer, App, i64, i64) {
    let (server, owner, _, ready) = world_with(&ACTIVITY_FEATURES).await;
    let voice = ready
        .channels
        .iter()
        .find(|channel| channel.kind == ChannelKind::Voice)
        .expect("a voice channel")
        .id;
    (server, owner, ready.self_user.id, voice)
}

/// A second member, whose app lists "activities", in the call `voice`.
async fn member_in_call(server: &TestServer, owner: &App, name: &str, voice: i64) -> App {
    let invite = owner
        .client
        .create_invite(&server.key(), None, None, None)
        .await
        .unwrap();
    let mut member = App::new(name, Some(&ACTIVITY_FEATURES));
    member.client.add_server(&invite.link, None).await.unwrap();
    member.ready().await;
    member
        .client
        .voice_join(&server.key(), voice)
        .await
        .unwrap();
    member
}

async fn start_paint(app: &App, key: &str, voice: i64) -> ActivitySession {
    app.client
        .start_activity(key, voice, "tickets".into(), "paint".into())
        .await
        .unwrap()
}

async fn open_activity(app: &App, key: &str, session: &ActivitySession) -> String {
    app.client
        .view_open_activity(
            key,
            session.id,
            session.channel_id,
            &session.activity_key,
            CONTEXT,
        )
        .await
        .unwrap()
}

/// Reads batches of `view` until one closes it, and returns the reason.
async fn closed(app: &mut App, view: &str) -> CloseReason {
    app.until(view, "a closed op", |batches| {
        ops(batches).any(|op| matches!(op, Patch::Closed { .. }))
    })
    .await;
    let last = app.batches.last().and_then(|batch| batch.ops.last());
    match last {
        Some(Patch::Closed { reason }) => *reason,
        other => panic!("the last op isn't closed: {other:?}"),
    }
}

/// The owner's connection drops and the server forgets its session, as
/// after a minute away: the core's next connection is a new session.
fn lose_the_session(server: &TestServer, user_id: i64) {
    server.handle.drop_user_connections(user_id);
    server.handle.reap_sessions_after(Duration::from_secs(120));
}

// The owner starts and opens it alone. Fails if `start_activity` or
// `view_open_activity` don't reach the server, if the VM's place isn't the
// activity (the first tree has no "activity"), if the server's view isn't
// in the started session, or if the session's create and its viewer update
// don't reach the app.
#[tokio::test(flavor = "multi_thread")]
async fn starting_and_opening_an_activity_draws_it() {
    let (server, mut owner, owner_id, voice) = call_world().await;
    let key = server.key();
    owner.client.voice_join(&key, voice).await.unwrap();

    let session = start_paint(&owner, &key, voice).await;
    let view = open_activity(&owner, &key, &session).await;

    owner
        .shows(
            &view,
            &format!("activity {} v1 count 0 opened 1", session.id),
        )
        .await;
    let first = shown_text(&owner.batches[..1]).unwrap_or_default();
    assert!(first.starts_with("activity "), "{first:?}");
    assert_eq!(
        (
            session.channel_id,
            session.plugin_id.as_str(),
            session.activity_key.as_str(),
            session.starter_id
        ),
        (voice, "tickets", "paint", owner_id)
    );
    owner
        .hears("the session's create", |payload| {
            matches!(payload, CoreEventPayload::ActivitySessionCreate(s) if s.id == session.id)
        })
        .await;
    owner
        .hears("the owner as its viewer", |payload| {
            matches!(payload, CoreEventPayload::ActivitySessionUpdate(s)
                if s.id == session.id && s.viewer_ids == [owner_id])
        })
        .await;
}

// The owner alone. Fails if the activity's `app.send` doesn't reach the
// plugin as the view's event, or `ui.send_place({ session_id })` doesn't
// reach the view.
#[tokio::test(flavor = "multi_thread")]
async fn a_press_in_an_activity_reaches_its_plugin_and_back() {
    let (server, mut owner, _, voice) = call_world().await;
    let key = server.key();
    owner.client.voice_join(&key, voice).await.unwrap();
    let session = start_paint(&owner, &key, voice).await;
    let view = open_activity(&owner, &key, &session).await;
    let id = session.id;
    owner
        .shows(&view, &format!("activity {id} v1 count 0 opened 1"))
        .await;

    owner.press(&view, button(&owner.batches));

    owner
        .shows(&view, &format!("activity {id} v1 count 1 opened 1"))
        .await;
}

// The owner, the starter, ends it. Fails if `end_activity` doesn't reach
// the server, or `ACTIVITY_ENDED` reaches the renderer as another reason.
#[tokio::test(flavor = "multi_thread")]
async fn ending_the_activity_closes_the_view_with_its_reason() {
    let (server, mut owner, _, voice) = call_world().await;
    let key = server.key();
    owner.client.voice_join(&key, voice).await.unwrap();
    let session = start_paint(&owner, &key, voice).await;
    let view = open_activity(&owner, &key, &session).await;
    owner
        .shows(
            &view,
            &format!("activity {} v1 count 0 opened 1", session.id),
        )
        .await;

    owner.client.end_activity(&key, session.id).await.unwrap();

    assert_eq!(closed(&mut owner, &view).await, CloseReason::ActivityEnded);
}

// The owner has it open and leaves; B stays, so the call goes on (an
// emptied call ends the session, which closes the view as ended instead).
// Fails if `LEFT_CALL` reaches the renderer as another reason.
#[tokio::test(flavor = "multi_thread")]
async fn leaving_the_call_closes_the_view() {
    let (server, mut owner, _, voice) = call_world().await;
    let key = server.key();
    owner.client.voice_join(&key, voice).await.unwrap();
    let _b = member_in_call(&server, &owner, "B", voice).await;
    let session = start_paint(&owner, &key, voice).await;
    let view = open_activity(&owner, &key, &session).await;
    owner
        .shows(
            &view,
            &format!("activity {} v1 count 0 opened 1", session.id),
        )
        .await;

    owner.client.voice_leave().await.unwrap();

    assert_eq!(closed(&mut owner, &view).await, CloseReason::LeftCall);
}

// Review Focus 5's core half. The owner has it open and loses the gateway
// session; B stays in the call, so the activity outlives the owner's voice
// state. Fails if a new session opens the view again as a channel's (the
// server refuses it) or not at all (no second `opened`), in another
// session (the id shown changes), with a new VM (the count goes back to
// 0), or if an event is delivered twice after it.
#[tokio::test(flavor = "multi_thread")]
async fn a_new_session_reopens_the_activity_keeping_state() {
    let (server, mut owner, owner_id, voice) = call_world().await;
    let key = server.key();
    owner.client.voice_join(&key, voice).await.unwrap();
    let _b = member_in_call(&server, &owner, "B", voice).await;
    let session = start_paint(&owner, &key, voice).await;
    let view = open_activity(&owner, &key, &session).await;
    let id = session.id;
    owner
        .shows(&view, &format!("activity {id} v1 count 0 opened 1"))
        .await;
    owner.press(&view, button(&owner.batches));
    owner
        .shows(&view, &format!("activity {id} v1 count 1 opened 1"))
        .await;

    lose_the_session(&server, owner_id);

    owner.ready().await;
    owner
        .shows(&view, &format!("activity {id} v1 count 1 opened 2"))
        .await;
    owner.press(&view, button(&owner.batches));
    owner
        .shows(&view, &format!("activity {id} v1 count 2 opened 2"))
        .await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    while let Ok(output) = owner.outputs.try_recv() {
        if output.view_id == view {
            owner
                .batches
                .push(serde_json::from_str(&output.json).unwrap());
        }
    }
    assert_eq!(
        shown_text(&owner.batches),
        Some(format!("activity {id} v1 count 2 opened 2"))
    );
}

// The owner has it open and loses the gateway session; B, its starter,
// ends it meanwhile. Fails if the refused reopen (`NOT_FOUND`) ends the
// view with an error instead of `closed` `activity_ended`.
#[tokio::test(flavor = "multi_thread")]
async fn an_activity_gone_while_away_closes_the_view() {
    let (server, mut owner, owner_id, voice) = call_world().await;
    let key = server.key();
    owner.client.voice_join(&key, voice).await.unwrap();
    let b = member_in_call(&server, &owner, "B", voice).await;
    let session = start_paint(&b, &key, voice).await;
    let view = open_activity(&owner, &key, &session).await;
    owner
        .shows(
            &view,
            &format!("activity {} v1 count 0 opened 1", session.id),
        )
        .await;

    lose_the_session(&server, owner_id);
    b.client.end_activity(&key, session.id).await.unwrap();

    assert_eq!(closed(&mut owner, &view).await, CloseReason::ActivityEnded);
}

// B starts it in the call; the owner can see the channel but isn't in the
// call. Fails if the core doesn't send the activity's place (a channel
// place would be refused as another error) or hides the server's code.
#[tokio::test(flavor = "multi_thread")]
async fn opening_without_the_call_is_refused() {
    let (server, owner, _, voice) = call_world().await;
    let key = server.key();
    let b = member_in_call(&server, &owner, "B", voice).await;
    let session = start_paint(&b, &key, voice).await;

    let opened = owner
        .client
        .view_open_activity(
            &key,
            session.id,
            session.channel_id,
            &session.activity_key,
            CONTEXT,
        )
        .await;

    assert!(
        matches!(
            &opened,
            Err(CoreError::Server { code: ErrorCode::Forbidden, message, .. })
                if message.contains("join the call")
        ),
        "{opened:?}"
    );
}
