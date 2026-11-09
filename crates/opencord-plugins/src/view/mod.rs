//! A view: the tree a client script draws, checked against the block
//! schema, and the diff that turns each new tree into ops for the renderer.

pub mod diff;
pub mod patch;
pub mod tree;

pub use diff::{IdSource, Reconciled, Retained, reconcile};
pub use patch::{Batch, CloseReason, DialogKind, ModalRoot, ModalSize, Patch};
pub use tree::{Node, TreeError, UI_NODE, from_lua};
