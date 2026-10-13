//! Other crates and integration tests link the core as a Rust library.

#[test]
fn core_is_linkable_from_another_crate() {
    let version = opencord_core::api::system::core_version();

    assert!(version.ends_with("(protocol v1)"), "got {version:?}");
}

/// Each system's backends say what it can do with video (docs/plans/
/// platforms.md); Linux has them all, and the portal picks the source.
#[cfg(target_os = "linux")]
#[test]
fn linux_receives_sends_and_shares_through_the_system_picker() {
    let support = opencord_core::api::client::video_support();

    assert!(support.receive && support.camera && support.screen);
    assert_eq!(support.screen_reason, None);
    assert!(!support.own_screen_picker);
}

/// Elsewhere, until a system's backends land, video stays off and says why.
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
#[test]
fn a_system_without_backends_has_no_video() {
    let support = opencord_core::api::client::video_support();

    assert!(!support.receive && !support.camera && !support.screen);
    assert!(support.screen_reason.is_some());
}
