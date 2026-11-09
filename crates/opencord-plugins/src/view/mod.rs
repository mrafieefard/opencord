//! A view: the tree a client script draws, checked against the block schema.

pub mod tree;

pub use tree::{Node, TreeError, UI_NODE, from_lua};
