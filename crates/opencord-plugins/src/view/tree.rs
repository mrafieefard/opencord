//! Turns what a client script's `render` returned into a [`Node`] tree,
//! checking every block, prop, event and child against [`crate::blocks`].
//!
//! A block is a Lua table whose metatable is the [`UI_NODE`] registry
//! value, with three fields the `ui.*` constructors set:
//! `block` (the block's name), `props` (a table, or nil) and `children`
//! (a list of blocks, or nil). Fields are read raw, so no metamethod of the
//! plugin's runs while a tree is checked.

use std::collections::{BTreeMap, HashSet};
use std::fmt;

use data_encoding::BASE64;
use mlua::{Function, Lua, Value};

use crate::blocks::{BlockSpec, COLORS, Children, IMAGE_SOURCES, KEY, PropKind, block, range};
use crate::limits::{
    CANVAS_MAX_ITEMS, CANVAS_MAX_PATH_POINTS, IMAGE_MAX_RAW_BYTES, VIEW_MAX_BLOCKS, VIEW_MAX_DEPTH,
    VIEW_MAX_OPTIONS, VIEW_MAX_PROP_CHARS, VIEW_MAX_TEXT_CHARS, VIEW_MAX_TREE_BYTES,
};

/// The registry key of the metatable every block carries.
pub const UI_NODE: &str = "opencord.ui_node";

/// One block of a tree. `H` is what a handler becomes: the Lua function in
/// the core, `()` where only the shape matters.
#[derive(Debug, Clone, PartialEq)]
pub struct Node<H> {
    pub block: String,
    pub key: Option<String>,
    pub props: BTreeMap<String, serde_json::Value>,
    /// By event name without `on_` (`press`); `list`'s `item` function is
    /// here too, under `item`.
    pub handlers: BTreeMap<String, H>,
    pub children: Vec<Node<H>>,
}

/// What's wrong with a tree, and where: `column > [2] row > [0] button`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeError {
    pub path: String,
    pub message: String,
}

impl fmt::Display for TreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            f.write_str(&self.message)
        } else {
            write!(f, "{}: {}", self.path, self.message)
        }
    }
}

impl std::error::Error for TreeError {}

/// Checks `value` and converts it; each handler goes through `handler`.
/// `lua` is the VM `value` came from; its registry holds [`UI_NODE`].
pub fn from_lua<H>(
    lua: &Lua,
    value: &Value,
    handler: impl Fn(Function) -> H,
) -> Result<Node<H>, TreeError> {
    let marker = match lua.named_registry_value::<Value>(UI_NODE) {
        Ok(Value::Table(marker)) => Some(marker.to_pointer()),
        Ok(_) => None,
        Err(error) => {
            return Err(TreeError {
                path: String::new(),
                message: lua_failed(error),
            });
        }
    };
    let mut walk = Walk {
        marker,
        handler: &handler,
        blocks: 0,
        bytes: 0,
        steps: Vec::new(),
    };
    walk.node(value, None, None)
}

/// How deeply plain data (canvas items, `animate`) nests inside one prop.
const MAX_DATA_DEPTH: usize = 32;
/// What a number, a boolean or a table costs of the tree's bytes.
const VALUE_BYTES: usize = 8;
/// A picker's `accept`: this many extensions of this many characters.
const MAX_STRINGS: usize = 100;
const MAX_STRING_CHARS: usize = 100;
/// The largest whole number a Lua number holds exactly.
const WHOLE: f64 = 9_007_199_254_740_992.0;
/// Names from the plugin are cut to this many characters in messages.
const SHOWN_CHARS: usize = 40;

type Json = serde_json::Value;
type Check<T> = Result<T, String>;
/// A block's key, props and handlers.
type Props<H> = (Option<String>, BTreeMap<String, Json>, BTreeMap<String, H>);

struct Walk<'a, F> {
    /// The `UI_NODE` metatable, if the VM has one.
    marker: Option<*const std::ffi::c_void>,
    handler: &'a F,
    blocks: usize,
    /// What the props so far cost (`VIEW_MAX_TREE_BYTES`).
    bytes: usize,
    /// One per block from the root down: `column`, `[2] row`.
    steps: Vec<String>,
}

impl<H, F: Fn(Function) -> H> Walk<'_, F> {
    fn node(
        &mut self,
        value: &Value,
        parent: Option<&'static BlockSpec>,
        index: Option<usize>,
    ) -> Result<Node<H>, TreeError> {
        self.steps
            .push(index.map(|index| format!("[{index}]")).unwrap_or_default());
        let node = self.check_node(value, parent, index);
        self.steps.pop();
        node
    }

    fn check_node(
        &mut self,
        value: &Value,
        parent: Option<&'static BlockSpec>,
        index: Option<usize>,
    ) -> Result<Node<H>, TreeError> {
        let Some(table) = self.as_block(value) else {
            return Err(self.fail("not a block"));
        };
        let name = match table.raw_get::<Value>(NODE_BLOCK) {
            Ok(Value::String(name)) => name.to_string_lossy(),
            Ok(_) => return Err(self.fail("not a block")),
            Err(error) => return Err(self.fail(lua_failed(error))),
        };
        let step = match index {
            Some(index) => format!("[{index}] {}", shown(&name)),
            None => shown(&name),
        };
        if let Some(last) = self.steps.last_mut() {
            *last = step;
        }

        if self.steps.len() > VIEW_MAX_DEPTH {
            return Err(self.fail(format!(
                "the tree is more than {VIEW_MAX_DEPTH} blocks deep"
            )));
        }
        self.blocks += 1;
        if self.blocks > VIEW_MAX_BLOCKS {
            return Err(self.fail(format!("the tree has more than {VIEW_MAX_BLOCKS} blocks")));
        }
        let Some(spec) = block(&name) else {
            return Err(self.fail(format!("unknown block \"{}\"", shown(&name))));
        };

        let in_stack = parent.is_some_and(|parent| parent.name == "stack");
        let props = self.props(&table, spec, in_stack);
        let (key, props, handlers) = self.here(props)?;
        if spec.name == "image" {
            let sources = image_sources(&props);
            self.here(sources)?;
        }
        let children = self.children(&table, spec)?;
        Ok(Node {
            block: spec.name.to_owned(),
            key,
            props,
            handlers,
            children,
        })
    }

    fn as_block(&self, value: &Value) -> Option<mlua::Table> {
        let Value::Table(table) = value else {
            return None;
        };
        let marker = self.marker?;
        table
            .metatable()
            .is_some_and(|metatable| metatable.to_pointer() == marker)
            .then(|| table.clone())
    }

    fn path(&self) -> String {
        self.steps.join(" > ")
    }

    fn fail(&self, message: impl Into<String>) -> TreeError {
        TreeError {
            path: self.path(),
            message: message.into(),
        }
    }

    fn here<T>(&self, checked: Check<T>) -> Result<T, TreeError> {
        checked.map_err(|message| self.fail(message))
    }

    fn props(
        &mut self,
        table: &mlua::Table,
        spec: &'static BlockSpec,
        in_stack: bool,
    ) -> Check<Props<H>> {
        let mut key = None;
        let mut props = BTreeMap::new();
        let mut handlers = BTreeMap::new();
        let given = match table.raw_get::<Value>(NODE_PROPS).map_err(lua_failed)? {
            Value::Nil => return Ok((key, props, handlers)),
            Value::Table(given) => given,
            _ => return Err("props must be a table".to_owned()),
        };

        // Unknown names are refused as they're met; the rest are bounded by
        // the schema, and checked in name order so errors don't depend on
        // the table's hash order.
        let mut known = Vec::new();
        for pair in given.pairs::<Value, Value>() {
            let (name, value) = pair.map_err(lua_failed)?;
            let Value::String(name) = name else {
                return Err("props must be named, like { gap = 8 }; \
                            children go in the second argument"
                    .to_owned());
            };
            let name = name.to_string_lossy();
            if name == KEY {
                key = Some(self.key(&value)?);
            } else if let Some(event) = name.strip_prefix("on_") {
                if !spec.has_event(event) {
                    return Err(format!("unknown event \"{}\"", shown(&name)));
                }
                let Value::Function(function) = value else {
                    return Err(format!("{name} must be a function"));
                };
                handlers.insert(event.to_owned(), (self.handler)(function));
            } else {
                let Some(kind) = spec.prop(&name, in_stack) else {
                    return Err(format!("unknown prop \"{}\"", shown(&name)));
                };
                known.push((name, kind, value));
            }
        }
        known.sort_by(|a, b| a.0.cmp(&b.0));

        for (name, kind, value) in known {
            if kind == PropKind::Function {
                let Value::Function(function) = value else {
                    return Err(format!("{name} must be a function"));
                };
                handlers.insert(name, (self.handler)(function));
                continue;
            }
            let converted = self.prop(spec, &name, kind, &value)?;
            props.insert(name, converted);
        }
        Ok((key, props, handlers))
    }

    fn children(
        &mut self,
        table: &mlua::Table,
        spec: &'static BlockSpec,
    ) -> Result<Vec<Node<H>>, TreeError> {
        let list = match table.raw_get::<Value>(NODE_CHILDREN) {
            Ok(Value::Nil) => return Ok(Vec::new()),
            Ok(Value::Table(list)) => list,
            Ok(_) => return Err(self.fail(NOT_A_CHILD_LIST)),
            Err(error) => return Err(self.fail(lua_failed(error))),
        };
        if list.raw_len() > VIEW_MAX_BLOCKS {
            return Err(self.fail(format!("the tree has more than {VIEW_MAX_BLOCKS} blocks")));
        }
        let length = list_length(&list).map_err(|error| self.fail(lua_failed(error)))?;
        let Some(length) = length else {
            return Err(self.fail(NOT_A_CHILD_LIST));
        };
        let refused = match spec.children {
            Children::None if length > 0 => Some(format!("{} takes no children", spec.name)),
            Children::Lazy if length > 0 => {
                Some(format!("{} takes no children; it draws item(i)", spec.name))
            }
            Children::One if length > 1 => Some(format!("{} takes one child", spec.name)),
            _ => None,
        };
        if let Some(message) = refused {
            return Err(self.fail(message));
        }

        let mut nodes = Vec::with_capacity(length);
        let mut keys = HashSet::new();
        for index in 0..length {
            let child = list
                .raw_get::<Value>(index + 1)
                .map_err(|error| self.fail(lua_failed(error)))?;
            let node = self.node(&child, Some(spec), Some(index))?;
            if let Some(key) = &node.key
                && !keys.insert(key.clone())
            {
                return Err(self.fail(format!("two children have the key \"{}\"", shown(key))));
            }
            nodes.push(node);
        }
        Ok(nodes)
    }

    /// Counts `bytes` against `VIEW_MAX_TREE_BYTES`.
    fn charge(&mut self, bytes: usize) -> Check<()> {
        self.bytes = self.bytes.saturating_add(bytes);
        if self.bytes > VIEW_MAX_TREE_BYTES {
            return Err(format!(
                "the tree's props are over {} MiB",
                VIEW_MAX_TREE_BYTES / (1024 * 1024)
            ));
        }
        Ok(())
    }

    fn key(&mut self, value: &Value) -> Check<String> {
        let key = match value {
            Value::String(_) => self.text(KEY, value, VIEW_MAX_PROP_CHARS)?,
            Value::Integer(_) | Value::Number(_) => {
                let number = lua_number(value).unwrap_or(f64::NAN);
                if !number.is_finite() || number.fract() != 0.0 || number.abs() >= WHOLE {
                    return Err("key must be a string or a whole number".to_owned());
                }
                (number as i64).to_string()
            }
            _ => return Err("key must be a string or a whole number".to_owned()),
        };
        self.charge(VALUE_BYTES)?;
        Ok(key)
    }

    fn prop(&mut self, spec: &BlockSpec, name: &str, kind: PropKind, value: &Value) -> Check<Json> {
        self.charge(VALUE_BYTES)?;
        match kind {
            PropKind::Number => {
                let number = number(spec.name, name, value)?;
                json_number(name, number)
            }
            PropKind::Integer => {
                let number = number(spec.name, name, value)?;
                if number.fract() != 0.0 {
                    return Err(format!("{name} must be a whole number"));
                }
                json_number(name, number)
            }
            PropKind::Text => {
                let limit = match (spec.name, name) {
                    ("text" | "rich_text", "text") => VIEW_MAX_TEXT_CHARS,
                    _ => VIEW_MAX_PROP_CHARS,
                };
                self.text(name, value, limit).map(Json::String)
            }
            PropKind::Bool => match value {
                Value::Boolean(flag) => Ok(Json::Bool(*flag)),
                _ => Err(format!("{name} must be true or false")),
            },
            PropKind::Color => self.color(name, value, true),
            PropKind::Size => match value {
                Value::String(word) if matches!(&*word.as_bytes(), b"fill" | b"fit") => {
                    Ok(Json::String(word.to_string_lossy()))
                }
                Value::Integer(_) | Value::Number(_) => {
                    let number = number(spec.name, name, value)?;
                    json_number(name, number)
                }
                _ => Err(format!("{name} must be a number, \"fill\" or \"fit\"")),
            },
            PropKind::Insets => self.insets(spec.name, name, value),
            PropKind::Choice(choices) => match value {
                Value::String(word)
                    if choices
                        .iter()
                        .any(|choice| choice.as_bytes() == &*word.as_bytes()) =>
                {
                    Ok(Json::String(word.to_string_lossy()))
                }
                _ => Err(format!("{name} must be one of {}", choices.join(", "))),
            },
            PropKind::Options => self.options(spec.name, name, value),
            PropKind::Items => self.items(name, value),
            PropKind::Json => self.plain(name, value, 0),
            PropKind::Stroke => self.stroke(spec.name, name, value),
            PropKind::Bytes => self.bytes(name, value),
            PropKind::Id => match value {
                Value::String(id)
                    if id.as_bytes().iter().all(u8::is_ascii_digit)
                        && id.to_str().is_ok_and(|id| id.parse::<u64>().is_ok()) =>
                {
                    self.charge(id.as_bytes().len())?;
                    Ok(Json::String(id.to_string_lossy()))
                }
                _ => Err(format!("{name} must be an id string")),
            },
            PropKind::Strings => self.strings(name, value),
            PropKind::Function => Err(format!("{name} must be a function")),
        }
    }

    /// A string of at most `limit` characters, in UTF-8.
    fn text(&mut self, name: &str, value: &Value, limit: usize) -> Check<String> {
        let Value::String(text) = value else {
            return Err(format!("{name} must be a string"));
        };
        let bytes = text.as_bytes();
        let too_long = || format!("{name} is longer than {limit} characters");
        // A character is at most 4 bytes, so a longer string needn't be read.
        if bytes.len() > limit.saturating_mul(4) {
            return Err(too_long());
        }
        let Ok(text) = std::str::from_utf8(&bytes) else {
            return Err(format!("{name} must be text in UTF-8"));
        };
        if text.chars().count() > limit {
            return Err(too_long());
        }
        self.charge(text.len())?;
        Ok(text.to_owned())
    }

    fn color(&mut self, name: &str, value: &Value, gradient: bool) -> Check<Json> {
        let wrong = || {
            if gradient {
                format!(
                    "{name} must be a colour name, \"#rrggbb\", \"#rrggbbaa\" \
                     or a gradient {{from, to, angle?}}"
                )
            } else {
                format!("{name} must be a colour name, \"#rrggbb\" or \"#rrggbbaa\"")
            }
        };
        match value {
            Value::String(text) => {
                let text = text.to_str().map_err(|_| wrong())?.to_owned();
                if !is_color(&text) {
                    return Err(wrong());
                }
                self.charge(text.len())?;
                Ok(Json::String(text))
            }
            Value::Table(table) if gradient => {
                let mut fields = fields(table, name, &["from", "to", "angle"])?;
                let (Some(from), Some(to)) = (fields.remove("from"), fields.remove("to")) else {
                    return Err(format!("{name} needs from and to"));
                };
                let mut gradient = serde_json::Map::new();
                gradient.insert(
                    "from".into(),
                    self.color(&format!("{name}.from"), &from, false)?,
                );
                gradient.insert("to".into(), self.color(&format!("{name}.to"), &to, false)?);
                if let Some(angle) = fields.remove("angle") {
                    let angle_name = format!("{name}.angle");
                    let angle = finite(&angle_name, &angle)?;
                    if !(-360.0..=360.0).contains(&angle) {
                        return Err(format!("{angle_name} must be between -360 and 360"));
                    }
                    gradient.insert("angle".into(), json_number(&angle_name, angle)?);
                }
                self.charge(VALUE_BYTES)?;
                Ok(Json::Object(gradient))
            }
            _ => Err(wrong()),
        }
    }

    fn insets(&mut self, block: &str, name: &str, value: &Value) -> Check<Json> {
        let table = match value {
            Value::Integer(_) | Value::Number(_) => {
                let number = number(block, name, value)?;
                return json_number(name, number);
            }
            Value::Table(table) => table,
            _ => {
                return Err(format!(
                    "{name} must be a number, {{top, right, bottom, left}} or {{x, y}}"
                ));
            }
        };
        let mut fields = fields(table, name, &["top", "right", "bottom", "left", "x", "y"])?;
        let axes = fields.contains_key("x") || fields.contains_key("y");
        let sides = ["top", "right", "bottom", "left"]
            .iter()
            .any(|side| fields.contains_key(side));
        if axes && sides {
            return Err(format!(
                "{name} takes {{top, right, bottom, left}} or {{x, y}}, not both"
            ));
        }
        let mut side = |field: &str| -> Check<f64> {
            match fields.remove(field) {
                None => Ok(0.0),
                Some(value) => number(block, &format!("{name}.{field}"), &value),
            }
        };
        let (top, right, bottom, left) = if axes {
            let (x, y) = (side("x")?, side("y")?);
            (y, x, y, x)
        } else {
            (side("top")?, side("right")?, side("bottom")?, side("left")?)
        };
        let mut insets = serde_json::Map::new();
        for (field, number) in [
            ("top", top),
            ("right", right),
            ("bottom", bottom),
            ("left", left),
        ] {
            insets.insert(field.into(), json_number(name, number)?);
        }
        self.charge(4 * VALUE_BYTES)?;
        Ok(Json::Object(insets))
    }

    fn options(&mut self, block: &str, name: &str, value: &Value) -> Check<Json> {
        let wrong = || format!("{name} must be a list of {{value, label, icon?}}");
        let Value::Table(list) = value else {
            return Err(wrong());
        };
        if list.raw_len() > VIEW_MAX_OPTIONS {
            return Err(format!("{name} has more than {VIEW_MAX_OPTIONS} options"));
        }
        let length = list_length(list).map_err(lua_failed)?.ok_or_else(wrong)?;
        let allowed: &[&str] = if block == "menu" {
            &["value", "label", "icon", "danger"]
        } else {
            &["value", "label", "icon"]
        };
        let mut options = Vec::with_capacity(length);
        for index in 0..length {
            let entry_name = format!("{name}[{index}]");
            let Value::Table(entry) = list.raw_get::<Value>(index + 1).map_err(lua_failed)? else {
                return Err(wrong());
            };
            let mut fields = fields(&entry, &entry_name, allowed)?;
            let mut option = serde_json::Map::new();
            let option_value = match fields.remove("value") {
                Some(Value::Boolean(flag)) => Json::Bool(flag),
                Some(text @ Value::String(_)) => Json::String(self.text(
                    &format!("{entry_name}.value"),
                    &text,
                    VIEW_MAX_PROP_CHARS,
                )?),
                Some(number @ (Value::Integer(_) | Value::Number(_))) => {
                    let value_name = format!("{entry_name}.value");
                    let number = finite(&value_name, &number)?;
                    json_number(&value_name, number)?
                }
                _ => {
                    return Err(format!(
                        "{entry_name}.value must be a string, a number or true or false"
                    ));
                }
            };
            option.insert("value".into(), option_value);
            let label = fields.remove("label").unwrap_or(Value::Nil);
            let label = self.text(&format!("{entry_name}.label"), &label, VIEW_MAX_PROP_CHARS)?;
            option.insert("label".into(), Json::String(label));
            if let Some(icon) = fields.remove("icon") {
                let icon = self.text(&format!("{entry_name}.icon"), &icon, VIEW_MAX_PROP_CHARS)?;
                option.insert("icon".into(), Json::String(icon));
            }
            if let Some(danger) = fields.remove("danger") {
                let Value::Boolean(danger) = danger else {
                    return Err(format!("{entry_name}.danger must be true or false"));
                };
                option.insert("danger".into(), Json::Bool(danger));
            }
            self.charge(4 * VALUE_BYTES)?;
            options.push(Json::Object(option));
        }
        Ok(Json::Array(options))
    }

    fn items(&mut self, name: &str, value: &Value) -> Check<Json> {
        let wrong = || format!("{name} must be a list of drawing items");
        let Value::Table(list) = value else {
            return Err(wrong());
        };
        if list.raw_len() > CANVAS_MAX_ITEMS {
            return Err(format!("{name} has more than {CANVAS_MAX_ITEMS} items"));
        }
        let length = list_length(list).map_err(lua_failed)?.ok_or_else(wrong)?;
        let mut ids = HashSet::new();
        let mut items = Vec::with_capacity(length);
        for index in 0..length {
            let item_name = format!("{name}[{index}]");
            let item = list.raw_get::<Value>(index + 1).map_err(lua_failed)?;
            let Json::Object(mut item) = self.plain(&item_name, &item, 1)? else {
                return Err(format!("{item_name} must be a table with an id"));
            };
            let id = match item.get("id") {
                Some(Json::String(id)) => id.clone(),
                Some(Json::Number(id)) if id.is_i64() || id.is_u64() => id.to_string(),
                Some(_) => {
                    return Err(format!("{item_name}.id must be a string or a whole number"));
                }
                None => return Err(format!("{item_name} needs an id")),
            };
            if !ids.insert(id.clone()) {
                return Err(format!(
                    "{name} has two items with the id \"{}\"",
                    shown(&id)
                ));
            }
            if let Some(Json::Array(points)) = item.get("points")
                && points.len() > 2 * CANVAS_MAX_PATH_POINTS
            {
                return Err(format!(
                    "{item_name}.points has more than {CANVAS_MAX_PATH_POINTS} points"
                ));
            }
            item.insert("id".into(), Json::String(id));
            items.push(Json::Object(item));
        }
        Ok(Json::Array(items))
    }

    /// Plain data: nil, booleans, finite numbers, strings, and tables that
    /// are either lists with no gaps or named fields.
    fn plain(&mut self, name: &str, value: &Value, depth: usize) -> Check<Json> {
        self.charge(VALUE_BYTES)?;
        match value {
            Value::Nil => Ok(Json::Null),
            Value::LightUserData(data) if data.0.is_null() => Ok(Json::Null),
            Value::Boolean(flag) => Ok(Json::Bool(*flag)),
            Value::Integer(_) | Value::Number(_) => {
                let number = finite(name, value)?;
                json_number(name, number)
            }
            Value::String(_) => self
                .text(name, value, VIEW_MAX_PROP_CHARS)
                .map(Json::String),
            Value::Table(table) => {
                if depth >= MAX_DATA_DEPTH {
                    return Err(format!("{name} nests deeper than {MAX_DATA_DEPTH} levels"));
                }
                self.plain_table(name, table, depth)
            }
            other => Err(format!(
                "{name} must be plain data, not a {}",
                other.type_name()
            )),
        }
    }

    fn plain_table(&mut self, name: &str, table: &mlua::Table, depth: usize) -> Check<Json> {
        let length = table.raw_len();
        let mut listed = 0;
        let mut named = Vec::new();
        for pair in table.pairs::<Value, Value>() {
            let (key, item) = pair.map_err(lua_failed)?;
            self.charge(VALUE_BYTES)?;
            match key {
                Value::String(key) => named.push((key, item)),
                Value::Integer(_) | Value::Number(_) => {
                    let index = lua_number(&key).unwrap_or(f64::NAN);
                    if index.fract() != 0.0 || index < 1.0 || index > length as f64 {
                        return Err(format!("{name} has a list with gaps"));
                    }
                    listed += 1;
                }
                _ => return Err(format!("{name} has a key that isn't a string or a number")),
            }
            if listed > 0 && !named.is_empty() {
                return Err(format!("{name} mixes list items and named fields"));
            }
        }
        if listed > 0 {
            if listed != length {
                return Err(format!("{name} has a list with gaps"));
            }
            let mut list = Vec::with_capacity(length);
            for index in 1..=length {
                let item = table.raw_get::<Value>(index).map_err(lua_failed)?;
                list.push(self.plain(name, &item, depth + 1)?);
            }
            return Ok(Json::Array(list));
        }
        let mut object = serde_json::Map::new();
        for (key, item) in named {
            let key = self.text(name, &Value::String(key), VIEW_MAX_PROP_CHARS)?;
            let item = self.plain(name, &item, depth + 1)?;
            object.insert(key, item);
        }
        Ok(Json::Object(object))
    }

    fn stroke(&mut self, block: &str, name: &str, value: &Value) -> Check<Json> {
        let Value::Table(table) = value else {
            return Err(format!("{name} must be {{width, color}}"));
        };
        let mut fields = fields(table, name, &["width", "color"])?;
        let mut stroke = serde_json::Map::new();
        if let Some(width) = fields.remove("width") {
            let width_name = format!("{name}.width");
            let (min, max) = range(block, name);
            let width = finite(&width_name, &width)?;
            if !(min..=max).contains(&width) {
                return Err(format!("{width_name} must be between {min} and {max}"));
            }
            stroke.insert("width".into(), json_number(&width_name, width)?);
        }
        if let Some(color) = fields.remove("color") {
            stroke.insert(
                "color".into(),
                self.color(&format!("{name}.color"), &color, false)?,
            );
        }
        Ok(Json::Object(stroke))
    }

    fn bytes(&mut self, name: &str, value: &Value) -> Check<Json> {
        let too_large = || format!("{name} is over {} KiB", IMAGE_MAX_RAW_BYTES / 1024);
        let encoded = match value {
            Value::String(text) => {
                let bytes = text.as_bytes();
                if bytes.len() > IMAGE_MAX_RAW_BYTES {
                    return Err(too_large());
                }
                BASE64.encode(&bytes)
            }
            Value::Buffer(buffer) => {
                if buffer.len() > IMAGE_MAX_RAW_BYTES {
                    return Err(too_large());
                }
                BASE64.encode(&buffer.to_vec())
            }
            _ => return Err(format!("{name} must be a string or a buffer")),
        };
        self.charge(encoded.len())?;
        Ok(Json::String(encoded))
    }

    fn strings(&mut self, name: &str, value: &Value) -> Check<Json> {
        let wrong = || format!("{name} must be a list of strings");
        let Value::Table(list) = value else {
            return Err(wrong());
        };
        if list.raw_len() > MAX_STRINGS {
            return Err(format!("{name} has more than {MAX_STRINGS} entries"));
        }
        let length = list_length(list).map_err(lua_failed)?.ok_or_else(wrong)?;
        let mut strings = Vec::with_capacity(length);
        for index in 0..length {
            let entry = list.raw_get::<Value>(index + 1).map_err(lua_failed)?;
            let entry = self.text(&format!("{name}[{index}]"), &entry, MAX_STRING_CHARS)?;
            strings.push(Json::String(entry));
        }
        Ok(Json::Array(strings))
    }
}

const NODE_BLOCK: &str = "block";
const NODE_PROPS: &str = "props";
const NODE_CHILDREN: &str = "children";
const NOT_A_CHILD_LIST: &str = "children must be a list of blocks, with no gaps";

/// The number of items when `list` is a list with no gaps (keys exactly 1
/// to its length), else `None`.
fn list_length(list: &mlua::Table) -> mlua::Result<Option<usize>> {
    let length = list.raw_len();
    let mut count = 0;
    for pair in list.pairs::<Value, Value>() {
        let (key, _) = pair?;
        let in_range = lua_number(&key)
            .is_some_and(|index| index.fract() == 0.0 && index >= 1.0 && index <= length as f64);
        if !in_range {
            return Ok(None);
        }
        count += 1;
    }
    Ok((count == length).then_some(length))
}

/// An image's source props: exactly one of [`IMAGE_SOURCES`].
fn image_sources(props: &BTreeMap<String, Json>) -> Check<()> {
    let given: Vec<&str> = IMAGE_SOURCES
        .iter()
        .copied()
        .filter(|source| props.contains_key(*source))
        .collect();
    match given[..] {
        [_] => Ok(()),
        [] => Err(format!(
            "an image needs one of {}",
            IMAGE_SOURCES[..IMAGE_SOURCES.len() - 1].join(", ")
                + " or "
                + IMAGE_SOURCES[IMAGE_SOURCES.len() - 1]
        )),
        [first, second, ..] => Err(format!(
            "an image takes one source, not {first} and {second}"
        )),
    }
}

/// The named fields of `table`, each one of `allowed`.
fn fields(
    table: &mlua::Table,
    name: &str,
    allowed: &[&'static str],
) -> Check<BTreeMap<&'static str, Value>> {
    let mut fields = BTreeMap::new();
    for pair in table.pairs::<Value, Value>() {
        let (key, value) = pair.map_err(lua_failed)?;
        let field = match &key {
            Value::String(key) => allowed
                .iter()
                .copied()
                .find(|field| field.as_bytes() == &*key.as_bytes()),
            _ => None,
        };
        let Some(field) = field else {
            let key = match &key {
                Value::String(key) => key.to_string_lossy(),
                other => other.type_name().to_owned(),
            };
            return Err(format!("{name} takes no field \"{}\"", shown(&key)));
        };
        fields.insert(field, value);
    }
    Ok(fields)
}

fn lua_number(value: &Value) -> Option<f64> {
    match value {
        Value::Integer(number) => Some(*number as f64),
        Value::Number(number) => Some(*number),
        _ => None,
    }
}

fn finite(name: &str, value: &Value) -> Check<f64> {
    let number = lua_number(value).ok_or_else(|| format!("{name} must be a number"))?;
    if !number.is_finite() {
        return Err(format!("{name} must be a finite number"));
    }
    Ok(number)
}

/// A finite number in the range [`range`] gives `prop` on `block`; `name`
/// is what messages call it (`padding.top`).
fn number(block: &str, name: &str, value: &Value) -> Check<f64> {
    let number = finite(name, value)?;
    let prop = name.split('.').next().unwrap_or(name);
    let (min, max) = range(block, prop);
    if !(min..=max).contains(&number) {
        return Err(format!("{name} must be between {min} and {max}"));
    }
    Ok(number)
}

/// Whole numbers go as integers, so `2` and `2.0` compare equal in a diff.
fn json_number(name: &str, number: f64) -> Check<Json> {
    if number.fract() == 0.0 && number.abs() < WHOLE {
        return Ok(Json::from(number as i64));
    }
    serde_json::Number::from_f64(number)
        .map(Json::Number)
        .ok_or_else(|| format!("{name} must be a finite number"))
}

fn is_color(text: &str) -> bool {
    match text.strip_prefix('#') {
        Some(hex) => matches!(hex.len(), 6 | 8) && hex.bytes().all(|b| b.is_ascii_hexdigit()),
        None => COLORS.contains(&text),
    }
}

/// `text` cut to [`SHOWN_CHARS`] characters, for messages.
fn shown(text: &str) -> String {
    match text.char_indices().nth(SHOWN_CHARS) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text.to_owned(),
    }
}

fn lua_failed(error: mlua::Error) -> String {
    format!("Lua failed: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::{BLOCKS, block};
    use crate::limits::{VIEW_MAX_BLOCKS, VIEW_MAX_DEPTH};
    use mlua::Table;
    use serde_json::json;

    /// A Lua with a stand-in for the core's `ui`: `ui.<block>(props,
    /// children)` returns `{block, props, children}` with the `UI_NODE`
    /// metatable.
    fn lua() -> Lua {
        let lua = Lua::new();
        let marker = lua.create_table().unwrap();
        lua.set_named_registry_value(UI_NODE, &marker).unwrap();
        let ui: Table = lua
            .load(
                r#"
                local marker = ...
                return setmetatable({}, { __index = function(_, block)
                    return function(props, children)
                        return setmetatable(
                            { block = block, props = props, children = children },
                            marker
                        )
                    end
                end })
                "#,
            )
            .call(marker)
            .unwrap();
        lua.globals().set("ui", ui).unwrap();
        lua
    }

    fn check(source: &str) -> Result<Node<()>, TreeError> {
        let lua = lua();
        let value: Value = lua.load(source).eval().unwrap();
        from_lua(&lua, &value, |_| ())
    }

    fn refused(source: &str) -> String {
        match check(source) {
            Ok(node) => panic!("accepted {source}: {node:?}"),
            Err(error) => error.to_string(),
        }
    }

    fn props(node: &Node<()>) -> serde_json::Value {
        serde_json::to_value(&node.props).unwrap()
    }

    // Fails if keys are dropped or not turned into text, if `on_press`
    // isn't stored as `press`, if a handler isn't the function given, or if
    // `padding = {x, y}` isn't sent as four sides.
    #[test]
    fn a_tree_converts_with_its_keys_props_and_handlers() {
        let lua = lua();
        let value: Value = lua
            .load(
                r##"
                press = function() return "pressed" end
                return ui.column({ gap = 8, padding = { x = 4, y = 2 }, background = "surface" }, {
                    ui.text({ key = "title", text = "Hi", size = 18, color = "#ff0000" }),
                    ui.button({ key = 7, text = "Go", variant = "primary", on_press = press }),
                })
                "##,
            )
            .eval()
            .unwrap();

        let column = from_lua(&lua, &value, |function| function).unwrap();

        assert_eq!(column.block, "column");
        assert_eq!(column.key, None);
        assert_eq!(
            serde_json::to_value(&column.props).unwrap(),
            json!({
                "gap": 8,
                "padding": { "top": 2, "right": 4, "bottom": 2, "left": 4 },
                "background": "surface",
            })
        );
        assert!(column.handlers.is_empty());
        let [title, go] = &column.children[..] else {
            panic!("two children: {:?}", column.children);
        };
        assert_eq!(title.block, "text");
        assert_eq!(title.key.as_deref(), Some("title"));
        assert_eq!(
            serde_json::to_value(&title.props).unwrap(),
            json!({ "text": "Hi", "size": 18, "color": "#ff0000" })
        );
        assert!(title.children.is_empty());
        assert_eq!(go.key.as_deref(), Some("7"));
        assert_eq!(
            serde_json::to_value(&go.props).unwrap(),
            json!({ "text": "Go", "variant": "primary" })
        );
        assert_eq!(go.handlers.keys().collect::<Vec<_>>(), ["press"]);
        assert_eq!(go.handlers["press"].call::<String>(()).unwrap(), "pressed");
    }

    // Fails if the path skips a step, counts children from 1, or names the
    // parent instead of the unknown block.
    #[test]
    fn an_unknown_block_is_refused_with_its_path() {
        assert_eq!(
            refused(
                r#"return ui.column({}, {
                    ui.text({ text = "a" }),
                    ui.text({ text = "b" }),
                    ui.row({}, { ui.buton({}) }),
                })"#
            ),
            r#"column > [2] row > [0] buton: unknown block "buton""#
        );
    }

    // Fails if props or events the block doesn't have are let through, or
    // if a handler may be something other than a function.
    #[test]
    fn an_unknown_prop_is_refused() {
        assert_eq!(
            refused(r#"return ui.text({ text = "a", colour = "accent" })"#),
            r#"text: unknown prop "colour""#
        );
        assert_eq!(
            refused(r#"return ui.text({ text = "a", on_press = function() end })"#),
            r#"text: unknown event "on_press""#
        );
        assert_eq!(
            refused(r#"return ui.button({ text = "a", on_press = "go" })"#),
            "button: on_press must be a function"
        );
        assert_eq!(
            refused(r#"return ui.column({ ui.text({ text = "a" }) })"#),
            "column: props must be named, like { gap = 8 }; children go in the second argument"
        );
        // `x` belongs to children of a stack only.
        assert_eq!(
            refused(r#"return ui.column({}, { ui.box({ x = 4 }) })"#),
            r#"column > [0] box: unknown prop "x""#
        );
        assert!(check(r#"return ui.stack({}, { ui.box({ x = 4, y = -2, right = 0 }) })"#).is_ok());
    }

    // Fails if a size takes NaN, infinity, a negative number or a word
    // other than fill and fit, or if a font size or an opacity isn't
    // bounded.
    #[test]
    fn a_nan_size_is_refused_with_its_path() {
        assert_eq!(
            refused(r#"return ui.column({}, { ui.box({ width = 0/0 }) })"#),
            "column > [0] box: width must be a finite number"
        );
        assert_eq!(
            refused(r#"return ui.box({ max_height = math.huge })"#),
            "box: max_height must be a finite number"
        );
        assert_eq!(
            refused(r#"return ui.box({ width = "wide" })"#),
            r#"box: width must be a number, "fill" or "fit""#
        );
        assert_eq!(
            refused(r#"return ui.box({ height = -1 })"#),
            "box: height must be between 0 and 100000"
        );
        assert_eq!(
            refused(r#"return ui.text({ text = "a", size = 5 })"#),
            "text: size must be between 6 and 200"
        );
        assert_eq!(
            refused(r#"return ui.text({ text = "a", opacity = 1.5 })"#),
            "text: opacity must be between 0 and 1"
        );
        assert_eq!(
            refused(r#"return ui.box({ padding = { top = 0/0 } })"#),
            "box: padding.top must be a finite number"
        );
        let node =
            check(r#"return ui.box({ width = "fill", height = 120.5, padding = 3 })"#).unwrap();
        assert_eq!(
            props(&node),
            json!({ "width": "fill", "height": 120.5, "padding": 3 })
        );
    }

    // Fails if sibling keys aren't compared, or a number key and the same
    // text aren't the same key.
    #[test]
    fn a_duplicate_key_is_refused() {
        assert_eq!(
            refused(
                r#"return ui.column({}, {
                    ui.text({ key = "a", text = "1" }),
                    ui.text({ text = "2" }),
                    ui.text({ key = "a", text = "3" }),
                })"#
            ),
            r#"column: two children have the key "a""#
        );
        assert_eq!(
            refused(r#"return ui.row({}, { ui.spacer({ key = 1 }), ui.spacer({ key = "1" }) })"#),
            r#"row: two children have the key "1""#
        );
        // The same key under two parents is fine.
        assert!(
            check(
                r#"return ui.column({}, {
                    ui.row({ key = "a" }, { ui.spacer({ key = "a" }) }),
                    ui.row({ key = "b" }, { ui.spacer({ key = "a" }) }),
                })"#
            )
            .is_ok()
        );
    }

    // Fails if blocks aren't counted, or counted with an off-by-one.
    #[test]
    fn too_many_blocks_is_refused() {
        let source = |children: usize| {
            format!(
                "local children = {{}}
                 for i = 1, {children} do children[i] = ui.spacer({{}}) end
                 return ui.column({{}}, children)"
            )
        };
        let full = check(&source(VIEW_MAX_BLOCKS - 1)).unwrap();
        assert_eq!(full.children.len(), VIEW_MAX_BLOCKS - 1);
        assert_eq!(
            refused(&source(VIEW_MAX_BLOCKS)),
            format!(
                "column > [{}] spacer: the tree has more than {VIEW_MAX_BLOCKS} blocks",
                VIEW_MAX_BLOCKS - 1
            )
        );
    }

    // Fails if depth isn't counted (the cycle then overflows the stack),
    // or counted with an off-by-one.
    #[test]
    fn too_deep_is_refused() {
        let source = |depth: usize| {
            format!(
                "local node = ui.spacer({{}})
                 for i = 2, {depth} do node = ui.box({{}}, {{ node }}) end
                 return node"
            )
        };
        assert!(check(&source(VIEW_MAX_DEPTH)).is_ok());
        assert!(refused(&source(VIEW_MAX_DEPTH + 1)).ends_with(&format!(
            "spacer: the tree is more than {VIEW_MAX_DEPTH} blocks deep"
        )));
        assert!(
            refused("local node = ui.box({}, {}); node.children[1] = node; return node").ends_with(
                &format!("box: the tree is more than {VIEW_MAX_DEPTH} blocks deep")
            )
        );
    }

    // Fails if a table that only looks like a block, or carries another
    // metatable, passes.
    #[test]
    fn a_plain_table_is_not_a_block() {
        assert_eq!(
            refused(r#"return { block = "text", props = { text = "a" } }"#),
            "not a block"
        );
        assert_eq!(
            refused(r#"return setmetatable({ block = "text" }, {})"#),
            "not a block"
        );
        assert_eq!(
            refused(r#"return ui.column({}, { { block = "text" } })"#),
            "column > [0]: not a block"
        );
        assert_eq!(refused(r#"return "text""#), "not a block");
        assert_eq!(
            refused(r#"return ui.column({}, { ui.spacer({}), false })"#),
            "column > [1]: not a block"
        );
    }

    // Fails if an image without a source, or with two, passes, or if
    // `bytes` isn't base64 or isn't capped.
    #[test]
    fn an_image_needs_exactly_one_source() {
        assert_eq!(
            refused(r#"return ui.image({ fit = "cover" })"#),
            "image: an image needs one of asset, file, file_token, url, attachment or bytes"
        );
        assert_eq!(
            refused(
                r#"return ui.image({ asset = "assets/a.png", url = "https://a.example/b.png" })"#
            ),
            "image: an image takes one source, not asset and url"
        );
        assert_eq!(
            refused(r#"return ui.image({ attachment = 12 })"#),
            "image: attachment must be an id string"
        );
        let asset = check(r#"return ui.image({ asset = "assets/a.png" })"#).unwrap();
        assert_eq!(props(&asset), json!({ "asset": "assets/a.png" }));
        let bytes = check(r#"return ui.image({ bytes = "hi!" })"#).unwrap();
        assert_eq!(props(&bytes), json!({ "bytes": "aGkh" }));
        let buffer = check(r#"return ui.image({ bytes = buffer.fromstring("hi!") })"#).unwrap();
        assert_eq!(props(&buffer), json!({ "bytes": "aGkh" }));
        assert!(check(r#"return ui.image({ bytes = string.rep("x", 256 * 1024) })"#).is_ok());
        assert_eq!(
            refused(r#"return ui.image({ bytes = string.rep("x", 256 * 1024 + 1) })"#),
            "image: bytes is over 256 KiB"
        );
    }

    // Fails if a colour takes a name outside the theme, a short or broken
    // hex, a gradient inside a gradient, or a gradient missing an end.
    #[test]
    fn colours_by_name_hex_and_gradient() {
        let node = check(
            r##"return ui.box({ background = { from = "accent", to = "#000000", angle = 45 } }, {
                ui.text({ text = "a", color = "#A1B2C3ff" }),
            })"##,
        )
        .unwrap();
        assert_eq!(
            props(&node),
            json!({ "background": { "from": "accent", "to": "#000000", "angle": 45 } })
        );
        assert_eq!(
            props(&node.children[0]),
            json!({ "text": "a", "color": "#A1B2C3ff" })
        );
        assert!(check(r#"return ui.divider({ color = "text_muted" })"#).is_ok());

        let wrong =
            r##"must be a colour name, "#rrggbb", "#rrggbbaa" or a gradient {from, to, angle?}"##;
        for color in [r#""red""#, r##""#abc""##, r##""#gggggg""##, "12"] {
            assert_eq!(
                refused(&format!("return ui.divider({{ color = {color} }})")),
                format!("divider: color {wrong}")
            );
        }
        assert_eq!(
            refused(
                r##"return ui.box({ background = { from = { from = "text", to = "text" }, to = "text" } })"##
            ),
            r##"box: background.from must be a colour name, "#rrggbb" or "#rrggbbaa""##
        );
        assert_eq!(
            refused(r#"return ui.box({ background = { from = "text" } })"#),
            "box: background needs from and to"
        );
        assert_eq!(
            refused(
                r#"return ui.box({ background = { from = "text", to = "text", angle = 0/0 } })"#
            ),
            "box: background.angle must be a finite number"
        );
    }

    // Fails if a block of §12.2 is missing from the schema or the schema
    // has one the design doesn't.
    #[test]
    fn every_block_in_the_design_table_is_in_the_schema() {
        let design = [
            "row",
            "column",
            "stack",
            "grid",
            "scroll",
            "box",
            "spacer",
            "divider",
            "list",
            "text",
            "rich_text",
            "image",
            "icon",
            "user",
            "badge",
            "progress",
            "spinner",
            "button",
            "icon_button",
            "checkbox",
            "switch",
            "radio",
            "slider",
            "select",
            "tabs",
            "segmented",
            "text_input",
            "image_picker",
            "file_picker",
            "modal",
            "sheet",
            "menu",
            "tooltip",
            "toast",
            "canvas",
        ];
        for name in design {
            assert!(block(name).is_some(), "{name} is missing");
        }
        let mut names: Vec<_> = BLOCKS.iter().map(|spec| spec.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), BLOCKS.len(), "a block is listed twice");
        assert_eq!(
            BLOCKS.len(),
            design.len(),
            "the schema has blocks the design doesn't"
        );
    }

    // Fails if a block's own prop shadows a common prop, a stack child's,
    // or (on a container) a container prop, so one name would mean two
    // things on one block.
    #[test]
    fn no_block_prop_shadows_a_common_one() {
        use crate::blocks::{COMMON_PROPS, CONTAINER_PROPS, STACK_CHILD_PROPS};
        for spec in BLOCKS {
            let container: &[_] = if spec.is_container() {
                CONTAINER_PROPS
            } else {
                &[]
            };
            for (name, _) in spec.props {
                let shared = COMMON_PROPS
                    .iter()
                    .chain(container)
                    .chain(STACK_CHILD_PROPS)
                    .any(|(common, _)| common == name);
                assert!(!shared, "{}.{name} shadows a common prop", spec.name);
                assert!(
                    !name.starts_with("on_"),
                    "{}.{name} looks like an event",
                    spec.name
                );
            }
        }
    }

    // Fails if a block that takes no children, or one, takes more, or if a
    // list takes children instead of drawing `item(i)`.
    #[test]
    fn children_follow_what_a_block_takes() {
        assert_eq!(
            refused(r#"return ui.text({ text = "a" }, { ui.spacer({}) })"#),
            "text: text takes no children"
        );
        assert_eq!(
            refused(r#"return ui.box({}, { ui.spacer({}), ui.spacer({}) })"#),
            "box: box takes one child"
        );
        assert!(check(r#"return ui.box({}, { ui.spacer({}) })"#).is_ok());
        assert_eq!(
            refused(r#"return ui.list({ count = 1 }, { ui.spacer({}) })"#),
            "list: list takes no children; it draws item(i)"
        );
        assert_eq!(
            refused(r#"return ui.column({}, ui.spacer({}))"#),
            "column: children must be a list of blocks, with no gaps"
        );
        assert_eq!(
            refused(r#"return ui.column({}, { ui.spacer({}), nil, ui.spacer({}) })"#),
            "column: children must be a list of blocks, with no gaps"
        );
    }

    // Fails if `item` is sent as a prop or dropped instead of kept with
    // the handlers.
    #[test]
    fn a_list_keeps_its_item_function_with_the_handlers() {
        let node = check(r#"return ui.list({ count = 3, item = function(i) end })"#).unwrap();
        assert_eq!(props(&node), json!({ "count": 3 }));
        assert_eq!(node.handlers.keys().collect::<Vec<_>>(), ["item"]);
        assert_eq!(
            refused(r#"return ui.list({ count = 3, item = 4 })"#),
            "list: item must be a function"
        );
    }

    // Fails if a string prop isn't capped, or `text` doesn't get its
    // longer cap.
    #[test]
    fn strings_are_capped() {
        assert!(check(r#"return ui.text({ text = string.rep("é", 20000) })"#).is_ok());
        assert_eq!(
            refused(r#"return ui.text({ text = string.rep("a", 20001) })"#),
            "text: text is longer than 20000 characters"
        );
        assert_eq!(
            refused(r#"return ui.button({ text = string.rep("a", 10001) })"#),
            "button: text is longer than 10000 characters"
        );
        assert_eq!(
            refused(r#"return ui.text({ text = 5 })"#),
            "text: text must be a string"
        );
    }

    // Fails if many blocks sharing one long string or table can make a
    // tree far larger than the Lua memory behind it.
    #[test]
    fn a_shared_string_cannot_blow_up_the_tree() {
        let source = |copies: usize| {
            format!(
                r#"local text = string.rep("a", 20000)
                   local children = {{}}
                   for i = 1, {copies} do children[i] = ui.text({{ text = text }}) end
                   return ui.column({{}}, children)"#
            )
        };
        assert!(check(&source(800)).is_ok());
        assert!(refused(&source(900)).ends_with("text: the tree's props are over 16 MiB"));
    }

    // Fails if options aren't checked or capped, or `danger` is allowed
    // outside a menu.
    #[test]
    fn options_are_checked_and_capped() {
        let node = check(
            r#"return ui.menu({ items = {
                { value = "edit", label = "Edit", icon = "pencil" },
                { value = 2, label = "Delete", danger = true },
            } }, { ui.icon_button({ icon = "more" }) })"#,
        )
        .unwrap();
        assert_eq!(
            props(&node),
            json!({ "items": [
                { "value": "edit", "label": "Edit", "icon": "pencil" },
                { "value": 2, "label": "Delete", "danger": true },
            ] })
        );
        assert_eq!(
            refused(
                r#"return ui.select({ options = { { value = "a", label = "A", danger = true } } })"#
            ),
            r#"select: options[0] takes no field "danger""#
        );
        assert_eq!(
            refused(r#"return ui.tabs({ options = { { value = "a" } } })"#),
            "tabs: options[0].label must be a string"
        );
        assert_eq!(
            refused(
                r#"local options = {}
                   for i = 1, 101 do options[i] = { value = i, label = "x" } end
                   return ui.segmented({ options = options })"#
            ),
            "segmented: options has more than 100 options"
        );
    }

    // Fails if canvas items may share an id, lack one, or carry a path
    // longer than the cap.
    #[test]
    fn canvas_items_need_unique_ids_and_short_paths() {
        let node = check(
            r#"return ui.canvas({ items = {
                { id = 1, type = "line", x1 = 0, y1 = 0, x2 = 10, y2 = 10 },
                { id = "p", type = "path", points = { 0, 0, 5, 5 }, closed = false },
            } })"#,
        )
        .unwrap();
        assert_eq!(
            props(&node),
            json!({ "items": [
                { "id": "1", "type": "line", "x1": 0, "y1": 0, "x2": 10, "y2": 10 },
                { "id": "p", "type": "path", "points": [0, 0, 5, 5], "closed": false },
            ] })
        );
        assert_eq!(
            refused(r#"return ui.canvas({ items = { { id = "a" }, { id = "a" } } })"#),
            r#"canvas: items has two items with the id "a""#
        );
        assert_eq!(
            refused(r#"return ui.canvas({ items = { { type = "rect" } } })"#),
            "canvas: items[0] needs an id"
        );
        assert_eq!(
            refused(
                r#"local points = {}
                   for i = 1, 20002 do points[i] = i end
                   return ui.canvas({ items = { { id = "p", points = points } } })"#
            ),
            "canvas: items[0].points has more than 10000 points"
        );
    }

    // Fails if plain data may hold functions, nest without end, or mix a
    // list with named fields.
    #[test]
    fn plain_data_is_plain() {
        let node = check(
            r#"return ui.box({ animate = { duration = 200, curve = "ease_out", tags = { "a", "b" } } })"#,
        )
        .unwrap();
        assert_eq!(
            props(&node),
            json!({ "animate": { "duration": 200, "curve": "ease_out", "tags": ["a", "b"] } })
        );
        assert_eq!(
            refused(r#"return ui.box({ animate = { curve = function() end } })"#),
            "box: animate must be plain data, not a function"
        );
        assert_eq!(
            refused(r#"return ui.box({ animate = { 1, 2, curve = "x" } })"#),
            "box: animate mixes list items and named fields"
        );
        assert_eq!(
            refused("local t = {}; t.t = t; return ui.box({ enter = t })"),
            "box: enter nests deeper than 32 levels"
        );
        assert_eq!(
            refused(r#"return ui.grid({ columns = 2.5 })"#),
            "grid: columns must be a whole number"
        );
        assert_eq!(
            refused(r#"return ui.user({ id = "12a" })"#),
            "user: id must be an id string"
        );
        assert!(check(r#"return ui.user({ id = "1234567890123", size = 32 })"#).is_ok());
    }
}
