use std::fs;

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
    assert!(plugin_dir.join("opencord.d.luau").is_file());
    assert!(plugin_dir.join(".luaurc").is_file());
    assert!(plugin_dir.join(".gitignore").is_file());

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
end)
"#,
    )
    .unwrap();

    let server_addr = server.handle.local_addr.to_string();
    let fingerprint_hex = hex::encode(server.handle.fingerprint);

    let opts = dev::DevOptions {
        server: &server_addr,
        fingerprint: Some(&fingerprint_hex),
        token: &token,
        folder: &plugin_dir,
    };

    // Run first iteration of push: asks for review
    let pushed_msg1 = dev::push_once(&opts).await.expect("first push succeeds");
    assert_eq!(
        pushed_msg1,
        "Waiting for an admin to review it on the Plugins page"
    );

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

    // Second push: installed immediately
    let pushed_msg2 = dev::push_once(&opts).await.expect("second push succeeds");
    assert!(pushed_msg2.contains("Installed 0.1.0, running"));

    // Poll logs
    let logs = dev::poll_logs_once(&opts, 0)
        .await
        .expect("poll log succeeds");
    let found = logs.iter().any(|line| line.contains("dev hello from test"));
    assert!(found, "expected log line found in logs: {logs:?}");

    // Test resilience: server restarts on same port and push still succeeds
    let port = server.handle.local_addr.port();
    let _server = server
        .restart(|c| c.server.bind = format!("127.0.0.1:{port}").parse().unwrap())
        .await;
    let pushed_msg3 = dev::push_once(&opts)
        .await
        .expect("push after restart succeeds");
    assert!(pushed_msg3.contains("Installed 0.1.0, running"));
}
