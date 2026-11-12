// SPDX-License-Identifier: MPL-2.0

//! Activities: what plugins register (plan M3 Task 3, design 2026-11-07
//! §15.1).

use std::time::Duration;

use opencord_proto::v1 as proto;
use proto::event::Kind as Event;
use proto::request::Kind;

use crate::common::{TestClient, TestServer, invite, media_token, owner};
use crate::support::{install, log_lines, manifest, manifest_with, package, session, wait_for_log};

/// What the desktop app lists in `Identify.features`.
const FEATURES: [&str; 3] = ["plugin_users", "plugin_channels", "activities"];
/// An app that shows plugin channels but not activities.
const WITHOUT_ACTIVITIES: [&str; 2] = ["plugin_users", "plugin_channels"];

/// Installs plugin `id` at `version` with `register_activities`, running
/// `source`, through `client` (an admin) whose Ready is `ready`.
async fn plugin_at(
    server: &TestServer,
    client: &mut TestClient,
    ready: &proto::Ready,
    id: &str,
    version: &str,
    source: &str,
) -> proto::Plugin {
    let zip = package(&[
        (
            "manifest.lua",
            &manifest_with(id, version, &["register_activities"], ""),
        ),
        ("server/main.luau", source),
        ("assets/paint.png", "not really a png"),
    ]);
    install(
        server,
        client,
        &media_token(ready),
        zip,
        &["register_activities"],
    )
    .await
}

/// Waits up to 5 s for an event `pick` takes.
async fn within<T>(client: &mut TestClient, pick: impl FnMut(&Event) -> Option<T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), client.wait_for(pick))
        .await
        .expect("the event never came")
}

/// The next list of activities `client` is sent.
async fn activities_update(client: &mut TestClient) -> Vec<proto::PluginActivity> {
    within(client, |event| match event {
        Event::PluginActivitiesUpdate(update) => Some(update.activities.clone()),
        _ => None,
    })
    .await
}

fn set_running(running: bool) -> Kind {
    Kind::SetPluginRunning(proto::SetPluginRunning {
        plugin_id: "paint".to_owned(),
        running,
    })
}

/// Creates a text channel: its `ChannelCreate` marks a point in a
/// session's events that every session reaches.
async fn marker(admin: &mut TestClient) -> i64 {
    match admin
        .ok(Kind::CreateChannel(proto::CreateChannel {
            kind: proto::ChannelKind::Text as i32,
            name: "marker".to_owned(),
            ..Default::default()
        }))
        .await
    {
        proto::response::Result::Channel(channel) => channel.id,
        other => panic!("expected a channel, got {other:?}"),
    }
}

/// Every event `client` was sent before the `ChannelCreate` of `marker`.
async fn events_until_created(client: &mut TestClient, marker: i64) -> Vec<Event> {
    let mut seen = Vec::new();
    loop {
        let event = client.next_event().await;
        if let Event::ChannelCreate(create) = &event
            && create.channel.as_ref().map(|channel| channel.id) == Some(marker)
        {
            return seen;
        }
        seen.push(event);
    }
}

#[tokio::test]
async fn a_registered_activity_reaches_apps_that_ask() {
    // The admin's app lists "activities" (hears the update, finds it in a
    // fresh Ready). The member's app doesn't (hears nothing, its fresh
    // Ready has none). Fails if the feature check is dropped from either,
    // or a field is lost on the way.
    let server = TestServer::start_with(|config| config.gateway.identify_per_minute = 20).await;
    let (mut owner_client, owner_ready) = owner(&server).await;
    let code = invite(&mut owner_client, None).await;
    let (mut admin, _) = session(&server, 1, None, &FEATURES).await;
    let (mut member, _) = session(&server, 2, Some(&code), &WITHOUT_ACTIVITIES).await;
    plugin_at(
        &server,
        &mut admin,
        &owner_ready,
        "paint",
        "1.0.0",
        r#"ui.register_activity("paint", {
            name = "Paint together", description = "A shared whiteboard",
            icon = "assets/paint.png",
        })"#,
    )
    .await;
    let expected = vec![proto::PluginActivity {
        plugin_id: "paint".to_owned(),
        activity_key: "paint".to_owned(),
        name: "Paint together".to_owned(),
        description: "A shared whiteboard".to_owned(),
        icon: "assets/paint.png".to_owned(),
        plugin_name: "Test plugin".to_owned(),
    }];

    assert_eq!(activities_update(&mut admin).await, expected);
    let (_later, new_ready) = session(&server, 3, Some(&code), &FEATURES).await;
    assert_eq!(new_ready.plugin_activities, expected);
    member
        .assert_no_event(Duration::from_millis(200), |event| {
            matches!(event, Event::PluginActivitiesUpdate(_))
        })
        .await;
    let (_member_again, member_ready) = session(&server, 2, None, &WITHOUT_ACTIVITIES).await;
    assert!(member_ready.plugin_activities.is_empty());
}

#[tokio::test]
async fn registering_needs_the_permission() {
    // Fails if a plugin without register_activities can register one.
    let server = TestServer::start().await;
    let (mut admin, ready) = owner(&server).await;
    let zip = package(&[
        ("manifest.lua", &manifest("bare", &[])),
        (
            "server/main.luau",
            r#"local ok, err = pcall(ui.register_activity, "paint", { name = "Paint" })
            log.info("denied: " .. tostring(err))"#,
        ),
    ]);
    install(&server, &mut admin, &media_token(&ready), zip, &[]).await;
    wait_for_log(
        &mut admin,
        "bare",
        "denied: PERMISSION_DENIED: the plugin doesn't have register_activities",
    )
    .await;
    let (_later, new_ready) = session(&server, 1, None, &FEATURES).await;
    assert!(new_ready.plugin_activities.is_empty());
}

#[tokio::test]
async fn a_bad_activity_is_refused() {
    // Fails if a malformed registration is taken or raises another text.
    let server = TestServer::start().await;
    let (mut admin, ready) = owner(&server).await;
    let source = r#"
        local function try(label, key, spec)
            local ok, err = pcall(ui.register_activity, key, spec)
            log.info(label .. ": " .. (ok and "ok" or tostring(err)))
        end
        try("capital key", "Paint", { name = "Paint" })
        try("spec", "paint", "Paint")
        try("empty name", "paint", { name = "" })
        try("outside icon", "paint", { name = "Paint", icon = "client/paint.png" })
        try("missing icon", "paint", { name = "Paint", icon = "assets/gone.png" })
        try("good", "paint", { name = "Paint", icon = "assets/paint.png" })
        log.info("done")
    "#;
    plugin_at(&server, &mut admin, &ready, "paint", "1.0.0", source).await;
    wait_for_log(&mut admin, "paint", "done").await;

    let lines = log_lines(&mut admin, "paint").await;
    for expected in [
        "capital key: INVALID_ARGUMENT: key must be 1 to 40 of a-z, 0-9 and _",
        "spec: INVALID_ARGUMENT: an activity is a table such as { name = \"Paint together\" }",
        "empty name: INVALID_ARGUMENT: name must be 1 to 60 characters, without control characters",
        "outside icon: INVALID_ARGUMENT: icon must be a .png, .jpg, .jpeg or .webp file under assets/",
        "missing icon: INVALID_ARGUMENT: icon assets/gone.png isn't in the plugin's package",
        "good: ok",
        "registered activity paint",
    ] {
        assert!(
            lines.iter().any(|line| line == expected),
            "{expected:?} in {lines:#?}"
        );
    }
    assert_eq!(
        lines
            .iter()
            .filter(|line| line.starts_with("registered activity"))
            .count(),
        1,
        "only the good one registered"
    );
}

#[tokio::test]
async fn the_21st_activity_is_too_many_but_a_replacement_is_not() {
    // Fails if a plugin can register more than 20 activities, or replacing
    // one of its 20 counts as a 21st.
    let server = TestServer::start().await;
    let (mut admin, ready) = owner(&server).await;
    let source = r#"
        for i = 1, 20 do
            ui.register_activity("game_" .. i, { name = "Game " .. i })
        end
        local ok, err = pcall(ui.register_activity, "game_21", { name = "One more" })
        log.info("21st: " .. tostring(err))
        ui.register_activity("game_1", { name = "Renamed" })
        log.info("replaced")
    "#;
    plugin_at(&server, &mut admin, &ready, "paint", "1.0.0", source).await;
    wait_for_log(&mut admin, "paint", "replaced").await;
    wait_for_log(
        &mut admin,
        "paint",
        "21st: PLUGIN_LIMIT: at most 20 activities",
    )
    .await;

    let (_later, new_ready) = session(&server, 1, None, &FEATURES).await;
    let activities = new_ready.plugin_activities;
    assert_eq!(activities.len(), 20);
    assert!(
        activities
            .iter()
            .any(|activity| activity.activity_key == "game_1" && activity.name == "Renamed")
    );
    assert!(
        !activities
            .iter()
            .any(|activity| activity.activity_key == "game_21")
    );
}

#[tokio::test]
async fn a_restart_that_registers_the_same_activities_changes_nothing() {
    // Fails if a restart clears the list while the new VM starts, sending
    // the list without the activity and then with it again.
    let server = TestServer::start().await;
    let (_owner, owner_ready) = owner(&server).await;
    let (mut admin, _) = session(&server, 1, None, &FEATURES).await;
    let source = r#"
        ui.register_activity("paint", { name = "Paint together" })
        log.info("registered")
    "#;
    plugin_at(&server, &mut admin, &owner_ready, "paint", "1.0.0", source).await;
    assert_eq!(activities_update(&mut admin).await.len(), 1);

    admin
        .ok(Kind::RestartPlugin(proto::RestartPlugin {
            plugin_id: "paint".to_owned(),
        }))
        .await;
    let marker = marker(&mut admin).await;

    let sent: Vec<Event> = events_until_created(&mut admin, marker)
        .await
        .into_iter()
        .filter(|event| matches!(event, Event::PluginActivitiesUpdate(_)))
        .collect();
    assert!(sent.is_empty(), "a restart sent {sent:?}");
    let lines = log_lines(&mut admin, "paint").await;
    assert_eq!(
        lines.iter().filter(|line| *line == "registered").count(),
        2,
        "it ran twice: {lines:#?}"
    );
}

#[tokio::test]
async fn stopping_takes_its_activities_away() {
    // Fails if a stopped plugin's activities stay listed, as an update or
    // in a fresh Ready.
    let server = TestServer::start_with(|config| config.gateway.identify_per_minute = 20).await;
    let (_owner, owner_ready) = owner(&server).await;
    let (mut admin, _) = session(&server, 1, None, &FEATURES).await;
    plugin_at(
        &server,
        &mut admin,
        &owner_ready,
        "paint",
        "1.0.0",
        r#"ui.register_activity("paint", { name = "Paint together" })"#,
    )
    .await;
    assert_eq!(activities_update(&mut admin).await.len(), 1);

    admin.ok(set_running(false)).await;
    assert!(activities_update(&mut admin).await.is_empty());
    let (_later, stopped_ready) = session(&server, 1, None, &FEATURES).await;
    assert!(stopped_ready.plugin_activities.is_empty());
}
