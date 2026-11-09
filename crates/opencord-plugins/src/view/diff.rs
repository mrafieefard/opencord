//! The tree diff (§11.3): compares a render's tree with the last one, as
//! React does, and gives the ops that turn the renderer's copy of the last
//! tree into the new one.
//!
//! - Siblings with keys match by key; siblings without keys match by their
//!   position among the unkeyed. The root matches the last root when their
//!   keys are equal.
//! - A match of the same block keeps its id; a match of another block is
//!   replaced: a new id, and the old subtree dropped. Nodes never move to
//!   another parent.
//! - Ops come in this order: `create` (children before parents), `props`,
//!   `events`, `draw`, `children`, then one `drop` listing every dropped id
//!   in ascending order. Within each kind, nodes come in the new tree's
//!   post-order. Identical trees give no ops.
//! - A canvas's `items` aren't a prop: they're diffed by item id into a
//!   `draw` op (see [`Patch::Draw`]), with `order` only when applying `set`
//!   and `remove` would leave the items in another order.

use std::collections::{BTreeMap, HashMap};

use super::patch::Patch;
use super::tree::Node;

type Json = serde_json::Value;

/// The block whose `items` go into `draw` ops, and that prop's name.
const CANVAS: &str = "canvas";
const ITEMS: &str = "items";

/// Hands out a view's node ids: 1, 2, 3, …, never one twice. Every tree of
/// a view (the main tree and each modal's) takes ids from the view's one
/// source.
#[derive(Debug, Default)]
pub struct IdSource {
    last: u64,
}

impl IdSource {
    pub fn new() -> Self {
        Self::default()
    }

    fn take(&mut self) -> u64 {
        self.last += 1;
        self.last
    }
}

/// The last tree sent, as the renderer holds it: ids, blocks, keys, props,
/// event names and children, and a canvas's items by id.
#[derive(Debug, Clone, PartialEq)]
pub struct Retained {
    root: Kept,
}

impl Retained {
    /// The root node's id.
    pub fn root(&self) -> u64 {
        self.root.id
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Kept {
    id: u64,
    block: String,
    key: Option<String>,
    /// Without a canvas's `items`.
    props: BTreeMap<String, Json>,
    /// Sorted.
    events: Vec<String>,
    children: Vec<Kept>,
    /// A canvas's items; empty for every other block.
    items: Items,
}

#[derive(Debug, Clone, PartialEq, Default)]
struct Items {
    order: Vec<String>,
    by_id: HashMap<String, Json>,
}

/// What [`reconcile`] gives: the tree to diff the next render against, the
/// ops for the renderer, and every handler of the new tree by node id and
/// event name.
pub struct Reconciled<H> {
    pub tree: Retained,
    pub ops: Vec<Patch>,
    pub handlers: Vec<(u64, String, H)>,
}

/// Diffs `next` against `prev` (`None` for a first render), taking ids for
/// new nodes from `ids`. `next` is as [`super::from_lua`] leaves it: sibling
/// keys unique, a canvas's items a list of objects with unique string ids.
pub fn reconcile<H>(prev: Option<&Retained>, next: Node<H>, ids: &mut IdSource) -> Reconciled<H> {
    let mut diff = Diff {
        ids,
        creates: Vec::new(),
        props: Vec::new(),
        events: Vec::new(),
        draws: Vec::new(),
        children: Vec::new(),
        dropped: Vec::new(),
        handlers: Vec::new(),
    };
    let old = prev.map(|tree| &tree.root);
    let matched = old.filter(|old| old.key == next.key);
    if let (Some(old), None) = (old, matched) {
        diff.drop_tree(old);
    }
    let root = diff.place(matched, next);

    let Diff {
        creates,
        props,
        events,
        draws,
        children,
        mut dropped,
        handlers,
        ..
    } = diff;
    let mut ops = creates;
    ops.extend(props);
    ops.extend(events);
    ops.extend(draws);
    ops.extend(children);
    if !dropped.is_empty() {
        dropped.sort_unstable();
        ops.push(Patch::Drop { ids: dropped });
    }
    Reconciled {
        tree: Retained { root },
        ops,
        handlers,
    }
}

/// The ops of one reconcile, by kind until they're joined in order.
struct Diff<'a, H> {
    ids: &'a mut IdSource,
    creates: Vec<Patch>,
    props: Vec<Patch>,
    events: Vec<Patch>,
    draws: Vec<Patch>,
    children: Vec<Patch>,
    dropped: Vec<u64>,
    handlers: Vec<(u64, String, H)>,
}

impl<H> Diff<'_, H> {
    /// `next` where `old` stood: kept under `old`'s id when it's the same
    /// block, else made anew with `old`'s subtree dropped.
    fn place(&mut self, old: Option<&Kept>, next: Node<H>) -> Kept {
        match old {
            Some(old) if old.block == next.block => self.update(old, next),
            Some(old) => {
                self.drop_tree(old);
                self.create(next)
            }
            None => self.create(next),
        }
    }

    fn create(&mut self, next: Node<H>) -> Kept {
        let id = self.ids.take();
        let Node {
            block,
            key,
            mut props,
            handlers,
            children,
        } = next;
        let children: Vec<Kept> = children
            .into_iter()
            .map(|child| self.create(child))
            .collect();
        let items = take_items(&block, &mut props);
        let events = self.events_of(id, handlers);
        self.creates.push(Patch::Create {
            id,
            block: block.clone(),
            key: key.clone(),
            props: props.clone(),
            events: events.clone(),
            children: children.iter().map(|child| child.id).collect(),
        });
        if let Some(draw) = draw(id, &Items::default(), &items) {
            self.draws.push(draw);
        }
        Kept {
            id,
            block,
            key,
            props,
            events,
            children,
            items,
        }
    }

    /// `next` matched `old`, of the same block, so it keeps `old`'s id and
    /// key.
    fn update(&mut self, old: &Kept, next: Node<H>) -> Kept {
        let id = old.id;
        let Node {
            block,
            key,
            mut props,
            handlers,
            children,
        } = next;
        let children = self.siblings(&old.children, children);
        let items = take_items(&block, &mut props);
        let events = self.events_of(id, handlers);

        if let Some(op) = props_op(id, &old.props, &props) {
            self.props.push(op);
        }
        if events != old.events {
            self.events.push(Patch::Events {
                id,
                events: events.clone(),
            });
        }
        if let Some(op) = draw(id, &old.items, &items) {
            self.draws.push(op);
        }
        let child_ids: Vec<u64> = children.iter().map(|child| child.id).collect();
        if !old
            .children
            .iter()
            .map(|child| child.id)
            .eq(child_ids.iter().copied())
        {
            self.children.push(Patch::Children {
                id,
                children: child_ids,
            });
        }
        Kept {
            id,
            block,
            key,
            props,
            events,
            children,
            items,
        }
    }

    /// Matches `next` against the last children of one parent and places
    /// each; the last children nothing matched are dropped.
    fn siblings(&mut self, old: &[Kept], next: Vec<Node<H>>) -> Vec<Kept> {
        let mut keyed = HashMap::new();
        let mut unkeyed = Vec::new();
        for (index, kept) in old.iter().enumerate() {
            match &kept.key {
                Some(key) => {
                    keyed.insert(key.as_str(), index);
                }
                None => unkeyed.push(index),
            }
        }
        // A last child matches at most once, even if keys repeat.
        let mut taken = vec![false; old.len()];
        let mut unkeyed_seen = 0;
        let mut placed = Vec::with_capacity(next.len());
        for child in next {
            let candidate = match &child.key {
                Some(key) => keyed.get(key.as_str()).copied(),
                None => {
                    unkeyed_seen += 1;
                    unkeyed.get(unkeyed_seen - 1).copied()
                }
            };
            let matched = candidate.filter(|&index| !taken[index]);
            if let Some(index) = matched {
                taken[index] = true;
            }
            placed.push(self.place(matched.map(|index| &old[index]), child));
        }
        for (kept, taken) in old.iter().zip(taken) {
            if !taken {
                self.drop_tree(kept);
            }
        }
        placed
    }

    /// The handler names of node `id`, in order; the handlers go to the
    /// list [`reconcile`] gives back.
    fn events_of(&mut self, id: u64, handlers: BTreeMap<String, H>) -> Vec<String> {
        let mut events = Vec::with_capacity(handlers.len());
        for (event, handler) in handlers {
            events.push(event.clone());
            self.handlers.push((id, event, handler));
        }
        events
    }

    fn drop_tree(&mut self, kept: &Kept) {
        self.dropped.push(kept.id);
        for child in &kept.children {
            self.drop_tree(child);
        }
    }
}

/// Takes a canvas's `items` list out of its props.
fn take_items(block: &str, props: &mut BTreeMap<String, Json>) -> Items {
    if block != CANVAS {
        return Items::default();
    }
    let list = match props.remove(ITEMS) {
        Some(Json::Array(list)) => list,
        // Not a list: `from_lua` never gives this, so it stays a prop.
        Some(other) => {
            props.insert(ITEMS.to_owned(), other);
            return Items::default();
        }
        None => return Items::default(),
    };
    let mut items = Items::default();
    for item in list {
        let id = match item.get("id") {
            Some(Json::String(id)) => id.clone(),
            other => other.unwrap_or(&Json::Null).to_string(),
        };
        // `from_lua` refuses repeated ids; should one come anyway, the
        // first item keeps it.
        if items.by_id.contains_key(&id) {
            continue;
        }
        items.order.push(id.clone());
        items.by_id.insert(id, item);
    }
    items
}

fn props_op(id: u64, old: &BTreeMap<String, Json>, new: &BTreeMap<String, Json>) -> Option<Patch> {
    let set: BTreeMap<String, Json> = new
        .iter()
        .filter(|(name, value)| old.get(*name) != Some(*value))
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    let unset: Vec<String> = old
        .keys()
        .filter(|name| !new.contains_key(*name))
        .cloned()
        .collect();
    (!set.is_empty() || !unset.is_empty()).then_some(Patch::Props { id, set, unset })
}

/// The `draw` op that turns canvas `id`'s `old` items into `new`, if any.
fn draw(id: u64, old: &Items, new: &Items) -> Option<Patch> {
    let remove: Vec<String> = old
        .order
        .iter()
        .filter(|item| !new.by_id.contains_key(*item))
        .cloned()
        .collect();
    let set: Vec<Json> = new
        .order
        .iter()
        .filter_map(|item| {
            let value = new.by_id.get(item)?;
            (old.by_id.get(item) != Some(value)).then(|| value.clone())
        })
        .collect();
    // Where the renderer leaves the items: the old ones still there, in
    // their order, then the new ones as `set` lists them.
    let left = old
        .order
        .iter()
        .filter(|item| new.by_id.contains_key(*item))
        .chain(
            new.order
                .iter()
                .filter(|item| !old.by_id.contains_key(*item)),
        );
    let order = (!left.eq(new.order.iter())).then(|| new.order.clone());
    if set.is_empty() && remove.is_empty() && order.is_none() {
        return None;
    }
    Some(Patch::Draw {
        id,
        set,
        remove,
        order,
    })
}
