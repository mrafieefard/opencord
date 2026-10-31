//! The event pump against a real server.

mod common;

use std::time::Duration;

use common::TestServer;
use opencord_core::identity::Identity;
use opencord_mobile::{ChannelKind, ConnectionRecord, Mobile, MobileEvent, parse_id};

const WAIT: Duration = Duration::from_secs(10);

/// Pulls batches until `pick` finds something.
async fn wait_for<T>(
    mobile: &Mobile,
    generation: u64,
    mut pick: impl FnMut(&MobileEvent) -> Option<T>,
) -> T {
    tokio::time::timeout(WAIT, async {
        loop {
            for event in mobile.next_events(generation, 64).await {
                if let Some(found) = pick(&event) {
                    return found;
                }
            }
        }
    })
    .await
    .expect("timed out waiting for an event")
}

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

#[tokio::test(flavor = "multi_thread")]
async fn an_untrusted_server_asks_first() {
    let server = TestServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    let mobile = start(&dir);
    let client = mobile.client();

    let first = client
        .add_server(&server.address(), server.claim_token())
        .await
        .unwrap();
    let opencord_core::api::types::AddServerOutcome::NeedsTrust {
        address,
        fingerprint,
    } = first
    else {
        panic!("expected to be asked first, got {first:?}");
    };
    assert_eq!(fingerprint, server.fingerprint());
    client.trust_fingerprint(&address, &fingerprint).unwrap();
    let second = client
        .add_server(&server.address(), server.claim_token())
        .await
        .unwrap();
    assert!(matches!(
        second,
        opencord_core::api::types::AddServerOutcome::Added(_)
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn ready_then_messages_arrive_with_string_ids() {
    let server = TestServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    let mobile = start(&dir);
    let generation = mobile.attach();
    mobile
        .client()
        .trust_fingerprint(&server.address(), &server.fingerprint())
        .unwrap();
    mobile
        .client()
        .add_server(&server.address(), server.claim_token())
        .await
        .unwrap();

    let mut connected = false;
    let snapshot = wait_for(&mobile, generation, |event| match event {
        MobileEvent::Connection {
            state: ConnectionRecord::Connected,
            ..
        } => {
            connected = true;
            None
        }
        MobileEvent::Ready { snapshot } => Some(snapshot.clone()),
        _ => None,
    })
    .await;
    assert!(connected, "Connected came before Ready");
    assert_eq!(snapshot.server_key, server.address());
    let general = snapshot
        .channels
        .iter()
        .find(|channel| channel.name == "general" && channel.kind == ChannelKind::Text)
        .expect("the default text channel")
        .clone();
    assert!(
        snapshot
            .access
            .iter()
            .any(|access| access.channel_id == general.id && access.send)
    );

    mobile
        .client()
        .send_message(
            &server.address(),
            parse_id(&general.id).unwrap(),
            "hello".to_owned(),
            "n1".to_owned(),
        )
        .await
        .unwrap();
    let message = wait_for(&mobile, generation, |event| match event {
        MobileEvent::MessageCreated { message, .. } => Some(message.clone()),
        _ => None,
    })
    .await;
    assert_eq!(message.content, "hello");
    assert_eq!(message.channel_id, general.id);
    assert_eq!(message.nonce.as_deref(), Some("n1"));
    assert_eq!(message.author_id, snapshot.self_user.id);
}
