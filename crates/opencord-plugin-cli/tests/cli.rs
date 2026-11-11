use std::fs;
use std::path::Path;
use std::time::Duration;

use opencord_plugin_cli::{dev, init, keys, pack};
use opencord_plugins::manifest;
use opencord_plugins::package::Package;
use opencord_plugins::signing::{self, SignatureState};
use opencord_proto::v1 as proto;
use opencord_proto::v1::request::Kind;
use opencord_server::testing::{TestServer, owner};
use tempfile::tempdir;

#[test]
fn init_creates_a_working_plugin() {
    let tmp = tempdir().unwrap();
    let id = "my-bot";
    init::init(id, tmp.path()).expect("init succeeds");

    let plugin_dir = tmp.path().join(id);
    assert!(plugin_dir.is_dir());
    assert!(plugin_dir.join("manifest.lua").is_file());
    assert!(plugin_dir.join("server/main.luau").is_file());
    assert!(plugin_dir.join("client/main.luau").is_file());
    assert!(plugin_dir.join("opencord.d.luau").is_file());
    assert!(plugin_dir.join(".luaurc").is_file());
    assert!(plugin_dir.join(".gitignore").is_file());
    assert!(plugin_dir.join(".vscode/settings.json").is_file());
    let settings_str = fs::read_to_string(plugin_dir.join(".vscode/settings.json")).unwrap();
    assert!(settings_str.contains("luau-lsp.types.definitionFiles"));

    // Pack unsigned
    let zip_path = pack::pack(&plugin_dir, None, None).expect("pack succeeds");
    assert!(zip_path.exists());

    let zip_bytes = fs::read(&zip_path).unwrap();
    let pkg = Package::read_zip(&zip_bytes).expect("valid zip package");
    let manifest_bytes = pkg.get("manifest.lua").expect("manifest.lua present");
    let manifest_src = std::str::from_utf8(manifest_bytes).unwrap();
    let parsed_manifest = manifest::evaluate(manifest_src).expect("manifest evaluates");
    assert_eq!(parsed_manifest.id, id);
    assert_eq!(parsed_manifest.api, 1);
    assert_eq!(parsed_manifest.client.as_deref(), Some("client/main.luau"));
    assert!(parsed_manifest.permissions.contains(&"create_channel_types".to_string()));
}

#[test]
fn pack_signs_what_the_server_verifies() {
    let tmp = tempdir().unwrap();
    let key_path = tmp.path().join("my.key");
    let (pubkey_str, _fp) = keys::keygen(Some(&key_path)).expect("keygen succeeds");

    let id = "signed-bot";
    init::init(id, tmp.path()).expect("init succeeds");
    let plugin_dir = tmp.path().join(id);

    let zip_path = pack::pack(&plugin_dir, Some(&key_path), None).expect("pack succeeds");
    let zip_bytes = fs::read(&zip_path).unwrap();
    let pkg = Package::read_zip(&zip_bytes).expect("valid zip");
    let manifest_bytes = pkg.get("manifest.lua").unwrap();
    let parsed_manifest = manifest::evaluate(std::str::from_utf8(manifest_bytes).unwrap()).unwrap();

    let expected_key = signing::PublicKey::parse(&pubkey_str).unwrap();
    let verified = signing::verify(&pkg, &parsed_manifest).expect("verifies");
    assert_eq!(verified, SignatureState::Signed(expected_key));
}

#[test]
fn pack_rewrites_the_public_key_line() {
    let tmp = tempdir().unwrap();
    let manifest_file = tmp.path().join("manifest.lua");

    // Case 1: no existing public_key, should insert after version
    fs::write(
        &manifest_file,
        r#"return {
  id = "test-plugin",
  name = "Test",
  version = "0.1.0",
  author = "Author",
  license = "MIT",
  api = 1,
}
"#,
    )
    .unwrap();

    pack::rewrite_manifest_public_key(&manifest_file, "ed25519:dummy1").unwrap();
    let content = fs::read_to_string(&manifest_file).unwrap();
    assert!(content.contains("public_key = \"ed25519:dummy1\""));
    assert!(content.contains("version = \"0.1.0\",\n  public_key = \"ed25519:dummy1\""));

    // Case 2: existing public_key, should replace in place
    pack::rewrite_manifest_public_key(&manifest_file, "ed25519:dummy2").unwrap();
    let content2 = fs::read_to_string(&manifest_file).unwrap();
    assert!(!content2.contains("dummy1"));
    assert!(content2.contains("public_key = \"ed25519:dummy2\""));
}

#[test]
fn keygen_refuses_to_overwrite() {
    let tmp = tempdir().unwrap();
    let key_path = tmp.path().join("test.key");

    let (pk1, _) = keys::keygen(Some(&key_path)).expect("first keygen succeeds");
    assert!(key_path.exists());

    let err = keys::keygen(Some(&key_path)).expect_err("second keygen should fail");
    assert!(err.to_lowercase().contains("already exists"));

    // File content untouched
    let loaded = signing::read_key(&key_path).unwrap();
    assert_eq!(signing::PublicKey::from(&loaded).to_string(), pk1);
}

#[test]
fn rotate_key_writes_valid_key_rotation() {
    let tmp = tempdir().unwrap();
    let old_key_path = tmp.path().join("old.key");
    let new_key_path = tmp.path().join("new.key");
    let (old_pub_str, _) = keys::keygen(Some(&old_key_path)).unwrap();
    let (new_pub_str, _) = keys::keygen(Some(&new_key_path)).unwrap();

    let id = "rotated-bot";
    init::init(id, tmp.path()).unwrap();
    let plugin_dir = tmp.path().join(id);

    // Initial pack with old key
    pack::pack(&plugin_dir, Some(&old_key_path), None).unwrap();

    // Rotate to new key
    pack::rotate_key(&old_key_path, &new_key_path, &plugin_dir).unwrap();
    assert!(plugin_dir.join("KEY-ROTATION").is_file());

    // Pack with new key
    let zip_path = pack::pack(&plugin_dir, Some(&new_key_path), None).unwrap();
    let zip_bytes = fs::read(&zip_path).unwrap();
    let pkg = Package::read_zip(&zip_bytes).unwrap();
    let manifest_bytes = pkg.get("manifest.lua").unwrap();
    let manifest = manifest::evaluate(std::str::from_utf8(manifest_bytes).unwrap()).unwrap();

    let old_pub = signing::PublicKey::parse(&old_pub_str).unwrap();
    let new_pub = signing::PublicKey::parse(&new_pub_str).unwrap();

    let rotation = signing::rotation(&pkg).unwrap().expect("rotation present");
    assert_eq!(rotation.old, old_pub);
    assert_eq!(rotation.new, new_pub);

    let verified = signing::verify(&pkg, &manifest).unwrap();
    assert_eq!(verified, SignatureState::Signed(new_pub));
}

#[test]
fn pack_refuses_a_key_inside_the_folder() {
    let tmp = tempdir().unwrap();
    let id = "my-plugin";
    init::init(id, tmp.path()).unwrap();
    let plugin_dir = tmp.path().join(id);

    let key_path = plugin_dir.join("my-plugin.key");
    keys::keygen(Some(&key_path)).unwrap();

    // 1. Pointing --key to a path inside the folder
    let err = pack::pack(&plugin_dir, Some(&key_path), None).unwrap_err();
    assert!(
        err.contains("is inside the plugin folder") && err.contains("refusing to pack private key"),
        "unexpected error: {err}"
    );

    // 2. Unsigned pack with a key file in the folder is refused by package::add_dir
    let err = pack::pack(&plugin_dir, None, None).unwrap_err();
    assert!(
        err.contains("contains a private key"),
        "unexpected error: {err}"
    );
}

#[tokio::test]
async fn dev_push_refuses_a_key_inside_the_folder() {
    let tmp = tempdir().unwrap();
    let id = "dev-bot";
    init::init(id, tmp.path()).unwrap();
    let plugin_dir = tmp.path().join(id);

    let key_path = plugin_dir.join("secret.key");
    keys::keygen(Some(&key_path)).unwrap();

    let opts = dev::DevOptions {
        server: "127.0.0.1:8000",
        fingerprint: None,
        token: "token",
        folder: &plugin_dir,
    };

    let err = dev::push_once(&opts).await.unwrap_err();
    match err {
        dev::PushError::Refused(msg) => {
            assert!(
                msg.contains("contains a private key"),
                "expected error mentioning private key, got: {msg}"
            );
        }
        other => panic!("expected PushError::Refused, got {other:?}"),
    }
}

#[test]
fn a_key_under_another_name_is_refused() {
    let tmp = tempdir().unwrap();
    let id = "notes-bot";
    init::init(id, tmp.path()).unwrap();
    let plugin_dir = tmp.path().join(id);

    // Write a key inside sub/notes.txt (does NOT end in .key)
    let sub = plugin_dir.join("sub");
    fs::create_dir_all(&sub).unwrap();
    let key_path = sub.join("notes.txt");
    keys::keygen(Some(&key_path)).unwrap();

    let output_zip = tmp.path().join("out.zip");
    let err = pack::pack(&plugin_dir, None, Some(&output_zip)).unwrap_err();

    assert!(
        err.contains("contains a private key"),
        "unexpected error: {err}"
    );
}

#[derive(Clone, Default)]
struct SharedBuffer(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for SharedBuffer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl SharedBuffer {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

async fn wait_for_output(buf: &SharedBuffer, needle: &str) {
    wait_for_count(buf, needle, 1).await;
}

/// Waits until `needle` has been printed `wanted` times, for output a run
/// prints more than once: the buffer keeps everything printed so far.
async fn wait_for_count(buf: &SharedBuffer, needle: &str, wanted: usize) {
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        if buf.text().matches(needle).count() >= wanted {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!(
        "timed out waiting for {wanted} of {needle:?} in output:\n{}",
        buf.text()
    );
}

#[tokio::test]
async fn dev_pushes_and_prints_the_log() {
    let server = TestServer::start().await;
    let (mut owner_client, _ready) = owner(&server).await;

    // Create a dev token
    let reply = owner_client
        .ok(Kind::CreatePluginDevToken(proto::CreatePluginDevToken {
            plugin_id: "dev-bot".to_owned(),
            days: 1,
        }))
        .await;
    let proto::response::Result::CreatedPluginDevToken(created) = reply else {
        panic!("expected CreatedPluginDevToken, got {reply:?}");
    };
    let token = created.secret;

    let tmp = tempdir().unwrap();
    let plugin_dir = tmp.path().join("dev-bot");
    fs::create_dir_all(plugin_dir.join("server")).unwrap();
    fs::write(
        plugin_dir.join("manifest.lua"),
        r#"return {
  id = "dev-bot",
  name = "Dev Bot",
  version = "0.1.0",
  author = "Tester",
  license = "MIT",
  api = 1,
  server = { "server/main.luau" },
}
"#,
    )
    .unwrap();
    fs::write(
        plugin_dir.join("server/main.luau"),
        r#"opencord.on("start", function()
  log.info("dev hello from test")
  log.info("dev hello from test")
end)
"#,
    )
    .unwrap();

    let server_addr: &'static str =
        Box::leak(server.handle.local_addr.to_string().into_boxed_str());
    let fingerprint_hex: &'static str =
        Box::leak(hex::encode(server.handle.fingerprint).into_boxed_str());
    let token: &'static str = Box::leak(token.into_boxed_str());
    let plugin_dir_ref: &'static Path = Box::leak(plugin_dir.clone().into_boxed_path());

    let opts = dev::DevOptions {
        server: server_addr,
        fingerprint: Some(fingerprint_hex),
        token,
        folder: plugin_dir_ref,
    };

    let buf = SharedBuffer::default();
    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);

    let dev_task = tokio::spawn({
        let buf = buf.clone();
        async move { dev::run_dev_with(opts, buf, Some(stop_rx)).await }
    });

    // Wait for initial push asking for review
    wait_for_output(
        &buf,
        "Waiting for an admin to review it on the Plugins page",
    )
    .await;

    // Admin reviews and accepts the plugin
    let reply = owner_client
        .ok(Kind::FetchPluginReviews(proto::FetchPluginReviews {}))
        .await;
    let proto::response::Result::PluginReviews(list) = reply else {
        panic!("expected PluginReviews");
    };
    let review = list
        .reviews
        .into_iter()
        .find(|r| r.plugin.as_ref().is_some_and(|p| p.id == "dev-bot"))
        .expect("review found");
    owner_client
        .ok(Kind::ConfirmPluginReview(proto::ConfirmPluginReview {
            review_id: review.review_id,
            granted: vec![],
            config: vec![],
            accept_license: true,
        }))
        .await;

    // Both identical lines from the same millisecond must be printed (Finding 10)
    wait_for_output(&buf, "INFO dev hello from test").await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let count = buf.text().matches("INFO dev hello from test").count();
    assert_eq!(
        count, 2,
        "identical log lines in same ms must not be dropped"
    );

    // Edit the file to trigger re-push
    tokio::time::sleep(Duration::from_millis(50)).await;
    fs::write(
        plugin_dir.join("server/main.luau"),
        r#"opencord.on("start", function()
  log.info("hello after edit")
end)
"#,
    )
    .unwrap();

    // Verify it pushed and installed
    wait_for_output(&buf, "Installed 0.1.0, running").await;

    // Verify the log was polled and printed
    wait_for_output(&buf, "INFO hello after edit").await;

    // Test push retry when server is down during an edit (Finding 5)
    let port = server.handle.local_addr.port();
    let server_dir = server.dir;
    server.handle.shutdown().await;

    // Edit while server is down
    tokio::time::sleep(Duration::from_millis(50)).await;
    fs::write(
        plugin_dir.join("server/main.luau"),
        r#"opencord.on("start", function()
  log.info("hello after restart")
end)
"#,
    )
    .unwrap();

    // Verify it attempts push, fails with connection error, and retries in 2 seconds
    wait_for_output(&buf, "Retrying in 2 seconds...").await;

    // Start server again on the same port and data directory
    let _server = TestServer::start_in(server_dir, |c| {
        c.server.bind = format!("127.0.0.1:{port}").parse().unwrap();
    })
    .await;

    // Verify it automatically reconnected, pushed and installed: the second
    // "Installed", since the edit before the restart printed the first.
    wait_for_count(&buf, "Installed 0.1.0, running", 2).await;
    wait_for_output(&buf, "INFO hello after restart").await;

    // Signal stop and join task
    let _ = stop_tx.send(true);
    let res = dev_task.await.unwrap();
    assert!(res.is_ok());
}

#[tokio::test]
async fn dev_initial_push_retries_when_server_is_down() {
    let tmp = tempdir().unwrap();
    let id = "offline-bot";
    init::init(id, tmp.path()).unwrap();
    let plugin_dir = tmp.path().join(id);

    let opts = dev::DevOptions {
        server: "127.0.0.1:49999",
        fingerprint: None,
        token: "fake-token",
        folder: Box::leak(plugin_dir.into_boxed_path()),
    };

    let buf = SharedBuffer::default();
    let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);

    let dev_task = tokio::spawn({
        let buf = buf.clone();
        async move { dev::run_dev_with(opts, buf, Some(stop_rx)).await }
    });

    // Twice: the message is printed before the wait, so one proves nothing
    // about a second attempt.
    wait_for_count(&buf, "Retrying in 2 seconds...", 2).await;

    let _ = stop_tx.send(true);
    let res = dev_task.await.unwrap();
    assert!(res.is_ok());
}
