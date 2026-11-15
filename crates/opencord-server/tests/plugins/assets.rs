// SPDX-License-Identifier: MPL-2.0

//! A plugin's client files, assets and files over HTTP (plan M2 Task 9,
//! design 2026-11-07 §13.1).

use std::time::Duration;

use ed25519_dalek::SigningKey;
use opencord_proto::v1 as proto;
use opencord_server::plugins::assets::client_files;
use proto::request::Kind;
use sha2::{Digest, Sha256};

use crate::common::{TestClient, TestServer, invite, join, media_token, owner, self_id};
use crate::support::{
    install, manifest, manifest_with, package, public_key, signed_package, wait_for_log,
};

const PLUGIN: &str = "gallery";
const SERVER: &str = r#"
opencord.on("start", function()
    files.put("secret.txt", "a private note")
    files.put("banner.png", "public banner bytes", { public = true })
    files.put("page.html", "<script>alert(1)</script>", { public = true })
    files.put("t/shadow.png", "a public file under t", { public = true })
    log.info("files ready")
end)
"#;
const CLIENT: &str = "return { init = function() return {} end }";
const LIST: &str = "return {}";
const LOGO: &str = "not really a png";
const FONT: &str = "not really a font";

fn sha256_hex(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

/// Installs [`PLUGIN`] with client files and assets, and waits for its
/// server script to have saved its files.
async fn gallery(server: &TestServer, owner: &mut TestClient, ready: &proto::Ready) {
    let zip = package(&[
        ("manifest.lua", &manifest(PLUGIN, &[])),
        ("server/main.luau", SERVER),
        ("client/main.luau", CLIENT),
        ("client/ui/list.luau", LIST),
        ("assets/logo.png", LOGO),
        ("assets/font.ttf", FONT),
    ]);
    install(server, owner, &media_token(ready), zip, &[]).await;
    wait_for_log(owner, PLUGIN, "files ready").await;
}

/// A GET with `token` as the media token: status, headers, body.
async fn get(
    server: &TestServer,
    path: &str,
    token: Option<&str>,
    extra: &[(&str, &str)],
) -> (u16, Vec<(String, String)>, Vec<u8>) {
    let bearer = token.map(|token| format!("Bearer {token}"));
    let mut headers: Vec<(&str, &str)> = extra.to_vec();
    if let Some(bearer) = &bearer {
        headers.push(("Authorization", bearer));
    }
    server.send_http_full("GET", path, &headers, b"").await
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

async fn member(
    server: &TestServer,
    owner: &mut TestClient,
    seed: u8,
) -> (TestClient, String, i64) {
    let code = invite(owner, None).await;
    let (client, ready) = join(server, seed, &code).await;
    (client, media_token(&ready), self_id(&ready))
}

// Fails if `client_files` lists `server/` or `manifest.lua`, misses a
// nested client file, or hashes or sizes a file wrongly.
#[tokio::test]
async fn client_files_list_hashes_and_sizes() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    gallery(&server, &mut owner, &ready).await;

    let handle = server.state().plugins.get(PLUGIN).unwrap();
    let listed: Vec<(String, String, u64)> = client_files(&handle)
        .into_iter()
        .map(|file| (file.path, file.sha256, file.size_bytes))
        .collect();
    let expected: Vec<(String, String, u64)> = [
        ("assets/font.ttf", FONT),
        ("assets/logo.png", LOGO),
        ("client/main.luau", CLIENT),
        ("client/ui/list.luau", LIST),
    ]
    .into_iter()
    .map(|(path, text)| (path.to_owned(), sha256_hex(text), text.len() as u64))
    .collect();
    assert_eq!(listed, expected);
}

// Fails if the assets route serves any package path outside `client/` and
// `assets/`, or reads the version folder by the request's path.
#[tokio::test]
async fn server_scripts_are_never_served() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let token = media_token(&ready);
    let signer = SigningKey::from_bytes(&[9; 32]);
    let extra = format!("public_key = \"{}\",", public_key(&signer));
    let zip = signed_package(
        &[
            ("manifest.lua", &manifest_with(PLUGIN, "1.0.0", &[], &extra)),
            ("server/main.luau", SERVER),
            ("client/main.luau", CLIENT),
        ],
        &signer,
        None,
    );
    install(&server, &mut owner, &token, zip, &[]).await;
    wait_for_log(&mut owner, PLUGIN, "files ready").await;

    for path in [
        "server/main.luau",
        "manifest.lua",
        "SIGNATURE",
        "data.sqlite",
    ] {
        let (status, _, body) = get(
            &server,
            &format!("/plugins/{PLUGIN}/assets/{path}"),
            Some(&token),
            &[],
        )
        .await;
        assert_eq!(status, 404, "{path} was served");
        assert!(!String::from_utf8_lossy(&body).contains("files.put"));
    }
    let (status, _, body) = get(
        &server,
        &format!("/plugins/{PLUGIN}/assets/client/main.luau"),
        Some(&token),
        &[],
    )
    .await;
    assert_eq!((status, body.as_slice()), (200, CLIENT.as_bytes()));
}

// Fails if a route joins a decoded path (or plugin id) onto the disk
// before checking it: `..` would then reach the server script, the
// plugin's database or a private file.
#[tokio::test]
async fn dotdot_and_encoded_paths_are_refused() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let token = media_token(&ready);
    gallery(&server, &mut owner, &ready).await;

    let paths = [
        "/plugins/gallery/assets/client/..%2Fserver%2Fmain.luau",
        "/plugins/gallery/assets/client/%2e%2e/server/main.luau",
        "/plugins/gallery/assets/assets/..%2F..%2Fdata.sqlite",
        "/plugins/gallery/assets/client//main.luau",
        "/plugins/gallery/assets//etc/passwd",
        "/plugins/gallery/files/..%2Fdata.sqlite",
        "/plugins/gallery/files/%2e%2e/data.sqlite",
        "/plugins/gallery/files/x/..%2Fsecret.txt",
        "/plugins/gallery/files/x//banner.png",
        "/plugins/gallery/files//etc/passwd",
        "/plugins/gallery/files/t/..%2Fsecret.txt",
        "/plugins/..%2Fplugins%2Fgallery/files/banner.png",
        "/plugins/%2e%2e/assets/gallery%2Fdata.sqlite",
    ];
    for path in paths {
        let (status, _, body) = get(&server, path, Some(&token), &[]).await;
        assert_eq!(status, 404, "{path} answered {status}");
        let body = String::from_utf8_lossy(&body);
        assert!(!body.contains("files.put") && !body.contains("private note"));
    }
}

// Fails if any of the three routes skips the media token, or takes the
// token of someone who is no longer a member.
#[tokio::test]
async fn assets_need_a_media_token() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    gallery(&server, &mut owner, &ready).await;
    let (_member, member_token, member_id) = member(&server, &mut owner, 2).await;
    let file_token = server
        .state()
        .plugins
        .file_tokens
        .issue(PLUGIN, "secret.txt", member_id)
        .unwrap();

    let paths = [
        format!("/plugins/{PLUGIN}/assets/assets/logo.png"),
        format!("/plugins/{PLUGIN}/files/banner.png"),
        format!("/plugins/{PLUGIN}/files/t/{file_token}"),
    ];
    for path in &paths {
        let (status, _, _) = get(&server, path, None, &[]).await;
        assert_eq!(status, 401, "{path} without a token");
        let (status, _, _) = get(&server, path, Some("not-a-token"), &[]).await;
        assert_eq!(status, 401, "{path} with a bad token");
        let (status, _, _) = get(&server, path, Some(&member_token), &[]).await;
        assert_eq!(status, 200, "{path} with the member's token");
    }

    owner
        .ok(Kind::KickMember(proto::KickMember {
            user_id: member_id,
            reason: None,
        }))
        .await;
    for path in &paths {
        let (status, _, _) = get(&server, path, Some(&member_token), &[]).await;
        assert_eq!(status, 403, "{path} after a kick");
    }
}

// Fails if public files are served with a type taken from anything but
// the allowed list (an HTML file would run on the server's origin), or if
// the token route hides public files under `t/`.
#[tokio::test]
async fn a_public_file_is_served() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    gallery(&server, &mut owner, &ready).await;
    let (_member, member_token, _) = member(&server, &mut owner, 2).await;

    let (status, headers, body) = get(
        &server,
        &format!("/plugins/{PLUGIN}/files/banner.png"),
        Some(&member_token),
        &[],
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body, b"public banner bytes");
    assert_eq!(header(&headers, "content-type"), Some("image/png"));
    assert_eq!(header(&headers, "x-content-type-options"), Some("nosniff"));

    let (status, headers, _) = get(
        &server,
        &format!("/plugins/{PLUGIN}/files/page.html"),
        Some(&member_token),
        &[],
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        header(&headers, "content-type"),
        Some("application/octet-stream")
    );
    assert_eq!(header(&headers, "x-content-type-options"), Some("nosniff"));

    // `t/<name>` is also the token route's shape.
    let (status, _, body) = get(
        &server,
        &format!("/plugins/{PLUGIN}/files/t/shadow.png"),
        Some(&member_token),
        &[],
    )
    .await;
    assert_eq!(
        (status, body.as_slice()),
        (200, b"a public file under t".as_slice())
    );

    let (status, _, _) = get(
        &server,
        &format!("/plugins/{PLUGIN}/files/missing.png"),
        Some(&member_token),
        &[],
    )
    .await;
    assert_eq!(status, 404);
}

// Fails if a private file is served by its path, if a token works for a
// member other than the one it was issued for, or under another plugin.
#[tokio::test]
async fn a_private_file_needs_its_own_token() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let owner_token = media_token(&ready);
    gallery(&server, &mut owner, &ready).await;
    let (_member, member_token, member_id) = member(&server, &mut owner, 2).await;

    let by_path = format!("/plugins/{PLUGIN}/files/secret.txt");
    let (status, _, body) = get(&server, &by_path, Some(&owner_token), &[]).await;
    assert_eq!(status, 404);
    assert!(!String::from_utf8_lossy(&body).contains("private note"));

    let for_member = server
        .state()
        .plugins
        .file_tokens
        .issue(PLUGIN, "secret.txt", member_id)
        .unwrap();
    let through_token = format!("/plugins/{PLUGIN}/files/t/{for_member}");
    let (status, _, body) = get(&server, &through_token, Some(&owner_token), &[]).await;
    assert_eq!(status, 404, "another member's token");
    assert!(!String::from_utf8_lossy(&body).contains("private note"));

    let (status, headers, body) = get(&server, &through_token, Some(&member_token), &[]).await;
    assert_eq!(
        (status, body.as_slice()),
        (200, b"a private note".as_slice())
    );
    assert_eq!(
        header(&headers, "content-type"),
        Some("application/octet-stream")
    );
    assert_eq!(header(&headers, "x-content-type-options"), Some("nosniff"));

    let other_plugin = format!("/plugins/other/files/t/{for_member}");
    let (status, _, _) = get(&server, &other_plugin, Some(&member_token), &[]).await;
    assert_eq!(status, 404, "the token under another plugin");
}

// Fails if a token is honoured past its hour, or if the hour runs on a
// clock tests can't move.
#[tokio::test]
async fn a_token_expires_after_an_hour() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let owner_token = media_token(&ready);
    gallery(&server, &mut owner, &ready).await;
    let tokens = &server.state().plugins.file_tokens;
    let file_token = tokens.issue(PLUGIN, "secret.txt", self_id(&ready)).unwrap();
    let path = format!("/plugins/{PLUGIN}/files/t/{file_token}");

    let (status, _, _) = get(&server, &path, Some(&owner_token), &[]).await;
    assert_eq!(status, 200);
    tokens.move_clock(Duration::from_secs(59 * 60));
    let (status, _, _) = get(&server, &path, Some(&owner_token), &[]).await;
    assert_eq!(status, 200, "within the hour");
    tokens.move_clock(Duration::from_secs(2 * 60));
    let (status, _, body) = get(&server, &path, Some(&owner_token), &[]).await;
    assert_eq!(status, 404, "after the hour");
    assert!(!String::from_utf8_lossy(&body).contains("private note"));
}

// Fails if uninstalling leaves the plugin's tokens: the same plugin
// installed again would honour a token issued before.
#[tokio::test]
async fn uninstalling_forgets_its_tokens() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let owner_token = media_token(&ready);
    gallery(&server, &mut owner, &ready).await;
    let file_token = server
        .state()
        .plugins
        .file_tokens
        .issue(PLUGIN, "secret.txt", self_id(&ready))
        .unwrap();
    let path = format!("/plugins/{PLUGIN}/files/t/{file_token}");
    let (status, _, _) = get(&server, &path, Some(&owner_token), &[]).await;
    assert_eq!(status, 200);

    owner
        .ok(Kind::UninstallPlugin(proto::UninstallPlugin {
            plugin_id: PLUGIN.to_owned(),
            delete_data: false,
        }))
        .await;
    gallery(&server, &mut owner, &ready).await;

    let (status, _, _) = get(&server, &path, Some(&owner_token), &[]).await;
    assert_eq!(status, 404);
}

// Fails if the ETag isn't the file's SHA-256, if an asset is marked
// immutable without a matching `v`, or isn't with one.
#[tokio::test]
async fn a_versioned_asset_is_immutable() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;
    let token = media_token(&ready);
    gallery(&server, &mut owner, &ready).await;
    let sha = sha256_hex(LOGO);
    let logo = format!("/plugins/{PLUGIN}/assets/assets/logo.png");

    let (status, headers, body) = get(&server, &logo, Some(&token), &[]).await;
    assert_eq!((status, body.as_slice()), (200, LOGO.as_bytes()));
    assert_eq!(
        header(&headers, "etag"),
        Some(format!("\"{sha}\"").as_str())
    );
    assert_eq!(header(&headers, "cache-control"), Some("no-cache"));
    assert_eq!(header(&headers, "content-type"), Some("image/png"));
    assert_eq!(header(&headers, "x-content-type-options"), Some("nosniff"));

    let (status, headers, _) = get(&server, &format!("{logo}?v={sha}"), Some(&token), &[]).await;
    assert_eq!(status, 200);
    assert_eq!(
        header(&headers, "cache-control"),
        Some("public, max-age=31536000, immutable")
    );

    let stale = sha256_hex("an older logo");
    let (status, headers, _) = get(&server, &format!("{logo}?v={stale}"), Some(&token), &[]).await;
    assert_eq!(status, 200);
    assert_eq!(header(&headers, "cache-control"), Some("no-cache"));

    let etag = format!("\"{sha}\"");
    let (status, _, body) = get(&server, &logo, Some(&token), &[("If-None-Match", &etag)]).await;
    assert_eq!(status, 304);
    assert!(body.is_empty());

    let (status, headers, _) = get(
        &server,
        &format!("/plugins/{PLUGIN}/assets/client/main.luau"),
        Some(&token),
        &[],
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        header(&headers, "content-type"),
        Some("text/plain; charset=utf-8")
    );
    let (_, headers, _) = get(
        &server,
        &format!("/plugins/{PLUGIN}/assets/assets/font.ttf"),
        Some(&token),
        &[],
    )
    .await;
    assert_eq!(header(&headers, "content-type"), Some("font/ttf"));
}
