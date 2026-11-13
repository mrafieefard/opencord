//! Named modals (§11.4, §12.5), through `start` and the batches its sink
//! gets.

use opencord_plugins::view::{ModalRoot, ModalSize, Patch};
use serde_json::{Value as Json, json};

use crate::views::test_support::{event, input, open, settle, spec};

/// `edit` shows its props' title and the state's `n`; Save closes the top
/// modal with `{ saved = n }`. Open opens it, Ask asks it and sends what
/// came back as `result`; `bump` changes the state.
const EDIT: &str = r#"
function init() return { n = 1 } end
app.modals.edit = function(s, props)
    return ui.column({}, {
        ui.text({ text = props.title .. " " .. s.n }),
        ui.button({ text = "Save", on_press = function() app.modal.close({ saved = s.n }) end }),
    })
end
function render()
    return ui.column({}, {
        ui.button({ text = "Open", on_press = function() app.modal.open("edit", { title = "Edit" }) end }),
        ui.button({ text = "Ask", on_press = function()
            app.send("result", app.modal.ask("edit", { title = "Ask" }))
        end }),
    })
end
app.on("bump", function() app.set(function(s) s.n += 1 end) end)
"#;

fn edit_root(id: u64) -> ModalRoot {
    ModalRoot {
        id,
        name: "edit".into(),
        size: ModalSize::Medium,
        sheet: None,
        dismissible: true,
    }
}

// Fails if an open modal isn't drawn, is drawn without the state or its
// props, or isn't put in a `roots` op after the main tree; and if its
// options don't default to medium, not a sheet, dismissible.
#[test]
fn opening_a_named_modal_adds_a_root_drawn_with_the_state() {
    let (view, recorder) = open(spec(EDIT));
    let seen = recorder.first_batch();
    let (main, modals) = seen.roots();
    assert!(modals.is_empty());
    event(&view, seen.node("button", "Open"), "press", json!({}));
    let seen = settle(&view, &recorder);
    assert_eq!(seen.batches.len(), 2, "{:#?}", seen.batches);
    let text = seen.node("text", "Edit 1");
    let column = *seen.batches[1]
        .ops
        .iter()
        .find_map(|op| match op {
            Patch::Create {
                id,
                block,
                children,
                ..
            } if block == "column" && children.contains(&text) => Some(id),
            _ => None,
        })
        .expect("the modal's column");
    assert_eq!(seen.roots(), (main, vec![edit_root(column)]));
}

// Fails if a modal is drawn once and not again on a redraw, or if every
// redraw sends `roots` though the stack didn't change.
#[test]
fn a_modal_redraws_when_the_state_changes() {
    let (view, recorder) = open(spec(EDIT));
    let seen = recorder.first_batch();
    event(&view, seen.node("button", "Open"), "press", json!({}));
    let seen = settle(&view, &recorder);
    let text = seen.node("text", "Edit 1");
    view.server_message("bump".into(), "null".into());
    let seen = settle(&view, &recorder);
    assert_eq!(seen.batches.len(), 3, "{:#?}", seen.batches);
    assert_eq!(
        seen.batches[2].ops,
        [Patch::Props {
            id: text,
            set: [("text".to_owned(), json!("Edit 2"))].into(),
            unset: vec![],
        }]
    );
}

// Fails if `ask` doesn't wait for the modal, loses what `close` gave, or
// if closing leaves the modal's nodes with the renderer.
#[test]
fn ask_returns_what_close_gave() {
    let (view, recorder) = open(spec(EDIT));
    let seen = recorder.first_batch();
    event(&view, seen.node("button", "Ask"), "press", json!({}));
    let seen = settle(&view, &recorder);
    assert!(seen.sent("result").is_empty(), "ask didn't wait");
    let modal = seen.roots().1[0].id;
    let (text, save) = (seen.node("text", "Ask 1"), seen.node("button", "Save"));
    view.server_message("bump".into(), "null".into());
    event(&view, save, "press", json!({}));
    let seen = settle(&view, &recorder);
    assert_eq!(seen.sent("result"), [json!({ "saved": 2 })]);
    assert!(seen.roots().1.is_empty());
    let mut expected = vec![modal, text, save];
    expected.sort_unstable();
    assert_eq!(seen.dropped(), expected);
}

// Fails if a dismissed modal stays open, or if the handler asking it never
// resumes or resumes with something other than nil.
#[test]
fn dismissing_resolves_ask_with_nil() {
    let (view, recorder) = open(spec(EDIT));
    let seen = recorder.first_batch();
    event(&view, seen.node("button", "Ask"), "press", json!({}));
    let seen = settle(&view, &recorder);
    assert_eq!(seen.roots().1.len(), 1);
    input(&view, "modal_dismiss", json!({ "name": "edit" }));
    let seen = settle(&view, &recorder);
    assert_eq!(seen.sent("result"), [Json::Null]);
    assert!(seen.roots().1.is_empty());
}

// Fails if `dismissible = false` on the modal's root isn't read into its
// root, or if a dismiss closes it anyway.
#[test]
fn a_non_dismissible_modal_ignores_dismiss() {
    let script = format!(
        "{EDIT}\napp.modals.edit = function(s)\n\
             return ui.modal({{ dismissible = false, size = 'large', title = 'Busy' }}, {{\n\
                 ui.button({{ text = 'Save', on_press = function()\n\
                     app.modal.close({{ saved = s.n }})\n\
                 end }}),\n\
             }})\n\
         end"
    );
    let (view, recorder) = open(spec(&script));
    let seen = recorder.first_batch();
    event(&view, seen.node("button", "Ask"), "press", json!({}));
    let seen = settle(&view, &recorder);
    let root = seen.roots().1[0].clone();
    assert_eq!(seen.first("modal"), root.id);
    assert_eq!(
        (root.size, root.sheet, root.dismissible),
        (ModalSize::Large, None, false)
    );
    let batches = seen.batches.len();
    input(&view, "modal_dismiss", json!({ "name": "edit" }));
    let seen = settle(&view, &recorder);
    assert_eq!(seen.batches.len(), batches, "{:#?}", seen.batches);
    assert!(seen.sent("result").is_empty());
    event(&view, seen.node("button", "Save"), "press", json!({}));
    let seen = settle(&view, &recorder);
    assert_eq!(seen.sent("result"), [json!({ "saved": 1 })]);
}

// Fails if opening an open name stacks a second copy, keeps it where it
// was, keeps its old props, or draws it from scratch.
#[test]
fn opening_an_open_name_moves_it_to_the_top_with_the_new_props() {
    let script = format!(
        "{EDIT}\napp.modals.other = function() return ui.text({{ text = 'Other' }}) end\n\
         app.on('stack', function()\n\
             app.modal.open('edit', {{ title = 'First' }})\n\
             app.modal.open('other')\n\
             app.send('open', {{ app.modal.is_open('edit'), app.modal.is_open('nope') }})\n\
         end)\n\
         app.on('again', function() app.modal.open('edit', {{ title = 'Again' }}) end)"
    );
    let (view, recorder) = open(spec(&script));
    recorder.first_batch();
    view.server_message("stack".into(), "null".into());
    let seen = settle(&view, &recorder);
    assert_eq!(seen.sent("open"), [json!([true, false])]);
    let (main, modals) = seen.roots();
    let names: Vec<&str> = modals.iter().map(|modal| modal.name.as_str()).collect();
    assert_eq!(names, ["edit", "other"]);
    let text = seen.node("text", "First 1");
    view.server_message("again".into(), "null".into());
    let seen = settle(&view, &recorder);
    let (again, moved) = seen.roots();
    assert_eq!(again, main);
    let names: Vec<&str> = moved.iter().map(|modal| modal.name.as_str()).collect();
    assert_eq!(names, ["other", "edit"]);
    assert_eq!(moved[1].id, modals[0].id);
    assert_eq!(seen.node("text", "First 1"), text);
    let last = seen.batches.last().unwrap();
    assert!(
        last.ops.contains(&Patch::Props {
            id: text,
            set: [("text".to_owned(), json!("Again 1"))].into(),
            unset: vec![],
        }),
        "{:#?}",
        last.ops
    );
}

// Fails if the modal calls take a name `app.modals` doesn't have, a bad
// name, a wait outside a handler, or a change while render draws.
#[test]
fn modal_calls_check_where_they_run_and_what_they_get() {
    let (_view, recorder) = open(spec(
        r#"
        local ok_missing, missing = pcall(app.modal.open, "nope")
        local ok_bad, bad = pcall(app.modal.open, "Bad Name")
        app.modals.edit = function() return ui.text({ text = "x" }) end
        local ok_ask, ask = pcall(app.modal.ask, "edit")
        app.send("result", { missing = missing, bad = bad, ask = ask,
            open = app.modal.is_open("edit"), any = ok_missing or ok_bad or ok_ask })
        function init() return {} end
        function render()
            app.modal.close_all()
            return ui.text({ text = "never" })
        end
        "#,
    ));
    let seen = recorder.first_batch();
    assert_eq!(
        seen.sent("result"),
        [json!({
            "missing": "INVALID_ARGUMENT: app.modals has no function called nope",
            "bad": "INVALID_ARGUMENT: a modal name is 1 to 64 characters of a-z, 0-9 and _ . : -",
            "ask": "FORBIDDEN: app.modal.ask works only in a handler",
            "open": false,
            "any": false,
        })]
    );
    assert!(
        seen.errors[0].contains("FORBIDDEN: app.modal.close_all can't run while render draws"),
        "{:?}",
        seen.errors
    );
}

// Fails if a modal root's sheet option does not distinguish unset (None),
// explicit sheet (Some(true)), and explicit modal card (Some(false)).
#[test]
fn modal_roots_reflect_sheet_option() {
    let script = r#"
    function init() return {} end
    app.modals.sheet_root = function() return ui.sheet({}, { ui.text({ text = "sheet" }) }) end
    app.modals.modal_root = function() return ui.modal({ sheet = false }, { ui.text({ text = "card" }) }) end
    app.modals.plain_root = function() return ui.box({}, { ui.text({ text = "plain" }) }) end
    app.on("open_all", function()
        app.modal.open("sheet_root")
        app.modal.open("modal_root")
        app.modal.open("plain_root")
    end)
    function render() return ui.text({ text = "root" }) end
    "#;
    let (view, recorder) = open(spec(script));
    recorder.first_batch();
    view.server_message("open_all".into(), "null".into());
    let seen = settle(&view, &recorder);
    let (_, modals) = seen.roots();
    assert_eq!(modals.len(), 3);
    assert_eq!(modals[0].name, "sheet_root");
    assert_eq!(modals[0].sheet, Some(true));
    assert_eq!(modals[1].name, "modal_root");
    assert_eq!(modals[1].sheet, Some(false));
    assert_eq!(modals[2].name, "plain_root");
    assert_eq!(modals[2].sheet, None);
}
