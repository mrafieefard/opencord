// SPDX-License-Identifier: MPL-2.0

//! Plugin timers tests (plan Task 20, design 2026-11-07 §10.3).

use opencord_proto::v1 as proto;

use crate::common::{TestClient, TestServer, media_token, owner};
use crate::support::{install, manifest, package, wait_for_log};

async fn plugin(
    server: &TestServer,
    owner: &mut TestClient,
    ready: &proto::Ready,
    id: &str,
    source: &str,
) -> proto::Plugin {
    let zip = package(&[
        ("manifest.lua", &manifest(id, &[])),
        ("server/main.luau", source),
    ]);
    install(server, owner, &media_token(ready), zip, &[]).await
}

#[tokio::test]
async fn after_fires_once() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;

    let _p = plugin(
        &server,
        &mut owner,
        &ready,
        "timers-after",
        r#"
        timer.after(0.05, function()
            log.info("after_fired")
        end)
        "#,
    )
    .await;

    wait_for_log(&mut owner, "timers-after", "after_fired").await;
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;

    let res = owner
        .ok(proto::request::Kind::FetchPluginLog(
            proto::FetchPluginLog {
                plugin_id: "timers-after".to_owned(),
                before: None,
                limit: 100,
            },
        ))
        .await;
    let proto::response::Result::PluginLog(log) = res else {
        panic!("expected PluginLog");
    };
    let fired_count = log
        .lines
        .iter()
        .filter(|l| l.text.contains("after_fired"))
        .count();
    assert_eq!(fired_count, 1, "after must fire exactly once");
}

#[tokio::test]
async fn every_repeats_until_cancelled() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;

    let _p = plugin(
        &server,
        &mut owner,
        &ready,
        "timers-every",
        r#"
        local count = 0
        local t
        t = timer.every(1.0, function()
            count = count + 1
            log.info("every_tick:" .. tostring(count))
            if count >= 2 then
                timer.cancel(t)
            end
        end)
        "#,
    )
    .await;

    wait_for_log(&mut owner, "timers-every", "every_tick:2").await;
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

    let res = owner
        .ok(proto::request::Kind::FetchPluginLog(
            proto::FetchPluginLog {
                plugin_id: "timers-every".to_owned(),
                before: None,
                limit: 100,
            },
        ))
        .await;
    let proto::response::Result::PluginLog(log) = res else {
        panic!("expected PluginLog");
    };
    let has_tick_3 = log.lines.iter().any(|l| l.text.contains("every_tick:3"));
    assert!(!has_tick_3, "timer should not tick after cancel");
}

#[tokio::test]
async fn bad_cron_is_refused() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;

    let _p = plugin(
        &server,
        &mut owner,
        &ready,
        "timers-bad-cron",
        r#"
        local ok, err = pcall(function()
            timer.cron("not a cron expression", function() end)
        end)
        if not ok then
            log.info("cron_rejected: " .. tostring(err))
        end

        local ok2, err2 = pcall(function()
            timer.cron("60 * * * *", function() end)
        end)
        if not ok2 then
            log.info("cron_out_of_bounds: " .. tostring(err2))
        end
        "#,
    )
    .await;

    wait_for_log(&mut owner, "timers-bad-cron", "cron_rejected:").await;
    wait_for_log(&mut owner, "timers-bad-cron", "cron_out_of_bounds:").await;

    let res = owner
        .ok(proto::request::Kind::FetchPluginLog(
            proto::FetchPluginLog {
                plugin_id: "timers-bad-cron".to_owned(),
                before: None,
                limit: 100,
            },
        ))
        .await;
    let proto::response::Result::PluginLog(log) = res else {
        panic!("expected PluginLog");
    };
    let line1 = log
        .lines
        .iter()
        .find(|l| l.text.contains("cron_rejected:"))
        .unwrap();
    assert!(
        line1.text.contains("INVALID_ARGUMENT"),
        "expected INVALID_ARGUMENT in: {}",
        line1.text
    );
    let line2 = log
        .lines
        .iter()
        .find(|l| l.text.contains("cron_out_of_bounds:"))
        .unwrap();
    assert!(
        line2.text.contains("INVALID_ARGUMENT"),
        "expected INVALID_ARGUMENT in: {}",
        line2.text
    );
}

#[tokio::test]
async fn timers_stop_with_the_plugin() {
    let server = TestServer::start().await;
    let (mut owner, ready) = owner(&server).await;

    let _p = plugin(
        &server,
        &mut owner,
        &ready,
        "timers-stop",
        r#"
        timer.after(0.4, function()
            log.info("should_never_fire")
        end)
        "#,
    )
    .await;

    // Stop plugin quickly before 0.4s
    owner
        .ok(proto::request::Kind::SetPluginRunning(
            proto::SetPluginRunning {
                plugin_id: "timers-stop".to_owned(),
                running: false,
            },
        ))
        .await;

    tokio::time::sleep(std::time::Duration::from_millis(600)).await;

    let res = owner
        .ok(proto::request::Kind::FetchPluginLog(
            proto::FetchPluginLog {
                plugin_id: "timers-stop".to_owned(),
                before: None,
                limit: 100,
            },
        ))
        .await;
    let proto::response::Result::PluginLog(log) = res else {
        panic!("expected PluginLog");
    };
    let fired = log
        .lines
        .iter()
        .any(|l| l.text.contains("should_never_fire"));
    assert!(!fired, "timer must not fire after plugin is stopped");
}

#[test]
fn cron_parses_and_finds_the_next_time() {
    use opencord_server::plugins::api::timers::{CronExpr, UtcDateTime};

    // Fixed base time: 2026-11-08 12:30:00 UTC (Sunday)
    let base = UtcDateTime {
        year: 2026,
        month: 11,
        day: 8,
        hour: 12,
        minute: 30,
        second: 0,
        weekday: 0, // Sunday
    };
    let base_secs = base.to_unix_secs();

    let cases = [
        ("* * * * *", 2026, 11, 8, 12, 31),
        ("*/5 * * * *", 2026, 11, 8, 12, 35),
        ("0 13 * * *", 2026, 11, 8, 13, 0),
        ("0 0 1 1 *", 2027, 1, 1, 0, 0),
        ("15,45 * * * *", 2026, 11, 8, 12, 45),
        ("0 0 * * 1", 2026, 11, 9, 0, 0),           // Next Monday
        ("30 14 1-15/2 11 *", 2026, 11, 9, 14, 30), // Day 9 (odd day)
    ];

    for (expr, y, m, d, h, min) in cases {
        let cron =
            CronExpr::parse(expr).unwrap_or_else(|e| panic!("failed to parse '{expr}': {e}"));
        let next_secs = cron.next_after(base_secs).expect("found next time");
        let dt = UtcDateTime::from_unix_secs(next_secs);
        assert_eq!(
            (dt.year, dt.month, dt.day, dt.hour, dt.minute),
            (y, m, d, h, min),
            "cron '{expr}' failed next time match"
        );
    }
}
