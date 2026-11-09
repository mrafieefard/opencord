//! What the view tests share: a recording `ViewSink`, a spec to start a
//! view from, and ways to wait for and read what it sent.

use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use opencord_plugins::view::{Batch, DialogKind, ModalRoot, Patch};
use serde_json::{Value as Json, json};

use super::{
    InputKind, Place, SafeArea, Theme, ViewContext, ViewHandle, ViewSink, ViewSpec, Viewer, start,
};

const WAIT: Duration = Duration::from_secs(5);
pub(super) const MAIN: &str = "client/main.luau";

/// `ping` answers `pong`; inputs run in order, so a pong means everything
/// sent before the ping was handled and drawn.
pub(super) const PING: &str = r#"app.on("ping", function(n) app.send("pong", n) end)"#;

#[derive(Default)]
pub(super) struct Recorder {
    pub(super) seen: Mutex<Seen>,
    changed: Condvar,
}

#[derive(Default, Debug, Clone)]
pub(super) struct Seen {
    pub(super) batches: Vec<Batch>,
    pub(super) sends: Vec<(String, Json)>,
    pub(super) errors: Vec<String>,
}

impl Seen {
    pub(super) fn sent(&self, name: &str) -> Vec<Json> {
        let named = self.sends.iter().filter(|(sent, _)| sent == name);
        named.map(|(_, data)| data.clone()).collect()
    }

    fn ops(&self) -> impl Iterator<Item = &Patch> {
        self.batches.iter().flat_map(|batch| &batch.ops)
    }

    /// The id of the last `block` created with this `text` prop.
    pub(super) fn node(&self, block: &str, text: &str) -> u64 {
        let mut found = None;
        for op in self.ops() {
            if let Patch::Create {
                id,
                block: b,
                props,
                ..
            } = op
                && b == block
                && props.get("text") == Some(&json!(text))
            {
                found = Some(*id);
            }
        }
        found.unwrap_or_else(|| panic!("no {block} \"{text}\" in {:#?}", self.batches))
    }

    /// The id of the first `block` created.
    pub(super) fn first(&self, block: &str) -> u64 {
        for op in self.ops() {
            if let Patch::Create { id, block: b, .. } = op
                && b == block
            {
                return *id;
            }
        }
        panic!("no {block} in {:#?}", self.batches);
    }

    pub(super) fn error_op(&self) -> (String, Option<String>) {
        for op in self.ops() {
            if let Patch::Error { message, details } = op {
                return (message.clone(), details.clone());
            }
        }
        panic!("no error op in {:#?}", self.batches);
    }

    /// The last `roots` op: the main root and the modals, bottom to top.
    pub(super) fn roots(&self) -> (u64, Vec<ModalRoot>) {
        let mut found = None;
        for op in self.ops() {
            if let Patch::Roots { main, modals } = op {
                found = Some((*main, modals.clone()));
            }
        }
        found.unwrap_or_else(|| panic!("no roots op in {:#?}", self.batches))
    }

    /// The last `dialog` op: its id, kind, text and default.
    pub(super) fn dialog(&self) -> (u64, DialogKind, String, Option<String>) {
        let mut found = None;
        for op in self.ops() {
            if let Patch::Dialog {
                id,
                kind,
                text,
                default,
            } = op
            {
                found = Some((*id, *kind, text.clone(), default.clone()));
            }
        }
        found.unwrap_or_else(|| panic!("no dialog op in {:#?}", self.batches))
    }

    /// The ids every batch dropped.
    pub(super) fn dropped(&self) -> Vec<u64> {
        let mut ids = Vec::new();
        for op in self.ops() {
            if let Patch::Drop { ids: dropped } = op {
                ids.extend(dropped);
            }
        }
        ids
    }

    /// The child ids `parent` has now, from its create and later
    /// `children` ops.
    pub(super) fn children(&self, parent: u64) -> Vec<u64> {
        let mut found = None;
        for op in self.ops() {
            match op {
                Patch::Create { id, children, .. } | Patch::Children { id, children }
                    if *id == parent =>
                {
                    found = Some(children.clone());
                }
                _ => {}
            }
        }
        found.unwrap_or_else(|| panic!("no node {parent} in {:#?}", self.batches))
    }

    /// The key a node was created with.
    pub(super) fn key(&self, node: u64) -> Option<String> {
        for op in self.ops() {
            if let Patch::Create { id, key, .. } = op
                && *id == node
            {
                return key.clone();
            }
        }
        panic!("no node {node} in {:#?}", self.batches);
    }

    /// The keys of `parent`'s children now.
    pub(super) fn child_keys(&self, parent: u64) -> Vec<String> {
        let children = self.children(parent);
        let keys = children.into_iter().map(|child| self.key(child));
        keys.map(|key| key.unwrap_or_default()).collect()
    }
}

impl ViewSink for Recorder {
    fn batch(&self, json: String) {
        let batch = serde_json::from_str(&json).expect("a batch in the wire format");
        self.update(|seen| seen.batches.push(batch));
    }

    fn send(&self, name: String, data_json: String) {
        let data = serde_json::from_str(&data_json).expect("JSON data");
        self.update(|seen| seen.sends.push((name, data)));
    }

    fn report_error(&self, text: String) {
        self.update(|seen| seen.errors.push(text));
    }
}

impl Recorder {
    fn update(&self, change: impl FnOnce(&mut Seen)) {
        change(&mut self.seen.lock().unwrap());
        self.changed.notify_all();
    }

    pub(super) fn wait(&self, what: &str, done: impl Fn(&Seen) -> bool) -> Seen {
        let deadline = Instant::now() + WAIT;
        let mut seen = self.seen.lock().unwrap();
        while !done(&seen) {
            let left = deadline.saturating_duration_since(Instant::now());
            assert!(!left.is_zero(), "timed out waiting for {what}: {seen:#?}");
            seen = self.changed.wait_timeout(seen, left).unwrap().0;
        }
        seen.clone()
    }

    pub(super) fn first_batch(&self) -> Seen {
        self.wait("the first batch", |seen| !seen.batches.is_empty())
    }
}

pub(super) fn spec(source: &str) -> ViewSpec {
    ViewSpec {
        view_id: "7".into(),
        plugin_id: "tickets".into(),
        entry: MAIN.into(),
        sources: [(MAIN.to_owned(), format!("{source}\n{PING}"))].into(),
        context: ViewContext {
            width: 800.0,
            height: 600.0,
            theme: Theme::Dark,
            input: InputKind::Mouse,
            safe_area: SafeArea::default(),
            text_scale: 1.0,
        },
        viewer: Viewer {
            id: "10".into(),
            name: "Ada".into(),
            roles: vec!["3".into()],
        },
        place: Place::Channel {
            channel_id: "42".into(),
        },
        show_errors: false,
    }
}

pub(super) fn open(spec: ViewSpec) -> (ViewHandle, Arc<Recorder>) {
    let recorder = Arc::new(Recorder::default());
    let handle = start(spec, recorder.clone());
    (handle, recorder)
}

/// Waits until the view handled everything sent to it before.
pub(super) fn settle(handle: &ViewHandle, recorder: &Recorder) -> Seen {
    let count = recorder.seen.lock().unwrap().sent("pong").len() + 1;
    handle.server_message("ping".into(), count.to_string());
    recorder.wait("pong", |seen| seen.sent("pong").len() >= count)
}

/// A renderer input of `kind` with these fields.
pub(super) fn input(handle: &ViewHandle, kind: &str, fields: Json) {
    let mut input = fields;
    input["kind"] = json!(kind);
    handle.input(&input.to_string());
}

pub(super) fn event(handle: &ViewHandle, node: u64, event: &str, data: Json) {
    input(
        handle,
        "event",
        json!({ "node": node, "event": event, "data": data }),
    );
}
