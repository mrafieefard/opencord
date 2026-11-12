//! Plugin packages for tests, and installing them (plan Tasks 7 and 8).

use std::time::{Duration, Instant};

use ed25519_dalek::SigningKey;
use opencord_plugins::manifest;
use opencord_plugins::package::Package;
use opencord_plugins::signing::{self, PublicKey};
use opencord_proto::v1 as proto;
use proto::request::Kind;

use crate::common::{TestClient, TestServer, decode_response};

/// A plugin's zip holding `files`.
pub fn package(files: &[(&str, &str)]) -> Vec<u8> {
    unpacked(files).write_zip()
}

fn unpacked(files: &[(&str, &str)]) -> Package {
    let dir = tempfile::tempdir().unwrap();
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    Package::from_dir(dir.path()).unwrap()
}

/// The key `key` signs with, as a manifest names it.
pub fn public_key(key: &SigningKey) -> PublicKey {
    PublicKey(key.verifying_key().to_bytes())
}

/// A zip holding `files`, signed by `key`; with `rotation_from`, it also
/// carries a `KEY-ROTATION` from that key to `key`. The manifest must name
/// `key` as its `public_key`.
pub fn signed_package(
    files: &[(&str, &str)],
    key: &SigningKey,
    rotation_from: Option<&SigningKey>,
) -> Vec<u8> {
    let mut package = unpacked(files);
    if let Some(old) = rotation_from {
        signing::write_rotation(&mut package, old, &public_key(key)).unwrap();
    }
    let source = std::str::from_utf8(package.get("manifest.lua").unwrap()).unwrap();
    let manifest = manifest::evaluate(source).unwrap();
    signing::sign(&mut package, &manifest, key).unwrap();
    package.write_zip()
}

/// A `manifest.lua` running `server/main.luau` and asking for `permissions`.
pub fn manifest(id: &str, permissions: &[&str]) -> String {
    manifest_with(id, "1.0.0", permissions, "")
}

/// [`manifest`] at `version`, with `extra` fields (`key = value,` lines).
pub fn manifest_with(id: &str, version: &str, permissions: &[&str], extra: &str) -> String {
    let permissions: Vec<String> = permissions.iter().map(|name| format!("{name:?}")).collect();
    format!(
        r#"return {{
    id = "{id}",
    name = "Test plugin",
    version = "{version}",
    author = "Tester",
    license = "MIT",
    api = 1,
    permissions = {{ {} }},
    server = {{ "server/main.luau" }},
    {extra}
}}
"#,
        permissions.join(", ")
    )
}

/// Uploads `zip` with the media token `token`: the status and the answer.
pub async fn upload_status(
    server: &TestServer,
    token: &str,
    zip: Vec<u8>,
) -> (u16, proto::response::Result) {
    let (status, body) = server
        .send_http(
            "POST",
            "/plugins/packages",
            &[("Authorization", &format!("Bearer {token}"))],
            &zip,
        )
        .await;
    (status, decode_response(&body))
}

pub async fn upload(server: &TestServer, token: &str, zip: Vec<u8>) -> proto::response::Result {
    upload_status(server, token, zip).await.1
}

/// The review an upload waits in; panics on a refusal.
pub async fn review(server: &TestServer, token: &str, zip: Vec<u8>) -> proto::PluginReview {
    match upload(server, token, zip).await {
        proto::response::Result::PluginReview(review) => *review,
        other => panic!("not a review: {other:?}"),
    }
}

/// The error an upload is refused with.
pub async fn refusal(server: &TestServer, token: &str, zip: Vec<u8>) -> proto::Error {
    match upload(server, token, zip).await {
        proto::response::Result::Error(error) => error,
        other => panic!("not refused: {other:?}"),
    }
}

pub fn confirm(review: &proto::PluginReview, granted: &[&str]) -> Kind {
    confirm_with_config(review, granted, Vec::new())
}

pub fn confirm_with_config(
    review: &proto::PluginReview,
    granted: &[&str],
    config: Vec<proto::PluginConfigValue>,
) -> Kind {
    Kind::ConfirmPluginReview(proto::ConfirmPluginReview {
        review_id: review.review_id.clone(),
        granted: granted.iter().map(|name| (*name).to_owned()).collect(),
        config,
        accept_license: true,
    })
}

/// Uploads and confirms `zip`, granting `granted`.
pub async fn install(
    server: &TestServer,
    client: &mut TestClient,
    token: &str,
    zip: Vec<u8>,
    granted: &[&str],
) -> proto::Plugin {
    install_with_config(server, client, token, zip, granted, Vec::new()).await
}

/// Uploads and confirms `zip`, granting `granted` and submitting `config`.
pub async fn install_with_config(
    server: &TestServer,
    client: &mut TestClient,
    token: &str,
    zip: Vec<u8>,
    granted: &[&str],
    config: Vec<proto::PluginConfigValue>,
) -> proto::Plugin {
    let review = review(server, token, zip).await;
    match client
        .ok(confirm_with_config(&review, granted, config))
        .await
    {
        proto::response::Result::Plugin(plugin) => plugin,
        other => panic!("not installed: {other:?}"),
    }
}

/// The plugin's last 500 log lines' text.
pub async fn log_lines(client: &mut TestClient, plugin_id: &str) -> Vec<String> {
    let reply = client
        .ok_paced(Kind::FetchPluginLog(proto::FetchPluginLog {
            plugin_id: plugin_id.to_owned(),
            before: None,
            limit: 500,
        }))
        .await;
    let proto::response::Result::PluginLog(log) = reply else {
        panic!("not a log: {reply:?}");
    };
    log.lines.into_iter().map(|line| line.text).collect()
}

/// Waits up to 5 s for a line holding `text` in the plugin's log, polling
/// at the pace the request rate limit refills.
pub async fn wait_for_log(client: &mut TestClient, plugin_id: &str, text: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let lines = log_lines(client, plugin_id).await;
        if lines.iter().any(|line| line.contains(text)) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "{text:?} never appeared in {plugin_id}'s log: {lines:#?}"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// A session for `seed`'s key, listing `features`. The key must already be
/// a member, or `invite` lets it join.
pub async fn session(
    server: &TestServer,
    seed: u8,
    invite: Option<&str>,
    features: &[&str],
) -> (TestClient, proto::Ready) {
    let mut client = TestClient::connect(server, crate::common::key(seed)).await;
    let mut identify = client.identify_message("Member", invite, None);
    identify.features = features.iter().map(|name| (*name).to_owned()).collect();
    client
        .send(proto::envelope::Payload::Identify(identify))
        .await;
    match client.recv_payload().await {
        proto::envelope::Payload::Ready(ready) => (client, *ready),
        other => panic!("expected Ready, got {other:?}"),
    }
}

/// The installed plugins.
pub async fn plugins(client: &mut TestClient) -> Vec<proto::Plugin> {
    match client.ok(Kind::FetchPlugins(proto::FetchPlugins {})).await {
        proto::response::Result::Plugins(list) => list.plugins,
        other => panic!("not plugins: {other:?}"),
    }
}
