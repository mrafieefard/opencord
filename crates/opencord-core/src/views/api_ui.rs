//! `ui.<block>(props, children?)` for every block of the schema (§12):
//! each returns the table `opencord_plugins::view::from_lua` reads, a
//! `{block, props, children}` with the `UI_NODE` metatable. The checks all
//! happen when `render` returns, where an error can name the block's path.

use mlua::{ChunkMode, Lua, Table};
use opencord_plugins::blocks::BLOCKS;
use opencord_plugins::view::UI_NODE;

/// Builds the constructors in Lua, so making a block never crosses into
/// Rust. Globals are read once, here, before any plugin code runs.
const UI: &str = r#"
local meta, names = ...
local setmetatable = setmetatable
local ui = {}
for _, name in names do
    ui[name] = function(props, children)
        return setmetatable({ block = name, props = props, children = children }, meta)
    end
end
return ui
"#;

/// Sets the global `ui`. Runs Lua, so the caller has a budget running.
pub(super) fn install(lua: &Lua) -> mlua::Result<()> {
    let meta = lua.create_table()?;
    // `getmetatable(block)` gives this text, so a script can't reach the
    // marker to dress up a table of its own as a block.
    meta.raw_set("__metatable", "ui block")?;
    meta.set_readonly(true);
    lua.set_named_registry_value(UI_NODE, &meta)?;

    let names = lua.create_sequence_from(BLOCKS.iter().map(|spec| spec.name))?;
    let ui: Table = lua
        .load(UI)
        .set_name("=ui")
        .set_mode(ChunkMode::Text)
        .call((meta, names))?;
    ui.set_readonly(true);
    lua.globals().raw_set("ui", ui)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use mlua::Value;
    use opencord_plugins::sandbox::{Vm, VmLimits};
    use opencord_plugins::view::from_lua;

    use super::*;

    fn vm() -> Vm {
        let vm = Vm::new(&VmLimits {
            memory_bytes: 16 * 1024 * 1024,
            handler_time: Duration::from_millis(100),
        })
        .unwrap();
        vm.budget().start(Duration::from_secs(5));
        install(&vm.lua).unwrap();
        vm
    }

    // Fails if a block of the schema has no constructor, or if a
    // constructor builds a table `from_lua` doesn't take as a block.
    #[test]
    fn every_block_has_a_constructor_from_lua_accepts() {
        let vm = vm();
        for spec in BLOCKS {
            let present: bool = vm
                .load(
                    "test",
                    &format!("return type(ui.{}) == 'function'", spec.name),
                )
                .unwrap()
                .call(())
                .unwrap();
            assert!(present, "ui.{} is missing", spec.name);
        }
        let tree: Value = vm
            .load(
                "test",
                "return ui.column({ gap = 4 }, { ui.text({ text = 'a' }), ui.spacer() })",
            )
            .unwrap()
            .call(())
            .unwrap();
        let node = from_lua(&vm.lua, &tree, |_| ()).unwrap();
        assert_eq!(node.block, "column");
        assert_eq!(node.children.len(), 2);
        assert_eq!(node.children[1].block, "spacer");
    }

    // Fails if the marker is reachable: a forged table would pass as a
    // block, and a changed marker would break every later tree.
    #[test]
    fn scripts_cannot_reach_or_change_the_marker() {
        let vm = vm();
        let hidden: String = vm
            .load("test", "return getmetatable(ui.text({ text = 'a' }))")
            .unwrap()
            .call(())
            .unwrap();
        assert_eq!(hidden, "ui block");
        let forged: Value = vm
            .load(
                "test",
                "return setmetatable({ block = 'text', props = { text = 'x' } }, \
                 { __metatable = 'ui block' })",
            )
            .unwrap()
            .call(())
            .unwrap();
        let error = from_lua(&vm.lua, &forged, |_| ()).unwrap_err();
        assert_eq!(error.message, "not a block");
        assert!(
            vm.load("test", "ui.text = nil")
                .unwrap()
                .call::<()>(())
                .is_err()
        );
        assert!(
            vm.load("test", "setmetatable(ui.text({ text = 'a' }), {})")
                .unwrap()
                .call::<()>(())
                .is_err()
        );
    }
}
