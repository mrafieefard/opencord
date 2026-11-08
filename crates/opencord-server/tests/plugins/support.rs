//! Plugin packages for tests (plan Task 7; Task 8 adds installing them).

use opencord_plugins::package::Package;

/// A plugin's zip holding `files`.
pub fn package(files: &[(&str, &str)]) -> Vec<u8> {
    let dir = tempfile::tempdir().unwrap();
    for (path, text) in files {
        let path = dir.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    Package::from_dir(dir.path()).unwrap().write_zip()
}

/// A `manifest.lua` running `server/main.luau` and asking for `permissions`.
pub fn manifest(id: &str, permissions: &[&str]) -> String {
    let permissions: Vec<String> = permissions.iter().map(|name| format!("{name:?}")).collect();
    format!(
        r#"return {{
    id = "{id}",
    name = "Test plugin",
    version = "1.0.0",
    author = "Tester",
    license = "MIT",
    api = 1,
    permissions = {{ {} }},
    server = {{ "server/main.luau" }},
}}
"#,
        permissions.join(", ")
    )
}
