// SPDX-License-Identifier: MPL-2.0

//! The paint example's client script (plan M3 Task 13), run in the VM as
//! the app runs it. Inputs have the wire format's shapes: the canvas's
//! `stroke` event is `{stroke, points, done}` with `points` flat
//! `[x, y, pressure, …]` in the canvas's pixels; `call` is
//! `{"kind":"call","members":[…],"speaking":[…]}`; `context` as the
//! renderer sends it.

use opencord_plugins::view::Patch;
use serde_json::{Value as Json, json};

use crate::views::test_support::{Recorder, Seen, event, input, open, settle, spec};
use crate::views::{Place, ViewHandle, ViewSpec};

const BOARD_WIDTH: f64 = 1600.0;
const BOARD_HEIGHT: f64 = 1000.0;

fn script() -> String {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/plugins/paint/client/main.luau"
    );
    std::fs::read_to_string(path).expect("examples/plugins/paint/client/main.luau")
}

/// The example's client in an activity's view of `width` × `height`.
fn paint(width: f64, height: f64) -> ViewSpec {
    let mut spec = spec(&script());
    spec.plugin_id = "paint".into();
    spec.show_errors = true;
    spec.context.width = width;
    spec.context.height = height;
    spec.place = Place::Activity {
        session_id: "9".into(),
        channel_id: "5".into(),
        activity: "paint".into(),
    };
    spec
}

/// Opens it, checks the first render drew without an error: the view, its
/// recorder and what it saw.
fn opened(width: f64, height: f64) -> (ViewHandle, std::sync::Arc<Recorder>, Seen) {
    let (view, recorder) = open(paint(width, height));
    let seen = recorder.first_batch();
    no_errors(&seen);
    (view, recorder, seen)
}

fn no_errors(seen: &Seen) {
    assert!(seen.errors.is_empty(), "client errors: {:?}", seen.errors);
    for op in seen.ops() {
        if let Patch::Error { message, details } = op {
            panic!("render error: {message} ({details:?})");
        }
    }
}

/// Every item a `draw` op set with this id, in order.
fn items_set(seen: &Seen, id: &str) -> Vec<Json> {
    let mut found = Vec::new();
    for op in seen.ops() {
        if let Patch::Draw { set, .. } = op {
            let named = set.iter().filter(|item| item["id"] == json!(id));
            found.extend(named.cloned());
        }
    }
    found
}

fn numbers(value: &Json) -> Vec<f64> {
    let list = value.as_array().expect("a list of numbers");
    list.iter().map(|n| n.as_f64().expect("a number")).collect()
}

/// The renderer's `stroke` event for ink stroke `stroke`.
fn stroke_event(view: &ViewHandle, canvas: u64, stroke: &str, points: &[f64], done: bool) {
    event(
        view,
        canvas,
        "stroke",
        json!({ "stroke": stroke, "points": points, "done": done }),
    );
}

/// What the server sends the session's views for one stroke.
fn server_stroke(id: &str, points: &[f64]) -> String {
    json!({ "id": id, "color": "#e03131", "width": 6, "points": points }).to_string()
}

// Fails if a stroke is kept or sent before `done`, if the points sent are
// view pixels instead of board units (an 800 × 500 view shows the board at
// half size, so each x and y doubles; pressure goes), or if the item drawn
// doesn't take the ink stroke's id (the renderer would keep its own ink
// beside it).
#[test]
fn a_finished_stroke_becomes_an_item_and_is_sent() {
    let (view, recorder, seen) = opened(800.0, 500.0);
    let canvas = seen.first("canvas");

    stroke_event(
        &view,
        canvas,
        "ink_1",
        &[10.0, 20.0, 0.5, 30.0, 40.0, 0.5],
        false,
    );
    let seen = settle(&view, &recorder);
    assert!(seen.sent("stroke").is_empty(), "sent before it was done");
    assert!(
        items_set(&seen, "ink_1").is_empty(),
        "kept before it was done"
    );

    stroke_event(&view, canvas, "ink_1", &[50.0, 60.0, 0.5], true);
    let seen = settle(&view, &recorder);
    no_errors(&seen);

    let sent = seen.sent("stroke");
    assert_eq!(sent.len(), 1, "one stroke sent: {sent:?}");
    assert_eq!(sent[0]["id"], "ink_1");
    assert_eq!(
        numbers(&sent[0]["points"]),
        [20.0, 40.0, 60.0, 80.0, 100.0, 120.0]
    );
    let items = items_set(&seen, "ink_1");
    assert_eq!(items.len(), 1, "one draw of the stroke: {items:?}");
    assert_eq!(items[0]["type"], "path");
    // Drawn back at the view's scale, where the pointer was.
    assert_eq!(
        numbers(&items[0]["points"]),
        [10.0, 20.0, 30.0, 40.0, 50.0, 60.0]
    );
}

// Fails if a stroke over 100 points is sent or drawn whole, or if thinning
// loses its first or last point.
#[test]
fn a_long_stroke_is_thinned_to_100_points() {
    let (view, recorder, seen) = opened(1600.0, 1000.0);
    let canvas = seen.first("canvas");
    let mut points = Vec::new();
    for i in 0..500 {
        points.extend([i as f64 * 3.0, 100.0 + (i % 7) as f64, 0.5]);
    }
    stroke_event(&view, canvas, "ink_long", &points, true);
    let seen = settle(&view, &recorder);
    no_errors(&seen);

    let sent = seen.sent("stroke");
    assert_eq!(sent.len(), 1);
    let sent = numbers(&sent[0]["points"]);
    assert_eq!(sent.len(), 200, "100 points of x and y");
    assert_eq!(sent[..2], [0.0, 100.0], "the first point stays");
    assert_eq!(
        sent[198..],
        [1497.0, 100.0 + (499 % 7) as f64],
        "the last too"
    );
    let items = items_set(&seen, "ink_long");
    assert_eq!(numbers(&items[0]["points"]).len(), 200);
}

// Fails if a stroke the server sends again (its echo to the sender, or a
// board sent after it arrived) is drawn a second time.
#[test]
fn a_stroke_from_someone_else_is_drawn_once() {
    let (view, recorder, _) = opened(800.0, 500.0);
    let message = server_stroke("ink_theirs", &[100.0, 200.0, 300.0, 400.0]);
    view.server_message("stroke".into(), message.clone());
    view.server_message("stroke".into(), message);
    let seen = settle(&view, &recorder);
    no_errors(&seen);

    let items = items_set(&seen, "ink_theirs");
    assert_eq!(items.len(), 1, "drawn once: {items:?}");
    assert_eq!(numbers(&items[0]["points"]), [50.0, 100.0, 150.0, 200.0]);
    assert_eq!(items[0]["stroke"]["color"], "#e03131");
    assert_eq!(items[0]["stroke"]["width"].as_f64(), Some(3.0));
}

// Fails if the items stay at the old size after a `context` input of
// another size: the board would then be drawn out of the canvas, or not
// fill it.
#[test]
fn resizing_keeps_the_picture() {
    let (view, recorder, _) = opened(800.0, 500.0);
    let board = [100.0, 200.0, 300.0, 400.0];
    view.server_message("stroke".into(), server_stroke("ink_a", &board));
    let seen = settle(&view, &recorder);
    assert_eq!(
        numbers(&items_set(&seen, "ink_a")[0]["points"]),
        [50.0, 100.0, 150.0, 200.0]
    );

    // Twice as wide as tall: the board's height decides, scale 0.4, and
    // the board is centred across: (1000 − 1600 × 0.4) / 2 = 180.
    input(
        &view,
        "context",
        json!({
            "width": 1000.0, "height": 400.0, "theme": "dark", "input": "mouse",
            "safe_area": { "top": 0.0, "right": 0.0, "bottom": 0.0, "left": 0.0 },
            "text_scale": 1.0,
        }),
    );
    let seen = settle(&view, &recorder);
    no_errors(&seen);
    let items = items_set(&seen, "ink_a");
    assert_eq!(items.len(), 2, "drawn again at the new size: {items:?}");
    let scale = (1000.0 / BOARD_WIDTH).min(400.0 / BOARD_HEIGHT);
    let left = (1000.0 - BOARD_WIDTH * scale) / 2.0;
    let expected = [
        left + 100.0 * scale,
        200.0 * scale,
        left + 300.0 * scale,
        400.0 * scale,
    ];
    let drawn = numbers(&items[1]["points"]);
    for (got, want) in drawn.iter().zip(expected) {
        assert!((got - want).abs() < 1e-9, "{drawn:?} != {expected:?}");
    }
}

// Fails if the line doesn't follow `call_members_change`, counts the
// viewer among the others, or reads a list the call input doesn't send.
#[test]
fn the_line_says_who_is_here() {
    let (view, recorder, seen) = opened(800.0, 500.0);
    let node = seen.node("text", "Just you");

    // The viewer is "10" (test_support's spec).
    input(
        &view,
        "call",
        json!({ "members": ["10", "11", "12"], "speaking": [] }),
    );
    let seen = settle(&view, &recorder);
    no_errors(&seen);
    assert_eq!(text_now(&seen, node), "You and 2 others");

    input(
        &view,
        "call",
        json!({ "members": ["10", "12"], "speaking": ["12"] }),
    );
    let seen = settle(&view, &recorder);
    assert_eq!(text_now(&seen, node), "You and 1 other");
}

/// The `text` prop node `id` has now, from its create and later `props` ops.
fn text_now(seen: &Seen, id: u64) -> String {
    let mut text = None;
    for op in seen.ops() {
        match op {
            Patch::Create { id: at, props, .. } if *at == id => {
                text = props.get("text").cloned();
            }
            Patch::Props { id: at, set, .. } if *at == id => {
                if let Some(set) = set.get("text") {
                    text = Some(set.clone());
                }
            }
            _ => {}
        }
    }
    let text = text.unwrap_or_else(|| panic!("no node {id} in {:#?}", seen.batches));
    text.as_str().expect("text").to_owned()
}
