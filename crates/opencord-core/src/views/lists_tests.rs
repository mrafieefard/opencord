//! Lazy lists (§12.2), through `start` and the batches its sink gets.

use opencord_plugins::view::Patch;
use serde_json::json;

use crate::views::test_support::{input, open, settle, spec};

/// A column holding, while `show`, a list of `count` texts "Item i";
/// `toggle` hides or shows it.
fn script(count: u32, reverse: bool) -> String {
    format!(
        r#"
        function init() return {{ show = true }} end
        function render(s)
            local children = {{}}
            if s.show then
                children[1] = ui.list({{ count = {count}, reverse = {reverse},
                    item = function(i) return ui.text({{ text = "Item " .. i }}) end }})
            end
            return ui.column({{}}, children)
        end
        app.on("toggle", function() app.set(function(s) s.show = not s.show end) end)
        "#
    )
}

fn keys(range: std::ops::Range<usize>) -> Vec<String> {
    range.map(|index| format!("#{index}")).collect()
}

// Fails if a list draws more than items 0–29 at first, calls `item` with
// 1-based indexes, sends `item` as one of its events, or doesn't redraw
// the range a `range` input asks for (end not included).
#[test]
fn a_list_renders_only_its_range() {
    let (view, recorder) = open(spec(&script(1000, false)));
    let seen = recorder.first_batch();
    let list = seen.first("list");
    assert_eq!(seen.child_keys(list), keys(0..30));
    seen.node("text", "Item 0");
    let created = seen.batches[0].ops.iter().find_map(|op| match op {
        Patch::Create { id, events, .. } if *id == list => Some(events.clone()),
        _ => None,
    });
    assert_eq!(created, Some(vec![]));
    let first = seen.children(list);

    input(
        &view,
        "range",
        json!({ "node": list, "start": 100, "end": 110 }),
    );
    let seen = settle(&view, &recorder);
    assert_eq!(seen.child_keys(list), keys(100..110));
    seen.node("text", "Item 109");
    let mut dropped = seen.dropped();
    dropped.sort_unstable();
    assert_eq!(dropped, first);
}

// Fails if a range over 200 items isn't cut to 200.
#[test]
fn a_range_over_200_is_cut() {
    let (view, recorder) = open(spec(&script(1000, false)));
    let list = recorder.first_batch().first("list");
    input(
        &view,
        "range",
        json!({ "node": list, "start": 5, "end": 900 }),
    );
    let seen = settle(&view, &recorder);
    assert_eq!(seen.child_keys(list), keys(5..205));
}

// Fails if a reversed list starts at its first items, or if a range
// passing `count` draws items that don't exist.
#[test]
fn reverse_starts_at_the_end() {
    let (view, recorder) = open(spec(&script(100, true)));
    let seen = recorder.first_batch();
    let list = seen.first("list");
    assert_eq!(seen.child_keys(list), keys(70..100));
    input(
        &view,
        "range",
        json!({ "node": list, "start": 90, "end": 150 }),
    );
    let seen = settle(&view, &recorder);
    assert_eq!(seen.child_keys(list), keys(90..100));
}

// Fails if a list's range outlives its node: the list drawn again would
// start where the old one was.
#[test]
fn a_dropped_list_forgets_its_range() {
    let (view, recorder) = open(spec(&script(1000, false)));
    let old = recorder.first_batch().first("list");
    input(
        &view,
        "range",
        json!({ "node": old, "start": 50, "end": 60 }),
    );
    view.server_message("toggle".into(), "null".into());
    let seen = settle(&view, &recorder);
    assert!(seen.dropped().contains(&old));
    view.server_message("toggle".into(), "null".into());
    let seen = settle(&view, &recorder);
    let list = seen
        .batches
        .last()
        .unwrap()
        .ops
        .iter()
        .find_map(|op| match op {
            Patch::Create { id, block, .. } if block == "list" => Some(*id),
            _ => None,
        })
        .expect("the list again");
    assert_ne!(list, old);
    assert_eq!(seen.child_keys(list), keys(0..30));
    // A range for the old node changes nothing now.
    let batches = seen.batches.len();
    input(
        &view,
        "range",
        json!({ "node": old, "start": 50, "end": 60 }),
    );
    assert_eq!(settle(&view, &recorder).batches.len(), batches);
}

/// A list whose `item(i)` runs `body`, under a column, shown with errors.
fn failing(body: &str) -> String {
    format!(
        r#"
        function init() return {{}} end
        function render()
            return ui.column({{}}, {{ ui.list({{ count = 50, item = function(i)
                {body}
            end }}) }})
        end
        "#
    )
}

// Fails if an error in `item`, or in the tree it returns, isn't a render
// error naming the list and the item.
#[test]
fn an_error_in_item_is_a_render_error_with_the_lists_path() {
    let mut lua_error = spec(&failing(
        "if i == 3 then error('boom') end return ui.text({ text = 'x' })",
    ));
    lua_error.show_errors = true;
    let (_view, recorder) = open(lua_error);
    let seen = recorder.first_batch();
    let details = seen.error_op().1.expect("details");
    assert!(
        details.starts_with("column > [0] list: item(3): client/main.luau:5: boom"),
        "{details}"
    );

    let mut tree_error = spec(&failing(
        "if i == 2 then return ui.text({ colour = 'red' }) end return ui.text({ text = 'x' })",
    ));
    tree_error.show_errors = true;
    let (_view, recorder) = open(tree_error);
    let seen = recorder.first_batch();
    assert_eq!(
        seen.error_op().1.as_deref(),
        Some("column > [0] list > [2] text: unknown prop \"colour\"")
    );
}

// Fails if items are checked as trees of their own: thirty items of 701
// blocks would then pass the 20,000-block limit; or if an item's own key
// is replaced, or two equal keys are let through.
#[test]
fn items_count_toward_the_tree_and_keep_their_keys() {
    let mut big = spec(&format!(
        "local spacers = {{}} for n = 1, 700 do spacers[n] = ui.spacer({{}}) end\n{}",
        failing("return ui.column({}, spacers)")
    ));
    big.show_errors = true;
    let (_view, recorder) = open(big);
    let details = recorder.first_batch().error_op().1.expect("details");
    assert!(
        details.starts_with("column > [0] list > [")
            && details.ends_with("spacer: the tree has more than 20000 blocks"),
        "{details}"
    );

    let (_view, recorder) = open(spec(&failing(
        "return ui.text({ key = 'k' .. (i % 40), text = 'x' })",
    )));
    let seen = recorder.first_batch();
    let list = seen.first("list");
    let own: Vec<String> = (0..30).map(|index| format!("k{index}")).collect();
    assert_eq!(seen.child_keys(list), own);

    let mut repeated = spec(&failing("return ui.text({ key = 'same', text = 'x' })"));
    repeated.show_errors = true;
    let (_view, recorder) = open(repeated);
    assert_eq!(
        recorder.first_batch().error_op().1.as_deref(),
        Some("column > [0] list: two children have the key \"same\"")
    );
}

// Fails if a list does not include its 0-based range start in its node props,
// or does not update `start` when a `range` input moves the window.
#[test]
fn a_list_sends_where_its_range_starts() {
    let (view, recorder) = open(spec(&script(100, false)));
    let seen = recorder.first_batch();
    let list = seen.first("list");
    let start = seen.batches[0].ops.iter().find_map(|op| match op {
        Patch::Create { id, props, .. } if *id == list => props.get("start").cloned(),
        _ => None,
    });
    assert_eq!(start, Some(json!(0)));

    input(
        &view,
        "range",
        json!({ "node": list, "start": 40, "end": 60 }),
    );
    let seen = settle(&view, &recorder);
    let start_prop = seen
        .batches
        .last()
        .unwrap()
        .ops
        .iter()
        .find_map(|op| match op {
            Patch::Props { id, set, .. } if *id == list => set.get("start").cloned(),
            Patch::Create { id, props, .. } if *id == list => props.get("start").cloned(),
            _ => None,
        });
    assert_eq!(start_prop, Some(json!(40)));
}
