//! Quick dialogs (§11.4), through `start` and the batches its sink gets.

use opencord_plugins::view::DialogKind;
use serde_json::{Value as Json, json};

use crate::views::test_support::{MAIN, PING, event, input, open, settle, spec};

const DELETE: &str = r#"
function init() return {} end
function render()
    return ui.column({}, {
        ui.button({ text = "Delete", on_press = function()
            app.send("asking")
            app.send("answered", app.confirm("Delete it?"))
        end }),
        ui.button({ text = "Rename", on_press = function()
            app.send("renamed", app.prompt("New name?", "Ada"))
        end }),
        ui.button({ text = "Other", on_press = function() app.send("other") end }),
    })
end
"#;

// Fails if `confirm` returns before the answer, blocks the view's other
// inputs while it waits, sends no `dialog` op, or reads an answer other
// than `true` as yes.
#[test]
fn confirm_pauses_the_handler_until_answered() {
    let (view, recorder) = open(spec(DELETE));
    let seen = recorder.first_batch();
    let (delete, other) = (seen.node("button", "Delete"), seen.node("button", "Other"));
    event(&view, delete, "press", json!({}));
    event(&view, other, "press", json!({}));
    let seen = settle(&view, &recorder);
    assert_eq!(seen.sent("asking").len(), 1);
    assert_eq!(
        seen.sent("other").len(),
        1,
        "the paused handler held up the view"
    );
    assert!(seen.sent("answered").is_empty(), "confirm didn't wait");
    let (first, kind, text, default) = seen.dialog();
    assert_eq!(
        (kind, text.as_str(), default),
        (DialogKind::Confirm, "Delete it?", None)
    );

    input(&view, "dialog", json!({ "id": first, "result": true }));
    let seen = settle(&view, &recorder);
    assert_eq!(seen.sent("answered"), [json!(true)]);

    event(&view, delete, "press", json!({}));
    let seen = settle(&view, &recorder);
    let (second, ..) = seen.dialog();
    assert_ne!(second, first);
    input(&view, "dialog", json!({ "id": second, "result": "yes" }));
    let seen = settle(&view, &recorder);
    assert_eq!(seen.sent("answered"), [json!(true), json!(false)]);
}

// Fails if an answer to a dialog that already closed (answered, or its
// handler ended by a reload) runs anything, or if a prompt loses its
// default or its answer.
#[test]
fn an_answer_after_close_is_dropped() {
    let (view, recorder) = open(spec(DELETE));
    let seen = recorder.first_batch();
    let rename = seen.node("button", "Rename");
    event(&view, rename, "press", json!({}));
    let seen = settle(&view, &recorder);
    let (id, kind, _, default) = seen.dialog();
    assert_eq!(
        (kind, default.as_deref()),
        (DialogKind::Prompt, Some("Ada"))
    );
    input(&view, "dialog", json!({ "id": id, "result": "Grace" }));
    input(&view, "dialog", json!({ "id": id, "result": "Again" }));
    let seen = settle(&view, &recorder);
    assert_eq!(seen.sent("renamed"), [json!("Grace")]);

    event(&view, rename, "press", json!({}));
    let seen = settle(&view, &recorder);
    let (waiting, ..) = seen.dialog();
    view.reload(
        MAIN.into(),
        [(MAIN.to_owned(), format!("{DELETE}\n{PING}"))].into(),
    );
    input(&view, "dialog", json!({ "id": waiting, "result": "Late" }));
    let seen = settle(&view, &recorder);
    assert_eq!(seen.sent("renamed"), [json!("Grace")]);
    assert!(seen.errors.is_empty(), "{:?}", seen.errors);
}

// Fails if dialog text isn't limited to 2,000 characters or checked for
// being a string, or if a dialog may wait outside a handler.
#[test]
fn dialogs_check_their_text_and_where_they_run() {
    let (view, recorder) = open(spec(
        r#"
        local ok_start, at_start = pcall(app.alert, "hi")
        app.send("start", { ok = ok_start, why = at_start })
        app.on("try", function()
            local long_ok, long = pcall(app.confirm, string.rep("x", 2001))
            local number_ok, number = pcall(app.prompt, "Name?", 5)
            local table_ok, table_why = pcall(app.alert, {})
            app.send("checks", { long = long, number = number, table = table_why,
                any = long_ok or number_ok or table_ok })
            app.alert(string.rep("y", 2000))
            app.send("alerted", "done")
        end)
        function init() return {} end
        function render() return ui.text({ text = "ok" }) end
        "#,
    ));
    let seen = recorder.first_batch();
    assert_eq!(
        seen.sent("start"),
        [json!({ "ok": false, "why": "FORBIDDEN: app.alert works only in a handler" })]
    );
    view.server_message("try".into(), "null".into());
    let seen = settle(&view, &recorder);
    assert_eq!(
        seen.sent("checks"),
        [json!({
            "long": "INVALID_ARGUMENT: app.confirm's text is at most 2000 characters",
            "number": "INVALID_ARGUMENT: app.prompt's default must be a string",
            "table": "INVALID_ARGUMENT: app.alert's text must be a string",
            "any": false,
        })]
    );
    let (id, kind, text, _) = seen.dialog();
    assert_eq!((kind, text.chars().count()), (DialogKind::Alert, 2000));
    input(&view, "dialog", json!({ "id": id, "result": Json::Null }));
    let seen = settle(&view, &recorder);
    assert_eq!(seen.sent("alerted"), [json!("done")]);
}
