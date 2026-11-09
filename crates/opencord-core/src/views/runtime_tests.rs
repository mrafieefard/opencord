//! The client VM's tests, through `start` and the batches its sink gets.

use std::sync::Arc;
use std::time::{Duration, Instant};

use opencord_plugins::view::{Batch, Patch};
use serde_json::{Value as Json, json};

use crate::views::test_support::{MAIN, PING, event, open, settle, spec};

const COUNTER: &str = r#"
function init() return { count = 0 } end
function render(s)
    return ui.column({}, {
        ui.text({ text = "Clicks: " .. s.count }),
        ui.button({ text = "Click", on_press = function()
            app.set(function(s) s.count += 1 end)
            app.set(function(s) s.count += 1 end)
        end }),
    })
end
"#;

// Fails if the first tree waits for an input, if it has no `roots`, or if
// `init` doesn't get the viewer, place, size and platform.
#[test]
fn init_then_render_sends_the_first_tree() {
    let (_view, recorder) = open(spec(
        r#"
        function init(ctx)
            return { line = ctx.viewer.name .. " in " .. ctx.place.channel_id
                .. " on " .. ctx.platform .. " at " .. ctx.size.width .. " " .. ctx.theme }
        end
        function render(s)
            return ui.column({ gap = 8 }, { ui.text({ text = s.line }) })
        end
        "#,
    ));
    let seen = recorder.wait("the first batch", |seen| !seen.batches.is_empty());
    let first = &seen.batches[0];
    assert_eq!((first.view.as_str(), first.seq), ("7", 1));
    let text = seen.node("text", "Ada in 42 on desktop at 800 dark");
    let Some(Patch::Create {
        id: column,
        block,
        children,
        ..
    }) = first.ops.get(1)
    else {
        panic!("{:#?}", first.ops);
    };
    assert_eq!(
        (block.as_str(), children.as_slice()),
        ("column", &[text][..])
    );
    assert_eq!(
        first.ops[2],
        Patch::Roots {
            main: *column,
            modals: vec![]
        }
    );
    assert_eq!(first.ops.len(), 3);
}

// Fails if each `app.set` redraws (three batches), or if `app.set` doesn't
// mark the view dirty (one batch).
#[test]
fn set_in_a_handler_redraws_once() {
    let (view, recorder) = open(spec(COUNTER));
    let seen = recorder.wait("the first batch", |seen| !seen.batches.is_empty());
    let text = seen.node("text", "Clicks: 0");
    event(&view, seen.node("button", "Click"), "press", json!({}));
    let seen = settle(&view, &recorder);
    assert_eq!(seen.batches.len(), 2, "{:#?}", seen.batches);
    assert_eq!(seen.batches[1].seq, 2);
    assert_eq!(
        seen.batches[1].ops,
        [Patch::Props {
            id: text,
            set: [("text".to_owned(), json!("Clicks: 2"))].into(),
            unset: vec![],
        }]
    );
}

// Fails if handlers are found by event alone (the first button's long
// press would run the second's) or by node alone, or if event data
// doesn't reach the handler.
#[test]
fn a_press_reaches_its_handler_by_node_and_event() {
    let (view, recorder) = open(spec(
        r#"
        function init() return {} end
        function render()
            return ui.column({}, {
                ui.button({ text = "A", on_press = function() app.send("a") end }),
                ui.button({ text = "B", on_press = function() app.send("b") end,
                    on_long_press = function() app.send("b_long") end }),
                ui.text_input({ value = "", on_change = function(e) app.send("typed", e.value) end }),
            })
        end
        "#,
    ));
    let seen = recorder.wait("the first batch", |seen| !seen.batches.is_empty());
    let (a, b) = (seen.node("button", "A"), seen.node("button", "B"));
    let input = seen.first("text_input");
    event(&view, a, "long_press", json!({}));
    event(&view, b, "press", json!({}));
    event(&view, b, "long_press", json!({}));
    event(&view, input + 99, "press", json!({}));
    event(&view, input, "change", json!({ "value": "hi" }));
    let seen = settle(&view, &recorder);
    let names: Vec<&str> = seen.sends.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, ["b", "b_long", "typed", "pong"]);
    assert_eq!(seen.sent("typed"), [json!("hi")]);
}

// Fails if a redraw keeps the last tree's functions: the second press
// would send 0 again, and the third would still run.
#[test]
fn a_handler_from_an_old_redraw_is_gone() {
    let (view, recorder) = open(spec(
        r#"
        function init() return { n = 0 } end
        function render(s)
            local n = s.n
            local props = { text = "Go" }
            if n < 2 then
                props.on_press = function()
                    app.send("pressed", n)
                    app.set(function(s) s.n += 1 end)
                end
            end
            return ui.column({}, { ui.button(props) })
        end
        "#,
    ));
    let seen = recorder.wait("the first batch", |seen| !seen.batches.is_empty());
    let button = seen.node("button", "Go");
    for _ in 0..3 {
        event(&view, button, "press", json!({}));
    }
    let seen = settle(&view, &recorder);
    assert_eq!(seen.sent("pressed"), [json!(0), json!(1)]);
    let last = seen.batches.last().unwrap();
    assert!(
        last.ops.contains(&Patch::Events {
            id: button,
            events: vec![]
        }),
        "{:#?}",
        last.ops
    );
}

const ENDLESS: &str = r#"
function init() return {} end
function render() while true do end end
"#;

// Fails if render runs without a budget (the error never comes), or if the
// view runs on a shared thread (the other view stops answering).
#[test]
fn an_endless_render_shows_the_error_and_the_app_goes_on() {
    let began = Instant::now();
    let (_stuck, stuck) = open(spec(ENDLESS));
    let (other, answering) = open(spec(COUNTER));
    let seen = stuck.wait("the error", |seen| !seen.batches.is_empty());
    assert!(
        began.elapsed() < Duration::from_secs(1),
        "{:?}",
        began.elapsed()
    );
    assert_eq!(seen.error_op(), (ERROR.to_owned(), None));
    assert!(
        seen.errors[0].starts_with("render: the handler ran longer than 50 ms"),
        "{:?}",
        seen.errors
    );
    settle(&other, &answering);
}

const ERROR: &str = "This plugin hit an error";

// Fails if the VM has no memory cap: the budget stops the bomb instead, a
// gigabyte later. Strings of 16 MiB reach the 32 MiB cap in two steps,
// well within the 50 ms budget even in a debug build on a busy machine.
#[test]
fn a_memory_bomb_stops_the_view_only() {
    let (bomb, bombed) = open(spec(
        r#"
        function init() return {} end
        function render()
            local t = {}
            while true do table.insert(t, string.rep("x", 16777216) .. #t) end
        end
        "#,
    ));
    let (other, answering) = open(spec(COUNTER));
    let seen = bombed.wait("the error", |seen| !seen.batches.is_empty());
    assert_eq!(seen.error_op().0, ERROR);
    assert_eq!(seen.errors, ["render: the plugin ran out of memory"]);
    settle(&other, &answering);
    bomb.close();
}

// Fails if the view's VM gets any of the server's API, or Luau's own
// file, process or loading functions.
#[test]
fn the_view_has_no_network_files_or_server_api() {
    let (_view, recorder) = open(spec(
        r#"
        local present = {}
        local env = { http = http, db = db, files = files, hooks = hooks, timer = timer,
            messages = messages, commands = commands, guild = guild, voice = voice, bus = bus,
            config = config, plugin = plugin, log = log, server = server, channels = channels,
            members = members, roles = roles, music = music, soundboard = soundboard, io = io,
            debug = debug, load = load, loadstring = loadstring, dofile = dofile,
            getfenv = getfenv, setfenv = setfenv, execute = os.execute, getenv = os.getenv }
        for name in env do table.insert(present, name) end
        table.sort(present)
        app.send("present", present)
        function init() return {} end
        function render() return ui.text({ text = "ok" }) end
        "#,
    ));
    let seen = recorder.wait("the check", |seen| !seen.sent("present").is_empty());
    assert_eq!(seen.sent("present"), [json!({})]);
}

// Fails if `require` reads outside `client/`, or loads a file twice.
#[test]
fn require_loads_client_files_only() {
    let mut spec = spec(
        r#"
        local util = require("lib/util")
        local ok_up, up = pcall(require, "../server/main")
        local ok_in, inside = pcall(require, "server/main")
        app.send("result", { same = util == require("lib/util"), loads = util.loads,
            up = ok_up, up_error = up, inside = ok_in, inside_error = inside })
        function init() return {} end
        function render() return ui.text({ text = "ok" }) end
        "#,
    );
    spec.sources.insert(
        "client/lib/util.luau".into(),
        "loads = (loads or 0) + 1 return { loads = loads }".into(),
    );
    spec.sources
        .insert("server/main.luau".into(), "return { secret = true }".into());
    let (_view, recorder) = open(spec);
    let seen = recorder.wait("the result", |seen| !seen.sent("result").is_empty());
    assert_eq!(
        seen.sent("result"),
        [json!({
            "same": true,
            "loads": 1,
            "up": false,
            "up_error": "INVALID_ARGUMENT: require takes a module name such as \"util\" or \"lib/util\"",
            "inside": false,
            "inside_error": "NOT_FOUND: the plugin has no client/server/main.luau or client/server/main.lua",
        })]
    );
}

// Fails if the 64 KiB check is missing or off by one, or if a bad name is
// let through.
#[test]
fn send_over_64_kib_raises() {
    let (_view, recorder) = open(spec(
        r#"
        local big_ok, big = pcall(app.send, "big", string.rep("x", 65535))
        local edge_ok = pcall(app.send, "edge", string.rep("x", 65534))
        local name_ok, name = pcall(app.send, "Bad Name", 1)
        app.send("result", { big = big_ok, big_error = big, edge = edge_ok,
            name = name_ok, name_error = name })
        function init() return {} end
        function render() return ui.text({ text = "ok" }) end
        "#,
    ));
    let seen = recorder.wait("the result", |seen| !seen.sent("result").is_empty());
    assert!(seen.sent("big").is_empty());
    assert_eq!(seen.sent("edge")[0].as_str().map(str::len), Some(65534));
    assert_eq!(
        seen.sent("result"),
        [json!({
            "big": false,
            "big_error": "PLUGIN_LIMIT: an event is at most 64 KiB",
            "edge": true,
            "name": false,
            "name_error": "INVALID_ARGUMENT: an event name is 1 to 64 characters of a-z, 0-9 and _ . : -",
        })]
    );
}

// Fails if `app.copy` works outside a press (from start or a server event),
// or if a copy inside one isn't sent as a `copy` op.
#[test]
fn copy_outside_a_press_raises_and_inside_sends_a_copy_op() {
    let (view, recorder) = open(spec(
        r#"
        local ok, why = pcall(app.copy, "early")
        app.send("at_start", { ok = ok, why = why })
        app.on("try", function()
            local ok = pcall(app.copy, "late")
            app.send("from_server", ok)
        end)
        function init() return {} end
        function render()
            return ui.button({ text = "Copy", on_press = function() app.copy("hello") end })
        end
        "#,
    ));
    let seen = recorder.wait("the first batch", |seen| !seen.batches.is_empty());
    assert_eq!(
        seen.sent("at_start"),
        [json!({ "ok": false, "why": "FORBIDDEN: app.copy works only while handling a press" })]
    );
    view.server_message("try".into(), "null".into());
    event(&view, seen.node("button", "Copy"), "press", json!({}));
    let seen = settle(&view, &recorder);
    assert_eq!(seen.sent("from_server"), [json!(false)]);
    assert_eq!(
        seen.batches[1],
        Batch {
            view: "7".into(),
            seq: 2,
            ops: vec![Patch::Copy {
                text: "hello".into()
            }],
        }
    );
    assert_eq!(seen.batches.len(), 2);
}

// Fails if `details` is sent to a viewer not allowed it, or isn't sent to
// one who is; and if the report misses where it happened.
#[test]
fn errors_hide_their_details_unless_allowed() {
    let script = "function init() return {} end\nfunction render() error('boom') end";
    let (_hidden, hidden) = open(spec(script));
    let seen = hidden.wait("the error", |seen| !seen.batches.is_empty());
    assert_eq!(seen.error_op(), (ERROR.to_owned(), None));
    assert!(
        seen.errors[0].starts_with("render: client/main.luau:2: boom\n"),
        "{:?}",
        seen.errors
    );

    let mut allowed = spec(script);
    allowed.show_errors = true;
    let (_shown, shown) = open(allowed);
    let seen = shown.wait("the error", |seen| !seen.batches.is_empty());
    let details = seen.error_op().1.expect("details for a developer");
    assert!(
        details.starts_with("client/main.luau:2: boom\n"),
        "{details}"
    );
    assert!(details.contains("render"), "no traceback: {details}");

    let mut tree = spec(
        "function init() return {} end\nfunction render() return ui.text({ colour = 'red' }) end",
    );
    tree.show_errors = true;
    let (_tree, broken) = open(tree);
    let seen = broken.wait("the error", |seen| !seen.batches.is_empty());
    assert_eq!(
        seen.error_op().1.as_deref(),
        Some("text: unknown prop \"colour\"")
    );
}

// Fails if close skips the listeners, or if the thread outlives `close`
// (it would still hold the sink).
#[test]
fn closing_runs_close_listeners_and_ends_the_thread() {
    let (view, recorder) = open(spec(
        r#"
        app.listen("close", function() app.send("bye") end)
        function init() return {} end
        function render() return ui.text({ text = "hi" }) end
        "#,
    ));
    recorder.wait("the first batch", |seen| !seen.batches.is_empty());
    let began = Instant::now();
    view.close();
    assert!(began.elapsed() < Duration::from_secs(1));
    assert_eq!(recorder.seen.lock().unwrap().sent("bye"), [Json::Null]);
    assert_eq!(
        Arc::strong_count(&recorder),
        1,
        "the view's thread still runs"
    );
}

// Fails if an error in a handler stops the view.
#[test]
fn a_handler_error_is_reported_and_the_view_goes_on() {
    let (view, recorder) = open(spec(
        r#"
        function init() return {} end
        function render()
            return ui.column({}, {
                ui.button({ text = "Bad", on_press = function() error("oops") end }),
                ui.button({ text = "Good", on_press = function() app.send("good") end }),
            })
        end
        "#,
    ));
    let seen = recorder.wait("the first batch", |seen| !seen.batches.is_empty());
    event(&view, seen.node("button", "Bad"), "press", json!({}));
    event(&view, seen.node("button", "Good"), "press", json!({}));
    let seen = settle(&view, &recorder);
    assert_eq!(seen.sent("good"), [Json::Null]);
    assert!(
        seen.errors[0].starts_with("on_press: client/main.luau:5: oops"),
        "{:?}",
        seen.errors
    );
    assert_eq!(seen.batches.len(), 1);
}

// Fails if `render` may call `app.set`: it would ask for a render after
// every render.
#[test]
fn set_while_rendering_is_an_error() {
    let (_view, recorder) = open(spec(
        "function init() return {} end\nfunction render() app.set(function() end) end",
    ));
    let seen = recorder.wait("the error", |seen| !seen.batches.is_empty());
    assert_eq!(seen.error_op().0, ERROR);
    assert!(
        seen.errors[0].contains("FORBIDDEN: render can't call app.set"),
        "{:?}",
        seen.errors
    );
}

// Fails if a reload loses the state, skips `migrate`, keeps functions in
// the state, or redraws from scratch rather than diffing.
#[test]
fn a_reload_keeps_the_state_and_runs_migrate() {
    let first = COUNTER.replace("{ count = 0 }", "{ count = 0, f = function() end }");
    let (view, recorder) = open(spec(&first));
    let seen = recorder.wait("the first batch", |seen| !seen.batches.is_empty());
    let text = seen.node("text", "Clicks: 0");
    event(&view, seen.node("button", "Click"), "press", json!({}));
    settle(&view, &recorder);

    let script = r#"
        function migrate(old)
            return { count = old.count * 10, had_function = old.f ~= nil }
        end
        function init() return { count = -1 } end
        function render(s)
            return ui.column({}, {
                ui.text({ text = "Clicks: " .. s.count .. " " .. tostring(s.had_function) }),
                ui.button({ text = "Click" }),
            })
        end
    "#;
    view.reload(
        MAIN.into(),
        [(MAIN.to_owned(), format!("{script}\n{PING}"))].into(),
    );
    let seen = settle(&view, &recorder);
    assert_eq!(
        seen.batches[2].ops,
        [
            Patch::Props {
                id: text,
                set: [("text".to_owned(), json!("Clicks: 20 false"))].into(),
                unset: vec![],
            },
            Patch::Events {
                id: seen.node("button", "Click"),
                events: vec![],
            }
        ]
    );
}
