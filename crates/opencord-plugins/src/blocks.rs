//! The blocks a plugin's client script draws with (§12): each block's props,
//! events and children, and the theme's colour names. The core checks every
//! tree against this, and the type definitions and docs follow it.

/// What a prop's value must be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropKind {
    /// A finite number, in the range [`range`] gives.
    Number,
    /// A whole number, in the range [`range`] gives.
    Integer,
    /// A string of at most [`crate::limits::VIEW_MAX_PROP_CHARS`] characters
    /// (`text` and `rich_text`'s `text`: [`crate::limits::VIEW_MAX_TEXT_CHARS`]).
    Text,
    Bool,
    /// A name from [`COLORS`], `#rrggbb`, `#rrggbbaa`, or a gradient
    /// `{from, to, angle?}` between two of those.
    Color,
    /// A number, `"fill"` or `"fit"`.
    Size,
    /// A number, `{top, right, bottom, left}` or `{x, y}`; sides left out
    /// are 0. Sent as the number, or as all four sides.
    Insets,
    /// One of these strings.
    Choice(&'static [&'static str]),
    /// A list of `{value, label, icon?}` (`menu` also takes `danger`), at
    /// most [`crate::limits::VIEW_MAX_OPTIONS`].
    Options,
    /// A canvas's drawing items: a list of plain tables, each with an `id`.
    Items,
    /// A Lua function that isn't an event (`list`'s `item`). It goes to the
    /// node's handlers under the prop's own name, never to the renderer.
    Function,
    /// Any plain data: tables, strings, finite numbers, booleans.
    Json,
    /// `{width?, color?}`: a border, or the ink a canvas draws with.
    Stroke,
    /// A Lua string or `buffer` of at most
    /// [`crate::limits::IMAGE_MAX_RAW_BYTES`], sent as base64.
    Bytes,
    /// An id: a string of digits.
    Id,
    /// A list of short strings (a picker's file extensions).
    Strings,
}

/// The children a block takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Children {
    None,
    /// At most one.
    One,
    Many,
    /// None from the tree: a `list` draws its items itself, from `item(i)`.
    Lazy,
}

#[derive(Debug)]
pub struct BlockSpec {
    pub name: &'static str,
    /// The block's own props; [`COMMON_PROPS`] and, for containers,
    /// [`CONTAINER_PROPS`] come on top.
    pub props: &'static [(&'static str, PropKind)],
    /// Event names without `on_`; [`COMMON_EVENTS`] come on top.
    pub events: &'static [&'static str],
    pub children: Children,
}

/// The theme's colours a plugin may name.
pub const COLORS: &[&str] = &[
    "text",
    "text_secondary",
    "text_muted",
    "background",
    "surface",
    "elevated",
    "border",
    "accent",
    "on_accent",
    "selected",
    "hover",
    "danger",
];

/// The prop that keys a block among its siblings: a string or a whole
/// number. It goes to the node's `key`, not its props.
pub const KEY: &str = "key";

/// Props every block takes.
pub const COMMON_PROPS: &[(&str, PropKind)] = &[
    ("width", PropKind::Size),
    ("height", PropKind::Size),
    ("min_width", PropKind::Size),
    ("max_width", PropKind::Size),
    ("min_height", PropKind::Size),
    ("max_height", PropKind::Size),
    ("flex", PropKind::Number),
    ("padding", PropKind::Insets),
    ("opacity", PropKind::Number),
    ("visible", PropKind::Bool),
    ("tooltip", PropKind::Text),
    ("label", PropKind::Text),
    ("animate", PropKind::Json),
    ("enter", PropKind::Json),
    ("exit", PropKind::Json),
];

/// Events every block has: the end of its `animate`, `enter` or `exit`.
pub const COMMON_EVENTS: &[&str] = &["animation_end"];

/// Blocks that also take [`CONTAINER_PROPS`].
pub const CONTAINERS: &[&str] = &["row", "column", "stack", "grid", "scroll", "box", "list"];

pub const CONTAINER_PROPS: &[(&str, PropKind)] = &[
    ("background", PropKind::Color),
    ("border", PropKind::Stroke),
    ("radius", PropKind::Number),
    (
        "shadow",
        PropKind::Choice(&["none", "small", "medium", "large"]),
    ),
];

/// Props a child of a `stack` also takes: where it sits.
pub const STACK_CHILD_PROPS: &[(&str, PropKind)] = &[
    ("x", PropKind::Number),
    ("y", PropKind::Number),
    ("right", PropKind::Number),
    ("bottom", PropKind::Number),
];

/// The props of an `image` that say where it comes from; it takes exactly one.
pub const IMAGE_SOURCES: &[&str] = &["asset", "file", "file_token", "url", "attachment", "bytes"];

/// Sizes, gaps, paddings and widths: logical pixels.
pub const MAX_SIZE: f64 = 100_000.0;

use PropKind::{
    Bool, Bytes, Choice, Color, Function, Id, Integer, Items, Json, Number, Options, Strings,
    Stroke, Text,
};

const ALIGN: PropKind = Choice(&["start", "center", "end", "space_between", "space_around"]);
const CROSS: PropKind = Choice(&["start", "center", "end", "stretch"]);
const FLEX: &[(&str, PropKind)] = &[
    ("gap", Number),
    ("align", ALIGN),
    ("cross", CROSS),
    ("wrap", Bool),
];
const PRESSES: &[&str] = &[
    "press",
    "double_press",
    "long_press",
    "secondary_press",
    "hover_enter",
    "hover_exit",
];
const SCROLLS: &[&str] = &["scroll", "reach_start", "reach_end"];
const TOGGLE: &[(&str, PropKind)] = &[("value", Bool), ("text", Text), ("disabled", Bool)];
const CHOICE_OF_OPTIONS: &[(&str, PropKind)] = &[("value", Json), ("options", Options)];
const UPLOADS: &[&str] = &["upload", "upload_error"];

const fn spec(
    name: &'static str,
    props: &'static [(&'static str, PropKind)],
    events: &'static [&'static str],
    children: Children,
) -> BlockSpec {
    BlockSpec {
        name,
        props,
        events,
        children,
    }
}

/// Every block of §12.2.
pub const BLOCKS: &[BlockSpec] = &[
    spec("row", FLEX, &[], Children::Many),
    spec("column", FLEX, &[], Children::Many),
    spec("stack", &[], &[], Children::Many),
    spec(
        "grid",
        &[
            ("columns", Integer),
            ("min_column_width", Number),
            ("gap", Number),
        ],
        &[],
        Children::Many,
    ),
    spec(
        "scroll",
        &[("direction", Choice(&["vertical", "horizontal"]))],
        SCROLLS,
        Children::One,
    ),
    spec("box", &[], PRESSES, Children::One),
    spec("spacer", &[("size", Number)], &[], Children::None),
    spec(
        "divider",
        &[("color", Color), ("thickness", Number)],
        &[],
        Children::None,
    ),
    spec(
        "list",
        &[
            ("count", Integer),
            ("reverse", Bool),
            ("gap", Number),
            ("item_height", Number),
            ("item", Function),
        ],
        SCROLLS,
        Children::Lazy,
    ),
    spec(
        "text",
        &[
            ("text", Text),
            ("size", Number),
            ("weight", Choice(&["regular", "medium", "semibold", "bold"])),
            ("italic", Bool),
            ("color", Color),
            ("align", Choice(&["start", "center", "end"])),
            ("max_lines", Integer),
            ("selectable", Bool),
            ("font", Text),
        ],
        &[],
        Children::None,
    ),
    spec("rich_text", &[("text", Text)], &[], Children::None),
    spec(
        "image",
        &[
            ("asset", Text),
            ("file", Text),
            ("file_token", Text),
            ("url", Text),
            ("attachment", Id),
            ("bytes", Bytes),
            ("fit", Choice(&["cover", "contain", "fill", "none"])),
            ("radius", Number),
            ("placeholder", Color),
            ("version", Integer),
        ],
        &["load", "error"],
        Children::None,
    ),
    spec(
        "icon",
        &[("name", Text), ("size", Number), ("color", Color)],
        &[],
        Children::None,
    ),
    spec(
        "user",
        &[("id", Id), ("size", Number), ("show_name", Bool)],
        &[],
        Children::None,
    ),
    spec(
        "badge",
        &[
            ("text", Text),
            ("tone", Choice(&["neutral", "accent", "danger"])),
        ],
        &[],
        Children::None,
    ),
    spec(
        "progress",
        &[("value", Number), ("style", Choice(&["bar", "ring"]))],
        &[],
        Children::None,
    ),
    spec("spinner", &[("size", Number)], &[], Children::None),
    spec(
        "button",
        &[
            ("text", Text),
            (
                "variant",
                Choice(&["primary", "secondary", "danger", "plain"]),
            ),
            ("icon", Text),
            ("loading", Bool),
            ("disabled", Bool),
        ],
        &["press", "long_press"],
        Children::None,
    ),
    spec(
        "icon_button",
        &[("icon", Text), ("disabled", Bool)],
        &["press"],
        Children::None,
    ),
    spec("checkbox", TOGGLE, &["change"], Children::None),
    spec("switch", TOGGLE, &["change"], Children::None),
    spec("radio", TOGGLE, &["change"], Children::None),
    spec(
        "slider",
        &[
            ("value", Number),
            ("min", Number),
            ("max", Number),
            ("step", Number),
            ("disabled", Bool),
        ],
        &["change", "change_end"],
        Children::None,
    ),
    spec(
        "select",
        &[
            ("value", Json),
            ("options", Options),
            ("placeholder", Text),
            ("disabled", Bool),
        ],
        &["change"],
        Children::None,
    ),
    spec("tabs", CHOICE_OF_OPTIONS, &["change"], Children::None),
    spec("segmented", CHOICE_OF_OPTIONS, &["change"], Children::None),
    spec(
        "text_input",
        &[
            ("value", Text),
            ("placeholder", Text),
            ("password", Bool),
            ("max_length", Integer),
            ("lines", Integer),
            ("disabled", Bool),
        ],
        &["change", "submit", "focus", "blur"],
        Children::None,
    ),
    spec("image_picker", &[("text", Text)], UPLOADS, Children::None),
    spec(
        "file_picker",
        &[("text", Text), ("accept", Strings)],
        UPLOADS,
        Children::None,
    ),
    spec(
        "modal",
        &[
            ("open", Bool),
            ("size", Choice(&["small", "medium", "large", "full"])),
            ("sheet", Bool),
            ("dismissible", Bool),
            ("title", Text),
        ],
        &["dismiss"],
        Children::One,
    ),
    spec(
        "sheet",
        &[("open", Bool), ("dismissible", Bool)],
        &["dismiss"],
        Children::One,
    ),
    spec("menu", &[("items", Options)], &["select"], Children::One),
    spec("tooltip", &[("text", Text)], &[], Children::One),
    spec(
        "toast",
        &[("text", Text), ("open", Bool), ("duration", Number)],
        &["dismiss"],
        Children::None,
    ),
    spec(
        "canvas",
        &[("items", Items), ("ink", Stroke)],
        &[
            "pointer_down",
            "pointer_move",
            "pointer_up",
            "pointer_cancel",
            "wheel",
            "pan",
            "scale",
            "stroke",
        ],
        Children::None,
    ),
];

pub fn block(name: &str) -> Option<&'static BlockSpec> {
    BLOCKS.iter().find(|spec| spec.name == name)
}

impl BlockSpec {
    pub fn is_container(&self) -> bool {
        CONTAINERS.contains(&self.name)
    }

    /// What `name` must be on this block, inside a `stack` or not; `None`
    /// when the block has no such prop. [`KEY`] and `on_…` aren't props.
    pub fn prop(&self, name: &str, in_stack: bool) -> Option<PropKind> {
        let container: &[(&str, PropKind)] = if self.is_container() {
            CONTAINER_PROPS
        } else {
            &[]
        };
        let stack_child: &[(&str, PropKind)] = if in_stack { STACK_CHILD_PROPS } else { &[] };
        self.props
            .iter()
            .chain(COMMON_PROPS)
            .chain(container)
            .chain(stack_child)
            .find(|(prop, _)| *prop == name)
            .map(|(_, kind)| *kind)
    }

    /// Whether the block has the event `name` (without `on_`).
    pub fn has_event(&self, name: &str) -> bool {
        self.events.contains(&name) || COMMON_EVENTS.contains(&name)
    }
}

/// The smallest and largest value of a `Number`, `Integer`, `Size` or
/// `Insets` prop.
pub fn range(block: &str, prop: &str) -> (f64, f64) {
    const WHOLE: f64 = 9_007_199_254_740_991.0;
    match (block, prop) {
        (_, "opacity") | ("progress", "value") => (0.0, 1.0),
        ("text", "size") => (6.0, 200.0),
        (_, "x" | "y" | "right" | "bottom") => (-MAX_SIZE, MAX_SIZE),
        ("slider", "value" | "min" | "max" | "step") => (-WHOLE, WHOLE),
        ("toast", "duration") => (0.0, 60.0),
        ("grid", "columns") => (1.0, 100.0),
        ("text_input", "lines") => (1.0, 100.0),
        ("text", "max_lines") => (1.0, 10_000.0),
        ("text_input", "max_length") => (0.0, 1_000_000.0),
        ("list", "count") => (0.0, 1_000_000_000.0),
        ("image", "version") => (0.0, WHOLE),
        _ => (0.0, MAX_SIZE),
    }
}
