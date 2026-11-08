//! Dev tokens and dev pushes (plan Task 12).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use ed25519_dalek::SigningKey;
use opencord_common::permissions::Permissions;
use opencord_proto::v1 as proto;
use proto::request::Kind;
use proto::response::Result as Reply;

use crate::common::{
    TestClient, TestServer, decode_response, invite, join, media_token, owner, self_id,
};
use crate::support::{
    confirm, install, manifest_with, package, plugins, public_key, review, signed_package,
    wait_for_log,
};

const GREETING: &str = r#"config = { { key = "greeting", label = "Greeting", kind = "text" } },"#;

/// Version `version` of plugin `id` asking for `permissions`, with `extra`
/// manifest fields, logging `text` when it starts.
fn bot(id: &str, version: &str, permissions: &[&str], extra: &str, text: &str) -> Vec<u8> {
    package(&[
        (
            "manifest.lua",
            &manifest_with(id, version, permissions, extra),
        ),
        (
            "server/main.luau",
            &format!("opencord.on('start', function() log.info({text:?}) end)"),
        ),
    ])
}

async fn create_token(
    owner: &mut TestClient,
    plugin_id: &str,
    days: u32,
) -> proto::CreatedPluginDevToken {
    let reply = owner
        .ok(Kind::CreatePluginDevToken(proto::CreatePluginDevToken {
            plugin_id: plugin_id.to_owned(),
            days,
        }))
        .await;
    let Reply::CreatedPluginDevToken(created) = reply else {
        panic!("no token: {reply:?}");
    };
    created
}

async fn tokens(owner: &mut TestClient) -> Vec<proto::PluginDevToken> {
    let reply = owner
        .ok(Kind::FetchPluginDevTokens(proto::FetchPluginDevTokens {}))
        .await;
    let Reply::PluginDevTokens(list) = reply else {
        panic!("no tokens: {reply:?}");
    };
    list.tokens
}

/// `PUT /plugins/dev/{id}` with `secret`: the status and the answer.
async fn push_once(server: &TestServer, secret: &str, id: &str, zip: &[u8]) -> (u16, Reply) {
    let (status, body) = server
        .send_http(
            "PUT",
            &format!("/plugins/dev/{id}"),
            &[("Authorization", &format!("Bearer {secret}"))],
            zip,
        )
        .await;
    (status, decode_response(&body))
}

/// [`push_once`], waiting out the push rate as `opencord-plugin dev` does.
async fn push(server: &TestServer, secret: &str, id: &str, zip: &[u8]) -> (u16, Reply) {
    loop {
        let (status, reply) = push_once(server, secret, id, zip).await;
        if status != 429 {
            return (status, reply);
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// `GET /plugins/dev/{id}/log?after=<after>`: the status, the lines and
/// how long the answer took.
async fn poll_log(
    server: &TestServer,
    secret: &str,
    id: &str,
    after: u64,
) -> (u16, Vec<proto::PluginLogLine>, Duration) {
    let started = Instant::now();
    let (status, body) = server
        .send_http(
            "GET",
            &format!("/plugins/dev/{id}/log?after={after}"),
            &[("Authorization", &format!("Bearer {secret}"))],
            &[],
        )
        .await;
    let lines = match decode_response(&body) {
        Reply::PluginLog(log) => log.lines,
        other => panic!("not a log: {other:?}"),
    };
    (status, lines, started.elapsed())
}

fn error_of(reply: Reply) -> proto::Error {
    match reply {
        Reply::Error(error) => error,
        other => panic!("not refused: {other:?}"),
    }
}

fn plugin_dir(server: &TestServer, id: &str) -> PathBuf {
    server.dir.path().join("data/plugins").join(id)
}

/// The version folders (`v<ms>`) in a plugin's folder.
fn versions(server: &TestServer, id: &str) -> Vec<String> {
    std::fs::read_dir(plugin_dir(server, id))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with('v'))
        .collect()
}

fn granted(plugin: &proto::Plugin) -> Vec<String> {
    plugin
        .permissions
        .iter()
        .filter(|grant| grant.granted)
        .map(|grant| grant.name.clone())
        .collect()
}

fn config_value(plugin: &proto::Plugin, key: &str) -> String {
    plugin
        .config
        .iter()
        .find(|value| value.key == key)
        .map(|value| value.value.clone())
        .unwrap_or_default()
}

/// Installs `zip` through an admin's review, granting `granted` and
/// setting the greeting to "hi".
async fn install_greeting(
    server: &TestServer,
    owner: &mut TestClient,
    token: &str,
    zip: Vec<u8>,
    granted: &[&str],
) -> proto::Plugin {
    let review = review(server, token, zip).await;
    let Kind::ConfirmPluginReview(request) = confirm(&review, granted) else {
        unreachable!()
    };
    let reply = owner
        .ok(Kind::ConfirmPluginReview(proto::ConfirmPluginReview {
            config: vec![proto::PluginConfigValue {
                key: "greeting".to_owned(),
                value: "hi".to_owned(),
                has_value: true,
            }],
            ..request
        }))
        .await;
    let Reply::Plugin(plugin) = reply else {
        panic!("not installed: {reply:?}");
    };
    plugin
}

async fn fetch_reviews(owner: &mut TestClient) -> Vec<proto::PluginReview> {
    let reply = owner
        .ok(Kind::FetchPluginReviews(proto::FetchPluginReviews {}))
        .await;
    let Reply::PluginReviews(list) = reply else {
        panic!("no reviews: {reply:?}");
    };
    list.reviews
}

#[tokio::test]
async fn a_dev_push_installs_and_restarts() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let v1 = bot(
        "devbot",
        "1.0.0",
        &["read_channels"],
        GREETING,
        "v1 started",
    );
    let old = install_greeting(
        &server,
        &mut owner,
        &media_token(&ready),
        v1,
        &["read_channels"],
    )
    .await;
    wait_for_log(&mut owner, "devbot", "v1 started").await;
    let created = create_token(&mut owner, "devbot", 0).await;
    let v2 = bot(
        "devbot",
        "1.0.1",
        &["read_channels"],
        GREETING,
        "v2 started",
    );

    let (status, reply) = push(&server, &created.secret, "devbot", &v2).await;

    assert_eq!(status, 200, "{reply:?}");
    let Reply::Plugin(plugin) = reply else {
        panic!("{reply:?}")
    };
    assert_eq!(plugin.version, "1.0.1");
    assert_eq!(plugin.signature, proto::PluginSignature::Dev as i32);
    assert_eq!(plugin.state, proto::PluginState::Running as i32);
    assert_eq!((plugin.user_id, plugin.role_id), (old.user_id, old.role_id));
    assert_eq!(granted(&plugin), vec!["read_channels"]);
    assert_eq!(config_value(&plugin, "greeting"), "hi");
    wait_for_log(&mut owner, "devbot", "v2 started").await;
    assert_eq!(versions(&server, "devbot").len(), 1);
    let listed = tokens(&mut owner).await;
    assert_eq!(listed.len(), 1);
    assert!(listed[0].last_used_at_ms.is_some());
}

// Catches: dev mode showing its error by rolling back to the old version.
#[tokio::test]
async fn a_dev_push_that_fails_to_start_shows_the_error() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let v1 = bot("broken", "1.0.0", &[], GREETING, "v1 started");
    install_greeting(&server, &mut owner, &media_token(&ready), v1, &[]).await;
    let created = create_token(&mut owner, "broken", 0).await;
    let v2 = package(&[
        (
            "manifest.lua",
            &manifest_with("broken", "1.0.1", &[], GREETING),
        ),
        (
            "server/main.luau",
            "opencord.on('start', function() error('v2 is broken') end)",
        ),
    ]);

    let (status, reply) = push(&server, &created.secret, "broken", &v2).await;

    assert_eq!(status, 200, "{reply:?}");
    let Reply::Plugin(plugin) = reply else {
        panic!("{reply:?}")
    };
    assert_eq!(plugin.version, "1.0.1");
    assert_eq!(plugin.state, proto::PluginState::Crashed as i32);
    assert!(
        plugin.status_reason.contains("v2 is broken"),
        "{}",
        plugin.status_reason
    );
    assert_eq!(plugins(&mut owner).await[0].version, "1.0.1");
}

#[tokio::test]
async fn a_first_dev_push_waits_for_review() {
    let server = TestServer::start().await;
    let (mut owner, _) = owner(&server).await;
    let created = create_token(&mut owner, "fresh", 0).await;

    let (status, reply) = push(
        &server,
        &created.secret,
        "fresh",
        &bot("fresh", "0.1.0", &[], "", "fresh started"),
    )
    .await;

    assert_eq!(status, 202, "{reply:?}");
    let Reply::PluginReview(first) = reply else {
        panic!("{reply:?}")
    };
    assert!(first.dev);
    let shown = first.plugin.as_ref().unwrap();
    assert_eq!(shown.signature, proto::PluginSignature::Dev as i32);
    assert!(first.installed.is_none());
    let announced = owner
        .wait_for(|event| match event {
            proto::event::Kind::PluginReviewUpdate(update) => update.review.clone(),
            _ => None,
        })
        .await;
    assert_eq!(announced.review_id, first.review_id);
    assert!(plugins(&mut owner).await.is_empty());

    // The next push replaces the waiting one.
    let (status, reply) = push(
        &server,
        &created.secret,
        "fresh",
        &bot("fresh", "0.1.1", &[], "", "fresh started"),
    )
    .await;
    assert_eq!(status, 202, "{reply:?}");
    let Reply::PluginReview(second) = reply else {
        panic!("{reply:?}")
    };
    let removed = owner
        .wait_for(|event| match event {
            proto::event::Kind::PluginReviewRemove(remove) => Some(remove.review_id.clone()),
            _ => None,
        })
        .await;
    assert_eq!(removed, first.review_id);
    let waiting = fetch_reviews(&mut owner).await;
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting[0].review_id, second.review_id);
    assert!(
        !server
            .dir
            .path()
            .join("data/plugins/.reviews")
            .join(&first.review_id)
            .exists()
    );

    let Reply::Plugin(plugin) = owner.ok(confirm(&second, &[])).await else {
        panic!("not installed");
    };
    assert_eq!(plugin.version, "0.1.1");
    assert_eq!(plugin.signature, proto::PluginSignature::Dev as i32);
    wait_for_log(&mut owner, "fresh", "fresh started").await;
}

// Catches: a push that gains a permission, a domain or a setting the admin
// never saw, by installing it straight away.
#[tokio::test]
async fn a_dev_push_asking_for_more_waits_for_review() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let v1 = bot(
        "grower",
        "1.0.0",
        &["read_channels", "internet"],
        "",
        "v1 started",
    );
    install(
        &server,
        &mut owner,
        &media_token(&ready),
        v1,
        &["read_channels"],
    )
    .await;
    let created = create_token(&mut owner, "grower", 0).await;
    let more_permissions = bot(
        "grower",
        "1.1.0",
        &["read_channels", "internet", "kick_members"],
        "",
        "v2",
    );
    let more_domains = bot(
        "grower",
        "1.2.0",
        &["read_channels", "internet"],
        r#"domains = { "api.example.com" },"#,
        "v3",
    );
    let required = bot(
        "grower",
        "1.3.0",
        &["read_channels", "internet"],
        r#"config = { { key = "key", label = "Key", kind = "secret", required = true } },"#,
        "v4",
    );

    let mut reviews = Vec::new();
    for zip in [&more_permissions, &more_domains, &required] {
        let (status, reply) = push(&server, &created.secret, "grower", zip).await;
        assert_eq!(status, 202, "{reply:?}");
        let Reply::PluginReview(review) = reply else {
            panic!("{reply:?}")
        };
        reviews.push(review);
    }

    assert_eq!(reviews[0].new_permissions, vec!["kick_members"]);
    assert_eq!(
        reviews[1].plugin.as_ref().unwrap().domains,
        vec!["api.example.com"]
    );
    assert!(reviews.iter().all(|review| review.dev));
    assert_eq!(reviews[2].installed.as_ref().unwrap().version, "1.0.0");
    let installed = plugins(&mut owner).await.remove(0);
    assert_eq!(installed.version, "1.0.0");
    assert_eq!(installed.state, proto::PluginState::Running as i32);
    let waiting = fetch_reviews(&mut owner).await;
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting[0].review_id, reviews[2].review_id);
}

#[tokio::test]
async fn a_dev_push_cannot_replace_a_signed_plugin() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let signer = SigningKey::from_bytes(&[1; 32]);
    let extra = format!("public_key = \"{}\",", public_key(&signer));
    let signed = signed_package(
        &[
            (
                "manifest.lua",
                &manifest_with("pinned", "1.0.0", &[], &extra),
            ),
            ("server/main.luau", ""),
        ],
        &signer,
        None,
    );
    install(&server, &mut owner, &media_token(&ready), signed, &[]).await;
    let created = create_token(&mut owner, "pinned", 0).await;

    let (status, reply) = push(
        &server,
        &created.secret,
        "pinned",
        &bot("pinned", "1.0.1", &[], "", "dev"),
    )
    .await;

    assert_eq!(status, 409);
    let error = error_of(reply);
    assert_eq!(error.code, proto::ErrorCode::PluginNotSameAuthor as i32);
    assert_eq!(
        error.message,
        "a signed plugin with this id is installed; dev mode can't replace it"
    );
    let still = plugins(&mut owner).await.remove(0);
    assert_eq!(still.version, "1.0.0");
    assert_eq!(still.signature, proto::PluginSignature::Signed as i32);
    assert!(fetch_reviews(&mut owner).await.is_empty());
}

// Catches: comparing the expiry with the creation time, or not at all.
#[tokio::test]
async fn an_expired_token_is_refused() {
    let server = TestServer::start().await;
    let (mut owner, _) = owner(&server).await;
    let created = create_token(&mut owner, "late", 1).await;
    let token = created.token.unwrap();
    assert_eq!(
        token.expires_at_ms - token.created_at_ms,
        24 * 60 * 60 * 1000
    );
    let zip = bot("late", "1.0.0", &[], "", "late");
    let (fresh, _) = push(&server, &created.secret, "late", &zip).await;

    server
        .handle
        .plugin_dev_tokens()
        .move_clock(Duration::from_secs(24 * 60 * 60 + 1));
    let (status, reply) = push(&server, &created.secret, "late", &zip).await;

    assert_eq!(fresh, 202);
    assert_eq!(status, 403);
    assert_eq!(error_of(reply).code, proto::ErrorCode::Forbidden as i32);
}

#[tokio::test]
async fn a_revoked_token_is_refused() {
    let server = TestServer::start().await;
    let (mut owner, _) = owner(&server).await;
    let created = create_token(&mut owner, "gone", 0).await;

    owner
        .ok(Kind::RevokePluginDevToken(proto::RevokePluginDevToken {
            token_id: created.token.unwrap().id,
        }))
        .await;
    let (status, reply) = push(
        &server,
        &created.secret,
        "gone",
        &bot("gone", "1.0.0", &[], "", "gone"),
    )
    .await;
    let (log_status, _) = server
        .send_http(
            "GET",
            "/plugins/dev/gone/log?after=0",
            &[("Authorization", &format!("Bearer {}", created.secret))],
            &[],
        )
        .await;

    assert_eq!(status, 403);
    assert_eq!(error_of(reply).code, proto::ErrorCode::Forbidden as i32);
    assert_eq!(log_status, 403);
    assert!(tokens(&mut owner).await.is_empty());
    assert!(fetch_reviews(&mut owner).await.is_empty());
}

// Catches: checking that the token exists but not which plugin it names,
// or trusting the address over the manifest.
#[tokio::test]
async fn a_token_for_another_plugin_is_refused() {
    let server = TestServer::start().await;
    let (mut owner, _) = owner(&server).await;
    let alpha = create_token(&mut owner, "alpha", 0).await;
    let beta = bot("beta", "1.0.0", &[], "", "beta");

    let (other_address, other_reply) = push(&server, &alpha.secret, "beta", &beta).await;
    let (other_manifest, manifest_reply) = push(&server, &alpha.secret, "alpha", &beta).await;
    let (unknown, _) = push(&server, "ocdev_notatoken", "alpha", &beta).await;
    let (anonymous, _) = server
        .send_http("PUT", "/plugins/dev/alpha", &[], &beta)
        .await;

    assert_eq!(other_address, 403);
    assert_eq!(
        error_of(other_reply).code,
        proto::ErrorCode::Forbidden as i32
    );
    assert_eq!(other_manifest, 403);
    assert_eq!(
        error_of(manifest_reply).code,
        proto::ErrorCode::Forbidden as i32
    );
    assert_eq!(unknown, 403);
    assert_eq!(anonymous, 401);
    assert!(fetch_reviews(&mut owner).await.is_empty());
}

// Catches: a token that outlives its creator's right to manage plugins.
#[tokio::test]
async fn a_token_stops_working_when_its_creator_cannot_manage_plugins() {
    let server = TestServer::start().await;
    let (mut owner, _) = owner(&server).await;
    let code = invite(&mut owner, None).await;
    let (mut alice, alice_ready) = join(&server, 2, &code).await;
    let alice_id = self_id(&alice_ready);
    let reply = owner
        .ok(Kind::CreateRole(proto::CreateRole {
            name: "Plugin admin".into(),
            color: 0,
            permissions: Permissions::MANAGE_PLUGINS.bits(),
            hoist: false,
            mentionable: false,
            max_upload_bytes: None,
        }))
        .await;
    let Reply::Role(role) = reply else {
        panic!("{reply:?}")
    };
    owner
        .ok(Kind::AddMemberRole(proto::AddMemberRole {
            user_id: alice_id,
            role_id: role.id,
        }))
        .await;
    let created = create_token(&mut alice, "handed", 0).await;
    let zip = bot("handed", "1.0.0", &[], "", "handed");
    let (while_admin, _) = push(&server, &created.secret, "handed", &zip).await;

    owner
        .ok(Kind::RemoveMemberRole(proto::RemoveMemberRole {
            user_id: alice_id,
            role_id: role.id,
        }))
        .await;
    let (status, reply) = push(&server, &created.secret, "handed", &zip).await;

    assert_eq!(while_admin, 202);
    assert_eq!(status, 403);
    assert_eq!(error_of(reply).code, proto::ErrorCode::Forbidden as i32);
}

// Catches: a secret stored in the clear, or sent again when listing.
#[tokio::test]
async fn the_secret_is_shown_once() {
    let server = TestServer::start().await;
    let (mut owner, _) = owner(&server).await;

    let created = create_token(&mut owner, "secretive", 0).await;
    let listed = owner
        .request(Kind::FetchPluginDevTokens(proto::FetchPluginDevTokens {}))
        .await;

    let secret = &created.secret;
    assert!(secret.starts_with("ocdev_"), "{secret}");
    assert_eq!(secret.len(), "ocdev_".len() + 52);
    assert!(
        secret["ocdev_".len()..]
            .chars()
            .all(|c| c.is_ascii_lowercase() || ('2'..='7').contains(&c))
    );
    assert_eq!(created.server_fingerprint.len(), 64);
    let token = created.token.clone().unwrap();
    assert_eq!(token.plugin_id, "secretive");
    assert_eq!(
        token.expires_at_ms - token.created_at_ms,
        7 * 24 * 60 * 60 * 1000
    );
    assert!(token.last_used_at_ms.is_none());
    let Reply::PluginDevTokens(list) = &listed else {
        panic!("{listed:?}")
    };
    assert_eq!(list.tokens, vec![token]);
    assert!(!format!("{listed:?}").contains(secret.as_str()));
    let other = create_token(&mut owner, "secretive", 0).await;
    assert_ne!(other.secret, created.secret);
    for name in ["opencord.db", "opencord.db-wal"] {
        let path = server.dir.path().join("data").join(name);
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            // The write-ahead log may have been folded in already.
            Err(_) if name.ends_with("-wal") => continue,
            Err(error) => panic!("{name}: {error}"),
        };
        assert!(
            !bytes
                .windows(secret.len())
                .any(|window| window == secret.as_bytes()),
            "{name} holds the secret"
        );
    }
}

#[tokio::test]
async fn creating_a_token_needs_manage_plugins() {
    let server = TestServer::start().await;
    let (mut owner, _) = owner(&server).await;
    let code = invite(&mut owner, None).await;
    let (mut alice, _) = join(&server, 2, &code).await;

    let create = alice
        .error(Kind::CreatePluginDevToken(proto::CreatePluginDevToken {
            plugin_id: "mine".to_owned(),
            days: 0,
        }))
        .await;
    let fetch = alice
        .error(Kind::FetchPluginDevTokens(proto::FetchPluginDevTokens {}))
        .await;
    let too_long = owner
        .error(Kind::CreatePluginDevToken(proto::CreatePluginDevToken {
            plugin_id: "mine".to_owned(),
            days: 31,
        }))
        .await;
    let bad_id = owner
        .error(Kind::CreatePluginDevToken(proto::CreatePluginDevToken {
            plugin_id: "Not An Id".to_owned(),
            days: 0,
        }))
        .await;

    assert_eq!(create.code, proto::ErrorCode::Forbidden as i32);
    assert_eq!(fetch.code, proto::ErrorCode::Forbidden as i32);
    assert_eq!(too_long.code, proto::ErrorCode::InvalidArgument as i32);
    assert_eq!(bad_id.code, proto::ErrorCode::InvalidArgument as i32);
}

#[tokio::test]
async fn dev_pushes_are_rate_limited() {
    let server = TestServer::start().await;
    let (mut owner, _) = owner(&server).await;
    let created = create_token(&mut owner, "hasty", 0).await;

    let mut statuses = Vec::new();
    for _ in 0..3 {
        statuses.push(
            push_once(&server, &created.secret, "hasty", b"not a zip")
                .await
                .0,
        );
    }

    assert_eq!(statuses, vec![400, 400, 429]);
}

#[tokio::test]
async fn log_long_poll_returns_new_lines() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let zip = bot("chatty", "1.0.0", &[], "", "chatty started");
    install(&server, &mut owner, &media_token(&ready), zip, &[]).await;
    wait_for_log(&mut owner, "chatty", "chatty started").await;
    let created = create_token(&mut owner, "chatty", 0).await;
    let (status, old, took) = poll_log(&server, &created.secret, "chatty", 0).await;
    assert_eq!(status, 200);
    assert!(old.iter().any(|line| line.text.contains("chatty started")));
    assert!(took < Duration::from_secs(5), "{took:?}");
    let after = old.last().unwrap().seq;

    let restart = async {
        tokio::time::sleep(Duration::from_millis(500)).await;
        owner
            .ok(Kind::RestartPlugin(proto::RestartPlugin {
                plugin_id: "chatty".to_owned(),
            }))
            .await
    };
    let ((status, new, took), _) =
        tokio::join!(poll_log(&server, &created.secret, "chatty", after), restart);

    assert_eq!(status, 200);
    assert!(!new.is_empty());
    assert!(new.iter().all(|line| line.seq > after), "{new:?}");
    assert!(new.windows(2).all(|pair| pair[0].seq < pair[1].seq));
    assert!(
        took >= Duration::from_millis(400),
        "answered before the line: {took:?}"
    );
    assert!(took < Duration::from_secs(10), "{took:?}");
}
