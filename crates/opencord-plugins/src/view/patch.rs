//! What the core sends a view's renderer: batches of ops, as JSON (the
//! plan's "view wire format"). The tree ops come from [`super::reconcile`];
//! the rest are the client VM's.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

type Json = serde_json::Value;

/// One batch for one view; the renderer applies its ops in order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Batch {
    pub view: String,
    pub seq: u64,
    pub ops: Vec<Patch>,
}

/// One change for the renderer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Patch {
    /// A new node. Its children were created earlier in the same batch. A
    /// canvas's items aren't in `props`: a `draw` op brings them.
    Create {
        id: u64,
        block: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
        props: BTreeMap<String, Json>,
        /// Handler names, without `on_`.
        events: Vec<String>,
        children: Vec<u64>,
    },
    /// Changed props only.
    Props {
        id: u64,
        set: BTreeMap<String, Json>,
        unset: Vec<String>,
    },
    /// The node's handler names changed; these are all of them.
    Events {
        id: u64,
        events: Vec<String>,
    },
    /// The node's whole child list, in order, after adds, moves and removes.
    Children {
        id: u64,
        children: Vec<u64>,
    },
    /// Nodes gone, whole subtrees listed; the renderer frees them.
    Drop {
        ids: Vec<u64>,
    },
    /// What's drawn: the main tree, then named modals, bottom to top.
    Roots {
        main: u64,
        modals: Vec<ModalRoot>,
    },
    /// A canvas's items changed. The renderer removes `remove`, then puts
    /// each item of `set` in place of the item with its id, or at the end
    /// when it has none; `order`, when sent, is then every item id in the
    /// order to draw them.
    Draw {
        id: u64,
        set: Vec<Json>,
        remove: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        order: Option<Vec<String>>,
    },
    /// A quick dialog; the answer comes back as a `dialog` input.
    Dialog {
        id: u64,
        kind: DialogKind,
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<String>,
    },
    /// Send `frame` inputs, or stop.
    Frames {
        on: bool,
    },
    /// Whether the plugin set `app.on_back`: the phone then asks it before
    /// back leaves the plugin.
    BackHandler {
        on: bool,
    },
    /// The answer to a `back` input: true when `app.on_back`'s handler
    /// returned true (the plugin went back itself, detail → list).
    Back {
        id: u64,
        handled: bool,
    },
    /// Play a sound from the plugin's assets.
    Sound {
        id: u64,
        asset: String,
        volume: f64,
    },
    Copy {
        text: String,
    },
    /// Asked of the viewer first.
    OpenLink {
        url: String,
    },
    OpenChannel {
        channel_id: String,
    },
    /// The view stopped; `details` (file, line, trace) only when allowed.
    Error {
        message: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        details: Option<String>,
    },
    /// "Update Opencord to open this" (§11.7).
    TooOld,
    /// The server closed the view.
    Closed {
        reason: CloseReason,
    },
}

/// A named modal on the stack, drawn from the node `id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModalRoot {
    pub id: u64,
    pub name: String,
    pub size: ModalSize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheet: Option<bool>,
    pub dismissible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModalSize {
    Small,
    Medium,
    Large,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DialogKind {
    Confirm,
    Alert,
    Prompt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseReason {
    PluginStopped,
    LostAccess,
    ChannelDeleted,
    Unavailable,
    /// The view's activity session ended.
    ActivityEnded,
    /// The viewer left the activity's call.
    LeftCall,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Fails if an op or a field is named differently from the wire format
    // (`open_link`, `channel_id`, `too_old`), or if an absent optional
    // field is sent as null.
    #[test]
    fn ops_the_diff_doesnt_make_have_the_wire_formats_shape() {
        let ops = vec![
            Patch::Roots {
                main: 1,
                modals: vec![ModalRoot {
                    id: 9,
                    name: "edit".into(),
                    size: ModalSize::Medium,
                    sheet: Some(false),
                    dismissible: true,
                }],
            },
            Patch::Dialog {
                id: 3,
                kind: DialogKind::Prompt,
                text: "Name?".into(),
                default: None,
            },
            Patch::Frames { on: true },
            Patch::BackHandler { on: true },
            Patch::Back {
                id: 2,
                handled: false,
            },
            Patch::Sound {
                id: 4,
                asset: "assets/ding.ogg".into(),
                volume: 0.5,
            },
            Patch::Copy { text: "hi".into() },
            Patch::OpenLink {
                url: "https://example.com".into(),
            },
            Patch::OpenChannel {
                channel_id: "42".into(),
            },
            Patch::Error {
                message: "This plugin hit an error".into(),
                details: None,
            },
            Patch::TooOld,
            Patch::Closed {
                reason: CloseReason::LostAccess,
            },
        ];
        let batch = Batch {
            view: "7".into(),
            seq: 2,
            ops,
        };
        let sent = serde_json::to_value(&batch).unwrap();
        assert_eq!(
            sent,
            json!({
                "view": "7",
                "seq": 2,
                "ops": [
                    { "op": "roots", "main": 1, "modals": [
                        { "id": 9, "name": "edit", "size": "medium", "sheet": false, "dismissible": true }
                    ] },
                    { "op": "dialog", "id": 3, "kind": "prompt", "text": "Name?" },
                    { "op": "frames", "on": true },
                    { "op": "back_handler", "on": true },
                    { "op": "back", "id": 2, "handled": false },
                    { "op": "sound", "id": 4, "asset": "assets/ding.ogg", "volume": 0.5 },
                    { "op": "copy", "text": "hi" },
                    { "op": "open_link", "url": "https://example.com" },
                    { "op": "open_channel", "channel_id": "42" },
                    { "op": "error", "message": "This plugin hit an error" },
                    { "op": "too_old" },
                    { "op": "closed", "reason": "lost_access" }
                ]
            })
        );
        let read: Batch = serde_json::from_value(sent).unwrap();
        assert_eq!(read, batch);
    }

    // Fails if an activity's close reason is named differently from the
    // wire format's `closed` op (plan M3, "Additions").
    #[test]
    fn activity_close_reasons_have_the_wire_formats_names() {
        let names = [CloseReason::ActivityEnded, CloseReason::LeftCall].map(|reason| {
            serde_json::to_value(Patch::Closed { reason }).unwrap()["reason"].clone()
        });
        assert_eq!(names, [json!("activity_ended"), json!("left_call")]);
    }
}
