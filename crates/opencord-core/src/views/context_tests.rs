//! The view's surroundings (§11.4, §11.5) and side effects, through `start`
//! and the batches its sink gets.

use opencord_plugins::view::Patch;
use serde_json::{Value as Json, json};

use crate::views::test_support::{Recorder, input, open, settle, spec};
use crate::views::{ViewHandle, ViewPlatform};

const SHOW: &str = r#"
function init() return {} end
function render() return ui.text({ text = app.theme .. " " .. app.breakpoint }) end
app.listen("resize", function(size) app.send("resized", size) end)
app.listen("theme_change", function(theme) app.send("theme", theme) end)
app.on("read", function()
    app.send("context", {
        viewer = app.viewer, place = app.place, size = app.size, theme = app.theme,
        platform = app.platform, breakpoint = app.breakpoint, input = app.input,
        safe_area = app.safe_area, text_scale = app.text_scale,
    })
end)
"#;

const WHY: &str = r#"
function init() return {} end
function render() return ui.text({ text = "ok" }) end
local function why(...)
    local ok, message = pcall(...)
    return if ok then "no error" else message
end
"#;

fn context(width: f64, theme: &str) -> Json {
    json!({
        "width": width, "height": 600.0, "theme": theme, "input": "touch",
        "safe_area": { "top": 1.0, "right": 2.0, "bottom": 3.0, "left": 4.0 },
        "text_scale": 1.5,
    })
}

fn read(view: &ViewHandle, recorder: &Recorder) -> Json {
    view.server_message("read".into(), "null".into());
    let sent = settle(view, recorder).sent("context");
    sent.last().expect("a context").clone()
}

fn try_it(view: &ViewHandle) {
    view.server_message("try".into(), "null".into());
}

// Fails if the fields aren't set before the first render, or a `context`
// input doesn't update them.
#[test]
fn the_context_is_read_from_app_and_follows_its_input() {
    let (view, recorder) = open(spec(SHOW));
    recorder.first_batch();
    let first = read(&view, &recorder);
    assert_eq!(
        first["viewer"],
        json!({ "id": "10", "name": "Ada", "roles": ["3"] })
    );
    assert_eq!(
        first["place"],
        json!({ "kind": "channel", "channel_id": "42" })
    );
    assert_eq!(first["platform"], json!("desktop"));
    assert_eq!(first["theme"], json!("dark"));
    assert_eq!(first["input"], json!("mouse"));

    input(&view, "context", context(900.0, "light"));
    let next = read(&view, &recorder);
    assert_eq!(next["theme"], json!("light"));
    assert_eq!(next["input"], json!("touch"));
    assert_eq!(next["size"]["width"].as_f64(), Some(900.0));
    assert_eq!(next["safe_area"]["left"].as_f64(), Some(4.0));
    assert_eq!(next["text_scale"].as_f64(), Some(1.5));
    assert_eq!(next["breakpoint"], json!("expanded"));
}

// Fails if the breakpoints are off by one, or `resize` doesn't carry them.
#[test]
fn breakpoints_follow_the_width() {
    let (view, recorder) = open(spec(SHOW));
    recorder.first_batch();
    assert_eq!(read(&view, &recorder)["breakpoint"], json!("medium"));
    for (width, expected) in [
        (599.0, "compact"),
        (600.0, "medium"),
        (839.0, "medium"),
        (840.0, "expanded"),
    ] {
        input(&view, "context", context(width, "dark"));
        assert_eq!(read(&view, &recorder)["breakpoint"], json!(expected));
        let sent = settle(&view, &recorder).sent("resized");
        let resized = sent.last().unwrap();
        assert_eq!(resized["width"].as_f64(), Some(width));
        assert_eq!(resized["breakpoint"], json!(expected), "at {width}");
    }
}

// Fails if a context change doesn't redraw (render reads `app.theme`), or
// sends `theme_change` when only the width changed.
#[test]
fn a_context_change_redraws_and_names_what_changed() {
    let (view, recorder) = open(spec(SHOW));
    recorder.first_batch();
    input(&view, "context", context(900.0, "dark"));
    assert!(settle(&view, &recorder).sent("theme").is_empty());
    input(&view, "context", context(900.0, "light"));
    let seen = settle(&view, &recorder);
    assert_eq!(seen.sent("theme"), [json!("light")]);
    assert_eq!(seen.sent("resized").len(), 1, "the size didn't change");
    let drew = seen.batches.iter().flat_map(|batch| &batch.ops).any(|op| {
        matches!(op, Patch::Props { set, .. }
            if set.get("text") == Some(&json!("light expanded")))
    });
    assert!(drew, "{:#?}", seen.batches);
}

// Fails if a name is refused that the plan lists, or another is taken.
#[test]
fn listen_refuses_unknown_names() {
    let (view, recorder) = open(spec(&format!(
        r#"{WHY}
        local function nothing() end
        app.on("try", function()
            local names = {{ "close", "pause", "resume", "resize", "theme_change",
                "connection_change", "focus", "blur", "key_down", "key_up",
                "call_members_change", "speaking_change" }}
            local refused = {{}}
            for _, name in names do
                local answer = why(app.listen, name, nothing)
                if answer ~= "no error" then table.insert(refused, answer) end
            end
            app.send("refused", #refused)
            app.send("why", why(app.listen, "pointer_down", nothing))
        end)
        "#
    )));
    recorder.first_batch();
    try_it(&view);
    let seen = settle(&view, &recorder);
    assert_eq!(seen.sent("refused"), [json!(0)]);
    assert_eq!(
        seen.sent("why"),
        [json!("INVALID_ARGUMENT: no view event called pointer_down")]
    );
}

// Fails if a lifecycle or key input doesn't reach its listeners with the
// fields the plan gives, or a repeated pause runs them twice.
#[test]
fn listeners_hear_the_lifecycle_the_connection_and_keys() {
    let (view, recorder) = open(spec(&format!(
        r#"{WHY}
        for _, name in {{ "pause", "resume", "focus", "blur" }} do
            app.listen(name, function() app.send(name) end)
        end
        app.listen("connection_change", function(change) app.send("connection", change) end)
        app.listen("key_down", function(key) app.send("key", key) end)
        "#
    )));
    recorder.first_batch();
    for event in [
        "pause",
        "pause",
        "resume",
        "focus",
        "blur",
        "connection_lost",
        "connection_back",
    ] {
        input(&view, "lifecycle", json!({ "event": event }));
    }
    let key = json!({
        "key": "a", "code": "KeyA", "shift": true, "ctrl": false,
        "alt": false, "meta": false, "repeat": false,
    });
    let mut down = key.clone();
    down["event"] = json!("key_down");
    input(&view, "key", down);
    let seen = settle(&view, &recorder);
    assert_eq!(seen.sent("pause").len(), 1, "a pause while paused ran it");
    assert_eq!(seen.sent("resume").len(), 1);
    assert_eq!(seen.sent("focus").len(), 1);
    assert_eq!(seen.sent("blur").len(), 1);
    assert_eq!(
        seen.sent("connection"),
        [json!({ "connected": false }), json!({ "connected": true })]
    );
    assert_eq!(seen.sent("key"), [key]);
}

// Fails if a scheme other than http or https gets through, a link over
// 2,000 characters is sent, or the text sent isn't the parsed address.
#[test]
fn open_link_refuses_other_schemes() {
    const BAD: &str = "INVALID_ARGUMENT: app.open_link takes an http or https address";
    let (view, recorder) = open(spec(&format!(
        r#"{WHY}
        app.on("try", function()
            app.send("why", {{
                script = why(app.open_link, "javascript:alert(1)"),
                file = why(app.open_link, "file:///etc/passwd"),
                data = why(app.open_link, "data:text/html,x"),
                ftp = why(app.open_link, "ftp://example.com"),
                bare = why(app.open_link, "example.com"),
                number = why(app.open_link, 5),
                long = why(app.open_link, "https://a.example/" .. string.rep("x", 2000)),
            }})
            app.open_link("https://example.com/a?b=1")
            app.open_link("HTTP://EXAMPLE.com")
        end)
        "#
    )));
    recorder.first_batch();
    try_it(&view);
    let seen = settle(&view, &recorder);
    assert_eq!(
        seen.sent("why"),
        [json!({
            "script": BAD, "file": BAD, "data": BAD, "ftp": BAD, "bare": BAD, "number": BAD,
            "long": "INVALID_ARGUMENT: a link is at most 2000 characters",
        })]
    );
    assert_eq!(
        seen.links(),
        ["https://example.com/a?b=1", "http://example.com/"]
    );
}

// Fails if a sound's asset or volume isn't checked, the default volume
// isn't 1, or a channel id isn't checked as a string of digits.
#[test]
fn sounds_and_channels_check_and_send_their_ops() {
    const ASSET: &str = "INVALID_ARGUMENT: app.play_sound takes a file in assets/";
    const ID: &str = "INVALID_ARGUMENT: channel_id must be an id string";
    let (view, recorder) = open(spec(&format!(
        r#"{WHY}
        app.on("try", function()
            app.send("why", {{
                nan = why(app.play_sound, "assets/a.ogg", {{ volume = 0/0 }}),
                loud = why(app.play_sound, "assets/a.ogg", {{ volume = 1.5 }}),
                up = why(app.play_sound, "../server/main.luau"),
                script = why(app.play_sound, "client/main.luau"),
                kind = why(app.play_sound, "assets/a.exe"),
                number = why(app.play_sound, 5),
                on_end = why(app.play_sound, "assets/a.ogg", {{ on_end = 5 }}),
                options = why(app.play_sound, "assets/a.ogg", 5),
                id_number = why(app.open_channel, 42),
                id_text = why(app.open_channel, "4x"),
            }})
            app.play_sound("assets/ding.ogg")
            app.play_sound("assets/ding.ogg", {{ volume = 0.5 }})
            app.open_channel("42")
        end)
        "#
    )));
    recorder.first_batch();
    try_it(&view);
    let seen = settle(&view, &recorder);
    assert_eq!(
        seen.sent("why"),
        [json!({
            "nan": "INVALID_ARGUMENT: volume must be a finite number",
            "loud": "INVALID_ARGUMENT: volume must be from 0 to 1",
            "up": ASSET, "script": ASSET, "kind": ASSET, "number": ASSET,
            "on_end": "INVALID_ARGUMENT: on_end must be a function",
            "options": "INVALID_ARGUMENT: app.play_sound's options must be a table",
            "id_number": ID, "id_text": ID,
        })]
    );
    let sounds: Vec<(String, f64)> = seen.sounds().into_iter().map(|(_, a, v)| (a, v)).collect();
    assert_eq!(
        sounds,
        [
            ("assets/ding.ogg".to_owned(), 1.0),
            ("assets/ding.ogg".to_owned(), 0.5)
        ]
    );
    assert_eq!(seen.channels(), ["42"]);
}

// Fails if `on_end` runs twice (its entry must leave on `sound_end`), runs
// for another sound's id, or the count of waiting sounds doesn't fall.
#[test]
fn on_end_runs_once_on_its_sound_end() {
    let (view, recorder) = open(spec(&format!(
        r#"{WHY}
        local function nothing() end
        app.on("play", function()
            app.play_sound("assets/a.ogg", {{ on_end = function() app.send("ended") end }})
        end)
        app.on("fill", function()
            for _ = 1, 99 do
                app.play_sound("assets/a.ogg", {{ on_end = nothing }})
            end
            app.send("full", why(app.play_sound, "assets/a.ogg", {{ on_end = nothing }}))
        end)
        app.on("again", function()
            app.send("room", why(app.play_sound, "assets/a.ogg", {{ on_end = nothing }}))
        end)
        "#
    )));
    recorder.first_batch();
    view.server_message("play".into(), "null".into());
    let id = settle(&view, &recorder).sounds()[0].0;

    input(&view, "sound_end", json!({ "id": id + 1000 }));
    assert!(settle(&view, &recorder).sent("ended").is_empty());

    view.server_message("fill".into(), "null".into());
    assert_eq!(
        settle(&view, &recorder).sent("full"),
        [json!(
            "PLUGIN_LIMIT: at most 100 sounds can wait for on_end"
        )]
    );

    input(&view, "sound_end", json!({ "id": id }));
    input(&view, "sound_end", json!({ "id": id }));
    assert_eq!(settle(&view, &recorder).sent("ended").len(), 1);
    view.server_message("again".into(), "null".into());
    assert_eq!(settle(&view, &recorder).sent("room"), [json!("no error")]);
}

// Fails if the ops one turn queues aren't counted: Rust's side of a script
// isn't under the VM's memory cap.
#[test]
fn too_many_effects_in_one_turn_raises() {
    let (view, recorder) = open(spec(&format!(
        r#"{WHY}
        app.on("try", function()
            for _ = 1, 1000 do app.open_link("https://example.com") end
            app.send("why", why(app.open_link, "https://example.com"))
        end)
        app.on("next", function()
            app.send("next", why(app.open_link, "https://example.com"))
        end)
        "#
    )));
    recorder.first_batch();
    try_it(&view);
    let seen = settle(&view, &recorder);
    assert_eq!(
        seen.sent("why"),
        [json!("PLUGIN_LIMIT: at most 1000 side effects at once")]
    );
    assert_eq!(seen.links().len(), 1000);
    view.server_message("next".into(), "null".into());
    assert_eq!(settle(&view, &recorder).sent("next"), [json!("no error")]);
}

// Fails if a phone's view tells its script it runs on a desktop, in
// `init`'s ctx or in `app.platform`.
#[test]
fn a_phone_view_says_phone() {
    let mut phone = spec(&format!(
        "{SHOW}\napp.on(\"init_platform\", function() app.send(\"platform\", app.state.platform) end)"
    ));
    phone.sources = phone
        .sources
        .into_iter()
        .map(|(path, source)| {
            (
                path,
                source.replace(
                    "function init() return {} end",
                    "function init(ctx) return { platform = ctx.platform } end",
                ),
            )
        })
        .collect();
    phone.platform = ViewPlatform::Phone;
    let (view, recorder) = open(phone);
    recorder.first_batch();
    assert_eq!(read(&view, &recorder)["platform"], json!("phone"));
    view.server_message("init_platform".into(), "null".into());
    assert_eq!(settle(&view, &recorder).sent("platform"), [json!("phone")]);
}
