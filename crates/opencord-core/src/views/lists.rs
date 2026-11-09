//! Lazy lists (§12.2): a `list` with `count` and `item(i)` draws only the
//! items in its range, rendered during `render` by calling `item(i)`
//! (0-based) and keyed `#<i>` unless the item has a key of its own.
//!
//! A list's range is kept by its place: the chain of blocks and keys (or
//! positions among unkeyed siblings) from its tree's root, which is exactly
//! what the diff matches nodes by. So a place is kept from one redraw to
//! the next exactly while the list keeps its node id, and a range leaves
//! with the list's node. Ranges start at the first [`FIRST_ITEMS`] items
//! (the last ones when `reverse`) and a `range` input sets them.

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use mlua::{Function, Lua, Value};
use opencord_plugins::limits::LIST_MAX_RANGE;
use opencord_plugins::view::{Node, Tally, TreeError, from_lua_counted};

use super::cut;
use super::draw::{Clock, Failure};

/// Items drawn before the renderer asks for a range.
const FIRST_ITEMS: usize = 30;
const LIST: &str = "list";
/// `list`'s item function: in the node's handlers, never an event.
pub(super) const ITEM: &str = "item";
/// Keys in error messages are cut to this many characters, as in the tree's.
const SHOWN_KEY_CHARS: usize = 40;

/// A tree's handlers as the diff carries them: the tree's own functions,
/// and in place of each list's `item`, the list's place, so the redraw
/// learns each list's node id.
pub(super) enum Bound {
    Handler(Function),
    Item(Place),
}

/// Where a node sits: its tree (0 for the main one, else the modal's
/// serial), then one step per block from the root.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct Place(Vec<Step>);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Step {
    Tree(u64),
    /// A keyed block: its block and key.
    Keyed(String, String),
    /// An unkeyed block: its block and its position among the unkeyed.
    Unkeyed(String, usize),
}

/// The lists' ranges. Entries leave when their list isn't in a redraw,
/// and with the view.
#[derive(Default)]
pub(super) struct Lists {
    /// Ranges a `range` input set, by the list's place.
    ranges: HashMap<Place, Range<usize>>,
    /// The lists of the last redraw, by node id, for `range` inputs.
    ids: HashMap<u64, Place>,
}

impl Lists {
    /// A `range` input (`end` not included), cut to [`LIST_MAX_RANGE`]
    /// items. `false` when `node` isn't a list of the last redraw.
    pub(super) fn set_range(&mut self, node: u64, start: u64, end: u64) -> bool {
        let Some(place) = self.ids.get(&node) else {
            return false;
        };
        let start = usize::try_from(start).unwrap_or(usize::MAX);
        let end = usize::try_from(end)
            .unwrap_or(usize::MAX)
            .clamp(start, start.saturating_add(LIST_MAX_RANGE));
        self.ranges.insert(place.clone(), start..end);
        true
    }

    /// The items a list at `place` draws now.
    fn span(&self, place: &Place, count: usize, reverse: bool) -> Range<usize> {
        let range = match self.ranges.get(place) {
            Some(range) => range.clone(),
            None if reverse => count.saturating_sub(FIRST_ITEMS)..count,
            None => 0..FIRST_ITEMS,
        };
        range.start.min(count)..range.end.min(count)
    }

    /// After a redraw: the lists it drew, by node id. Ranges of lists it
    /// didn't draw leave.
    pub(super) fn drawn(&mut self, ids: HashMap<u64, Place>) {
        let places: HashSet<&Place> = ids.values().collect();
        self.ranges.retain(|place, _| places.contains(place));
        self.ids = ids;
    }
}

/// Fills the lists of one redraw's trees, within its clock and tally.
pub(super) struct Filler<'a> {
    pub(super) lua: &'a Lua,
    pub(super) lists: &'a Lists,
    pub(super) clock: &'a mut Clock,
    pub(super) tally: &'a mut Tally,
}

/// Where the walk is: the place, the path for errors, and the depth.
struct At {
    place: Vec<Step>,
    path: Vec<String>,
    depth: usize,
    /// `render` or `app.modals.<name>`, for errors.
    what: String,
}

impl Filler<'_> {
    /// Fills every lazy list in `root`, the root of tree `tree` (0 for the
    /// main one, else the modal's serial). `what` names the tree's function
    /// in errors.
    pub(super) fn fill(
        &mut self,
        tree: u64,
        root: &mut Node<Bound>,
        what: &str,
    ) -> Result<(), Failure> {
        let step = match &root.key {
            Some(key) => Step::Keyed(root.block.clone(), key.clone()),
            None => Step::Unkeyed(root.block.clone(), 0),
        };
        let mut at = At {
            place: vec![Step::Tree(tree), step],
            path: vec![root.block.clone()],
            depth: 1,
            what: what.to_owned(),
        };
        self.node(root, &mut at)
    }

    fn node(&mut self, node: &mut Node<Bound>, at: &mut At) -> Result<(), Failure> {
        if node.block == LIST {
            return self.items(node, at);
        }
        let mut unkeyed = 0;
        for (index, child) in node.children.iter_mut().enumerate() {
            let step = match &child.key {
                Some(key) => Step::Keyed(child.block.clone(), key.clone()),
                None => {
                    unkeyed += 1;
                    Step::Unkeyed(child.block.clone(), unkeyed - 1)
                }
            };
            let path = format!("[{index}] {}", child.block);
            self.below(child, at, step, path)?;
        }
        Ok(())
    }

    /// Walks `child`, one step below `at`.
    fn below(
        &mut self,
        child: &mut Node<Bound>,
        at: &mut At,
        step: Step,
        path: String,
    ) -> Result<(), Failure> {
        at.place.push(step);
        at.path.push(path);
        at.depth += 1;
        let walked = self.node(child, at);
        at.depth -= 1;
        at.path.pop();
        at.place.pop();
        walked
    }

    /// Renders the items of a list in its range as its children.
    fn items(&mut self, list: &mut Node<Bound>, at: &mut At) -> Result<(), Failure> {
        let Some(Bound::Handler(item)) = list.handlers.remove(ITEM) else {
            return Ok(());
        };
        let place = Place(at.place.clone());
        list.handlers
            .insert(ITEM.to_owned(), Bound::Item(place.clone()));
        let count = list.props.get("count").and_then(serde_json::Value::as_u64);
        let count = usize::try_from(count.unwrap_or(0)).unwrap_or(usize::MAX);
        let reverse = list
            .props
            .get("reverse")
            .and_then(serde_json::Value::as_bool);
        let span = self.lists.span(&place, count, reverse.unwrap_or(false));

        let list_path = at.path.join(" > ");
        let mut keys = HashSet::new();
        let mut items = Vec::with_capacity(span.len());
        for index in span {
            let value = self
                .clock
                .call(|| item.call::<Value>(index))
                .map_err(|error| {
                    Failure::lua(at.what.clone(), error)
                        .within(format!("{list_path}: item({index})"))
                })?;
            let mut child =
                from_lua_counted(self.lua, &value, Bound::Handler, self.tally, at.depth).map_err(
                    |error| {
                        let path = match error.path.as_str() {
                            "" => format!("{list_path} > [{index}]"),
                            inner => format!("{list_path} > [{index}] {inner}"),
                        };
                        Failure::tree(at.what.clone(), TreeError { path, ..error })
                    },
                )?;
            let key = child.key.get_or_insert_with(|| format!("#{index}")).clone();
            if !keys.insert(key.clone()) {
                let message = format!(
                    "two children have the key \"{}\"",
                    cut(&key, SHOWN_KEY_CHARS)
                );
                let error = TreeError {
                    path: list_path,
                    message,
                };
                return Err(Failure::tree(at.what.clone(), error));
            }
            let step = Step::Keyed(child.block.clone(), key);
            let path = format!("[{index}] {}", child.block);
            self.below(&mut child, at, step, path)?;
            items.push(child);
        }
        list.children = items;
        Ok(())
    }
}

#[cfg(test)]
#[path = "lists_tests.rs"]
mod tests;
