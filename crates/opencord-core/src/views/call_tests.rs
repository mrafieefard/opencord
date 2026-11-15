//! The call in an activity's view (§11.4, §11.5): the `call` input,
//! `app.call` and the two listeners, through `start` and what its sink gets.
//! Inputs are built in the wire format's shape:
//! `{"kind":"call","members":[…],"speaking":[…]}`.

use opencord_plugins::view::Patch;
use serde_json::{Value as Json, json};

use crate::views::test_support::{Recorder, input, open, settle, spec};
use crate::views::{Place, ViewHandle, ViewSpec};

/// Reads the call and tells what each listener heard, with `app.call` as
/// the listener saw it. Lists go as comma-joined text, so an empty one
/// can't be confused with an empty object.
const CALL: &str = r#"
function init(ctx) return { place = ctx.place, m = "", s = "" } end
function render(state) return ui.text({ text = "m" .. state.m .. " s" .. state.s }) end
local function ids(list) return table.concat(list, ",") end
local function call_now()
    return { channel_id = app.call.channel_id, members = ids(app.call.members),
        speaking = ids(app.call.speaking) }
end
app.listen("call_members_change", function(change)
    app.send("members", { members = ids(change.members), joined = ids(change.joined),
        left = ids(change.left), call = call_now() })
end)
app.listen("speaking_change", function(change)
    app.send("speaking", { speaking = ids(change.speaking), call = call_now() })
end)
app.on("read", function()
    app.send("call", if app.call == nil then "nil" else call_now())
end)
app.on("place", function()
    app.send("place", { ctx = app.state.place, app = app.place })
end)
app.on("try", function()
    local function why(fn)
        local ok, message = pcall(fn)
        return if ok then "no error" else message
    end
    app.send("why", {
        member = why(function() app.call.members[1] = "3" end),
        speaking = why(function() app.call.speaking = {} end),
        call = why(function() app.call = nil end),
    })
end)
"#;

fn activity(source: &str) -> ViewSpec {
    let mut spec = spec(source);
    spec.place = Place::Activity {
        session_id: "9".into(),
        channel_id: "5".into(),
        activity: "paint".into(),
    };
    spec
}

fn call(view: &ViewHandle, members: Json, speaking: Json) {
    input(
        view,
        "call",
        json!({ "members": members, "speaking": speaking }),
    );
}

fn read(view: &ViewHandle, recorder: &Recorder) -> Json {
    view.server_message("read".into(), "null".into());
    let sent = settle(view, recorder).sent("call");
    sent.last().expect("the call").clone()
}

// Fails if `ctx.place` or `app.place` lacks one of the activity place's
// four fields, or names it a channel.
#[test]
fn an_activity_view_reads_its_place() {
    let (view, recorder) = open(activity(CALL));
    recorder.first_batch();
    view.server_message("place".into(), "null".into());
    let place = json!({
        "kind": "activity", "session_id": "9", "channel_id": "5", "activity": "paint",
    });
    assert_eq!(
        settle(&view, &recorder).sent("place"),
        [json!({ "ctx": place, "app": place })]
    );
}

// Fails if `app.call` isn't there (empty) before the first input, isn't
// filled before the listener runs, `joined` isn't everyone at first, or
// `speaking_change` runs for an empty list that didn't change.
#[test]
fn the_first_call_input_fills_app_call_and_tells_listeners() {
    let (view, recorder) = open(activity(CALL));
    recorder.first_batch();
    assert_eq!(
        read(&view, &recorder),
        json!({ "channel_id": "5", "members": "", "speaking": "" })
    );
    call(&view, json!(["1", "2"]), json!([]));
    let seen = settle(&view, &recorder);
    assert_eq!(
        seen.sent("members"),
        [json!({
            "members": "1,2", "joined": "1,2", "left": "",
            "call": { "channel_id": "5", "members": "1,2", "speaking": "" },
        })]
    );
    assert!(seen.sent("speaking").is_empty(), "{:?}", seen.sends);
}

// Fails if a listener runs for a list that didn't change.
#[test]
fn only_what_changed_is_told() {
    let (view, recorder) = open(activity(CALL));
    recorder.first_batch();
    call(&view, json!(["1", "2"]), json!([]));
    settle(&view, &recorder);
    call(&view, json!(["1", "2"]), json!(["2"]));
    let seen = settle(&view, &recorder);
    assert_eq!(seen.sent("members").len(), 1, "the members didn't change");
    assert_eq!(
        seen.sent("speaking"),
        [json!({
            "speaking": "2",
            "call": { "channel_id": "5", "members": "1,2", "speaking": "2" },
        })]
    );
    call(&view, json!(["1", "2"]), json!(["2"]));
    let seen = settle(&view, &recorder);
    assert_eq!(seen.sent("members").len(), 1, "the same input again");
    assert_eq!(seen.sent("speaking").len(), 1, "the same input again");
}

// Fails if `left` isn't the one who left, or `joined` names someone who
// was already there.
#[test]
fn someone_leaving_is_in_left() {
    let (view, recorder) = open(activity(CALL));
    recorder.first_batch();
    call(&view, json!(["1", "2"]), json!([]));
    call(&view, json!(["2"]), json!([]));
    let members = settle(&view, &recorder).sent("members");
    assert_eq!(
        members.last(),
        Some(&json!({
            "members": "2", "joined": "", "left": "1",
            "call": { "channel_id": "5", "members": "2", "speaking": "" },
        }))
    );
}

// Fails if an id that isn't a string of digits, or a list over 1,000 ids,
// gets into `app.call` or reaches a listener, or if 1,000 is refused.
#[test]
fn a_bad_call_input_changes_nothing() {
    let (view, recorder) = open(activity(CALL));
    recorder.first_batch();
    call(&view, json!(["1", "2"]), json!([]));
    settle(&view, &recorder);
    let many: Vec<String> = (1..=1_001).map(|id| id.to_string()).collect();
    for (members, speaking) in [
        (json!(["1", "x1"]), json!([])),
        (json!(["1", "2"]), json!(["x1"])),
        (json!(["1", 2]), json!([])),
        (json!(["1", ""]), json!([])),
        (json!(many), json!([])),
        (json!(["1", "2"]), json!(many)),
    ] {
        call(&view, members, speaking);
    }
    assert_eq!(
        read(&view, &recorder),
        json!({ "channel_id": "5", "members": "1,2", "speaking": "" })
    );
    let seen = settle(&view, &recorder);
    assert_eq!(seen.sent("members").len(), 1, "{:?}", seen.sent("members"));
    assert!(
        seen.sent("speaking").is_empty(),
        "{:?}",
        seen.sent("speaking")
    );

    // The limit itself is fine.
    call(&view, json!(many[..1_000]), json!([]));
    let seen = settle(&view, &recorder);
    assert_eq!(seen.sent("members").len(), 2);
}

// Fails if a channel's view gets an `app.call`, or a `call` input reaches
// its listeners.
#[test]
fn a_channel_view_has_no_call() {
    let (view, recorder) = open(spec(CALL));
    recorder.first_batch();
    assert_eq!(read(&view, &recorder), json!("nil"));
    call(&view, json!(["1", "2"]), json!(["1"]));
    assert_eq!(read(&view, &recorder), json!("nil"));
    let seen = settle(&view, &recorder);
    assert!(
        seen.sends
            .iter()
            .all(|(name, _)| name != "members" && name != "speaking")
    );
    assert!(seen.errors.is_empty(), "{:?}", seen.errors);
}

// Fails if `app.call` or one of its lists can be written by a script.
#[test]
fn app_call_cant_be_changed() {
    let (view, recorder) = open(activity(CALL));
    recorder.first_batch();
    call(&view, json!(["1", "2"]), json!(["2"]));
    settle(&view, &recorder);
    view.server_message("try".into(), "null".into());
    let why = settle(&view, &recorder).sent("why");
    let why = why.last().expect("an answer");
    for field in ["member", "speaking", "call"] {
        let message = why[field].as_str().unwrap_or_default();
        assert!(message.contains("readonly"), "{field}: {message}");
    }
    assert_eq!(
        read(&view, &recorder),
        json!({ "channel_id": "5", "members": "1,2", "speaking": "2" })
    );
}

// Fails if a reload's new VM starts with an empty `app.call` instead of
// the lists the renderer last sent (the renderer sends them only on a
// change).
#[test]
fn a_reload_keeps_the_call() {
    let spec = activity(CALL);
    let (entry, sources) = (spec.entry.clone(), spec.sources.clone());
    let (view, recorder) = open(spec);
    recorder.first_batch();
    call(&view, json!(["1", "2"]), json!(["2"]));
    settle(&view, &recorder);
    view.reload(entry, sources);
    assert_eq!(
        read(&view, &recorder),
        json!({ "channel_id": "5", "members": "1,2", "speaking": "2" })
    );
}

// Fails if an input that runs both listeners redraws after each of them,
// or runs `speaking_change` before `call_members_change`.
#[test]
fn a_listener_that_sets_state_redraws_once() {
    let (view, recorder) = open(activity(&format!(
        r#"{CALL}
        app.listen("call_members_change", function(change)
            app.set(function(state) state.m = ids(change.members) end)
        end)
        app.listen("speaking_change", function(change)
            app.set(function(state) state.s = ids(change.speaking) end)
        end)
        "#
    )));
    recorder.first_batch();
    let before = settle(&view, &recorder).batches.len();
    call(&view, json!(["1", "2"]), json!(["2"]));
    let seen = settle(&view, &recorder);
    let after = &seen.batches[before..];
    assert_eq!(after.len(), 1, "{after:#?}");
    let drew = after[0].ops.iter().any(
        |op| matches!(op, Patch::Props { set, .. } if set.get("text") == Some(&json!("m1,2 s2"))),
    );
    assert!(drew, "{after:#?}");
    let order: Vec<&str> = seen.sends.iter().map(|(name, _)| name.as_str()).collect();
    let members = order.iter().position(|name| *name == "members");
    let speaking = order.iter().position(|name| *name == "speaking");
    assert!(members.is_some() && members < speaking, "{order:?}");
}
