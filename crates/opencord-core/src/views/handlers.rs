//! Handlers in the tree, by node id and event name (Claude's call 2), and
//! the futures every handler runs in.
//!
//! Each redraw replaces the whole map, so a handler from an old redraw is
//! gone, and a tap that races a redraw reaches the node's current function.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use futures_util::stream::FuturesUnordered;
use mlua::Function;
use opencord_plugins::limits::{VIEW_FRAME_TIME, VIEW_HANDLER_TIME};
use opencord_plugins::sandbox::{Budgeted, VmError};

use super::api_app::Shared;

/// The events during which `app.copy` works.
const PRESSES: &[&str] = &["press", "double_press", "long_press", "secondary_press"];

/// The handlers of the tree last drawn.
#[derive(Default)]
pub(super) struct Handlers {
    by_node: HashMap<(u64, String), Function>,
}

impl Handlers {
    /// The handlers of a new tree, in place of the last tree's.
    pub(super) fn replace(&mut self, handlers: Vec<(u64, String, Function)>) {
        self.by_node = handlers
            .into_iter()
            .map(|(node, event, handler)| ((node, event), handler))
            .collect();
    }

    pub(super) fn get(&self, node: u64, event: &str) -> Option<Function> {
        self.by_node.get(&(node, event.to_owned())).cloned()
    }

    pub(super) fn clear(&mut self) {
        self.by_node.clear();
    }
}

/// What a running handler is for: what it may do, and how long it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HandlerKind {
    Plain,
    /// `app.copy` works during it.
    Press,
    /// `app.on_frame`'s function, with the frame budget.
    Frame,
}

impl HandlerKind {
    /// The kind of a handler for a tree's `event`.
    pub(super) fn of_event(event: &str) -> HandlerKind {
        if PRESSES.contains(&event) {
            HandlerKind::Press
        } else {
            HandlerKind::Plain
        }
    }

    pub(super) fn limit(self) -> Duration {
        match self {
            HandlerKind::Plain | HandlerKind::Press => VIEW_HANDLER_TIME,
            HandlerKind::Frame => VIEW_FRAME_TIME,
        }
    }
}

/// A handler that ended, and how.
pub(super) struct Finished {
    /// What ran, for the error report: `on_press`, `app.on("count")`.
    pub(super) what: String,
    pub(super) kind: HandlerKind,
    pub(super) result: Result<(), VmError>,
}

/// The handlers that started and haven't ended: a handler waiting on the
/// host (a dialog, Task 4) stays here while later inputs go on.
pub(super) type Running = FuturesUnordered<Pin<Box<dyn Future<Output = Finished>>>>;

/// Wraps a handler's call: Lua gets the handler's budget over all its
/// polls, and `app.copy` works during them for a press.
pub(super) fn handler<F>(
    what: String,
    kind: HandlerKind,
    shared: Arc<Shared>,
    budgeted: Budgeted<F>,
) -> Pin<Box<dyn Future<Output = Finished>>>
where
    F: Future<Output = mlua::Result<()>> + 'static,
{
    let call = Pressing {
        call: budgeted,
        shared,
        press: kind == HandlerKind::Press,
    };
    Box::pin(async move {
        let result = call.await.map_err(VmError::from);
        Finished { what, kind, result }
    })
}

/// Marks each poll of a press handler as "handling a press".
struct Pressing<F> {
    call: Budgeted<F>,
    shared: Arc<Shared>,
    press: bool,
}

impl<F: Future> Future for Pressing<F> {
    type Output = F::Output;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<F::Output> {
        // `Budgeted` keeps its call boxed, so it and this are `Unpin`.
        let this = &mut *self;
        this.shared.set_in_press(this.press);
        let poll = Pin::new(&mut this.call).poll(cx);
        this.shared.set_in_press(false);
        poll
    }
}
