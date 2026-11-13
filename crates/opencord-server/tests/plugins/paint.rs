// SPDX-License-Identifier: MPL-2.0

//! The paint example (plan M3 Task 13), installed from
//! `examples/plugins/paint` and driven through the gateway as the app
//! drives it: the client script asks for the `board` when it opens, and
//! sends `stroke` `{ id, color, width, points }` (board units) and `clear`.

use std::path::Path;
use std::time::Duration;

use opencord_plugins::package::Package;
use opencord_proto::v1 as proto;
use proto::event::Kind as Event;
use proto::request::Kind;
use proto::response::Result as Reply;
use serde_json::{Value as Json, json};

use crate::common::{TestClient, TestServer, channel_id, invite, owner};
use crate::support::{install, log_lines, session, wait_for_log};

/// What the desktop app lists in `Identify.features`.
const FEATURES: [&str; 3] = ["plugin_users", "plugin_channels", "activities"];

/// A server running the paint example, installed by the owner.
struct World {
    server: TestServer,
    /// Installs the plugin and reads its log; lists no features.
    owner: TestClient,
    code: String,
    /// The voice channel "General".
    voice: i64,
}

impl World {
    async fn new() -> Self {
        let server = TestServer::start_with(|config| config.gateway.identify_per_minute = 60).await;
        let (mut owner, ready) = owner(&server).await;
        let code = invite(&mut owner, None).await;
        let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/plugins/paint");
        let zip = Package::from_dir(&folder)
            .expect("the paint folder packs")
            .write_zip();
        let token = crate::common::media_token(&ready);
        install(&server, &mut owner, &token, zip, &["register_activities"]).await;
        wait_for_log(&mut owner, "paint", "paint started").await;
        let voice = channel_id(&ready, "General");
        Self {
            server,
            owner,
            code,
            voice,
        }
    }

    /// A member's session for key `seed`, as the desktop app connects.
    async fn member(&self, seed: u8) -> TestClient {
        session(&self.server, seed, Some(&self.code), &FEATURES)
            .await
            .0
    }

    /// A second voice channel.
    async fn second_call(&mut self) -> i64 {
        match self
            .owner
            .ok(Kind::CreateChannel(proto::CreateChannel {
                kind: proto::ChannelKind::Voice as i32,
                name: "Second".to_owned(),
                ..Default::default()
            }))
            .await
        {
            Reply::Channel(channel) => channel.id,
            other => panic!("not a channel: {other:?}"),
        }
    }

    /// How many lines of the plugin's log start with `prefix`, once one has
    /// and late lines had time to come.
    async fn lines_starting(&mut self, prefix: &str) -> usize {
        wait_for_log(&mut self.owner, "paint", prefix).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        let lines = log_lines(&mut self.owner, "paint").await;
        lines.iter().filter(|line| line.starts_with(prefix)).count()
    }
}

fn join(channel_id: i64) -> Kind {
    Kind::UpdateVoiceState(proto::UpdateVoiceState {
        channel_id: Some(channel_id),
        ..Default::default()
    })
}

/// Joins `channel_id` and starts paint there: the session's id.
async fn start(client: &mut TestClient, channel_id: i64) -> i64 {
    client.ok(join(channel_id)).await;
    match client
        .ok_paced(Kind::StartActivity(proto::StartActivity {
            channel_id,
            plugin_id: "paint".to_owned(),
            activity_key: "paint".to_owned(),
        }))
        .await
    {
        Reply::ActivitySession(session) => session.id,
        other => panic!("not started: {other:?}"),
    }
}

/// Opens the session's view as the app does, which then asks for the
/// board: the view's id.
async fn open(client: &mut TestClient, session_id: i64) -> i64 {
    let opened = client
        .ok_paced(Kind::OpenView(proto::OpenView {
            place: Some(proto::ViewPlace {
                kind: Some(proto::view_place::Kind::ActivitySessionId(session_id)),
            }),
            platform: "desktop".to_owned(),
        }))
        .await;
    let Reply::ViewOpened(opened) = opened else {
        panic!("not opened: {opened:?}");
    };
    send(client, opened.view_id, "board", json!({})).await;
    opened.view_id
}

fn event(view_id: i64, name: &str, data: &Json) -> Kind {
    Kind::SendViewEvent(proto::SendViewEvent {
        view_id,
        name: name.to_owned(),
        data_json: data.to_string(),
    })
}

/// Sends a view event, waiting out the view's rate limit.
async fn send(client: &mut TestClient, view_id: i64, name: &str, data: Json) {
    client.ok_paced(event(view_id, name, &data)).await;
}

/// A stroke as the client script sends it.
fn stroke(id: &str) -> Json {
    json!({ "id": id, "color": "#e03131", "width": 6, "points": [10, 20, 30, 40] })
}

/// Draws strokes `ids` in order from `view_id`.
async fn draw(client: &mut TestClient, view_id: i64, ids: impl IntoIterator<Item = String>) {
    for id in ids {
        send(client, view_id, "stroke", stroke(&id)).await;
    }
}

/// The data of the next view message `name` that `client` hears.
async fn message(client: &mut TestClient, name: &str) -> Json {
    let data = tokio::time::timeout(
        Duration::from_secs(5),
        client.wait_for(|event| match event {
            Event::ViewClosed(closed) => panic!("a view closed: {closed:?}"),
            Event::ViewMessage(message) if message.name == name => Some(message.data_json.clone()),
            _ => None,
        }),
    )
    .await
    .unwrap_or_else(|_| panic!("no {name} came"));
    serde_json::from_str(&data).expect("JSON")
}

/// Asserts `client` hears no view message for a while.
async fn hears_nothing(client: &mut TestClient) {
    client
        .assert_no_event(Duration::from_millis(300), |event| {
            matches!(event, Event::ViewMessage(_))
        })
        .await;
}

/// The ids in a `strokes` event, in order.
fn ids(strokes: &Json) -> Vec<String> {
    let list = strokes["strokes"].as_array().expect("a list of strokes");
    let id = |stroke: &Json| stroke["id"].as_str().expect("an id").to_owned();
    list.iter().map(id).collect()
}

/// The `strokes` events a newly opened view gets, until a quiet moment.
async fn board(client: &mut TestClient) -> Vec<Vec<String>> {
    let mut events = Vec::new();
    loop {
        let next = tokio::time::timeout(
            Duration::from_secs(2),
            client.wait_for(|event| match event {
                Event::ViewMessage(message) if message.name == "strokes" => {
                    Some(message.data_json.clone())
                }
                _ => None,
            }),
        )
        .await;
        match next {
            Ok(data) => events.push(ids(&serde_json::from_str(&data).expect("JSON"))),
            Err(_) => return events,
        }
    }
}

/// `value` with every number as a float: Lua has one kind of number, so
/// `6` may come back as `6.0`.
fn floats(value: Json) -> Json {
    match value {
        Json::Number(number) => json!(number.as_f64().expect("a finite number")),
        Json::Array(list) => Json::Array(list.into_iter().map(floats).collect()),
        Json::Object(fields) => {
            Json::Object(fields.into_iter().map(|(k, v)| (k, floats(v))).collect())
        }
        other => other,
    }
}

fn named(prefix: &str, range: std::ops::Range<usize>) -> Vec<String> {
    range.map(|n| format!("{prefix}{n}")).collect()
}

#[tokio::test]
async fn a_stroke_reaches_the_others_in_the_call() {
    // A draws in the call (and gets the stroke back, as every view of the
    // session does). B is in the call with it open (gets it). C is in the
    // call without it open (gets nothing). D runs paint in another call,
    // open (gets nothing: it's another session's board).
    let mut world = World::new().await;
    let mut a = world.member(2).await;
    let mut b = world.member(3).await;
    let mut c = world.member(4).await;
    let mut d = world.member(5).await;
    let voice = world.voice;
    let second = world.second_call().await;
    let paint = start(&mut a, voice).await;
    b.ok(join(voice)).await;
    c.ok(join(voice)).await;
    let a_view = open(&mut a, paint).await;
    open(&mut b, paint).await;
    let other = start(&mut d, second).await;
    open(&mut d, other).await;

    send(&mut a, a_view, "stroke", stroke("ink_1")).await;
    let sent = floats(stroke("ink_1"));
    assert_eq!(floats(message(&mut b, "stroke").await), sent);
    assert_eq!(floats(message(&mut a, "stroke").await), sent);
    hears_nothing(&mut c).await;
    hears_nothing(&mut d).await;
}

#[tokio::test]
async fn someone_opening_later_gets_the_board() {
    // A draws 65 strokes. B is in the call and opens it afterwards: the
    // board arrives in events of 30, 30 and 5, oldest first.
    let world = World::new().await;
    let mut a = world.member(2).await;
    let mut b = world.member(3).await;
    let paint = start(&mut a, world.voice).await;
    b.ok(join(world.voice)).await;
    let a_view = open(&mut a, paint).await;
    draw(&mut a, a_view, named("s", 0..65)).await;

    open(&mut b, paint).await;
    let events = board(&mut b).await;
    let sizes: Vec<usize> = events.iter().map(Vec::len).collect();
    assert_eq!(sizes, [30, 30, 5]);
    assert_eq!(events.concat(), named("s", 0..65));
}

#[tokio::test]
async fn a_bad_stroke_is_dropped() {
    // A draws bad strokes, then one good one. B has it open (gets only the
    // good one). C opens it afterwards (its board holds only the good one).
    let mut world = World::new().await;
    let mut a = world.member(2).await;
    let mut b = world.member(3).await;
    let mut c = world.member(4).await;
    let paint = start(&mut a, world.voice).await;
    b.ok(join(world.voice)).await;
    c.ok(join(world.voice)).await;
    let a_view = open(&mut a, paint).await;
    open(&mut b, paint).await;

    let mut off_palette = stroke("bad_colour");
    off_palette["color"] = json!("#123456");
    let mut too_wide = stroke("bad_width");
    too_wide["width"] = json!(25);
    let mut too_many = stroke("bad_count");
    too_many["points"] = json!(vec![10; 201]);
    let mut off_board = stroke("bad_point");
    off_board["points"] = json!([10, 20, 5000, 40]);
    for bad in [off_palette, too_wide, too_many, off_board] {
        send(&mut a, a_view, "stroke", bad).await;
    }
    // A NaN width never reaches the plugin: JSON has no NaN, so the
    // gateway refuses the event.
    let nan = r##"{"id":"bad_nan","color":"#e03131","width":NaN,"points":[1,2,3,4]}"##;
    let refused = a
        .error(Kind::SendViewEvent(proto::SendViewEvent {
            view_id: a_view,
            name: "stroke".to_owned(),
            data_json: nan.to_owned(),
        }))
        .await;
    assert_eq!(refused.code, proto::ErrorCode::InvalidArgument as i32);
    send(&mut a, a_view, "stroke", stroke("good")).await;

    assert_eq!(message(&mut b, "stroke").await["id"], "good");
    for why in [
        "dropped a stroke: its colour isn't in the palette",
        "dropped a stroke: its width isn't from 1 to 24",
        "dropped a stroke: it needs 2 to 200 numbers",
        "dropped a stroke: a point is off the board",
    ] {
        assert_eq!(world.lines_starting(why).await, 1, "{why}");
    }
    assert_eq!(world.lines_starting("dropped a stroke").await, 4);
    open(&mut c, paint).await;
    assert_eq!(board(&mut c).await, [["good"]]);
}

#[tokio::test]
async fn clear_empties_it_for_everyone() {
    // A draws two strokes. B clears (A and B both hear it). A draws one
    // more. C opens it afterwards: its board holds only the last one.
    let world = World::new().await;
    let mut a = world.member(2).await;
    let mut b = world.member(3).await;
    let mut c = world.member(4).await;
    let paint = start(&mut a, world.voice).await;
    b.ok(join(world.voice)).await;
    c.ok(join(world.voice)).await;
    let a_view = open(&mut a, paint).await;
    let b_view = open(&mut b, paint).await;
    draw(&mut a, a_view, named("s", 0..2)).await;

    send(&mut b, b_view, "clear", json!({})).await;
    message(&mut a, "clear").await;
    message(&mut b, "clear").await;
    send(&mut a, a_view, "stroke", stroke("after")).await;

    open(&mut c, paint).await;
    assert_eq!(board(&mut c).await, [["after"]]);
}

#[tokio::test]
async fn the_board_goes_with_the_session() {
    // A starts it, draws, ends it, and starts it again. B opens the new
    // session: its board holds only what was drawn in it.
    let mut world = World::new().await;
    let mut a = world.member(2).await;
    let mut b = world.member(3).await;
    let first = start(&mut a, world.voice).await;
    let a_view = open(&mut a, first).await;
    draw(&mut a, a_view, named("old", 0..3)).await;

    a.ok_paced(Kind::EndActivity(proto::EndActivity { session_id: first }))
        .await;
    let gone = format!("the board of session {first} is gone (3 strokes)");
    assert_eq!(world.lines_starting(&gone).await, 1);

    let second = start(&mut a, world.voice).await;
    assert_ne!(second, first);
    let a_view = open(&mut a, second).await;
    send(&mut a, a_view, "stroke", stroke("new")).await;
    b.ok(join(world.voice)).await;
    open(&mut b, second).await;
    assert_eq!(board(&mut b).await, [["new"]]);
}

#[tokio::test]
async fn the_301st_stroke_drops_the_first() {
    // A draws 301 strokes. B opens it afterwards: 300 of them, from the
    // second on.
    let world = World::new().await;
    let mut a = world.member(2).await;
    let mut b = world.member(3).await;
    let paint = start(&mut a, world.voice).await;
    b.ok(join(world.voice)).await;
    let a_view = open(&mut a, paint).await;
    draw(&mut a, a_view, named("s", 0..301)).await;

    open(&mut b, paint).await;
    let events = board(&mut b).await;
    assert_eq!(events.len(), 10);
    assert_eq!(events.concat(), named("s", 1..301));
}
