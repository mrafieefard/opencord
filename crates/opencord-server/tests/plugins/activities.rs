// SPDX-License-Identifier: MPL-2.0

//! Activities: what plugins register (plan M3 Task 3, design 2026-11-07
//! §15.1) and the sessions members start in calls (Task 4, §15.2, §15.4).

use std::time::{Duration, Instant};

use opencord_common::limits::VOICE_GRACE_PERIOD;
use opencord_common::permissions::Permissions;
use opencord_proto::v1 as proto;
use proto::event::Kind as Event;
use proto::request::Kind;
use proto::response::Result as Response;

use crate::common::{
    Received, TestClient, TestServer, channel_id, invite, media_token, owner, self_id,
};
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
    plugin_with(
        server,
        client,
        ready,
        id,
        version,
        &["register_activities"],
        source,
    )
    .await
}

/// [`plugin_at`], asking for and granted `permissions`.
async fn plugin_with(
    server: &TestServer,
    client: &mut TestClient,
    ready: &proto::Ready,
    id: &str,
    version: &str,
    permissions: &[&str],
    source: &str,
) -> proto::Plugin {
    let zip = package(&[
        (
            "manifest.lua",
            &manifest_with(id, version, permissions, r#"client = "client/main.luau","#),
        ),
        ("server/main.luau", source),
        (
            "client/main.luau",
            "return { init = function() return {} end }",
        ),
        ("assets/paint.png", "not really a png"),
    ]);
    install(server, client, &media_token(ready), zip, permissions).await
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

// ---------------------------------------------------------------------------
// Activity sessions (plan M3 Task 4).
// ---------------------------------------------------------------------------

/// The session tests' plugin: five activities, a log line for each start
/// and end, and "quiz" ended by the plugin as it starts.
const SESSIONS_SOURCE: &str = r#"
for _, key in ipairs({ "paint", "chess", "cards", "quiz", "draw" }) do
    ui.register_activity(key, { name = key })
end
opencord.on("activity_start", function(event)
    local session = event.session
    log.info(string.format("start %s %s by %s/%s in %s/%s at %s with %s",
        session.id, session.activity, event.starter.id, session.starter_id,
        event.call.channel_id, session.channel_id, type(session.started_at_ms),
        table.concat(event.call.member_ids, ",")))
    if session.activity == "quiz" then
        local first = activity.finish(session)
        local again = activity.finish(session.id)
        log.info("plugin ended " .. tostring(first) .. " then " .. tostring(again))
    end
end)
opencord.on("activity_end", function(event)
    log.info("end " .. event.session.id .. " " .. event.session.activity .. " " .. event.reason)
end)
log.info("ready")
"#;

/// A server running [`SESSIONS_SOURCE`] as "paint", installed by the owner.
struct World {
    server: TestServer,
    /// The owner: installs, stops and restarts the plugin, and manages
    /// channels. Its app lists no features, so it hears of no session.
    owner: TestClient,
    owner_ready: proto::Ready,
    code: String,
    /// The voice channel "General".
    voice: i64,
}

impl World {
    async fn new() -> Self {
        Self::running(SESSIONS_SOURCE).await
    }

    /// A server running `source` as "paint"; `source` logs "ready".
    async fn running(source: &str) -> Self {
        Self::running_for(source, 100).await
    }

    /// [`World::running`], each handler given `handler_ms` to run.
    async fn running_for(source: &str, handler_ms: u32) -> Self {
        let server = TestServer::start_with(|config| {
            config.gateway.identify_per_minute = 60;
            config.plugins.handler_ms = handler_ms;
        })
        .await;
        let (mut owner, owner_ready) = owner(&server).await;
        let code = invite(&mut owner, None).await;
        plugin_at(&server, &mut owner, &owner_ready, "paint", "1.0.0", source).await;
        wait_for_log(&mut owner, "paint", "ready").await;
        let voice = channel_id(&owner_ready, "General");
        Self {
            server,
            owner,
            owner_ready,
            code,
            voice,
        }
    }

    /// A member's session for key `seed`, listing `features`: the client,
    /// the member's id and the Ready.
    async fn member(&self, seed: u8, features: &[&str]) -> (TestClient, i64, proto::Ready) {
        let (client, ready) = session(&self.server, seed, Some(&self.code), features).await;
        let id = self_id(&ready);
        (client, id, ready)
    }

    /// Denies `user_id` `denied` in `channel_id`, by a member overwrite.
    async fn deny(&mut self, channel_id: i64, user_id: i64, denied: Permissions) {
        self.overwrite(channel_id, user_id, Permissions::empty(), denied)
            .await;
    }

    /// Sets `user_id`'s member overwrite in `channel_id` to `allowed` and
    /// `denied`, replacing the one before.
    async fn overwrite(
        &mut self,
        channel_id: i64,
        user_id: i64,
        allowed: Permissions,
        denied: Permissions,
    ) {
        self.owner
            .ok(Kind::SetChannelOverwrite(proto::SetChannelOverwrite {
                channel_id,
                overwrite: Some(proto::PermissionOverwrite {
                    target_kind: proto::OverwriteTarget::Member as i32,
                    target_id: user_id,
                    allow: allowed.bits(),
                    deny: denied.bits(),
                }),
            }))
            .await;
    }

    /// How many lines of the plugin's log start with `prefix`.
    async fn log_count(&mut self, prefix: &str) -> usize {
        log_lines(&mut self.owner, "paint")
            .await
            .iter()
            .filter(|line| line.starts_with(prefix))
            .count()
    }

    /// Waits for a line starting with `prefix`, lets late handlers run,
    /// and returns how many there are.
    async fn settled_count(&mut self, prefix: &str) -> usize {
        wait_for_log(&mut self.owner, "paint", prefix).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        self.log_count(prefix).await
    }
}

fn join(channel_id: i64) -> Kind {
    Kind::UpdateVoiceState(proto::UpdateVoiceState {
        channel_id: Some(channel_id),
        ..Default::default()
    })
}

fn leave() -> Kind {
    Kind::UpdateVoiceState(proto::UpdateVoiceState::default())
}

fn start_in(channel_id: i64, plugin_id: &str, key: &str) -> Kind {
    Kind::StartActivity(proto::StartActivity {
        channel_id,
        plugin_id: plugin_id.to_owned(),
        activity_key: key.to_owned(),
    })
}

fn start(channel_id: i64, key: &str) -> Kind {
    start_in(channel_id, "paint", key)
}

fn end(session_id: i64) -> Kind {
    Kind::EndActivity(proto::EndActivity { session_id })
}

/// Sends `kind`, waiting out the request rate limit: the answer.
async fn paced(client: &mut TestClient, kind: Kind) -> Response {
    loop {
        match client.request(kind.clone()).await {
            Response::Error(error) if error.code == proto::ErrorCode::RateLimited as i32 => {
                let wait = error.retry_after_ms.unwrap_or(100);
                tokio::time::sleep(Duration::from_millis(wait)).await;
            }
            answer => return answer,
        }
    }
}

/// Starts `key` in `channel_id`: the session.
async fn started(client: &mut TestClient, channel_id: i64, key: &str) -> proto::ActivitySession {
    match paced(client, start(channel_id, key)).await {
        Response::ActivitySession(session) => session,
        other => panic!("not started: {other:?}"),
    }
}

/// The error code `kind` is refused with.
async fn refused(client: &mut TestClient, kind: Kind) -> proto::ErrorCode {
    match paced(client, kind).await {
        Response::Error(error) => proto::ErrorCode::try_from(error.code).unwrap(),
        other => panic!("not refused: {other:?}"),
    }
}

/// The next session `client` hears created.
async fn created(client: &mut TestClient) -> proto::ActivitySession {
    within(client, |event| match event {
        Event::ActivitySessionCreate(create) => create.session.clone(),
        _ => None,
    })
    .await
}

/// The delete of session `session_id` that `client` hears.
async fn deleted(client: &mut TestClient, session_id: i64) -> proto::ActivitySessionDelete {
    within(client, |event| match event {
        Event::ActivitySessionDelete(delete) if delete.session_id == session_id => Some(*delete),
        _ => None,
    })
    .await
}

fn is_session_event(event: &Event) -> bool {
    matches!(
        event,
        Event::ActivitySessionCreate(_)
            | Event::ActivitySessionUpdate(_)
            | Event::ActivitySessionDelete(_)
    )
}

/// Asserts `client` hears nothing of any session for a while.
async fn hears_no_session(client: &mut TestClient) {
    client
        .assert_no_event(Duration::from_millis(300), is_session_event)
        .await;
}

fn reason(delete: &proto::ActivitySessionDelete) -> proto::ActivityEndReason {
    proto::ActivityEndReason::try_from(delete.reason).unwrap()
}

#[tokio::test]
async fn starting_needs_the_call_and_the_permission() {
    // A is in the call with the permission (the starter's road). B is in
    // the call, denied START_ACTIVITIES by an overwrite there (FORBIDDEN).
    // C can see the channel but isn't in the call (INVALID_ARGUMENT). D is
    // in the call, its app without "activities" (NOT_FOUND).
    let mut world = World::new().await;
    let (mut a, _, _) = world.member(2, &FEATURES).await;
    let (mut b, b_id, _) = world.member(3, &FEATURES).await;
    let (mut c, _, _) = world.member(4, &FEATURES).await;
    let (mut d, _, _) = world.member(5, &WITHOUT_ACTIVITIES).await;
    let voice = world.voice;
    world.deny(voice, b_id, Permissions::START_ACTIVITIES).await;
    for client in [&mut a, &mut b, &mut d] {
        client.ok(join(voice)).await;
    }

    let session = started(&mut a, voice, "paint").await;
    assert_eq!(session.channel_id, voice);
    assert_eq!(session.activity_key, "paint");
    assert_eq!(
        refused(&mut b, start(voice, "paint")).await,
        proto::ErrorCode::Forbidden
    );
    assert_eq!(
        refused(&mut c, start(voice, "paint")).await,
        proto::ErrorCode::InvalidArgument
    );
    assert_eq!(
        refused(&mut d, start(voice, "paint")).await,
        proto::ErrorCode::NotFound
    );
}

#[tokio::test]
async fn a_started_activity_reaches_who_can_see_the_channel() {
    // A starts it (the starter's road, and a call member). B is in the
    // call (hears the create). C sees the channel and isn't in the call
    // (hears the create). D can't see the channel (hears nothing). E sees
    // the channel, its app without "activities" (hears nothing).
    let mut world = World::new().await;
    let (mut a, a_id, _) = world.member(2, &FEATURES).await;
    let (mut b, b_id, _) = world.member(3, &FEATURES).await;
    let (mut c, _, _) = world.member(4, &FEATURES).await;
    let (mut d, d_id, _) = world.member(5, &FEATURES).await;
    let (mut e, _, _) = world.member(6, &WITHOUT_ACTIVITIES).await;
    let voice = world.voice;
    world.deny(voice, d_id, Permissions::VIEW_CHANNEL).await;
    a.ok(join(voice)).await;
    b.ok(join(voice)).await;

    let session = started(&mut a, voice, "paint").await;
    assert_eq!(session.starter_id, a_id);
    assert_eq!(session.plugin_id, "paint");
    assert_eq!(session.name, "paint");
    assert_eq!(session.plugin_name, "Test plugin");
    assert!(session.started_at_ms > 0);
    assert!(session.viewer_ids.is_empty());
    assert_eq!(created(&mut b).await, session);
    assert_eq!(created(&mut c).await, session);
    hears_no_session(&mut d).await;
    hears_no_session(&mut e).await;

    let mut members = [a_id, b_id];
    members.sort_unstable();
    let expected = format!(
        "start {} paint by {a_id}/{a_id} in {voice}/{voice} at number with {},{}",
        session.id, members[0], members[1]
    );
    wait_for_log(&mut world.owner, "paint", &expected).await;
}

/// Sends `kind` as request `request_id` without waiting for the answer.
async fn send_request(client: &mut TestClient, request_id: u64, kind: Kind) {
    client
        .send_envelope(proto::Envelope {
            seq: 0,
            request_id,
            payload: Some(proto::envelope::Payload::Request(proto::Request {
                kind: Some(kind),
            })),
        })
        .await;
}

/// The answer to request `request_id`; events meanwhile are skipped.
async fn answer(client: &mut TestClient, request_id: u64) -> Response {
    loop {
        match client.recv().await {
            Received::Envelope(envelope) => match envelope.payload {
                Some(proto::envelope::Payload::Response(response))
                    if envelope.request_id == request_id =>
                {
                    return response.result.expect("a result");
                }
                _ => {}
            },
            Received::Closed(code) => panic!("closed ({code:?})"),
        }
    }
}

#[tokio::test]
async fn starting_twice_gives_the_running_session() {
    // A and B are in the call and press start at once (both get the one
    // session). C sees the channel, not in the call (hears one create).
    let mut world = World::new().await;
    let (mut a, _, _) = world.member(2, &FEATURES).await;
    let (mut b, _, _) = world.member(3, &FEATURES).await;
    let (mut c, _, _) = world.member(4, &FEATURES).await;
    let voice = world.voice;
    a.ok(join(voice)).await;
    b.ok(join(voice)).await;

    send_request(&mut a, 900_001, start(voice, "paint")).await;
    send_request(&mut b, 900_002, start(voice, "paint")).await;
    let Response::ActivitySession(first) = answer(&mut a, 900_001).await else {
        panic!("A's start failed");
    };
    let Response::ActivitySession(second) = answer(&mut b, 900_002).await else {
        panic!("B's start failed");
    };
    assert_eq!(first.id, second.id);

    let creates = c
        .collect_for(Duration::from_millis(500))
        .await
        .into_iter()
        .filter(|event| matches!(event, Event::ActivitySessionCreate(_)))
        .count();
    assert_eq!(creates, 1);
    assert_eq!(world.settled_count(&format!("start {}", first.id)).await, 1);
}

#[tokio::test]
async fn a_fifth_activity_is_refused() {
    // A is in General and starts four, then a fifth (refused). B is in
    // another call and starts one there (fine: the limit is per call).
    let mut world = World::new().await;
    let (mut a, _, _) = world.member(2, &FEATURES).await;
    let (mut b, _, _) = world.member(3, &FEATURES).await;
    let voice = world.voice;
    let other = match world
        .owner
        .ok(Kind::CreateChannel(proto::CreateChannel {
            kind: proto::ChannelKind::Voice as i32,
            name: "Second".to_owned(),
            ..Default::default()
        }))
        .await
    {
        Response::Channel(channel) => channel.id,
        other => panic!("expected a channel, got {other:?}"),
    };
    a.ok(join(voice)).await;
    b.ok(join(other)).await;

    for key in ["paint", "chess", "cards", "draw"] {
        started(&mut a, voice, key).await;
    }
    let error = match paced(&mut a, start(voice, "quiz")).await {
        Response::Error(error) => error,
        other => panic!("a fifth started: {other:?}"),
    };
    assert_eq!(error.code, proto::ErrorCode::PluginLimit as i32);
    assert_eq!(error.message, "this call already has 4 activities");
    started(&mut b, other, "paint").await;
}

#[tokio::test]
async fn an_unknown_or_stopped_activity_is_unavailable() {
    // A is in the call with the permission throughout: an unknown key, an
    // unknown plugin, then the plugin stopped by the owner.
    let mut world = World::new().await;
    let (mut a, _, _) = world.member(2, &FEATURES).await;
    let voice = world.voice;
    a.ok(join(voice)).await;

    assert_eq!(
        refused(&mut a, start(voice, "golf")).await,
        proto::ErrorCode::PluginUnavailable
    );
    assert_eq!(
        refused(&mut a, start_in(voice, "nope", "paint")).await,
        proto::ErrorCode::PluginUnavailable
    );
    world.owner.ok(set_running(false)).await;
    assert_eq!(
        refused(&mut a, start(voice, "paint")).await,
        proto::ErrorCode::PluginUnavailable
    );
}

#[tokio::test]
async fn only_the_starter_or_a_channel_manager_ends_it() {
    // A starts both (the starter's road: ends "paint"). B is in the call,
    // neither starter nor manager (FORBIDDEN, and hears each delete). The
    // owner manages channels and didn't start "chess" (ends it).
    let mut world = World::new().await;
    let (mut a, _, _) = world.member(2, &FEATURES).await;
    let (mut b, _, _) = world.member(3, &FEATURES).await;
    let voice = world.voice;
    a.ok(join(voice)).await;
    b.ok(join(voice)).await;
    let paint = started(&mut a, voice, "paint").await;
    let chess = started(&mut a, voice, "chess").await;

    assert_eq!(
        refused(&mut b, end(paint.id)).await,
        proto::ErrorCode::Forbidden
    );
    assert_eq!(
        refused(&mut b, end(paint.id + chess.id)).await,
        proto::ErrorCode::NotFound
    );
    paced(&mut a, end(paint.id)).await;
    assert_eq!(
        reason(&deleted(&mut b, paint.id).await),
        proto::ActivityEndReason::Ended
    );
    let Response::Ack(_) = paced(&mut world.owner, end(chess.id)).await else {
        panic!("the owner couldn't end it");
    };
    assert_eq!(
        reason(&deleted(&mut b, chess.id).await),
        proto::ActivityEndReason::Ended
    );
    hears_no_session(&mut b).await;
    for session in [&paint, &chess] {
        let line = format!("end {} {} ended", session.id, session.activity_key);
        assert_eq!(world.settled_count(&line).await, 1, "{line}");
    }
}

/// [`SESSIONS_SOURCE`], and a `message_create` handler that holds the VM
/// (its thread, so no event is taken) inside `config.get`, a synchronous
/// call that blocks on the plugin's config lock while the test holds its
/// write side ([`opencord_server::plugins::PluginHandle::config_lock_for_tests`]).
/// Nothing between "holding" and `config.get` yields, so once the test
/// reads "holding" the VM takes no event until the test lets go. The
/// thread sleeps on the lock rather than spinning, and the test, not a
/// look at the plugin's views, decides when it goes.
const HELD_SOURCE: &str = r#"
opencord.on("message_create", function()
    log.info("holding")
    config.get("held")
    log.info("let go")
end)
"#;

#[tokio::test]
#[expect(
    clippy::await_holding_lock,
    reason = "the write side is held on purpose: it keeps the plugin's VM thread blocked"
)]
async fn a_session_ended_before_the_vm_takes_its_start_starts_then_ends() {
    // Catches (3623716): the VM looking the session up when it takes
    // `activity_start`. The session has ended by then, so the start is
    // skipped and the `activity_end` behind it dropped: the log shows
    // neither. The VM is held inside a handler while A starts the session,
    // opens its view and ends it; the test lets it go once the end is
    // acknowledged, so the session has ended before the VM takes its start.
    let source = format!("{SESSIONS_SOURCE}{HELD_SOURCE}");
    let mut world = World::running_for(&source, 5_000).await;
    let (mut a, a_id, _) = world.member(2, &FEATURES).await;
    let voice = world.voice;
    a.ok(join(voice)).await;

    let config = world
        .server
        .state()
        .plugins
        .get("paint")
        .expect("paint is installed")
        .config_lock_for_tests();
    let hold = config
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    world.server.state().plugins.deliver(
        "paint",
        opencord_server::plugins::PluginEvent {
            name: "message_create".to_owned(),
            payload: serde_json::json!({}),
            reply: None,
            command: None,
        },
    );
    wait_for_log(&mut world.owner, "paint", "holding").await;
    let paint = started(&mut a, voice, "paint").await;
    open_view(&mut a, paint.id).await;
    let Response::Ack(_) = paced(&mut a, end(paint.id)).await else {
        panic!("A couldn't end it");
    };
    drop(hold);

    let lines = lines_after(&mut world, &format!("end {}", paint.id)).await;
    let let_go = lines.iter().position(|line| line == "let go");
    let start_at = lines
        .iter()
        .position(|line| line.starts_with(&format!("start {} ", paint.id)));
    assert!(let_go.is_some(), "the handler never let go: {lines:?}");
    assert!(
        start_at.is_none_or(|start_at| let_go < Some(start_at)),
        "the VM took the start before the end, so the race wasn't made: {lines:?}"
    );
    let start = format!(
        "start {} paint by {a_id}/{a_id} in {voice}/{voice} at number with {a_id}",
        paint.id
    );
    let end_line = format!("end {} paint ended", paint.id);
    let ours: Vec<&String> = lines
        .iter()
        .filter(|line| line.starts_with(&format!("start {} ", paint.id)) || **line == end_line)
        .collect();
    assert_eq!(ours, [&start, &end_line], "{lines:?}");
}

#[tokio::test]
async fn the_plugin_ends_its_own_session() {
    // A starts "quiz", which the plugin ends as it starts (true, then
    // false). B is in the call and hears the delete.
    let mut world = World::new().await;
    let (mut a, _, _) = world.member(2, &FEATURES).await;
    let (mut b, _, _) = world.member(3, &FEATURES).await;
    let voice = world.voice;
    a.ok(join(voice)).await;
    b.ok(join(voice)).await;

    let quiz = started(&mut a, voice, "quiz").await;
    assert_eq!(
        reason(&deleted(&mut b, quiz.id).await),
        proto::ActivityEndReason::Plugin
    );
    wait_for_log(&mut world.owner, "paint", "plugin ended true then false").await;
    wait_for_log(
        &mut world.owner,
        "paint",
        &format!("ended activity session {}", quiz.id),
    )
    .await;
    assert_eq!(
        world
            .settled_count(&format!("end {} quiz plugin", quiz.id))
            .await,
        1
    );
}

#[tokio::test]
async fn a_plugin_cant_end_anothers_session() {
    // A starts "paint" and posts its id in #general. The plugin "other"
    // reads it and tries to end it (false). B is in the call and hears no
    // delete.
    let mut world = World::new().await;
    let (mut a, _, _) = world.member(2, &FEATURES).await;
    let (mut b, _, _) = world.member(3, &FEATURES).await;
    let voice = world.voice;
    let ready = world.owner_ready.clone();
    plugin_with(
        &world.server,
        &mut world.owner,
        &ready,
        "other",
        "1.0.0",
        &["register_activities", "read_messages"],
        r#"opencord.on("message_create", function(event)
            log.info("other ended " .. tostring(activity.finish(event.message.content)))
        end)"#,
    )
    .await;
    a.ok(join(voice)).await;
    b.ok(join(voice)).await;
    let paint = started(&mut a, voice, "paint").await;

    a.ok(Kind::SendMessage(proto::SendMessage {
        channel_id: channel_id(&ready, "general"),
        content: paint.id.to_string(),
        ..Default::default()
    }))
    .await;
    wait_for_log(&mut world.owner, "other", "other ended false").await;
    assert_eq!(created(&mut b).await.id, paint.id);
    hears_no_session(&mut b).await;
    assert_eq!(started(&mut a, voice, "paint").await.id, paint.id);
}

#[tokio::test]
async fn the_call_emptying_ends_its_sessions() {
    // A starts it and leaves first (the session stays). B is the last
    // to leave (the session ends). C sees the channel, not in the call
    // (hears the one delete).
    let mut world = World::new().await;
    let (mut a, _, _) = world.member(2, &FEATURES).await;
    let (mut b, _, _) = world.member(3, &FEATURES).await;
    let (mut c, _, _) = world.member(4, &FEATURES).await;
    let voice = world.voice;
    a.ok(join(voice)).await;
    b.ok(join(voice)).await;
    let paint = started(&mut a, voice, "paint").await;
    created(&mut c).await;

    a.ok(leave()).await;
    hears_no_session(&mut c).await;
    b.ok(leave()).await;
    assert_eq!(
        reason(&deleted(&mut c, paint.id).await),
        proto::ActivityEndReason::CallEmpty
    );
    hears_no_session(&mut c).await;
    assert_eq!(
        world
            .settled_count(&format!("end {} paint call_empty", paint.id))
            .await,
        1
    );
}

/// Runs the voice reaper as of `at`, as the server's reaper does.
async fn expire_voice(server: &TestServer, at: Instant) {
    let state = server.state();
    let _writes = state.write_lock().await;
    opencord_server::voice::expire_detached(state, at);
}

#[tokio::test]
async fn a_dropped_last_member_ends_the_session_after_the_grace() {
    // A is alone in the call, starts it and drops (the session waits out
    // the grace, then ends). C sees the channel, not in the call (hears
    // the delete only after the grace).
    let world = World::new().await;
    let (mut a, _, a_ready) = world.member(2, &FEATURES).await;
    let (mut c, _, _) = world.member(4, &FEATURES).await;
    let voice = world.voice;
    a.ok(join(voice)).await;
    let paint = started(&mut a, voice, "paint").await;
    created(&mut c).await;

    a.close().await;
    let state = world.server.state();
    let deadline = Instant::now() + Duration::from_secs(5);
    while state
        .sessions
        .get(&a_ready.session_id)
        .and_then(|session| session.detached_for(Instant::now()))
        .is_none()
    {
        assert!(Instant::now() < deadline, "A's session never detached");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    expire_voice(
        &world.server,
        Instant::now() + VOICE_GRACE_PERIOD - Duration::from_secs(1),
    )
    .await;
    hears_no_session(&mut c).await;
    expire_voice(
        &world.server,
        Instant::now() + VOICE_GRACE_PERIOD + Duration::from_secs(1),
    )
    .await;
    assert_eq!(
        reason(&deleted(&mut c, paint.id).await),
        proto::ActivityEndReason::CallEmpty
    );
}

#[tokio::test]
async fn deleting_the_channel_ends_its_sessions() {
    // A is in the call of "Temp" and starts it; the owner deletes "Temp".
    // C could see "Temp", not in the call (hears the session go).
    let mut world = World::new().await;
    let (mut a, _, _) = world.member(2, &FEATURES).await;
    let (mut c, _, _) = world.member(4, &FEATURES).await;
    let temp = match world
        .owner
        .ok(Kind::CreateChannel(proto::CreateChannel {
            kind: proto::ChannelKind::Voice as i32,
            name: "Temp".to_owned(),
            ..Default::default()
        }))
        .await
    {
        Response::Channel(channel) => channel.id,
        other => panic!("expected a channel, got {other:?}"),
    };
    a.ok(join(temp)).await;
    let paint = started(&mut a, temp, "paint").await;
    assert_eq!(created(&mut c).await.id, paint.id);

    world
        .owner
        .ok(Kind::DeleteChannel(proto::DeleteChannel {
            channel_id: temp,
        }))
        .await;
    deleted(&mut c, paint.id).await;
    hears_no_session(&mut c).await;
    assert_eq!(
        world
            .settled_count(&format!("end {} paint call_empty", paint.id))
            .await,
        1
    );
}

#[tokio::test]
async fn stopping_ends_sessions() {
    // A starts it; the owner stops the plugin. B is in the call and hears
    // the delete; the plugin's stop grace runs its activity_end.
    let mut world = World::new().await;
    let (mut a, _, _) = world.member(2, &FEATURES).await;
    let (mut b, _, _) = world.member(3, &FEATURES).await;
    let voice = world.voice;
    a.ok(join(voice)).await;
    b.ok(join(voice)).await;
    let paint = started(&mut a, voice, "paint").await;
    wait_for_log(&mut world.owner, "paint", &format!("start {}", paint.id)).await;

    world.owner.ok(set_running(false)).await;
    assert_eq!(
        reason(&deleted(&mut b, paint.id).await),
        proto::ActivityEndReason::PluginStopped
    );
    assert_eq!(
        world
            .settled_count(&format!("end {} paint plugin_stopping", paint.id))
            .await,
        1
    );
}

#[tokio::test]
async fn sessions_survive_a_restart_and_the_new_vm_hears_them() {
    // A starts it; the owner restarts the plugin. B is in the call and
    // hears no delete; the new VM hears the start once.
    let mut world = World::new().await;
    let (mut a, _, _) = world.member(2, &FEATURES).await;
    let (mut b, _, _) = world.member(3, &FEATURES).await;
    let voice = world.voice;
    a.ok(join(voice)).await;
    b.ok(join(voice)).await;
    let paint = started(&mut a, voice, "paint").await;
    created(&mut b).await;
    let line = format!("start {}", paint.id);
    wait_for_log(&mut world.owner, "paint", &line).await;

    world
        .owner
        .ok(Kind::RestartPlugin(proto::RestartPlugin {
            plugin_id: "paint".to_owned(),
        }))
        .await;
    let deadline = Instant::now() + Duration::from_secs(5);
    while world.log_count(&line).await < 2 {
        assert!(
            Instant::now() < deadline,
            "the new VM never heard the start"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(world.log_count(&line).await, 2);
    assert_eq!(world.log_count(&format!("end {}", paint.id)).await, 0);
    hears_no_session(&mut b).await;
    assert_eq!(started(&mut a, voice, "paint").await.id, paint.id);
}

#[tokio::test]
async fn a_restart_without_the_activity_ends_its_sessions() {
    // A starts "paint"; the owner updates the plugin to a version that
    // registers only "chess". B is in the call and hears the delete.
    let mut world = World::new().await;
    let (mut a, _, _) = world.member(2, &FEATURES).await;
    let (mut b, _, _) = world.member(3, &FEATURES).await;
    let voice = world.voice;
    a.ok(join(voice)).await;
    b.ok(join(voice)).await;
    let paint = started(&mut a, voice, "paint").await;
    let chess = started(&mut a, voice, "chess").await;

    let ready = world.owner_ready.clone();
    plugin_at(
        &world.server,
        &mut world.owner,
        &ready,
        "paint",
        "1.0.1",
        r#"ui.register_activity("chess", { name = "chess" })"#,
    )
    .await;
    assert_eq!(
        reason(&deleted(&mut b, paint.id).await),
        proto::ActivityEndReason::Unavailable
    );
    hears_no_session(&mut b).await;
    assert_eq!(started(&mut a, voice, "chess").await.id, chess.id);
}

#[tokio::test]
async fn sessions_are_in_ready_only_for_apps_that_ask() {
    // A starts it from the call. C sees the channel (its fresh Ready has
    // it). E sees the channel, its app without "activities" (none). D
    // can't see the channel (none).
    let mut world = World::new().await;
    let (mut a, _, _) = world.member(2, &FEATURES).await;
    let (_d, d_id, _) = world.member(5, &FEATURES).await;
    let voice = world.voice;
    world.deny(voice, d_id, Permissions::VIEW_CHANNEL).await;
    a.ok(join(voice)).await;
    let paint = started(&mut a, voice, "paint").await;

    let (_c, _, c_ready) = world.member(4, &FEATURES).await;
    assert_eq!(c_ready.activity_sessions, vec![paint]);
    let (_e, _, e_ready) = world.member(6, &WITHOUT_ACTIVITIES).await;
    assert!(e_ready.activity_sessions.is_empty());
    let (_d_again, d_ready) = session(&world.server, 5, None, &FEATURES).await;
    assert!(d_ready.activity_sessions.is_empty());
}

// ---------------------------------------------------------------------------
// Views in an activity session (plan M3 Task 5).
// ---------------------------------------------------------------------------

/// The view tests' plugin: "paint" registered, a log line for each view's
/// open (with its place) and close, and for each session's end; "reach"
/// sends to a place and lists its views.
const VIEWS_SOURCE: &str = r#"
ui.register_activity("paint", { name = "paint" })
opencord.on("view_open", function(event)
    local view = event.view
    local place = view.place
    log.info(string.format("open %s by %s %s %s %s %s", view.id, view.user.id,
        tostring(place.kind), tostring(place.session_id), tostring(place.channel_id),
        tostring(place.activity)))
end)
opencord.on("view_close", function(event)
    log.info("close " .. event.view.id .. " " .. event.reason)
end)
opencord.on("activity_end", function(event)
    log.info("end " .. event.session.id .. " " .. event.reason)
end)
ui.on("reach", function(view, data)
    local here = ui.send_place({ session_id = data.session_id }, "here", {})
    local voice = ui.send_place({ channel_id = data.channel_id }, "voice", {})
    local ids = {}
    for _, listed in ipairs(ui.views({ session_id = data.session_id })) do
        table.insert(ids, listed.id)
    end
    local _, err = pcall(ui.views, {})
    log.info(string.format("reach %d %d [%s] %s", here, voice, table.concat(ids, ","),
        tostring(err)))
end)
log.info("ready")
"#;

fn open_in(session_id: i64) -> Kind {
    Kind::OpenView(proto::OpenView {
        place: Some(proto::ViewPlace {
            kind: Some(proto::view_place::Kind::ActivitySessionId(session_id)),
        }),
        platform: "desktop".to_owned(),
    })
}

/// Opens the activity session's view: the view's id.
async fn open_view(client: &mut TestClient, session_id: i64) -> i64 {
    match paced(client, open_in(session_id)).await {
        Response::ViewOpened(opened) => opened.view_id,
        other => panic!("not opened: {other:?}"),
    }
}

fn close_view(view_id: i64) -> Kind {
    Kind::CloseView(proto::CloseView { view_id })
}

/// The reason of the `ViewClosed` of `view_id` that `client` hears.
async fn view_closed(client: &mut TestClient, view_id: i64) -> proto::ViewCloseReason {
    within(client, |event| match event {
        Event::ViewClosed(closed) if closed.view_id == view_id => {
            Some(proto::ViewCloseReason::try_from(closed.reason).unwrap())
        }
        _ => None,
    })
    .await
}

/// The viewers of the next `ActivitySessionUpdate` `client` hears.
async fn viewers_update(client: &mut TestClient) -> Vec<i64> {
    within(client, |event| match event {
        Event::ActivitySessionUpdate(update) => {
            update.session.as_ref().map(|s| s.viewer_ids.clone())
        }
        _ => None,
    })
    .await
}

async fn hears_no_update(client: &mut TestClient) {
    client
        .assert_no_event(Duration::from_millis(300), |event| {
            matches!(event, Event::ActivitySessionUpdate(_))
        })
        .await;
}

/// The plugin's log lines, waiting for one starting with `prefix`.
async fn lines_after(world: &mut World, prefix: &str) -> Vec<String> {
    wait_for_log(&mut world.owner, "paint", prefix).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    log_lines(&mut world.owner, "paint").await
}

fn sorted(mut ids: Vec<i64>) -> Vec<i64> {
    ids.sort_unstable();
    ids
}

#[tokio::test]
async fn only_someone_in_the_call_opens_it() {
    // A is in the call and starts and opens it (the opener's road, and a
    // made-up session id: NOT_FOUND). B sees the channel, isn't in the
    // call (FORBIDDEN). C is in the call, its app without "activities"
    // (NOT_FOUND).
    let world = World::running(VIEWS_SOURCE).await;
    let (mut a, _, _) = world.member(2, &FEATURES).await;
    let (mut b, _, _) = world.member(3, &FEATURES).await;
    let (mut c, _, _) = world.member(4, &WITHOUT_ACTIVITIES).await;
    let voice = world.voice;
    a.ok(join(voice)).await;
    c.ok(join(voice)).await;
    let paint = started(&mut a, voice, "paint").await;

    open_view(&mut a, paint.id).await;
    assert_eq!(
        refused(&mut b, open_in(paint.id)).await,
        proto::ErrorCode::Forbidden
    );
    assert_eq!(
        refused(&mut c, open_in(paint.id)).await,
        proto::ErrorCode::NotFound
    );
    assert_eq!(
        refused(&mut a, open_in(paint.id + 1)).await,
        proto::ErrorCode::NotFound
    );
}

#[tokio::test]
async fn opening_tells_the_plugin_where() {
    // A is in the call, starts it and opens it: the plugin's view_open has
    // the activity's place.
    let mut world = World::running(VIEWS_SOURCE).await;
    let (mut a, a_id, _) = world.member(2, &FEATURES).await;
    let voice = world.voice;
    a.ok(join(voice)).await;
    let paint = started(&mut a, voice, "paint").await;

    let view = open_view(&mut a, paint.id).await;
    wait_for_log(
        &mut world.owner,
        "paint",
        &format!("open {view} by {a_id} activity {} {voice} paint", paint.id),
    )
    .await;
}

#[tokio::test]
async fn viewers_are_counted_by_person() {
    // A is in the call and opens it from two apps (one viewer). B is in
    // the call and opens it (a second viewer). C sees the channel, isn't
    // in the call, and hears each update. D joins later (its Ready has the
    // viewers).
    let world = World::running(VIEWS_SOURCE).await;
    let (mut a, a_id, _) = world.member(2, &FEATURES).await;
    let (mut a_again, _) = session(&world.server, 2, None, &FEATURES).await;
    let (mut b, b_id, _) = world.member(3, &FEATURES).await;
    let (mut c, _, _) = world.member(4, &FEATURES).await;
    let voice = world.voice;
    a.ok(join(voice)).await;
    b.ok(join(voice)).await;
    let paint = started(&mut a, voice, "paint").await;
    created(&mut c).await;

    let first = open_view(&mut a, paint.id).await;
    assert_eq!(viewers_update(&mut c).await, vec![a_id]);
    let second = open_view(&mut a_again, paint.id).await;
    hears_no_update(&mut c).await;
    open_view(&mut b, paint.id).await;
    assert_eq!(viewers_update(&mut c).await, sorted(vec![a_id, b_id]));

    let (_d, _, d_ready) = world.member(5, &FEATURES).await;
    assert_eq!(d_ready.activity_sessions.len(), 1);
    assert_eq!(
        d_ready.activity_sessions[0].viewer_ids,
        sorted(vec![a_id, b_id])
    );
    assert_eq!(
        started(&mut a, voice, "paint").await.viewer_ids,
        sorted(vec![a_id, b_id]),
        "starting it again answers the viewers too"
    );

    paced(&mut a, close_view(first)).await;
    hears_no_update(&mut c).await;
    paced(&mut a_again, close_view(second)).await;
    assert_eq!(viewers_update(&mut c).await, vec![b_id]);
}

#[tokio::test]
async fn leaving_the_call_closes_ones_view() {
    // A and B are in the call with it open. A leaves (A's view closes,
    // LEFT_CALL). B stays (B's view still gets the plugin's events).
    let mut world = World::running(VIEWS_SOURCE).await;
    let (mut a, _, _) = world.member(2, &FEATURES).await;
    let (mut b, b_id, _) = world.member(3, &FEATURES).await;
    let voice = world.voice;
    a.ok(join(voice)).await;
    b.ok(join(voice)).await;
    let paint = started(&mut a, voice, "paint").await;
    let a_view = open_view(&mut a, paint.id).await;
    let b_view = open_view(&mut b, paint.id).await;

    a.ok(leave()).await;
    assert_eq!(
        view_closed(&mut a, a_view).await,
        proto::ViewCloseReason::LeftCall
    );
    // B heard [A] and [A, B] as they opened it; the next is A leaving.
    let mut viewers = Vec::new();
    while viewers != vec![b_id] {
        viewers = viewers_update(&mut b).await;
    }
    wait_for_log(
        &mut world.owner,
        "paint",
        &format!("close {a_view} left_call"),
    )
    .await;

    b.ok(Kind::SendViewEvent(proto::SendViewEvent {
        view_id: b_view,
        name: "reach".to_owned(),
        data_json: format!(r#"{{"session_id":"{}","channel_id":"{voice}"}}"#, paint.id),
    }))
    .await;
    within(&mut b, |event| match event {
        Event::ViewClosed(closed) => panic!("B's view closed: {closed:?}"),
        Event::ViewMessage(message) if message.view_id == b_view && message.name == "here" => {
            Some(())
        }
        _ => None,
    })
    .await;
}

#[tokio::test]
async fn ending_a_session_closes_its_views_once() {
    // A starts it and ends it, with it open. B is in the call with it
    // open. C sees the channel, not in the call (hears the delete and no
    // update before it).
    let mut world = World::running(VIEWS_SOURCE).await;
    let (mut a, _, _) = world.member(2, &FEATURES).await;
    let (mut b, _, _) = world.member(3, &FEATURES).await;
    let (mut c, _, _) = world.member(4, &FEATURES).await;
    let voice = world.voice;
    a.ok(join(voice)).await;
    b.ok(join(voice)).await;
    let paint = started(&mut a, voice, "paint").await;
    let a_view = open_view(&mut a, paint.id).await;
    let b_view = open_view(&mut b, paint.id).await;
    let mut viewers = Vec::new();
    while viewers.len() < 2 {
        viewers = viewers_update(&mut c).await;
    }
    // The plugin handles both opens before the end, or a close can be
    // missing from the log.
    for view in [a_view, b_view] {
        wait_for_log(&mut world.owner, "paint", &format!("open {view} ")).await;
    }

    paced(&mut a, end(paint.id)).await;
    for (client, view) in [(&mut a, a_view), (&mut b, b_view)] {
        let closes: Vec<proto::ViewClosed> = client
            .collect_for(Duration::from_millis(500))
            .await
            .into_iter()
            .filter_map(|event| match event {
                Event::ViewClosed(closed) => Some(closed),
                _ => None,
            })
            .collect();
        assert_eq!(
            closes,
            vec![proto::ViewClosed {
                view_id: view,
                reason: proto::ViewCloseReason::ActivityEnded as i32,
            }]
        );
    }
    let seen = within(&mut c, |event| match event {
        Event::ActivitySessionUpdate(update) => Some(Err(update.clone())),
        Event::ActivitySessionDelete(delete) => Some(Ok(*delete)),
        _ => None,
    })
    .await;
    assert_eq!(
        reason(&seen.expect("an update came before the delete")),
        proto::ActivityEndReason::Ended
    );

    let lines = lines_after(&mut world, &format!("end {}", paint.id)).await;
    let at = |line: String| -> Vec<usize> {
        lines
            .iter()
            .enumerate()
            .filter(|(_, logged)| **logged == line)
            .map(|(index, _)| index)
            .collect()
    };
    let a_close = at(format!("close {a_view} activity_ended"));
    let b_close = at(format!("close {b_view} activity_ended"));
    let ended = at(format!("end {} ended", paint.id));
    assert_eq!(
        (a_close.len(), b_close.len(), ended.len()),
        (1, 1, 1),
        "{lines:#?}"
    );
    assert!(a_close[0] < ended[0] && b_close[0] < ended[0], "{lines:#?}");
}

#[tokio::test]
async fn the_call_emptying_closes_the_last_view() {
    // A is alone in the call with it open, and leaves: the view closes
    // once, then the session ends.
    let mut world = World::running(VIEWS_SOURCE).await;
    let (mut a, _, _) = world.member(2, &FEATURES).await;
    let voice = world.voice;
    a.ok(join(voice)).await;
    let paint = started(&mut a, voice, "paint").await;
    let view = open_view(&mut a, paint.id).await;
    // The plugin handles the open before A leaves, or its close can be
    // missing from the log.
    wait_for_log(&mut world.owner, "paint", &format!("open {view} ")).await;

    a.ok(leave()).await;
    let closes = a
        .collect_for(Duration::from_millis(500))
        .await
        .into_iter()
        .filter(|event| matches!(event, Event::ViewClosed(_)))
        .count();
    assert_eq!(closes, 1);
    let lines = lines_after(&mut world, &format!("end {} call_empty", paint.id)).await;
    let closes: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.starts_with(&format!("close {view} ")))
        .map(|(index, _)| index)
        .collect();
    let ended = lines
        .iter()
        .position(|line| *line == format!("end {} call_empty", paint.id))
        .unwrap();
    assert_eq!(closes.len(), 1, "{lines:#?}");
    assert!(closes[0] < ended, "{lines:#?}");
}

#[tokio::test]
async fn events_reach_a_sessions_views_only() {
    // A is in General with its session open. B is in "Second" with its
    // own session of the same activity open (reached by neither of A's
    // sends).
    let mut world = World::running(VIEWS_SOURCE).await;
    let (mut a, _, _) = world.member(2, &FEATURES).await;
    let (mut b, _, _) = world.member(3, &FEATURES).await;
    let voice = world.voice;
    let second = match world
        .owner
        .ok(Kind::CreateChannel(proto::CreateChannel {
            kind: proto::ChannelKind::Voice as i32,
            name: "Second".to_owned(),
            ..Default::default()
        }))
        .await
    {
        Response::Channel(channel) => channel.id,
        other => panic!("expected a channel, got {other:?}"),
    };
    a.ok(join(voice)).await;
    b.ok(join(second)).await;
    let here = started(&mut a, voice, "paint").await;
    let there = started(&mut b, second, "paint").await;
    let a_view = open_view(&mut a, here.id).await;
    let b_view = open_view(&mut b, there.id).await;

    a.ok(Kind::SendViewEvent(proto::SendViewEvent {
        view_id: a_view,
        name: "reach".to_owned(),
        data_json: format!(r#"{{"session_id":"{}","channel_id":"{voice}"}}"#, here.id),
    }))
    .await;
    wait_for_log(
        &mut world.owner,
        "paint",
        &format!(
            "reach 1 0 [{a_view}] INVALID_ARGUMENT: place must be a table such as \
             {{ channel_id = \"…\" }} or {{ session_id = \"…\" }}"
        ),
    )
    .await;
    b.assert_no_event(
        Duration::from_millis(300),
        |event| matches!(event, Event::ViewMessage(message) if message.view_id == b_view),
    )
    .await;
}

#[tokio::test]
async fn an_activity_view_counts_toward_the_four() {
    // A is in the call and opens it four times from one app; the fifth
    // is refused.
    let world = World::running(VIEWS_SOURCE).await;
    let (mut a, _, _) = world.member(2, &FEATURES).await;
    let voice = world.voice;
    a.ok(join(voice)).await;
    let paint = started(&mut a, voice, "paint").await;
    for _ in 0..4 {
        open_view(&mut a, paint.id).await;
    }
    assert_eq!(
        refused(&mut a, open_in(paint.id)).await,
        proto::ErrorCode::PluginLimit
    );
}

#[tokio::test]
async fn gaining_and_losing_sight_of_the_channel_sends_the_session() {
    // A is in the call with it open (the one viewer). C can't see the
    // channel until an overwrite allows it (hears the create, with A
    // watching), then is denied again (hears the delete). D goes through
    // the same overwrites, its app without "activities" (hears neither).
    let mut world = World::running(VIEWS_SOURCE).await;
    let (mut a, a_id, _) = world.member(2, &FEATURES).await;
    let (mut c, c_id, _) = world.member(4, &FEATURES).await;
    let (mut d, d_id, _) = world.member(5, &WITHOUT_ACTIVITIES).await;
    let voice = world.voice;
    for user_id in [c_id, d_id] {
        world.deny(voice, user_id, Permissions::VIEW_CHANNEL).await;
    }
    a.ok(join(voice)).await;
    let paint = started(&mut a, voice, "paint").await;
    open_view(&mut a, paint.id).await;
    hears_no_session(&mut c).await;

    for user_id in [c_id, d_id] {
        world
            .overwrite(
                voice,
                user_id,
                Permissions::VIEW_CHANNEL,
                Permissions::empty(),
            )
            .await;
    }
    let seen = created(&mut c).await;
    assert_eq!(seen.id, paint.id);
    assert_eq!(seen.viewer_ids, vec![a_id]);

    for user_id in [c_id, d_id] {
        world.deny(voice, user_id, Permissions::VIEW_CHANNEL).await;
    }
    assert_eq!(deleted(&mut c, paint.id).await.channel_id, voice);
    hears_no_session(&mut d).await;
}
