//! One redraw (§11.3, §12.5): `render(state)`, every lazy list's items and
//! every open modal's `fn(state, props)`, checked, then each tree diffed
//! against its own last one.
//!
//! - All the redraw's Lua shares one handler's budget, and its trees share
//!   the render's limits (`Tally`): a list's items and the modals count.
//! - Every tree is made and checked before any is diffed, so a failing
//!   redraw takes no ids and changes nothing kept.
//! - The batch's ops: each tree's (the main one, then the modals bottom to
//!   top), a `roots` op when the main root or the modals changed, and one
//!   `drop` with every node gone, a closed modal's whole tree included.

use std::collections::HashMap;
use std::time::Duration;

use mlua::{Function, Lua, Table, Value};
use opencord_plugins::limits::{VIEW_ERROR_REPORT_MAX_CHARS, VIEW_HANDLER_TIME};
use opencord_plugins::sandbox::{Budget, VmError};
use opencord_plugins::view::{
    IdSource, ModalRoot, ModalSize, Node, Patch, Retained, Tally, TreeError, from_lua_counted,
    reconcile,
};

use super::cut;
use super::lists::{Bound, Filler, ITEM, Lists, Place};
use super::modals::{self, OpenModal};

/// What the viewer sees when the script fails (§11.3).
const ERROR_MESSAGE: &str = "This plugin hit an error";
/// The main tree's number among a view's trees; a modal's is its serial.
const MAIN_TREE: u64 = 0;

/// One redraw's share of Lua time: a handler's budget over all its calls,
/// counting only while Lua runs.
pub(super) struct Clock {
    budget: Budget,
    left: Duration,
}

impl Clock {
    fn new(budget: Budget) -> Clock {
        Clock {
            budget,
            left: VIEW_HANDLER_TIME,
        }
    }

    pub(super) fn call<R>(&mut self, call: impl FnOnce() -> mlua::Result<R>) -> Result<R, VmError> {
        self.budget.start(self.left);
        let result = call();
        self.left = self.left.saturating_sub(self.budget.used());
        self.budget.stop();
        result.map_err(|error| match VmError::from(error) {
            // The budget was started with what was left; the limit is the
            // whole redraw's.
            VmError::BudgetExceeded { .. } => VmError::BudgetExceeded {
                limit: VIEW_HANDLER_TIME,
            },
            other => other,
        })
    }
}

/// What a view drew last, as the renderer holds it.
#[derive(Default)]
pub(super) struct Drawing {
    ids: IdSource,
    main: Option<Retained>,
    /// Each open modal's tree, by serial. Entries leave when the modal
    /// isn't in a redraw, and with the view.
    modals: HashMap<u64, Retained>,
    lists: Lists,
    /// The last `roots` op sent; `None` sends one again.
    shown: Option<Shown>,
}

#[derive(Clone, PartialEq)]
struct Shown {
    main: u64,
    modals: Vec<ModalRoot>,
}

/// What a redraw reads from the VM.
pub(super) struct Scene<'a> {
    pub(super) lua: &'a Lua,
    pub(super) budget: &'a Budget,
    pub(super) state: &'a Table,
    pub(super) render: &'a Function,
}

/// A redraw's ops, and the handlers of every tree by node id and event.
pub(super) struct Drawn {
    pub(super) ops: Vec<Patch>,
    pub(super) handlers: Vec<(u64, String, Function)>,
}

impl Drawing {
    pub(super) fn redraw(&mut self, scene: Scene<'_>) -> Result<Drawn, Failure> {
        let mut clock = Clock::new(scene.budget.clone());
        let mut tally = Tally::default();
        let mut filler = Filler {
            lua: scene.lua,
            lists: &self.lists,
            clock: &mut clock,
            tally: &mut tally,
        };
        let value = filler
            .clock
            .call(|| scene.render.call::<Value>(scene.state))
            .map_err(|error| Failure::lua("render", error))?;
        let mut main = from_lua_counted(scene.lua, &value, Bound::Handler, filler.tally, 0)
            .map_err(|error| Failure::tree("render", error))?;
        filler.fill(MAIN_TREE, &mut main, "render")?;

        let open = modals::open_modals(scene.lua)
            .map_err(|error| Failure::lua("render", VmError::from(error)))?;
        let mut drawn = Vec::with_capacity(open.len());
        for modal in open {
            let what = format!("app.modals.{}", modal.name);
            let Some(function) = &modal.function else {
                return Err(Failure::host(
                    what,
                    "app.modals has no function for this open modal",
                ));
            };
            let value = filler
                .clock
                .call(|| function.call::<Value>((scene.state, &modal.props)))
                .map_err(|error| Failure::lua(what.clone(), error))?;
            let mut node = from_lua_counted(scene.lua, &value, Bound::Handler, filler.tally, 0)
                .map_err(|error| Failure::tree(what.clone(), error))?;
            filler.fill(modal.serial, &mut node, &what)?;
            drawn.push((modal, node));
        }
        Ok(self.diff(main, drawn))
    }

    /// Diffs the checked trees; nothing here can fail.
    fn diff(&mut self, main: Node<Bound>, modals: Vec<(OpenModal, Node<Bound>)>) -> Drawn {
        let mut ops = Vec::new();
        let mut dropped = Vec::new();
        let mut bound = Vec::new();
        let main = reconcile(self.main.as_ref(), main, &mut self.ids);
        split(main.ops, &mut ops, &mut dropped);
        bound.extend(main.handlers);
        let mut shown = Shown {
            main: main.tree.root(),
            modals: Vec::with_capacity(modals.len()),
        };
        let mut trees = HashMap::with_capacity(modals.len());
        for (modal, node) in modals {
            let (size, sheet, dismissible) = options(&node);
            let reconciled = reconcile(self.modals.get(&modal.serial), node, &mut self.ids);
            split(reconciled.ops, &mut ops, &mut dropped);
            bound.extend(reconciled.handlers);
            shown.modals.push(ModalRoot {
                id: reconciled.tree.root(),
                name: modal.name,
                size,
                sheet,
                dismissible,
            });
            trees.insert(modal.serial, reconciled.tree);
        }
        for (serial, tree) in &self.modals {
            if !trees.contains_key(serial) {
                dropped.extend(tree.ids());
            }
        }
        if self.shown.as_ref() != Some(&shown) {
            ops.push(Patch::Roots {
                main: shown.main,
                modals: shown.modals.clone(),
            });
            self.shown = Some(shown);
        }
        if !dropped.is_empty() {
            dropped.sort_unstable();
            ops.push(Patch::Drop { ids: dropped });
        }

        let mut handlers = Vec::with_capacity(bound.len());
        let mut lists = HashMap::new();
        for (id, event, bound) in bound {
            match bound {
                Bound::Handler(handler) => handlers.push((id, event, handler)),
                Bound::Item(place) => {
                    lists.insert(id, place);
                }
            }
        }
        hide_items(&mut ops, &lists);
        self.lists.drawn(lists);
        self.main = Some(main.tree);
        self.modals = trees;
        Drawn { ops, handlers }
    }

    /// A `range` input; `true` when it set a list's range.
    pub(super) fn set_range(&mut self, node: u64, start: u64, end: u64) -> bool {
        self.lists.set_range(node, start, end)
    }

    /// Whether the open modal `name`, as last drawn, may be dismissed.
    pub(super) fn dismissible(&self, name: &str) -> bool {
        self.shown.as_ref().is_some_and(|shown| {
            shown
                .modals
                .iter()
                .any(|modal| modal.name == name && modal.dismissible)
        })
    }

    /// After an `error` op: the next redraw sends `roots` again.
    pub(super) fn forget_shown(&mut self) {
        self.shown = None;
    }
}

/// Moves `from`'s ops to `ops`, all but its `drop`, whose ids go to
/// `dropped`.
fn split(from: Vec<Patch>, ops: &mut Vec<Patch>, dropped: &mut Vec<u64>) {
    for op in from {
        match op {
            Patch::Drop { ids } => dropped.extend(ids),
            op => ops.push(op),
        }
    }
}

/// A named modal's size, sheet and dismissible: from its root when that's
/// a `modal` or a `sheet` block, else medium, not a sheet, dismissible.
fn options(root: &Node<Bound>) -> (ModalSize, bool, bool) {
    let flag = |name: &str, default: bool| {
        let value = root.props.get(name).and_then(serde_json::Value::as_bool);
        value.unwrap_or(default)
    };
    match root.block.as_str() {
        "modal" => {
            let size = match root.props.get("size") {
                // The tree's check allows only the four sizes.
                Some(size) => serde_json::from_value(size.clone()).unwrap_or(ModalSize::Medium),
                None => ModalSize::Medium,
            };
            (size, flag("sheet", false), flag("dismissible", true))
        }
        "sheet" => (ModalSize::Medium, true, flag("dismissible", true)),
        _ => (ModalSize::Medium, false, true),
    }
}

/// A list's `item` rides through the diff as a handler, so the redraw
/// learns its node id; the renderer never hears of it.
fn hide_items(ops: &mut [Patch], lists: &HashMap<u64, Place>) {
    for op in ops {
        if let Patch::Create { id, events, .. } | Patch::Events { id, events } = op
            && lists.contains_key(id)
        {
            events.retain(|event| event != ITEM);
        }
    }
}

/// An error in the script, or in starting it, and where it happened.
pub(super) struct Failure {
    /// `init`, `render`, `on_press`…
    pub(super) what: String,
    message: String,
    traceback: Option<String>,
    /// For the log: `budget`, `memory`, `lua`, `tree` or `host`.
    pub(super) kind: &'static str,
}

impl Failure {
    pub(super) fn lua(what: impl Into<String>, error: VmError) -> Failure {
        let kind = match &error {
            VmError::BudgetExceeded { .. } => "budget",
            VmError::OutOfMemory => "memory",
            VmError::Lua { .. } => "lua",
        };
        let (message, traceback) = match error {
            VmError::Lua { message, traceback } => (message, traceback),
            other => (other.to_string(), None),
        };
        Failure {
            what: what.into(),
            message,
            traceback,
            kind,
        }
    }

    pub(super) fn tree(what: impl Into<String>, error: TreeError) -> Failure {
        Failure {
            what: what.into(),
            message: error.to_string(),
            traceback: None,
            kind: "tree",
        }
    }

    pub(super) fn host(what: impl Into<String>, message: impl Into<String>) -> Failure {
        Failure {
            what: what.into(),
            message: message.into(),
            traceback: None,
            kind: "host",
        }
    }

    /// The same error, its message led by where in the tree it happened.
    pub(super) fn within(mut self, place: String) -> Failure {
        self.message = format!("{place}: {}", self.message);
        self
    }

    /// The error and its traceback: what a developer needs.
    fn details(&self) -> String {
        match &self.traceback {
            Some(traceback) => format!("{}\n{traceback}", self.message),
            None => self.message.clone(),
        }
    }

    /// The text for the plugin's log.
    pub(super) fn report(&self) -> String {
        format!("{}: {}", self.what, self.details())
    }

    pub(super) fn op(&self, show_errors: bool) -> Patch {
        Patch::Error {
            message: ERROR_MESSAGE.to_owned(),
            details: show_errors.then(|| cut(&self.details(), VIEW_ERROR_REPORT_MAX_CHARS)),
        }
    }
}
