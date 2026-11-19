use std::sync::OnceLock;
use tempfile::TempDir;

static TEST_DIR: OnceLock<TempDir> = OnceLock::new();

pub fn test_dirs() -> (String, String) {
    let dir = TEST_DIR.get_or_init(|| tempfile::tempdir().unwrap());
    let data = dir.path().join("data");
    let cache = dir.path().join("cache");
    let _ = std::fs::create_dir_all(&data);
    let _ = std::fs::create_dir_all(&cache);
    (
        data.to_string_lossy().into_owned(),
        cache.to_string_lossy().into_owned(),
    )
}

pub fn init_once() {
    let (data, cache) = test_dirs();
    let _ = crate::api::setup::core_init(data, cache);
}
