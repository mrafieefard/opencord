//! `app.on_back` (§11.4), through `start` and the batches its sink gets.

use opencord_plugins::view::Patch;
use serde_json::json;

use crate::views::ViewHandle;
use crate::views::test_support::{Recorder, Seen, input, open, settle, spec};

fn back(view: &ViewHandle, id: u64) {
    input(view, "back", json!({ "id": id }));
}

/// Waits for the answer to back `id`.
fn answer(recorder: &Recorder, id: u64) -> bool {
    let seen = recorder.wait("the back answer", |seen| {
        seen.backs().iter().any(|(answered, _)| *answered == id)
    });
    let backs = seen.backs();
    let answers: Vec<_> = backs
        .iter()
        .filter(|(answered, _)| *answered == id)
        .collect();
    assert_eq!(answers.len(), 1, "one answer per back: {backs:?}");
    answers[0].1
}

const PLAIN: &str = r#"
function init() return {} end
function render() return ui.text({ text = "list" }) end
"#;

// Fails if a back the plugin has no handler for goes unanswered (the phone
// would wait out its timeout on every back), or is answered handled, or if a
// `back_handler` op is sent without `app.on_back`.
#[test]
fn back_with_no_handler_is_answered_not_handled() {
    let (view, recorder) = open(spec(PLAIN));
    let first = recorder.first_batch();
    assert!(first.back_handlers().is_empty(), "{first:#?}");
    back(&view, 1);
    assert!(!answer(&recorder, 1));
}

const DETAIL: &str = r#"
function init() return { detail = true } end
function render(state)
    return ui.text({ text = if state.detail then "detail" else "list" })
end
app.on_back(function()
    local was = false
    app.set(function(state) was = state.detail; state.detail = false end)
    return was
end)
"#;

// Fails if `app.on_back` doesn't tell the renderer once, or if the
// handler's `true` isn't answered handled (detail → list), or its `false`
// not handled.
#[test]
fn on_back_tells_the_renderer_and_its_answer_comes_back() {
    let (view, recorder) = open(spec(DETAIL));
    assert_eq!(recorder.first_batch().back_handlers(), [true]);
    back(&view, 1);
    assert!(answer(&recorder, 1));
    let seen = settle(&view, &recorder);
    let to_list = seen.ops().any(
        |op| matches!(op, Patch::Props { set, .. } if set.get("text") == Some(&json!("list"))),
    );
    assert!(to_list, "the handler's app.set redraws: {seen:#?}");
    back(&view, 2);
    assert!(!answer(&recorder, 2));
    assert_eq!(settle(&view, &recorder).back_handlers(), [true]);
}

// Fails if a handler that raises, or returns something other than `true`,
// leaves the back unanswered or answers it handled.
#[test]
fn a_handler_that_raises_or_returns_no_true_is_not_handled() {
    for handler in ["error('boom')", "return 1", "return nil", "return 'yes'"] {
        let (view, recorder) = open(spec(&format!(
            "{PLAIN}\napp.on_back(function() {handler} end)"
        )));
        recorder.first_batch();
        back(&view, 5);
        assert!(!answer(&recorder, 5), "{handler}");
    }
}

// Fails if a handler over its budget leaves the back unanswered.
#[test]
fn a_handler_over_its_budget_is_not_handled() {
    let (view, recorder) = open(spec(&format!(
        "{PLAIN}\napp.on_back(function() while true do end end)"
    )));
    recorder.first_batch();
    back(&view, 3);
    assert!(!answer(&recorder, 3));
}

const BREAKS: &str = r#"
function init() return { broken = false } end
function render(state)
    if state.broken then error("render broke") end
    return ui.text({ text = "list" })
end
app.on_back(function() return true end)
app.on("break", function() app.set(function(state) state.broken = true end) end)
"#;

// Fails if a stopped view leaves a back unanswered (no Lua runs), or the
// renderer isn't told its handler is gone.
#[test]
fn a_stopped_view_answers_back_not_handled() {
    let (view, recorder) = open(spec(BREAKS));
    recorder.first_batch();
    view.server_message("break".into(), "null".into());
    recorder.wait("the error", |seen: &Seen| {
        seen.ops().any(|op| matches!(op, Patch::Error { .. }))
    });
    back(&view, 4);
    assert!(!answer(&recorder, 4));
    // A stopped view answers no ping, so no `settle`.
    let seen = recorder.wait("the answer", |seen| !seen.backs().is_empty());
    assert_eq!(seen.back_handlers(), [true, false]);
}
