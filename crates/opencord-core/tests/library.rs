//! Other crates and integration tests link the core as a Rust library.

#[test]
fn core_is_linkable_from_another_crate() {
    let version = opencord_core::api::system::core_version();

    assert!(version.ends_with("(protocol v1)"), "got {version:?}");
}
