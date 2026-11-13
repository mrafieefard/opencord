//! Plugin views through the facade (plugins milestone 4, Task 1): the view
//! stream's generations and the views a JavaScript reload leaves behind.

use std::time::Duration;

use opencord_core::identity::Identity;
use opencord_mobile::{Mobile, MobileError};

fn start(dir: &tempfile::TempDir) -> Mobile {
    let mobile = Mobile::start(
        &dir.path().join("data"),
        &dir.path().join("cache"),
        tokio::runtime::Handle::current(),
        false,
    )
    .unwrap();
    mobile
        .client()
        .set_identity(Identity::generate(), "Ava".to_owned());
    mobile
}

// Fails if a stale generation can still read the stream (drop the
// generation checks in `ViewPump::next`: it would take the newer start's
// batches), or if the newer wait is ended by the attach too. The older
// wait itself ends either way: the core's new stream closes the old one.
#[tokio::test(flavor = "multi_thread")]
async fn a_newer_attach_ends_the_older_wait() {
    let dir = tempfile::tempdir().unwrap();
    let mobile = std::sync::Arc::new(start(&dir));
    let older = mobile.view_outputs_attach();
    let waiting = {
        let mobile = mobile.clone();
        tokio::spawn(async move { mobile.next_view_outputs(older, 64).await })
    };
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !waiting.is_finished(),
        "nothing arrived, yet the wait ended"
    );

    let newer = mobile.view_outputs_attach();
    assert_ne!(older, newer);
    let ended = tokio::time::timeout(Duration::from_secs(5), waiting)
        .await
        .expect("the older wait went on after a newer attach")
        .unwrap();
    assert!(ended.is_empty());
    // A stale generation asked again answers at once, and takes nothing.
    let stale = tokio::time::timeout(Duration::from_secs(2), mobile.next_view_outputs(older, 64))
        .await
        .expect("a stale generation waited on the newer start's stream");
    assert!(stale.is_empty());

    let still_waiting = tokio::time::timeout(
        Duration::from_millis(200),
        mobile.next_view_outputs(newer, 64),
    )
    .await;
    assert!(still_waiting.is_err(), "the newer wait ended with no batch");
}

// Fails if attach leaves the earlier start's views in the set (skip the
// drain in `view_outputs_attach`), or forgets views opened after it.
#[tokio::test(flavor = "multi_thread")]
async fn attach_closes_views_left_by_an_older_generation() {
    let dir = tempfile::tempdir().unwrap();
    let mobile = start(&dir);
    mobile.view_outputs_attach();
    mobile.note_view_opened("view-a");
    mobile.note_view_opened("view-b");
    mobile.note_view_closed("view-b");
    assert_eq!(mobile.open_view_ids(), vec!["view-a".to_owned()]);

    mobile.view_outputs_attach();
    assert!(mobile.open_view_ids().is_empty());

    mobile.note_view_opened("view-c");
    assert_eq!(mobile.open_view_ids(), vec!["view-c".to_owned()]);
}

// Fails if an id that isn't a number reaches the core (parse after the
// call), or comes back as another kind of error.
#[tokio::test(flavor = "multi_thread")]
async fn a_bad_id_is_invalid_input() {
    let opened =
        opencord_mobile::api::plugins::view_open("key".to_owned(), "x".to_owned(), "{}".to_owned())
            .await;
    assert!(
        matches!(&opened, Err(MobileError::InvalidInput { message }) if message.starts_with("not an id")),
        "{opened:?}"
    );
    let activity = opencord_mobile::api::plugins::view_open_activity(
        "key".to_owned(),
        "1".to_owned(),
        "".to_owned(),
        "paint".to_owned(),
        "{}".to_owned(),
    )
    .await;
    assert!(
        matches!(activity, Err(MobileError::InvalidInput { .. })),
        "{activity:?}"
    );
}

// Fails if the facade checks features itself and differs from the core
// (accept 33 characters), or turns the core's error into another kind.
#[tokio::test(flavor = "multi_thread")]
async fn features_are_checked_by_the_core() {
    let dir = tempfile::tempdir().unwrap();
    let mobile = start(&dir);
    mobile
        .set_plugin_features(vec![
            "plugin_users".to_owned(),
            "plugin_channels".to_owned(),
        ])
        .unwrap();
    let refused = mobile.set_plugin_features(vec!["a".repeat(33)]);
    assert!(
        matches!(refused, Err(MobileError::InvalidInput { .. })),
        "{refused:?}"
    );
}
