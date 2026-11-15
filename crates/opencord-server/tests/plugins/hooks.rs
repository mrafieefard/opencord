// SPDX-License-Identifier: MPL-2.0

//! Web hook tests (plan Task 19, design 2026-11-07 §10.8, §21).

use opencord_proto::v1 as proto;

use crate::common::{TestClient, TestServer, media_token, owner};
use crate::support::{install, manifest, package, wait_for_log};

async fn plugin_with(
    server: &TestServer,
    owner: &mut TestClient,
    ready: &proto::Ready,
    id: &str,
    permissions: &[&str],
    source: &str,
) -> proto::Plugin {
    let zip = package(&[
        ("manifest.lua", &manifest(id, permissions)),
        ("server/main.luau", source),
    ]);
    install(server, owner, &media_token(ready), zip, permissions).await
}

#[tokio::test]
async fn no_such_plugin_is_404() {
    let server = TestServer::start().await;
    let (status, _) = server
        .send_http("POST", "/plugins/nonexistent/hooks/github", &[], b"{}")
        .await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn no_handler_for_path_is_404() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;

    let _p = plugin_with(
        &server,
        &mut owner,
        &ready,
        "hook-404",
        &["receive_web_requests"],
        r#"
        hooks.on("github", function(req)
            return { status = 200 }
        end)
        log.info("ready")
        "#,
    )
    .await;

    wait_for_log(&mut owner, "hook-404", "ready").await;

    let (status, _) = server
        .send_http("POST", "/plugins/hook-404/hooks/other", &[], b"{}")
        .await;
    assert_eq!(status, 404);
}

#[tokio::test]
async fn stopped_plugin_is_503() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;

    let _p = plugin_with(
        &server,
        &mut owner,
        &ready,
        "hook-stopped",
        &["receive_web_requests"],
        r#"
        hooks.on("github", function(req)
            return { status = 200 }
        end)
        log.info("ready")
        "#,
    )
    .await;

    wait_for_log(&mut owner, "hook-stopped", "ready").await;

    // Stop the plugin
    owner
        .ok(proto::request::Kind::SetPluginRunning(
            proto::SetPluginRunning {
                plugin_id: "hook-stopped".to_owned(),
                running: false,
            },
        ))
        .await;

    let (status, _) = server
        .send_http("POST", "/plugins/hook-stopped/hooks/github", &[], b"{}")
        .await;
    assert_eq!(status, 503);
}

#[tokio::test]
async fn missing_permission_is_403() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;

    // Installed WITHOUT receive_web_requests
    let _p = plugin_with(
        &server,
        &mut owner,
        &ready,
        "hook-no-perm",
        &[],
        r#"
        hooks.on("github", function(req)
            return { status = 200 }
        end)
        log.info("ready")
        "#,
    )
    .await;

    wait_for_log(&mut owner, "hook-no-perm", "ready").await;

    let (status, _) = server
        .send_http("POST", "/plugins/hook-no-perm/hooks/github", &[], b"{}")
        .await;
    assert_eq!(status, 403);
}

#[tokio::test]
async fn body_over_1mb_is_413() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;

    let _p = plugin_with(
        &server,
        &mut owner,
        &ready,
        "hook-1mb",
        &["receive_web_requests"],
        r#"
        hooks.on("large", function(req)
            return { status = 200 }
        end)
        log.info("ready")
        "#,
    )
    .await;

    wait_for_log(&mut owner, "hook-1mb", "ready").await;

    let large_body = vec![b'a'; 1024 * 1024 + 1];
    let (status, _) = server
        .send_http("POST", "/plugins/hook-1mb/hooks/large", &[], &large_body)
        .await;
    assert_eq!(status, 413);
}

#[tokio::test]
async fn rate_limit_exceeded_is_429() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;

    let _p = plugin_with(
        &server,
        &mut owner,
        &ready,
        "hook-rate",
        &["receive_web_requests"],
        r#"
        hooks.on("ping", function(req)
            return { status = 200, body = "pong" }
        end)
        log.info("ready")
        "#,
    )
    .await;

    wait_for_log(&mut owner, "hook-rate", "ready").await;

    let mut successes = 0;
    let mut hit_429 = false;
    for _ in 0..100 {
        let (status, _) = server
            .send_http("GET", "/plugins/hook-rate/hooks/ping", &[], b"")
            .await;
        if status == 200 {
            successes += 1;
        } else if status == 429 {
            hit_429 = true;
            break;
        }
    }
    assert!(
        successes >= 60,
        "expected at least 60 successes before 429, got {successes}"
    );
    assert!(hit_429, "expected to hit 429");
}

#[tokio::test]
async fn slow_plugin_is_504() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;

    let _p = plugin_with(
        &server,
        &mut owner,
        &ready,
        "hook-slow",
        &["receive_web_requests"],
        r#"
        hooks.on("delay", function(req)
            wait(11.0)
            return { status = 200 }
        end)
        log.info("ready")
        "#,
    )
    .await;

    wait_for_log(&mut owner, "hook-slow", "ready").await;

    let (status, _) = server
        .send_http("GET", "/plugins/hook-slow/hooks/delay", &[], b"")
        .await;
    assert_eq!(status, 504);
}

#[tokio::test]
async fn bad_return_value_is_500() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;

    let _p = plugin_with(
        &server,
        &mut owner,
        &ready,
        "hook-bad-ret",
        &["receive_web_requests"],
        r#"
        hooks.on("nil", function(req)
            return nil
        end)
        hooks.on("bad-status", function(req)
            return { status = 999 }
        end)
        log.info("ready")
        "#,
    )
    .await;

    wait_for_log(&mut owner, "hook-bad-ret", "ready").await;

    let (status1, _) = server
        .send_http("GET", "/plugins/hook-bad-ret/hooks/nil", &[], b"")
        .await;
    assert_eq!(status1, 500);

    let (status2, _) = server
        .send_http("GET", "/plugins/hook-bad-ret/hooks/bad-status", &[], b"")
        .await;
    assert_eq!(status2, 500);
}

#[tokio::test]
async fn a_hook_answers_with_the_plugins_response() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;

    let _p = plugin_with(
        &server,
        &mut owner,
        &ready,
        "hook-echo",
        &["receive_web_requests"],
        r#"
        hooks.on("echo", function(req)
            assert(req.method == "POST", "expected method POST")
            assert(req.path == "echo", "expected path echo")
            assert(req.query.user == "alice", "expected query user=alice")
            assert(req.headers["x-custom-header"] == "custom-val", "expected custom header")
            assert(req.body == "payload-data", "expected body payload-data")

            return {
                status = 201,
                headers = {
                    ["content-type"] = "application/json",
                    ["x-response-id"] = "12345",
                },
                body = '{"created":true}'
            }
        end)
        log.info("ready")
        "#,
    )
    .await;

    wait_for_log(&mut owner, "hook-echo", "ready").await;

    let (status, headers, body) = server
        .send_http_full(
            "POST",
            "/plugins/hook-echo/hooks/echo?user=alice",
            &[("X-Custom-Header", "custom-val")],
            b"payload-data",
        )
        .await;

    assert_eq!(status, 201);
    assert_eq!(std::str::from_utf8(&body).unwrap(), r#"{"created":true}"#);

    let content_type = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
        .map(|(_, v)| v.as_str());
    assert_eq!(content_type, Some("application/json"));

    let resp_id = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("x-response-id"))
        .map(|(_, v)| v.as_str());
    assert_eq!(resp_id, Some("12345"));

    // Check action log
    wait_for_log(&mut owner, "hook-echo", "hook POST echo → 201").await;
}

#[tokio::test]
async fn cookies_never_reach_the_plugin() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;

    let _p = plugin_with(
        &server,
        &mut owner,
        &ready,
        "hook-cookies",
        &["receive_web_requests"],
        r#"
        hooks.on("check-cookies", function(req)
            assert(req.headers["cookie"] == nil, "cookie header must be nil")
            assert(req.cookie == nil, "req.cookie must be nil")
            return { status = 200, body = "ok" }
        end)
        log.info("ready")
        "#,
    )
    .await;

    wait_for_log(&mut owner, "hook-cookies", "ready").await;

    let (status, _) = server
        .send_http(
            "GET",
            "/plugins/hook-cookies/hooks/check-cookies",
            &[("Cookie", "session=abcdef; token=12345")],
            b"",
        )
        .await;
    assert_eq!(status, 200);
}

#[tokio::test]
async fn set_cookie_is_dropped() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;

    let _p = plugin_with(
        &server,
        &mut owner,
        &ready,
        "hook-drop-cookie",
        &["receive_web_requests"],
        r#"
        hooks.on("drop-cookie", function(req)
            return {
                status = 200,
                headers = {
                    ["set-cookie"] = "secret=shhh; Path=/",
                    ["Set-Cookie"] = "another=cookie",
                    ["x-safe"] = "safe-header",
                },
                body = "clean"
            }
        end)
        log.info("ready")
        "#,
    )
    .await;

    wait_for_log(&mut owner, "hook-drop-cookie", "ready").await;

    let (status, headers, body) = server
        .send_http_full(
            "GET",
            "/plugins/hook-drop-cookie/hooks/drop-cookie",
            &[],
            b"",
        )
        .await;

    assert_eq!(status, 200);
    assert_eq!(std::str::from_utf8(&body).unwrap(), "clean");

    let has_set_cookie = headers
        .iter()
        .any(|(k, _)| k.eq_ignore_ascii_case("set-cookie"));
    assert!(!has_set_cookie, "Set-Cookie header must be dropped");

    let has_safe = headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("x-safe") && v == "safe-header");
    assert!(has_safe, "safe header must be kept");
}

#[tokio::test]
async fn framing_headers_from_a_hook_are_dropped() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;

    let _p = plugin_with(
        &server,
        &mut owner,
        &ready,
        "hook-framing",
        &["receive_web_requests"],
        r#"
        hooks.on("framing", function(req)
            return {
                status = 200,
                headers = {
                    ["content-length"] = "9999",
                    ["transfer-encoding"] = "chunked",
                    ["connection"] = "custom-token",
                    ["keep-alive"] = "timeout=5",
                    ["proxy-connection"] = "keep-alive",
                    ["te"] = "trailers",
                    ["trailer"] = "X-Foo",
                    ["upgrade"] = "websocket",
                    ["x-safe"] = "preserved",
                },
                body = "framed"
            }
        end)
        log.info("ready")
        "#,
    )
    .await;

    wait_for_log(&mut owner, "hook-framing", "ready").await;

    let (status, headers, body) = server
        .send_http_full("GET", "/plugins/hook-framing/hooks/framing", &[], b"")
        .await;

    assert_eq!(status, 200);
    assert_eq!(std::str::from_utf8(&body).unwrap(), "framed");

    let custom_conn = headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("connection") && v.contains("custom-token"));
    assert!(!custom_conn, "custom connection token must be dropped");

    for forbidden in [
        "transfer-encoding",
        "keep-alive",
        "proxy-connection",
        "te",
        "trailer",
        "upgrade",
    ] {
        let present = headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case(forbidden));
        assert!(!present, "header {forbidden} must be dropped");
    }
    let cl = headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
        .map(|(_, v)| v.as_str());
    assert_ne!(cl, Some("9999"));

    let has_safe = headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("x-safe") && v == "preserved");
    assert!(has_safe, "safe header must be kept");
}

#[tokio::test]
async fn a_failing_hook_is_logged() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;

    let _p = plugin_with(
        &server,
        &mut owner,
        &ready,
        "hook-failing",
        &["receive_web_requests"],
        r#"
        hooks.on("fail", function(req)
            error("intentional_hook_crash", 0)
        end)
        log.info("ready")
        "#,
    )
    .await;

    wait_for_log(&mut owner, "hook-failing", "ready").await;

    let (status, _) = server
        .send_http("GET", "/plugins/hook-failing/hooks/fail", &[], b"")
        .await;

    assert_eq!(status, 500);

    wait_for_log(&mut owner, "hook-failing", "intentional_hook_crash").await;
}
