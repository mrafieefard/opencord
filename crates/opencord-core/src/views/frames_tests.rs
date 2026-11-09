//! The frame loop (§11.5), through `start` and the batches its sink gets.

use serde_json::json;

use crate::views::ViewHandle;
use crate::views::test_support::{Recorder, input, open, settle, spec};

const LOOP: &str = r#"
function init() return { n = 0 } end
function render(state) return ui.text({ text = "n" .. state.n }) end
local function tick(dt) app.send("frame", dt) end
app.on("start", function() app.on_frame(tick) end)
app.on("stop", function() app.off_frame() end)
"#;

fn frame(view: &ViewHandle, seq: u64) {
    input(view, "frame", json!({ "dt": 0.5, "seq": seq }));
}

/// Turns frames on; the last batch's `seq`.
fn start_frames(view: &ViewHandle, recorder: &Recorder) -> u64 {
    view.server_message("start".into(), "null".into());
    let seen = settle(view, recorder);
    seen.batches.last().map_or(0, |batch| batch.seq)
}

// Fails if `on_frame` sends no `frames` op (or one per call), `off_frame`
// doesn't stop them, or `fn` doesn't get `dt`.
#[test]
fn frames_turn_on_and_off() {
    let (view, recorder) = open(spec(LOOP));
    let seq = recorder.first_batch().batches[0].seq;
    frame(&view, seq);
    assert!(settle(&view, &recorder).sent("frame").is_empty());

    view.server_message("start".into(), "null".into());
    let seq = start_frames(&view, &recorder);
    assert_eq!(settle(&view, &recorder).frames(), [true]);

    frame(&view, seq);
    assert_eq!(settle(&view, &recorder).sent("frame"), [json!(0.5)]);

    view.server_message("stop".into(), "null".into());
    assert_eq!(settle(&view, &recorder).frames(), [true, false]);
    frame(&view, seq + 1);
    assert_eq!(settle(&view, &recorder).sent("frame").len(), 1);
}

// Fails if a frame's `app.set` doesn't redraw.
#[test]
fn a_frame_redraws() {
    let (view, recorder) = open(spec(
        r#"
        function init() return { n = 0 } end
        function render(state) return ui.text({ text = "n" .. state.n }) end
        app.on("start", function()
            app.on_frame(function() app.set(function(state) state.n += 1 end) end)
        end)
        "#,
    ));
    recorder.first_batch();
    let seq = start_frames(&view, &recorder);
    let before = settle(&view, &recorder).batches.len();
    frame(&view, seq);
    let seen = settle(&view, &recorder);
    assert_eq!(seen.batches.len(), before + 1, "{:#?}", seen.batches);
}

// Fails if a frame older than the last batch still runs: a slow renderer
// would queue them.
#[test]
fn a_stale_frame_is_skipped() {
    let (view, recorder) = open(spec(LOOP));
    recorder.first_batch();
    let seq = start_frames(&view, &recorder);
    assert!(seq >= 2, "needs an older batch to name");
    frame(&view, seq - 1);
    assert!(settle(&view, &recorder).sent("frame").is_empty());
    frame(&view, seq);
    assert_eq!(settle(&view, &recorder).sent("frame").len(), 1);
}

// Fails if the frame loop keeps running while the view is hidden, or
// doesn't come back after.
#[test]
fn frames_wait_while_paused() {
    let (view, recorder) = open(spec(LOOP));
    recorder.first_batch();
    start_frames(&view, &recorder);

    input(&view, "lifecycle", json!({ "event": "pause" }));
    let seen = settle(&view, &recorder);
    assert_eq!(seen.frames(), [true, false]);
    frame(&view, seen.batches.last().unwrap().seq);
    assert!(settle(&view, &recorder).sent("frame").is_empty());

    input(&view, "lifecycle", json!({ "event": "resume" }));
    let seen = settle(&view, &recorder);
    assert_eq!(seen.frames(), [true, false, true]);
    frame(&view, seen.batches.last().unwrap().seq);
    assert_eq!(settle(&view, &recorder).sent("frame").len(), 1);
}

// Fails if a frame gets the 50 ms a handler has rather than 8 ms (it spins
// for 20 ms), isn't reported, or leaves frames on. The view must go on.
#[test]
fn a_slow_frame_stops_frames() {
    let (view, recorder) = open(spec(
        r#"
        function init() return {} end
        function render() return ui.text({ text = "ok" }) end
        app.on("start", function()
            app.on_frame(function()
                local from = os.clock()
                while os.clock() - from < 0.02 do end
                app.send("done")
            end)
        end)
        "#,
    ));
    recorder.first_batch();
    let seq = start_frames(&view, &recorder);
    frame(&view, seq);
    let seen = recorder.wait("frames off", |seen| seen.frames().len() == 2);
    assert_eq!(seen.frames(), [true, false]);
    assert!(seen.sent("done").is_empty(), "the frame ran past 8 ms");
    assert!(
        seen.errors
            .iter()
            .any(|error| error.starts_with("app.on_frame")),
        "{:?}",
        seen.errors
    );

    frame(&view, seen.batches.last().unwrap().seq);
    let seen = settle(&view, &recorder);
    assert_eq!(seen.errors.len(), 1, "it ran again: {:?}", seen.errors);
}
