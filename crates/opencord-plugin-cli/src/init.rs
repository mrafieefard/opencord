use std::fs;
use std::path::Path;

use opencord_plugins::manifest::valid_id;

const OPENCORD_D_LUAU: &str = include_str!("../../../crates/opencord-plugins/api/opencord.d.luau");

const LUAURC: &str = r#"{
  "languageMode": "strict",
  "lint": {
    "*": true
  },
  "types": [
    "opencord.d.luau"
  ]
}
"#;

const VSCODE_SETTINGS: &str = r#"{
  "luau-lsp.types.definitionFiles": [
    "opencord.d.luau"
  ]
}
"#;

const GITIGNORE: &str = r#"*.zip
.DS_Store
signing.key
"#;

pub fn init(id: &str, parent_dir: &Path) -> Result<(), String> {
    if !valid_id(id) {
        return Err(format!(
            "invalid id '{id}': must be 3 to 40 lowercase letters, digits, and hyphens"
        ));
    }

    let plugin_dir = parent_dir.join(id);
    if plugin_dir.exists() {
        return Err(format!(
            "directory '{}' already exists",
            plugin_dir.display()
        ));
    }

    let server_dir = plugin_dir.join("server");
    fs::create_dir_all(&server_dir)
        .map_err(|e| format!("failed to create directory '{}': {e}", server_dir.display()))?;

    let client_dir = plugin_dir.join("client");
    fs::create_dir_all(&client_dir)
        .map_err(|e| format!("failed to create directory '{}': {e}", client_dir.display()))?;

    let vscode_dir = plugin_dir.join(".vscode");
    fs::create_dir_all(&vscode_dir)
        .map_err(|e| format!("failed to create directory '{}': {e}", vscode_dir.display()))?;

    let manifest_content = format!(
        r#"return {{
  id = "{id}",
  name = "{id}",
  version = "0.1.0",
  author = "Your Name",
  description = "A plugin for Opencord.",
  license = "MIT",
  api = 1,
  permissions = {{ "create_channel_types" }},
  server = {{ "server/main.luau" }},
  client = "client/main.luau",
}}
"#
    );

    let main_luau_content = r#"local count = 0

opencord.on("start", function()
  log.info("started")
  ui.register_channel_type("counter", {
    name = "Counter",
    description = "A counting channel",
  })
end)

ui.on("clicked", function(view, _data)
  count += 1
  view:send("count", count)
end)
"#;

    let client_main_luau_content = r#"function init(_ctx)
  return { count = 0 }
end

function render(s)
  return ui.column({ gap = 8, padding = 12 }, {
    ui.text({ text = "Clicks: " .. s.count, size = 18 }),
    ui.button({ text = "Click", on_press = function()
      app.set(function(s) s.count += 1 end)
      app.send("clicked")
    end }),
  })
end

app.on("count", function(n)
  app.set(function(s) s.count = n end)
end)
"#;

    fs::write(plugin_dir.join("manifest.lua"), manifest_content)
        .map_err(|e| format!("failed to write manifest.lua: {e}"))?;
    fs::write(server_dir.join("main.luau"), main_luau_content)
        .map_err(|e| format!("failed to write server/main.luau: {e}"))?;
    fs::write(client_dir.join("main.luau"), client_main_luau_content)
        .map_err(|e| format!("failed to write client/main.luau: {e}"))?;
    fs::write(plugin_dir.join("opencord.d.luau"), OPENCORD_D_LUAU)
        .map_err(|e| format!("failed to write opencord.d.luau: {e}"))?;
    fs::write(plugin_dir.join(".luaurc"), LUAURC)
        .map_err(|e| format!("failed to write .luaurc: {e}"))?;
    fs::write(vscode_dir.join("settings.json"), VSCODE_SETTINGS)
        .map_err(|e| format!("failed to write .vscode/settings.json: {e}"))?;
    fs::write(plugin_dir.join(".gitignore"), GITIGNORE)
        .map_err(|e| format!("failed to write .gitignore: {e}"))?;

    println!("Created plugin '{}' in {}", id, plugin_dir.display());
    Ok(())
}
