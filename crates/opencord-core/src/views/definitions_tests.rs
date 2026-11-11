// SPDX-License-Identifier: MPL-2.0

//! Tests that the client VM exposes every global, method, and context field
//! required by the plugin specification (plan Task 25, design 2026-11-07 §11).

use super::test_support::{open, settle, spec};

// Fails if the client VM globals or app/ui methods are missing or have wrong types.
#[test]
fn client_vm_globals_and_shapes_match_specification() {
    let script = r#"
local checked = false

function init(ctx)
    -- Check ctx fields
    assert(type(ctx.viewer) == "table", "ctx.viewer missing")
    assert(type(ctx.viewer.id) == "string", "ctx.viewer.id missing")
    assert(type(ctx.viewer.name) == "string", "ctx.viewer.name missing")
    assert(type(ctx.viewer.roles) == "table", "ctx.viewer.roles missing")
    assert(type(ctx.place) == "table", "ctx.place missing")
    assert(ctx.place.kind == "channel", "ctx.place.kind invalid")
    assert(type(ctx.place.channel_id) == "string", "ctx.place.channel_id missing")
    assert(type(ctx.platform) == "string", "ctx.platform missing")
    assert(type(ctx.size) == "table", "ctx.size missing")
    assert(type(ctx.size.width) == "number", "ctx.size.width missing")
    assert(type(ctx.size.height) == "number", "ctx.size.height missing")
    assert(type(ctx.theme) == "string", "ctx.theme missing")
    return { ok = true }
end

function render(state)
    if not checked then
        checked = true

        -- 1. Check globals
        assert(type(app) == "table", "global app missing")
        assert(type(ui) == "table", "global ui missing")
        assert(type(require) == "function", "global require missing")
        assert(type(json) == "table", "global json missing")

        -- 2. Check app functions
        assert(type(app.set) == "function", "app.set missing")
        assert(type(app.send) == "function", "app.send missing")
        assert(type(app.on) == "function", "app.on missing")
        assert(type(app.listen) == "function", "app.listen missing")
        assert(type(app.copy) == "function", "app.copy missing")
        assert(type(app.on_back) == "function", "app.on_back missing")
        assert(type(app.open_link) == "function", "app.open_link missing")
        assert(type(app.open_channel) == "function", "app.open_channel missing")
        assert(type(app.play_sound) == "function", "app.play_sound missing")
        assert(type(app.confirm) == "function", "app.confirm missing")
        assert(type(app.alert) == "function", "app.alert missing")
        assert(type(app.prompt) == "function", "app.prompt missing")
        assert(type(app.after) == "function", "app.after missing")
        assert(type(app.every) == "function", "app.every missing")
        assert(type(app.cancel) == "function", "app.cancel missing")
        assert(type(app.on_frame) == "function", "app.on_frame missing")
        assert(type(app.off_frame) == "function", "app.off_frame missing")

        -- 3. Check app.modal and app.modals
        assert(type(app.modal) == "table", "app.modal missing")
        assert(type(app.modal.open) == "function", "app.modal.open missing")
        assert(type(app.modal.ask) == "function", "app.modal.ask missing")
        assert(type(app.modal.close) == "function", "app.modal.close missing")
        assert(type(app.modal.close_all) == "function", "app.modal.close_all missing")
        assert(type(app.modal.is_open) == "function", "app.modal.is_open missing")
        assert(type(app.modals) == "table", "app.modals missing")

        -- 4. Check app.storage
        assert(type(app.storage) == "table", "app.storage missing")
        assert(type(app.storage.get) == "function", "app.storage.get missing")
        assert(type(app.storage.set) == "function", "app.storage.set missing")
        assert(type(app.storage.delete) == "function", "app.storage.delete missing")

        -- 5. Check app context fields
        assert(type(app.state) == "table", "app.state missing")
        assert(app.state.ok == true, "app.state.ok missing")
        assert(type(app.viewer) == "table", "app.viewer missing")
        assert(type(app.viewer.id) == "string", "app.viewer.id missing")
        assert(type(app.viewer.name) == "string", "app.viewer.name missing")
        assert(type(app.viewer.roles) == "table", "app.viewer.roles missing")
        assert(type(app.place) == "table", "app.place missing")
        assert(app.place.kind == "channel", "app.place.kind missing")
        assert(type(app.place.channel_id) == "string", "app.place.channel_id missing")
        assert(type(app.platform) == "string", "app.platform missing")
        assert(type(app.size) == "table", "app.size missing")
        assert(type(app.size.width) == "number", "app.size.width missing")
        assert(type(app.size.height) == "number", "app.size.height missing")
        assert(type(app.safe_area) == "table", "app.safe_area missing")
        assert(type(app.safe_area.top) == "number", "app.safe_area.top missing")
        assert(type(app.safe_area.right) == "number", "app.safe_area.right missing")
        assert(type(app.safe_area.bottom) == "number", "app.safe_area.bottom missing")
        assert(type(app.safe_area.left) == "number", "app.safe_area.left missing")
        assert(type(app.theme) == "string", "app.theme missing")
        assert(type(app.breakpoint) == "string", "app.breakpoint missing")
        assert(type(app.input) == "string", "app.input missing")
        assert(type(app.text_scale) == "number", "app.text_scale missing")

        app.send("verified", true)
    end

    return ui.box({}, { ui.text({ text = "OK" }) })
end
"#;

    let (handle, recorder) = open(spec(script));
    let seen = settle(&handle, &recorder);
    assert!(
        seen.errors.is_empty(),
        "client script errors: {:?}",
        seen.errors
    );
    let verified = seen.sent("verified");
    assert_eq!(verified.len(), 1, "app.send('verified') must have run");
    assert_eq!(verified[0], serde_json::json!(true));
    handle.close();
}
