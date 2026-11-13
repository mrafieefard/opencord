//! Device storage (§11.4), through `start`, and the file it leaves.

use std::path::{Path, PathBuf};

use serde_json::{Value as Json, json};

use crate::views::ViewSpec;
use crate::views::test_support::{open, settle, spec};

const FILE: &str = "storage.json";

const SAVES: &str = r#"
function init() return {} end
function render() return ui.text({ text = "ok" }) end
local function why(...)
    local ok, message = pcall(...)
    return if ok then "no error" else message
end
app.on("put", function(pair) app.send("put", why(app.storage.set, pair.key, pair.value)) end)
app.on("fill", function(fill)
    app.send("put", why(app.storage.set, fill.key, string.rep("x", fill.size)))
end)
app.on("read", function(key) app.send("read", { value = app.storage.get(key) }) end)
app.on("save", function()
    app.storage.set("name", "Ada")
    app.storage.set("prefs", { volume = 5, tags = { "a", "b" } })
    app.storage.set("flag", true)
    app.storage.set("gone", 1)
    app.storage.delete("gone")
    app.storage.set("cleared", 1)
    app.storage.set("cleared", nil)
end)
app.on("load", function()
    app.send("loaded", {
        name = app.storage.get("name"),
        prefs = app.storage.get("prefs"),
        flag = app.storage.get("flag"),
        gone = app.storage.get("gone"),
        cleared = app.storage.get("cleared"),
    })
end)
"#;

fn folder(dir: &Path) -> PathBuf {
    dir.join("plugins").join("abc").join("tickets")
}

fn stored_in(dir: &Path) -> ViewSpec {
    let mut view = spec(SAVES);
    view.storage_dir = folder(dir);
    view
}

fn on_disk(dir: &Path) -> Json {
    let text = std::fs::read_to_string(folder(dir).join(FILE)).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn put(key: &str, value: impl Into<Json>) -> String {
    json!({ "key": key, "value": value.into() }).to_string()
}

fn loaded() -> Json {
    json!({ "name": "Ada", "prefs": { "volume": 5, "tags": ["a", "b"] }, "flag": true })
}

// Fails if a set stays in memory (the new view starts from the file), if
// the file isn't JSON, if a file is left aside, or if delete or `set(k, nil)`
// keep the key.
#[test]
fn storage_round_trips_and_survives_a_new_view() {
    let dir = tempfile::tempdir().unwrap();
    let (view, recorder) = open(stored_in(dir.path()));
    recorder.first_batch();
    view.server_message("save".into(), "null".into());
    view.server_message("load".into(), "null".into());
    assert_eq!(settle(&view, &recorder).sent("loaded"), [loaded()]);
    view.close();

    assert_eq!(on_disk(dir.path()), loaded());
    let names: Vec<String> = std::fs::read_dir(folder(dir.path()))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, [FILE], "a file was left aside");

    let (view, recorder) = open(stored_in(dir.path()));
    recorder.first_batch();
    view.server_message("load".into(), "null".into());
    assert_eq!(settle(&view, &recorder).sent("loaded"), [loaded()]);
    view.close();
}

// Fails if each view keeps its own copy: the second to save would wipe out
// the first's key.
#[test]
fn two_views_of_a_plugin_share_its_storage() {
    let dir = tempfile::tempdir().unwrap();
    let (first, first_seen) = open(stored_in(dir.path()));
    let (second, second_seen) = open(stored_in(dir.path()));
    first_seen.first_batch();
    second_seen.first_batch();
    first.server_message("put".into(), put("a", 1));
    settle(&first, &first_seen);
    second.server_message("put".into(), put("b", 2));
    settle(&second, &second_seen);
    second.server_message("read".into(), "\"a\"".into());
    assert_eq!(
        settle(&second, &second_seen).sent("read"),
        [json!({ "value": 1 })]
    );
    first.close();
    second.close();

    assert_eq!(on_disk(dir.path()), json!({ "a": 1, "b": 2 }));
}

// Fails if the limit counts one value rather than the total, a refused set
// changes what's stored, or replacing a value counts the old one too.
#[test]
fn full_storage_raises() {
    const FULL: &str = "PLUGIN_LIMIT: this plugin's device storage is full";
    const KIB: usize = 1024;
    let dir = tempfile::tempdir().unwrap();
    let (view, recorder) = open(stored_in(dir.path()));
    recorder.first_batch();

    let fill = |key: String, size: usize| {
        let fill = json!({ "key": key, "size": size }).to_string();
        view.server_message("fill".into(), fill);
    };

    // Ten values of 100 KiB fit, an eleventh doesn't; 20 KiB more on the
    // first fits only if its old 100 KiB leave the count.
    for index in 0..=10 {
        fill(format!("k{index}"), 100 * KIB);
    }
    fill("k0".to_owned(), 120 * KIB);
    let sent = settle(&view, &recorder).sent("put");
    assert_eq!(sent.len(), 12, "{sent:?}");
    assert_eq!(&sent[..10], vec![json!("no error"); 10]);
    assert_eq!(sent[10], json!(FULL));
    assert_eq!(sent[11], json!("no error"));
    view.server_message("read".into(), "\"k10\"".into());
    assert_eq!(settle(&view, &recorder).sent("read"), [json!({})]);
    view.close();

    let text = std::fs::read_to_string(folder(dir.path()).join(FILE)).unwrap();
    assert!(text.len() <= 1024 * KIB, "{} bytes", text.len());
    let stored: Json = serde_json::from_str(&text).unwrap();
    assert_eq!(stored["k0"].as_str().map(str::len), Some(120 * KIB));
    assert!(stored.get("k10").is_none());
}

// Fails if keys are counted in bytes, or aren't checked at all.
#[test]
fn storage_keys_are_checked() {
    const BAD: &str = "INVALID_ARGUMENT: a storage key is 1 to 200 characters of text";
    let dir = tempfile::tempdir().unwrap();
    let mut view = spec(
        r#"
        function init() return {} end
        function render() return ui.text({ text = "ok" }) end
        local function why(...)
            local ok, message = pcall(...)
            return if ok then "no error" else message
        end
        app.on("try", function()
            app.send("why", {
                empty = why(app.storage.get, ""),
                long = why(app.storage.set, string.rep("k", 201), 1),
                number = why(app.storage.delete, 5),
                wide = why(app.storage.set, string.rep("é", 200), 1),
                longest = why(app.storage.set, string.rep("k", 200), 1),
                not_json = why(app.storage.set, "f", print),
            })
        end)
        "#,
    );
    view.storage_dir = dir.path().to_path_buf();
    let (view, recorder) = open(view);
    recorder.first_batch();
    view.server_message("try".into(), "null".into());
    assert_eq!(
        settle(&view, &recorder).sent("why"),
        [json!({
            "empty": BAD,
            "long": BAD,
            "number": BAD,
            "wide": "no error",
            "longest": "no error",
            "not_json": "INVALID_ARGUMENT: json.encode can't encode a function",
        })]
    );
    view.close();
}

// Fails if a file that can't be read counts as empty: the next save would
// replace what's in it.
#[test]
fn a_broken_file_raises_and_is_left_alone() {
    const BROKEN: &str = "INTERNAL: this plugin's device storage can't be read";
    let dir = tempfile::tempdir().unwrap();
    let path = folder(dir.path()).join(FILE);
    std::fs::create_dir_all(folder(dir.path())).unwrap();
    std::fs::write(&path, "{ not json").unwrap();

    let (view, recorder) = open(stored_in(dir.path()));
    recorder.first_batch();
    view.server_message("put".into(), put("a", 1));
    assert_eq!(settle(&view, &recorder).sent("put"), [json!(BROKEN)]);
    view.close();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{ not json");
}

// Fails if a failed save is swallowed, or reported on every turn.
#[test]
fn a_failed_save_is_reported_once() {
    let dir = tempfile::tempdir().unwrap();
    let blocker = dir.path().join("tickets");
    let mut view = spec(SAVES);
    view.storage_dir = blocker.clone();
    let (view, recorder) = open(view);
    recorder.first_batch();
    // A file where the folder must go: the save can't make it.
    std::fs::write(&blocker, "").unwrap();
    view.server_message("put".into(), put("a", 1));
    settle(&view, &recorder);
    view.server_message("put".into(), put("a", 2));
    let seen = settle(&view, &recorder);
    assert_eq!(seen.errors.len(), 1, "{:?}", seen.errors);
    assert!(
        seen.errors[0].starts_with("app.storage:"),
        "{:?}",
        seen.errors
    );
    view.server_message("read".into(), "\"a\"".into());
    assert_eq!(
        settle(&view, &recorder).sent("read"),
        [json!({ "value": 2 })]
    );
}
