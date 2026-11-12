//! The view's surroundings on `app` (§11.4): who is looking, where, and
//! the plugin area's size, theme, input and insets. `context` inputs keep
//! them current; a script reads them but can't change them.

use mlua::{Lua, Table};

use super::{InputKind, Place, Theme, ViewContext, Viewer};

/// `app.platform`: the phone (M4) will say its own.
pub(super) const PLATFORM: &str = "desktop";
/// `app.breakpoint`: under this wide is `compact`.
const COMPACT_BELOW: f64 = 600.0;
/// Under this wide is `medium`; from here it's `expanded`.
const MEDIUM_BELOW: f64 = 840.0;

/// The name for a width of the plugin area, not of the window.
pub(super) fn breakpoint(width: f64) -> &'static str {
    if width < COMPACT_BELOW {
        "compact"
    } else if width < MEDIUM_BELOW {
        "medium"
    } else {
        "expanded"
    }
}

impl Theme {
    pub(super) fn name(self) -> &'static str {
        match self {
            Theme::Dark => "dark",
            Theme::Light => "light",
        }
    }
}

impl InputKind {
    fn name(self) -> &'static str {
        match self {
            InputKind::Mouse => "mouse",
            InputKind::Touch => "touch",
        }
    }
}

/// Sets every field. Runs no Lua.
pub(super) fn install(
    lua: &Lua,
    app: &Table,
    viewer: &Viewer,
    place: &Place,
    context: &ViewContext,
) -> mlua::Result<()> {
    writing(app, || {
        app.raw_set("viewer", viewer_table(lua, viewer)?)?;
        app.raw_set("place", place_table(lua, place)?)?;
        app.raw_set("platform", PLATFORM)?;
        set_fields(lua, app, context)
    })
}

/// A `context` input changed these.
pub(super) fn update(lua: &Lua, app: &Table, context: &ViewContext) -> mlua::Result<()> {
    writing(app, || set_fields(lua, app, context))
}

fn writing(app: &Table, change: impl FnOnce() -> mlua::Result<()>) -> mlua::Result<()> {
    app.set_readonly(false);
    let changed = change();
    app.set_readonly(true);
    changed
}

fn set_fields(lua: &Lua, app: &Table, context: &ViewContext) -> mlua::Result<()> {
    let size = numbers(lua, [("width", context.width), ("height", context.height)])?;
    let safe_area = numbers(
        lua,
        [
            ("top", context.safe_area.top),
            ("right", context.safe_area.right),
            ("bottom", context.safe_area.bottom),
            ("left", context.safe_area.left),
        ],
    )?;
    app.raw_set("size", size)?;
    app.raw_set("safe_area", safe_area)?;
    app.raw_set("theme", context.theme.name())?;
    app.raw_set("breakpoint", breakpoint(context.width))?;
    app.raw_set("input", context.input.name())?;
    app.raw_set("text_scale", context.text_scale)
}

fn numbers<const N: usize>(lua: &Lua, pairs: [(&str, f64); N]) -> mlua::Result<Table> {
    let table = lua.create_table_from(pairs)?;
    table.set_readonly(true);
    Ok(table)
}

fn viewer_table(lua: &Lua, viewer: &Viewer) -> mlua::Result<Table> {
    let roles = lua.create_sequence_from(viewer.roles.iter().map(String::as_str))?;
    roles.set_readonly(true);
    let table = lua.create_table()?;
    table.raw_set("id", viewer.id.as_str())?;
    table.raw_set("name", viewer.name.as_str())?;
    table.raw_set("roles", roles)?;
    table.set_readonly(true);
    Ok(table)
}

fn place_table(lua: &Lua, place: &Place) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    match place {
        Place::Channel { channel_id } => {
            table.raw_set("kind", "channel")?;
            table.raw_set("channel_id", channel_id.as_str())?;
        }
    }
    table.set_readonly(true);
    Ok(table)
}

#[cfg(test)]
#[path = "context_tests.rs"]
mod tests;
