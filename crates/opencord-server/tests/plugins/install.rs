//! Installing, updating and removing plugins (plan Task 8).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ed25519_dalek::SigningKey;
use opencord_common::permissions::Permissions;
use opencord_plugins::limits::PENDING_REVIEW_TIME;
use opencord_proto::v1 as proto;
use proto::request::Kind;
use proto::response::Result as Reply;

use crate::common::{
    TestClient, TestServer, channel_id, invite, join, media_token, owner, self_id,
};
use crate::support::{
    confirm, install, log_lines, manifest, manifest_with, package, plugins, public_key, refusal,
    review, signed_package, upload_status, wait_for_log,
};

fn logs_at_start(text: &str) -> String {
    format!("opencord.on('start', function() log.info({text:?}) end)")
}

fn plugin_dir(server: &TestServer, id: &str) -> PathBuf {
    server.dir.path().join("data/plugins").join(id)
}

/// Every file under `dir`, none when it doesn't exist.
fn files_under(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flat_map(|entry| {
            let path = entry.unwrap().path();
            if path.is_dir() {
                files_under(&path)
            } else {
                vec![path]
            }
        })
        .collect()
}

/// The version folders (`v<ms>`) in a plugin's folder.
fn versions(server: &TestServer, id: &str) -> Vec<String> {
    std::fs::read_dir(plugin_dir(server, id))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with('v'))
        .collect()
}

fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

/// Version `version` of plugin `id`, signed by `signer`, logging `text`
/// when it starts.
fn signed(id: &str, version: &str, signer: &SigningKey, rotation: Option<&SigningKey>) -> Vec<u8> {
    let extra = format!("public_key = \"{}\",", public_key(signer));
    signed_package(
        &[
            ("manifest.lua", &manifest_with(id, version, &[], &extra)),
            ("server/main.luau", &logs_at_start("started")),
        ],
        signer,
        rotation,
    )
}

fn unsigned(id: &str, version: &str) -> Vec<u8> {
    package(&[
        ("manifest.lua", &manifest_with(id, version, &[], "")),
        ("server/main.luau", ""),
    ])
}

fn with_greeting(review: &proto::PluginReview, granted: &[&str], greeting: &str) -> Kind {
    let Kind::ConfirmPluginReview(request) = confirm(review, granted) else {
        unreachable!()
    };
    Kind::ConfirmPluginReview(proto::ConfirmPluginReview {
        config: vec![proto::PluginConfigValue {
            key: "greeting".to_owned(),
            value: greeting.to_owned(),
            has_value: true,
        }],
        ..request
    })
}

const GREETING: &str = r#"config = { { key = "greeting", label = "Greeting", kind = "text" } },"#;

fn config_value(plugin: &proto::Plugin, key: &str) -> String {
    plugin
        .config
        .iter()
        .find(|value| value.key == key)
        .map(|value| value.value.clone())
        .unwrap_or_default()
}

fn granted(plugin: &proto::Plugin) -> Vec<String> {
    plugin
        .permissions
        .iter()
        .filter(|grant| grant.granted)
        .map(|grant| grant.name.clone())
        .collect()
}

async fn uninstall(owner: &mut TestClient, id: &str, delete_data: bool) {
    owner
        .ok(Kind::UninstallPlugin(proto::UninstallPlugin {
            plugin_id: id.to_owned(),
            delete_data,
        }))
        .await;
}

#[tokio::test]
async fn an_admin_installs_a_plugin_and_it_runs() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let zip = package(&[
        ("manifest.lua", &manifest("hello", &[])),
        ("server/main.luau", &logs_at_start("hello from start")),
    ]);

    let plugin = install(&server, &mut owner, &media_token(&ready), zip, &[]).await;

    assert_eq!(plugin.id, "hello");
    assert_eq!(plugin.state, proto::PluginState::Running as i32);
    wait_for_log(&mut owner, "hello", "hello from start").await;
    assert_eq!(plugins(&mut owner).await, vec![plugin]);
}

// Catches: an upload that checks only the token, not the permission.
#[tokio::test]
async fn members_without_manage_plugins_cannot_upload() {
    let server = TestServer::start().await;
    let (mut owner, _) = owner(&server).await;
    let code = invite(&mut owner, None).await;
    let (_alice, alice_ready) = join(&server, 2, &code).await;
    let zip = unsigned("hello", "1.0.0");

    let (status, reply) = upload_status(&server, &media_token(&alice_ready), zip.clone()).await;
    let (anonymous, _) = server
        .send_http("POST", "/plugins/packages", &[], &zip)
        .await;

    assert_eq!(status, 403);
    let Reply::Error(error) = reply else {
        panic!("{reply:?}")
    };
    assert_eq!(error.code, proto::ErrorCode::Forbidden as i32);
    assert_eq!(anonymous, 401);
    assert!(files_under(&server.dir.path().join("data/plugins")).is_empty());
}

fn zip_of(entries: &[(&str, &[u8])], link: Option<&str>) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in entries {
        writer.start_file(*name, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    if let Some(name) = link {
        writer
            .add_symlink(name, "../../../etc/passwd", options)
            .unwrap();
    }
    writer.finish().unwrap().into_inner()
}

// Catches: unpacking before checking, or keeping what a refused upload
// wrote.
#[tokio::test]
async fn a_refused_zip_leaves_no_files() {
    let server = TestServer::start().await;
    let (_owner, ready) = owner(&server).await;
    let token = media_token(&ready);
    let manifest = manifest("hostile", &[]);
    let manifest = manifest.as_bytes();
    let zeros = vec![0u8; 2 << 20];
    let cases: Vec<(Vec<u8>, &str)> = vec![
        (
            zip_of(&[("manifest.lua", manifest), ("../x", b"x")], None),
            "../x: the path isn't allowed",
        ),
        (
            zip_of(&[("manifest.lua", manifest), ("/x", b"x")], None),
            "/x: the path isn't allowed",
        ),
        (
            zip_of(&[("manifest.lua", manifest)], Some("server/link.lua")),
            "server/link.lua: links aren't allowed",
        ),
        (
            zip_of(
                &[
                    ("manifest.lua", manifest),
                    ("server/a.lua", b"x"),
                    ("server/A.lua", b"x"),
                ],
                None,
            ),
            "two files differ only by upper and lower case",
        ),
        (
            zip_of(
                &[("manifest.lua", manifest), ("assets/zeros.wav", &zeros)],
                None,
            ),
            "assets/zeros.wav is compressed suspiciously well",
        ),
        (
            zip_of(&[("server/main.luau", b"")], None),
            "manifest.lua is missing",
        ),
    ];

    for (zip, rule) in cases {
        let (status, reply) = upload_status(&server, &token, zip).await;
        let Reply::Error(error) = reply else {
            panic!("{rule}: {reply:?}")
        };
        assert_eq!(status, 400, "{rule}");
        assert_eq!(
            error.code,
            proto::ErrorCode::PluginPackageInvalid as i32,
            "{rule}"
        );
        assert!(error.message.contains(rule), "{rule}: {}", error.message);
    }
    assert!(files_under(&server.dir.path().join("data/plugins")).is_empty());
}

// Catches: trusting the `granted` list an app sends.
#[tokio::test]
async fn permissions_the_admin_does_not_hold_cannot_be_granted() {
    let server = TestServer::start().await;
    let (mut owner, _) = owner(&server).await;
    let code = invite(&mut owner, None).await;
    let (mut alice, alice_ready) = join(&server, 2, &code).await;
    let Reply::Role(role) = owner
        .ok(Kind::CreateRole(proto::CreateRole {
            name: "Plugin admins".to_owned(),
            permissions: Permissions::MANAGE_PLUGINS.bits(),
            ..Default::default()
        }))
        .await
    else {
        panic!("no role");
    };
    owner
        .ok(Kind::AddMemberRole(proto::AddMemberRole {
            user_id: self_id(&alice_ready),
            role_id: role.id,
        }))
        .await;
    let zip = package(&[
        (
            "manifest.lua",
            &manifest("asker", &["read_channels", "kick_members"]),
        ),
        ("server/main.luau", ""),
    ]);

    let review = review(&server, &media_token(&alice_ready), zip).await;
    let refused = alice
        .error(confirm(&review, &["read_channels", "kick_members"]))
        .await;
    let installed = alice.ok(confirm(&review, &["read_channels"])).await;

    let shown = &review.plugin.as_ref().unwrap().permissions;
    let kick = shown.iter().find(|p| p.name == "kick_members").unwrap();
    let read = shown.iter().find(|p| p.name == "read_channels").unwrap();
    assert!(!kick.granted && !kick.grantable);
    assert!(read.granted && read.grantable);
    assert_eq!(refused.code, proto::ErrorCode::Forbidden as i32);
    assert_eq!(refused.message, "you can't grant kick_members");
    let Reply::Plugin(plugin) = installed else {
        panic!("{installed:?}")
    };
    assert_eq!(granted(&plugin), vec!["read_channels"]);
}

#[tokio::test]
async fn a_required_setting_must_be_filled() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let extra = r#"config = { { key = "welcome", label = "Welcome channel", kind = "channel", required = true } },"#;
    let zip = package(&[
        (
            "manifest.lua",
            &manifest_with("welcome", "1.0.0", &[], extra),
        ),
        ("server/main.luau", ""),
    ]);
    let review = review(&server, &media_token(&ready), zip).await;
    let with = |value: &str| {
        let Kind::ConfirmPluginReview(request) = confirm(&review, &[]) else {
            unreachable!()
        };
        Kind::ConfirmPluginReview(proto::ConfirmPluginReview {
            config: vec![proto::PluginConfigValue {
                key: "welcome".to_owned(),
                value: value.to_owned(),
                has_value: true,
            }],
            ..request
        })
    };

    let missing = owner.error(confirm(&review, &[])).await;
    let wrong = owner.error(with("123")).await;
    let general = channel_id(&ready, "general").to_string();
    let installed = owner.ok(with(&general)).await;

    assert_eq!(missing.code, proto::ErrorCode::InvalidArgument as i32);
    assert_eq!(missing.message, "Welcome channel is required");
    assert_eq!(
        wrong.message,
        "Welcome channel must be a channel of this server"
    );
    let Reply::Plugin(plugin) = installed else {
        panic!("{installed:?}")
    };
    assert_eq!(config_value(&plugin, "welcome"), general);
}

#[tokio::test]
async fn an_update_keeps_data_and_shows_new_permissions() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let token = media_token(&ready);
    let v1 = package(&[
        (
            "manifest.lua",
            &manifest_with("keeper", "1.0.0", &["read_channels"], GREETING),
        ),
        ("server/main.luau", ""),
    ]);
    let first = review(&server, &token, v1).await;
    let Reply::Plugin(old) = owner
        .ok(with_greeting(&first, &["read_channels"], "hi"))
        .await
    else {
        panic!("not installed");
    };
    std::fs::write(plugin_dir(&server, "keeper").join("data.sqlite"), b"kept").unwrap();
    let v2 = package(&[
        (
            "manifest.lua",
            &manifest_with(
                "keeper",
                "2.0.0",
                &["read_channels", "kick_members"],
                GREETING,
            ),
        ),
        ("server/main.luau", ""),
    ]);

    let update = review(&server, &token, v2).await;
    let Reply::Plugin(new) = owner
        .ok(confirm(&update, &["read_channels", "kick_members"]))
        .await
    else {
        panic!("not updated");
    };

    assert_eq!(update.installed.as_ref().unwrap().version, "1.0.0");
    assert_eq!(update.new_permissions, vec!["kick_members"]);
    assert_eq!(new.version, "2.0.0");
    assert_eq!((new.user_id, new.role_id), (old.user_id, old.role_id));
    assert_eq!(config_value(&new, "greeting"), "hi");
    // In the manifest's order.
    assert_eq!(granted(&new), vec!["read_channels", "kick_members"]);
    assert_eq!(
        std::fs::read(plugin_dir(&server, "keeper").join("data.sqlite")).unwrap(),
        b"kept"
    );
    assert_eq!(versions(&server, "keeper").len(), 1);
}

// Catches: deleting the old version before the new one started, or
// leaving the plugin on the broken version.
#[tokio::test]
async fn an_update_that_fails_to_start_rolls_back() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let token = media_token(&ready);
    let v1 = package(&[
        (
            "manifest.lua",
            &manifest_with("phoenix", "1.0.0", &["read_channels"], GREETING),
        ),
        ("server/main.luau", &logs_at_start("v1 started")),
    ]);
    let first = review(&server, &token, v1).await;
    owner
        .ok(with_greeting(&first, &["read_channels"], "hi"))
        .await;
    wait_for_log(&mut owner, "phoenix", "v1 started").await;
    let old_versions = versions(&server, "phoenix");
    let v2 = package(&[
        (
            "manifest.lua",
            &manifest_with("phoenix", "2.0.0", &["read_channels"], GREETING),
        ),
        (
            "server/main.luau",
            "opencord.on('start', function() error('v2 is broken') end)",
        ),
    ]);

    let update = review(&server, &token, v2).await;
    let error = owner.error(confirm(&update, &[])).await;

    assert_eq!(error.code, proto::ErrorCode::PluginUnavailable as i32);
    assert!(
        error
            .message
            .starts_with("the new version failed to start: ")
            && error.message.contains("v2 is broken"),
        "{}",
        error.message
    );
    let back = plugins(&mut owner).await.remove(0);
    assert_eq!(back.version, "1.0.0");
    assert_eq!(back.state, proto::PluginState::Running as i32);
    assert_eq!(config_value(&back, "greeting"), "hi");
    assert_eq!(granted(&back), vec!["read_channels"]);
    let lines = log_lines(&mut owner, "phoenix").await;
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.contains("v1 started"))
            .count(),
        2,
        "{lines:#?}"
    );
    assert!(
        lines
            .iter()
            .any(|line| line.contains("the new version failed to start"))
    );
    assert_eq!(versions(&server, "phoenix"), old_versions);
}

// Catches: comparing keys only when the installed plugin is unsigned.
#[tokio::test]
async fn a_different_key_cannot_update_a_signed_plugin() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let token = media_token(&ready);
    install(
        &server,
        &mut owner,
        &token,
        signed("signed", "1.0.0", &key(1), None),
        &[],
    )
    .await;

    let (status, reply) =
        upload_status(&server, &token, signed("signed", "2.0.0", &key(2), None)).await;

    assert_eq!(status, 409);
    let Reply::Error(error) = reply else {
        panic!("{reply:?}")
    };
    assert_eq!(error.code, proto::ErrorCode::PluginNotSameAuthor as i32);
    assert_eq!(error.message, "this isn't from the same author");
}

#[tokio::test]
async fn a_rotation_lets_the_new_key_update() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let token = media_token(&ready);
    install(
        &server,
        &mut owner,
        &token,
        signed("rotor", "1.0.0", &key(1), None),
        &[],
    )
    .await;

    let rotated = signed("rotor", "2.0.0", &key(2), Some(&key(1)));
    let plugin = install(&server, &mut owner, &token, rotated, &[]).await;
    let old_key = refusal(&server, &token, signed("rotor", "3.0.0", &key(1), None)).await;

    assert_eq!(plugin.version, "2.0.0");
    assert_eq!(plugin.key_fingerprint, public_key(&key(2)).fingerprint());
    assert_eq!(old_key.code, proto::ErrorCode::PluginNotSameAuthor as i32);
}

#[tokio::test]
async fn an_unsigned_zip_cannot_replace_a_signed_plugin() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let token = media_token(&ready);
    install(
        &server,
        &mut owner,
        &token,
        signed("pinned", "1.0.0", &key(1), None),
        &[],
    )
    .await;

    let error = refusal(&server, &token, unsigned("pinned", "2.0.0")).await;

    assert_eq!(error.code, proto::ErrorCode::PluginNotSameAuthor as i32);
}

#[tokio::test]
async fn uninstalling_without_deleting_keeps_data_for_a_reinstall() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let token = media_token(&ready);
    let first = install(
        &server,
        &mut owner,
        &token,
        signed("kept", "1.0.0", &key(1), None),
        &[],
    )
    .await;
    let data = plugin_dir(&server, "kept").join("data.sqlite");
    std::fs::write(&data, b"kept").unwrap();

    uninstall(&mut owner, "kept", false).await;
    let listed = plugins(&mut owner).await;
    let unsigned = refusal(&server, &token, unsigned("kept", "1.0.0")).await;
    let again = install(
        &server,
        &mut owner,
        &token,
        signed("kept", "1.0.0", &key(1), None),
        &[],
    )
    .await;

    assert!(listed.is_empty());
    assert_eq!(unsigned.code, proto::ErrorCode::PluginNotSameAuthor as i32);
    assert_eq!(again.user_id, first.user_id);
    assert_eq!(std::fs::read(&data).unwrap(), b"kept");
    assert_eq!(versions(&server, "kept").len(), 1);
}

#[tokio::test]
async fn uninstalling_with_delete_removes_everything() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let token = media_token(&ready);
    install(
        &server,
        &mut owner,
        &token,
        signed("gone", "1.0.0", &key(1), None),
        &[],
    )
    .await;
    std::fs::write(plugin_dir(&server, "gone").join("data.sqlite"), b"data").unwrap();

    uninstall(&mut owner, "gone", true).await;

    assert!(!plugin_dir(&server, "gone").exists());
    assert!(plugins(&mut owner).await.is_empty());
    // Nothing pins it any more: another author may use the id.
    install(
        &server,
        &mut owner,
        &token,
        signed("gone", "1.0.0", &key(2), None),
        &[],
    )
    .await;
}

#[tokio::test]
async fn reviews_expire() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let review = review(&server, &media_token(&ready), unsigned("late", "1.0.0")).await;
    let folder = server
        .dir
        .path()
        .join("data/plugins/.reviews")
        .join(&review.review_id);
    let fetch = Kind::FetchPluginReviews(proto::FetchPluginReviews {});
    let Reply::PluginReviews(waiting) = owner.ok(fetch.clone()).await else {
        panic!("no reviews");
    };
    assert_eq!(waiting.reviews.len(), 1);
    assert!(folder.is_dir());

    server
        .handle
        .plugin_reviews()
        .move_clock(PENDING_REVIEW_TIME + Duration::from_secs(1));
    let late = owner.error(confirm(&review, &[])).await;
    let swept = server.handle.plugin_reviews().sweep().await;
    let Reply::PluginReviews(left) = owner.ok(fetch).await else {
        panic!("no reviews");
    };

    assert_eq!(late.code, proto::ErrorCode::NotFound as i32);
    assert_eq!(swept, vec![review.review_id.clone()]);
    assert!(left.reviews.is_empty());
    assert!(!folder.exists());
}

#[tokio::test]
async fn a_plugin_restarted_by_the_server_comes_back() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let zip = package(&[
        ("manifest.lua", &manifest("phoenix", &[])),
        ("server/main.luau", &logs_at_start("up again")),
    ]);
    install(&server, &mut owner, &media_token(&ready), zip, &[]).await;
    drop(owner);

    let server = server.restart(|_| {}).await;
    let (mut owner, _) = crate::common::owner_again(&server).await;

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let listed = plugins(&mut owner).await;
        if listed[0].state == proto::PluginState::Running as i32 {
            break;
        }
        assert!(Instant::now() < deadline, "{listed:#?}");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    wait_for_log(&mut owner, "phoenix", "up again").await;
}
