//! The tree diff against its fixtures (`tests/fixtures/patches/*.json`,
//! which the renderer's tests replay too) and against random trees, each
//! checked by applying its ops the way a renderer would.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use opencord_plugins::view::{IdSource, Node, Patch, Retained, reconcile};
use serde_json::{Value as Json, json};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/patches");

/// The fixtures the plan asks for; the folder may hold more.
const REQUIRED: &[&str] = &[
    "first_render",
    "append",
    "insert_middle_keyed",
    "move_keyed",
    "remove_with_subtree",
    "prop_change",
    "prop_unset",
    "same_key_new_block",
    "unkeyed_by_position",
    "events_change",
    "canvas_items",
    "identical",
];

struct Fixture {
    name: String,
    before: Option<Json>,
    after: Json,
    ops: Json,
}

fn fixtures() -> Vec<Fixture> {
    let mut paths: Vec<_> = std::fs::read_dir(FIXTURES)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            let text = std::fs::read_to_string(&path).unwrap();
            let mut file: Json =
                serde_json::from_str(&text).unwrap_or_else(|error| panic!("{name}: {error}"));
            let before = match file["before"].take() {
                Json::Null => None,
                tree => Some(tree),
            };
            Fixture {
                name,
                before,
                after: file["after"].take(),
                ops: file["ops"].take(),
            }
        })
        .collect()
}

/// A fixture tree (`{block, key?, props, events, children}`) as a node
/// whose handlers are `()`.
fn node(tree: &Json) -> Node<()> {
    node_with(tree, &mut |_| ())
}

fn node_with<H>(tree: &Json, handler: &mut impl FnMut(&str) -> H) -> Node<H> {
    let object = tree.as_object().expect("a tree is an object");
    Node {
        block: object["block"].as_str().unwrap().to_owned(),
        key: object
            .get("key")
            .map(|key| key.as_str().unwrap().to_owned()),
        props: object["props"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect(),
        handlers: object["events"]
            .as_array()
            .unwrap()
            .iter()
            .map(|event| {
                let event = event.as_str().unwrap();
                (event.to_owned(), handler(event))
            })
            .collect(),
        children: object["children"]
            .as_array()
            .unwrap()
            .iter()
            .map(|child| node_with(child, handler))
            .collect(),
    }
}

/// A fixture tree in the form [`Renderer::tree`] gives: a canvas's items
/// apart from its props, events as a set.
fn canonical(tree: &Json) -> Json {
    let block = tree["block"].as_str().unwrap();
    let mut props = tree["props"].as_object().unwrap().clone();
    let items = match block {
        "canvas" => props.remove("items").unwrap_or_else(|| json!([])),
        _ => json!([]),
    };
    let events: BTreeSet<&str> = tree["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|event| event.as_str().unwrap())
        .collect();
    let children: Vec<Json> = tree["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(canonical)
        .collect();
    json!({
        "block": block,
        "key": tree.get("key").cloned().unwrap_or(Json::Null),
        "props": props,
        "items": items,
        "events": events,
        "children": children,
    })
}

/// One node as a renderer holds it.
#[derive(Debug)]
struct Held {
    block: String,
    key: Option<String>,
    props: BTreeMap<String, Json>,
    events: Vec<String>,
    children: Vec<u64>,
    items: Vec<Json>,
}

/// A small reference renderer: applies ops the way the wire format says,
/// and fails on any op a correct diff wouldn't send (out of order, one
/// that changes nothing, one naming a node it doesn't have).
#[derive(Default)]
struct Renderer {
    nodes: BTreeMap<u64, Held>,
    /// Every id ever created, so none is made twice.
    made: HashSet<u64>,
}

/// Where each op may come in a batch.
fn rank(op: &Patch) -> u8 {
    match op {
        Patch::Create { .. } => 0,
        Patch::Props { .. } => 1,
        Patch::Events { .. } => 2,
        Patch::Draw { .. } => 3,
        Patch::Children { .. } => 4,
        Patch::Drop { .. } => 5,
        other => panic!("the diff sent {other:?}"),
    }
}

fn item_id(item: &Json) -> &str {
    item["id"].as_str().expect("an item's id is a string")
}

impl Renderer {
    fn apply(&mut self, ops: &[Patch], at: &str) {
        let mut last = 0;
        for op in ops {
            let rank = rank(op);
            assert!(rank >= last, "{at}: {op:?} came after a later kind of op");
            last = rank;
            self.apply_one(op, at);
        }
        self.check_shape(at);
    }

    fn held(&mut self, id: u64, at: &str) -> &mut Held {
        self.nodes
            .get_mut(&id)
            .unwrap_or_else(|| panic!("{at}: no node {id}"))
    }

    fn apply_one(&mut self, op: &Patch, at: &str) {
        match op {
            Patch::Create {
                id,
                block,
                key,
                props,
                events,
                children,
            } => {
                assert!(self.made.insert(*id), "{at}: node {id} made twice");
                for child in children {
                    assert!(
                        self.nodes.contains_key(child),
                        "{at}: {id}'s child {child} wasn't made before it"
                    );
                }
                assert!(
                    !(block == "canvas" && props.contains_key("items")),
                    "{at}: canvas items sent as a prop"
                );
                self.nodes.insert(
                    *id,
                    Held {
                        block: block.clone(),
                        key: key.clone(),
                        props: props.clone(),
                        events: events.clone(),
                        children: children.clone(),
                        items: Vec::new(),
                    },
                );
            }
            Patch::Props { id, set, unset } => {
                let held = self.held(*id, at);
                assert!(
                    !set.is_empty() || !unset.is_empty(),
                    "{at}: an empty props op"
                );
                assert!(
                    !(held.block == "canvas"
                        && (set.contains_key("items") || unset.iter().any(|n| n == "items"))),
                    "{at}: canvas items sent as a prop"
                );
                for (name, value) in set {
                    assert_ne!(
                        held.props.get(name),
                        Some(value),
                        "{at}: {name} sent unchanged"
                    );
                    held.props.insert(name.clone(), value.clone());
                }
                for name in unset {
                    assert!(
                        held.props.remove(name).is_some(),
                        "{at}: unset {name} it hadn't"
                    );
                }
            }
            Patch::Events { id, events } => {
                let held = self.held(*id, at);
                assert_ne!(&held.events, events, "{at}: events sent unchanged");
                held.events = events.clone();
            }
            Patch::Draw {
                id,
                set,
                remove,
                order,
            } => {
                let held = self.held(*id, at);
                assert_eq!(held.block, "canvas", "{at}: draw on a {}", held.block);
                assert!(
                    !set.is_empty() || !remove.is_empty() || order.is_some(),
                    "{at}: an empty draw op"
                );
                for gone in remove {
                    let index = held.items.iter().position(|item| item_id(item) == gone);
                    let index = index.unwrap_or_else(|| panic!("{at}: removes {gone} it hadn't"));
                    held.items.remove(index);
                }
                for item in set {
                    match held
                        .items
                        .iter()
                        .position(|kept| item_id(kept) == item_id(item))
                    {
                        Some(index) => {
                            assert_ne!(&held.items[index], item, "{at}: item sent unchanged");
                            held.items[index] = item.clone();
                        }
                        None => held.items.push(item.clone()),
                    }
                }
                if let Some(order) = order {
                    let now: Vec<&str> = held.items.iter().map(item_id).collect();
                    assert_ne!(now, *order, "{at}: an order that changes nothing");
                    let mut sorted_now = now.clone();
                    let mut sorted_order: Vec<&str> = order.iter().map(String::as_str).collect();
                    sorted_now.sort_unstable();
                    sorted_order.sort_unstable();
                    assert_eq!(sorted_now, sorted_order, "{at}: order isn't the items");
                    let items: Vec<Json> = order
                        .iter()
                        .map(|id| {
                            let item = held.items.iter().find(|item| item_id(item) == id);
                            item.unwrap().clone()
                        })
                        .collect();
                    held.items = items;
                }
            }
            Patch::Children { id, children } => {
                for child in children {
                    assert!(self.nodes.contains_key(child), "{at}: no child {child}");
                }
                let held = self.held(*id, at);
                assert_ne!(&held.children, children, "{at}: children sent unchanged");
                held.children = children.clone();
            }
            Patch::Drop { ids } => {
                assert!(!ids.is_empty(), "{at}: an empty drop op");
                for id in ids {
                    assert!(
                        self.nodes.remove(id).is_some(),
                        "{at}: drops {id} it hadn't"
                    );
                }
            }
            other => panic!("{at}: the diff sent {other:?}"),
        }
    }

    /// After a batch the nodes are one tree: every child is held and has
    /// one parent, one node has none, and every node is reachable from it.
    fn check_shape(&self, at: &str) {
        let mut parents = HashSet::new();
        for held in self.nodes.values() {
            for child in &held.children {
                assert!(
                    self.nodes.contains_key(child),
                    "{at}: child {child} was dropped"
                );
                assert!(parents.insert(*child), "{at}: {child} has two parents");
            }
        }
        let roots: Vec<u64> = self
            .nodes
            .keys()
            .copied()
            .filter(|id| !parents.contains(id))
            .collect();
        assert_eq!(roots.len(), 1, "{at}: roots {roots:?}");
        let mut reached = 0;
        let mut stack = roots;
        while let Some(id) = stack.pop() {
            reached += 1;
            stack.extend(&self.nodes[&id].children);
        }
        assert_eq!(
            reached,
            self.nodes.len(),
            "{at}: nodes left that nothing holds"
        );
    }

    fn root(&self) -> u64 {
        let children: HashSet<u64> = self
            .nodes
            .values()
            .flat_map(|held| held.children.iter().copied())
            .collect();
        *self.nodes.keys().find(|id| !children.contains(id)).unwrap()
    }

    fn tree(&self) -> Json {
        self.tree_at(self.root())
    }

    fn tree_at(&self, id: u64) -> Json {
        let held = &self.nodes[&id];
        let events: BTreeSet<&str> = held.events.iter().map(String::as_str).collect();
        let children: Vec<Json> = held.children.iter().map(|id| self.tree_at(*id)).collect();
        json!({
            "block": held.block,
            "key": held.key,
            "props": held.props,
            "items": held.items,
            "events": events,
            "children": children,
        })
    }
}

/// Reconciles `trees` one after another, applying each batch to a fresh
/// renderer, and checks that the renderer then holds that tree, and that
/// drawing the same tree again sends nothing.
fn replay(name: &str, trees: &[&Json]) {
    let mut ids = IdSource::new();
    let mut renderer = Renderer::default();
    let mut last: Option<Retained> = None;
    for (step, tree) in trees.iter().enumerate() {
        let at = format!("{name}, step {step}");
        let out = reconcile(last.as_ref(), node(tree), &mut ids);
        renderer.apply(&out.ops, &at);
        assert_eq!(
            renderer.tree(),
            canonical(tree),
            "{at}: the renderer's tree"
        );
        let again = reconcile(Some(&out.tree), node(tree), &mut ids);
        assert_eq!(again.ops, Vec::<Patch>::new(), "{at}: the same tree again");
        last = Some(again.tree);
    }
}

/// xorshift64, so the random trees are the same on every run.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }

    fn pick<'a>(&mut self, from: &[&'a str]) -> &'a str {
        from[self.below(from.len())]
    }

    fn shuffle<T>(&mut self, list: &mut [T]) {
        for index in (1..list.len()).rev() {
            list.swap(index, self.below(index + 1));
        }
    }
}

const BLOCKS: &[&str] = &["column", "row", "stack", "text", "button", "canvas", "menu"];
const CONTAINERS: &[&str] = &["column", "row", "stack"];
const KEYS: &[&str] = &["a", "b", "c", "d", "e", "f"];
const PROPS: &[&str] = &["gap", "text", "color"];
const EVENTS: &[&str] = &["press", "change", "hover_enter"];
const ITEM_IDS: &[&str] = &["p", "q", "r", "s", "t", "u"];
const DEEPEST: usize = 4;

fn random_value(rng: &mut Rng) -> Json {
    match rng.below(3) {
        0 => json!(rng.below(4)),
        1 => json!(format!("v{}", rng.below(3))),
        _ => json!({ "n": rng.below(2) }),
    }
}

fn random_items(rng: &mut Rng) -> Json {
    let mut ids = ITEM_IDS.to_vec();
    rng.shuffle(&mut ids);
    let mut items = Vec::new();
    for id in ids {
        if rng.chance(50) {
            items.push(json!({ "id": id, "x": rng.below(3) }));
        }
    }
    Json::Array(items)
}

fn random_events(rng: &mut Rng) -> Json {
    let events: Vec<&str> = EVENTS.iter().copied().filter(|_| rng.chance(40)).collect();
    json!(events)
}

fn random_tree(rng: &mut Rng, depth: usize) -> Json {
    let block = rng.pick(BLOCKS);
    let mut props = serde_json::Map::new();
    for name in PROPS {
        if rng.chance(50) {
            props.insert((*name).to_owned(), random_value(rng));
        }
    }
    match block {
        "canvas" => {
            props.insert("items".into(), random_items(rng));
        }
        // A menu's `items` is a plain prop, not drawing items.
        "menu" => {
            props.insert(
                "items".into(),
                json!([{ "value": rng.below(3), "label": "x" }]),
            );
        }
        _ => {}
    }
    let mut children = Vec::new();
    if CONTAINERS.contains(&block) && depth < DEEPEST {
        for _ in 0..rng.below(5) {
            let mut child = random_tree(rng, depth + 1);
            if rng.chance(50) {
                child["key"] = json!(rng.pick(KEYS));
            }
            children.push(child);
        }
        unique_keys(&mut children);
    }
    json!({ "block": block, "props": props, "events": random_events(rng), "children": children })
}

/// Drops repeated keys among siblings, as `from_lua` refuses them.
fn unique_keys(children: &mut [Json]) {
    let mut seen = HashSet::new();
    for child in children {
        if let Some(key) = child.get("key").and_then(Json::as_str)
            && !seen.insert(key.to_owned())
        {
            child.as_object_mut().unwrap().remove("key");
        }
    }
}

/// A tree like `tree`, changed here and there the ways a render changes.
fn mutate(rng: &mut Rng, mut tree: Json, depth: usize) -> Json {
    if rng.chance(6) {
        let mut fresh = random_tree(rng, depth);
        if let Some(key) = tree.get("key") {
            fresh["key"] = key.clone();
        }
        return fresh;
    }
    if rng.chance(6) {
        let block = rng.pick(BLOCKS);
        tree["block"] = json!(block);
        if !CONTAINERS.contains(&block) {
            tree["children"] = json!([]);
        }
        if block == "canvas" {
            tree["props"]["items"] = random_items(rng);
        }
    }
    for name in PROPS {
        if rng.chance(25) {
            let props = tree["props"].as_object_mut().unwrap();
            if props.contains_key(*name) && rng.chance(50) {
                props.remove(*name);
            } else {
                props.insert((*name).to_owned(), random_value(rng));
            }
        }
    }
    if tree["block"] == "canvas" {
        let items = mutate_items(rng, tree["props"]["items"].take());
        tree["props"]["items"] = items;
    }
    if rng.chance(20) {
        tree["events"] = random_events(rng);
    }
    let block = tree["block"].as_str().unwrap().to_owned();
    if CONTAINERS.contains(&block.as_str()) {
        let Json::Array(old) = tree["children"].take() else {
            panic!("children are a list");
        };
        let mut children = Vec::new();
        for child in old {
            if !rng.chance(15) {
                children.push(mutate(rng, child, depth + 1));
            }
        }
        if rng.chance(30) {
            rng.shuffle(&mut children);
        }
        if depth < DEEPEST {
            for _ in 0..rng.below(3) {
                let mut child = random_tree(rng, depth + 1);
                if rng.chance(50) {
                    child["key"] = json!(rng.pick(KEYS));
                }
                let at = rng.below(children.len() + 1);
                children.insert(at, child);
            }
        }
        for child in &mut children {
            if rng.chance(10) {
                toggle_key(rng, child);
            }
        }
        unique_keys(&mut children);
        tree["children"] = Json::Array(children);
    }
    // The root's key changes too, which replaces the whole tree.
    if depth == 0 && rng.chance(10) {
        toggle_key(rng, &mut tree);
    }
    tree
}

/// Takes the node's key away, or gives it one.
fn toggle_key(rng: &mut Rng, node: &mut Json) {
    if node.as_object_mut().unwrap().remove("key").is_none() {
        node["key"] = json!(rng.pick(KEYS));
    }
}

fn mutate_items(rng: &mut Rng, items: Json) -> Json {
    let mut items: Vec<Json> = match items {
        Json::Array(items) => items.into_iter().filter(|_| !rng.chance(20)).collect(),
        _ => Vec::new(),
    };
    for item in &mut items {
        if rng.chance(20) {
            item["x"] = json!(rng.below(3));
        }
    }
    for id in ITEM_IDS {
        if rng.chance(15) && !items.iter().any(|item| item["id"] == *id) {
            items.push(json!({ "id": id, "x": rng.below(3) }));
        }
    }
    if rng.chance(30) {
        rng.shuffle(&mut items);
    }
    Json::Array(items)
}

// Fails if any op of any fixture differs: a wrong id, a missing or extra
// op, ops out of their kinds' order, or a field named differently from the
// wire format (`set`, `unset`, `order`).
#[test]
fn every_fixture_gives_its_ops() {
    let fixtures = fixtures();
    let names: HashSet<&str> = fixtures
        .iter()
        .map(|fixture| fixture.name.as_str())
        .collect();
    for required in REQUIRED {
        assert!(names.contains(required), "no fixture {required}");
    }
    for fixture in &fixtures {
        let mut ids = IdSource::new();
        let before = fixture
            .before
            .as_ref()
            .map(|before| reconcile(None, node(before), &mut ids).tree);
        let out = reconcile(before.as_ref(), node(&fixture.after), &mut ids);
        let ops = serde_json::to_value(&out.ops).unwrap();
        assert_eq!(ops, fixture.ops, "fixture {}", fixture.name);
        let parsed: Vec<Patch> = serde_json::from_value(fixture.ops.clone()).unwrap();
        assert_eq!(parsed, out.ops, "fixture {} read back", fixture.name);
    }
}

// Fails if the diff leaves out a change (a moved child, an unset prop, an
// item's new place), sends one that changes nothing, sends ops out of
// order, drops a node still in the tree, or keeps one that left.
#[test]
fn applying_the_ops_rebuilds_the_tree() {
    for fixture in fixtures() {
        let mut trees = Vec::new();
        trees.extend(fixture.before.as_ref());
        trees.push(&fixture.after);
        replay(&fixture.name, &trees);
    }
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for pair in 0..1_000 {
        let before = random_tree(&mut rng, 0);
        let after = if rng.chance(15) {
            random_tree(&mut rng, 0)
        } else {
            mutate(&mut rng, before.clone(), 0)
        };
        let then = mutate(&mut rng, after.clone(), 0);
        replay(&format!("random pair {pair}"), &[&before, &after, &then]);
    }
}

// Fails if ids restart with each render, if a node made again (a new
// block under the same key) keeps its old id, or if a dropped id is
// handed out again.
#[test]
fn ids_are_never_reused() {
    let mut rng = Rng(7);
    let mut ids = IdSource::new();
    let mut last: Option<Retained> = None;
    let mut made = HashSet::new();
    let mut highest = 0;
    let mut tree = random_tree(&mut rng, 0);
    for step in 0..300 {
        let out = reconcile(last.as_ref(), node(&tree), &mut ids);
        if step == 0 {
            assert_eq!(out.tree.root(), 1, "the first id");
        }
        let mut newest = highest;
        for op in &out.ops {
            if let Patch::Create { id, .. } = op {
                assert!(made.insert(*id), "step {step}: id {id} made twice");
                assert!(
                    *id > highest,
                    "step {step}: id {id} is older than the last batch"
                );
                newest = newest.max(*id);
            }
        }
        highest = newest;
        last = Some(out.tree);
        tree = if rng.chance(10) {
            random_tree(&mut rng, 0)
        } else {
            mutate(&mut rng, tree, 0)
        };
    }
    assert!(made.len() > 300, "the walk made only {} nodes", made.len());
}

fn sorted(handlers: Vec<(u64, String, String)>) -> Vec<(u64, String, String)> {
    let mut handlers = handlers;
    handlers.sort();
    handlers
}

// Fails if a handler is listed under another node's id, under the wrong
// event, if a kept node's handler is the one from the render before, or
// if a dropped node's handlers still come back.
#[test]
fn handlers_come_back_by_node_id_and_event() {
    let first = json!({
        "block": "column", "props": {}, "events": [],
        "children": [
            { "block": "button", "key": "save", "props": { "text": "Save" },
              "events": ["press", "long_press"], "children": [] },
            { "block": "text_input", "props": {}, "events": ["change"], "children": [] }
        ]
    });
    // The keyed button moves after the input; both keep their ids.
    let second = json!({
        "block": "column", "props": {}, "events": [],
        "children": [
            { "block": "text_input", "props": {}, "events": ["change"], "children": [] },
            { "block": "button", "key": "save", "props": { "text": "Save" },
              "events": ["press", "long_press"], "children": [] }
        ]
    });
    let third = json!({
        "block": "column", "props": {}, "events": [],
        "children": [
            { "block": "text_input", "props": {}, "events": ["change"], "children": [] }
        ]
    });
    let handler = |render: &'static str| move |event: &str| format!("{render} {event}");
    let entry = |id: u64, event: &str, function: &str| (id, event.to_owned(), function.to_owned());

    let mut ids = IdSource::new();
    let one = reconcile(None, node_with(&first, &mut handler("first")), &mut ids);
    assert_eq!(
        sorted(one.handlers),
        vec![
            entry(2, "long_press", "first long_press"),
            entry(2, "press", "first press"),
            entry(3, "change", "first change"),
        ]
    );

    let two = reconcile(
        Some(&one.tree),
        node_with(&second, &mut handler("second")),
        &mut ids,
    );
    assert_eq!(
        sorted(two.handlers),
        vec![
            entry(2, "long_press", "second long_press"),
            entry(2, "press", "second press"),
            entry(3, "change", "second change"),
        ]
    );

    let three = reconcile(
        Some(&two.tree),
        node_with(&third, &mut handler("third")),
        &mut ids,
    );
    assert_eq!(
        sorted(three.handlers),
        vec![entry(3, "change", "third change")]
    );
}
