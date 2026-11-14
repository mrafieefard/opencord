//! Timers (§11.4), on tokio's paused clock: `View::run` in this test's
//! runtime, so time moves only when the view has nothing left to do.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value as Json, json};
use tokio::sync::mpsc;
use tokio::time::sleep;

use super::View;
use crate::views::test_support::{MAIN, PING, Recorder, open, settle, spec};
use crate::views::{Command, ViewSpec};

const BARE: &str = r#"
function init() return {} end
function render() return ui.text({ text = "ok" }) end
"#;

fn ms(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

/// The view's end of a test on the paused clock.
struct Clocked {
    commands: mpsc::UnboundedSender<Command>,
    recorder: Arc<Recorder>,
}

impl Clocked {
    fn send(&self, command: Command) {
        assert!(self.commands.send(command).is_ok(), "the view ended early");
    }

    fn lifecycle(&self, event: &str) {
        let input = json!({ "kind": "lifecycle", "event": event });
        self.send(Command::Input(input.to_string()));
    }

    fn message(&self, name: &str) {
        self.send(Command::ServerMessage {
            name: name.to_owned(),
            data_json: "null".to_owned(),
        });
    }

    fn sent(&self, name: &str) -> Vec<Json> {
        self.recorder.seen.lock().unwrap().sent(name)
    }

    fn errors(&self) -> Vec<String> {
        self.recorder.seen.lock().unwrap().errors.clone()
    }
}

/// Runs `test` beside the view's own loop, then closes the view.
async fn clocked(view: ViewSpec, test: impl AsyncFnOnce(&Clocked)) {
    let recorder = Arc::new(Recorder::default());
    let (commands, receiver) = mpsc::unbounded_channel();
    let handle = Clocked {
        commands,
        recorder: recorder.clone(),
    };
    let mut view = View::new(view, recorder);
    tokio::join!(view.run(receiver), async {
        test(&handle).await;
        handle.send(Command::Close);
    });
}

// Fails if `after` repeats, `every` stops after one, or a timer fires early.
#[tokio::test(start_paused = true)]
async fn after_fires_once_and_every_repeats() {
    let source = format!(
        r#"{BARE}
        local ticks = 0
        app.after(1, function() app.send("after") end)
        app.every(0.5, function() ticks += 1 app.send("tick", ticks) end)
        "#
    );
    clocked(spec(&source), async |view: &Clocked| {
        sleep(ms(900)).await;
        assert!(view.sent("after").is_empty(), "app.after fired early");
        assert_eq!(view.sent("tick"), [json!(1)]);
        sleep(ms(1200)).await;
        assert_eq!(view.sent("after").len(), 1);
        assert_eq!(view.sent("tick"), [json!(1), json!(2), json!(3), json!(4)]);
        sleep(ms(10_000)).await;
        assert_eq!(view.sent("after").len(), 1, "app.after fired again");
        assert_eq!(view.sent("tick").len(), 24, "app.every lost its pace");
    })
    .await;
}

// Fails if a pause doesn't stop the timers, if they lose the time they had
// left (firing at once on resume, or starting over), or if a timer made
// during the pause counts the pause.
#[tokio::test(start_paused = true)]
async fn timers_wait_while_paused() {
    let source = format!(
        r#"{BARE}
        app.after(1, function() app.send("after") end)
        app.every(1, function() app.send("tick") end)
        app.on("later", function() app.after(1, function() app.send("later") end) end)
        "#
    );
    clocked(spec(&source), async |view: &Clocked| {
        sleep(ms(400)).await;
        view.lifecycle("pause");
        sleep(ms(10_000)).await;
        assert!(view.sent("after").is_empty(), "a timer fired while paused");
        assert!(view.sent("tick").is_empty(), "a timer fired while paused");

        view.message("later");
        sleep(ms(5_000)).await;
        view.lifecycle("resume");
        sleep(ms(500)).await;
        assert!(
            view.sent("after").is_empty(),
            "the pause counted as waiting"
        );
        sleep(ms(200)).await;
        assert_eq!(view.sent("after").len(), 1, "the time left was lost");
        assert_eq!(view.sent("tick").len(), 1);
        assert!(
            view.sent("later").is_empty(),
            "the pause counted as waiting"
        );
        sleep(ms(400)).await;
        assert_eq!(view.sent("later").len(), 1);
    })
    .await;
}

// Fails if cancelling leaves the timer running, or disturbs another timer
// (removing an integer key with `raw_remove` shifts the ones after it).
#[tokio::test(start_paused = true)]
async fn a_cancelled_timer_never_fires() {
    let source = format!(
        r#"{BARE}
        local one = app.after(1, function() app.send("one") end)
        local two = app.every(1, function() app.send("two") end)
        app.after(1, function() app.send("three") end)
        app.cancel(one)
        app.cancel(two)
        app.cancel(one)
        "#
    );
    clocked(spec(&source), async |view: &Clocked| {
        sleep(ms(3_000)).await;
        assert!(view.sent("one").is_empty());
        assert!(view.sent("two").is_empty());
        assert_eq!(view.sent("three").len(), 1);
    })
    .await;
}

// Fails if a reload leaves the old VM's timers in the list: they'd fire
// into nothing and take the new script's 100.
#[tokio::test(start_paused = true)]
async fn a_reload_drops_the_old_timers() {
    let fill = r#"for _ = 1, 99 do app.after(1000, function() end) end"#;
    let old = format!(r#"{BARE} {fill} app.after(1, function() app.send("old") end)"#);
    let new = format!(r#"{BARE} {fill} app.after(3, function() app.send("new") end)"#);
    clocked(spec(&old), async |view: &Clocked| {
        view.send(Command::Reload {
            entry: MAIN.to_owned(),
            sources: [(MAIN.to_owned(), format!("{new}\n{PING}"))].into(),
        });
        sleep(ms(2_000)).await;
        assert!(view.sent("old").is_empty(), "the old VM's timer fired");
        sleep(ms(2_000)).await;
        assert_eq!(view.sent("new").len(), 1);
        assert!(view.errors().is_empty(), "{:?}", view.errors());
    })
    .await;
}

// Fails if the count isn't 100, or if a cancelled timer keeps its place.
#[test]
fn a_101st_timer_raises() {
    let (view, recorder) = open(spec(&format!(
        r#"{BARE}
        app.on("fill", function()
            local first
            for _ = 1, 100 do
                local timer = app.after(1000, function() end)
                first = first or timer
            end
            local ok, why = pcall(app.every, 1000, function() end)
            app.send("full", {{ ok = ok, why = why }})
            app.cancel(first)
            app.send("room", (pcall(app.after, 1000, function() end)))
        end)
        "#
    )));
    recorder.first_batch();
    view.server_message("fill".into(), "null".into());
    let seen = settle(&view, &recorder);
    assert_eq!(
        seen.sent("full"),
        [json!({ "ok": false, "why": "PLUGIN_LIMIT: at most 100 timers" })]
    );
    assert_eq!(seen.sent("room"), [json!(true)]);
}

// Fails if a delay isn't checked for being a finite number in range
// (`Duration::from_secs_f64` panics on NaN, which would end the thread).
#[test]
fn a_nan_delay_raises() {
    let (view, recorder) = open(spec(&format!(
        r#"{BARE}
        local function nothing() end
        local function why(...)
            local ok, message = pcall(...)
            return if ok then "no error" else message
        end
        app.on("try", function()
            app.send("why", {{
                nan = why(app.after, 0/0, nothing),
                infinite = why(app.every, math.huge, nothing),
                text = why(app.after, "soon", nothing),
                fast = why(app.every, 0.05, nothing),
                negative = why(app.after, -1, nothing),
                long = why(app.after, 1e12, nothing),
                no_function = why(app.after, 1, 5),
                cancel = why(app.cancel, "x"),
            }})
        end)
        "#
    )));
    recorder.first_batch();
    view.server_message("try".into(), "null".into());
    let seen = settle(&view, &recorder);
    assert_eq!(
        seen.sent("why"),
        [json!({
            "nan": "INVALID_ARGUMENT: seconds must be a finite number",
            "infinite": "INVALID_ARGUMENT: seconds must be a finite number",
            "text": "INVALID_ARGUMENT: seconds must be a finite number",
            "fast": "INVALID_ARGUMENT: app.every's seconds must be at least 0.1",
            "negative": "INVALID_ARGUMENT: app.after's seconds must be at least 0",
            "long": "INVALID_ARGUMENT: app.after's seconds must be at most 31536000",
            "no_function": "INVALID_ARGUMENT: app.after takes a function",
            "cancel": "INVALID_ARGUMENT: app.cancel takes a timer",
        })]
    );
}
