//! Chat, attachment files and music through the exported calls, as
//! JavaScript makes them, against a real server. One process has one core,
//! so the steps run in order in one test.

mod common;

use common::{TestServer, png};
use opencord_mobile::api::{chat, files, identity, music, servers, setup};
use opencord_mobile::{
    AddServerResult, AttachmentVariant, MusicPlayState, OutgoingFileRecord, ServerSnapshot,
};

#[tokio::test(flavor = "multi_thread")]
async fn chat_files_and_music_answer_through_the_facade() {
    let server = TestServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    setup::core_init(
        dir.path().join("data").to_string_lossy().into_owned(),
        dir.path().join("cache").to_string_lossy().into_owned(),
    )
    .unwrap();
    let generation = setup::events_attach().unwrap();
    let new = identity::identity_generate();
    identity::identity_load(new.secret, "Ava".to_owned()).unwrap();
    servers::server_trust(server.address(), server.fingerprint()).unwrap();
    let added = servers::server_add(server.address(), server.claim_token())
        .await
        .unwrap();
    assert!(matches!(added, AddServerResult::Added { .. }));
    let ready = ready(generation).await;
    let key = server.address();
    let general = channel(&ready, "general");

    send_fetch_edit_delete(&key, &general).await;
    music_answers_on_a_fresh_server(&key, &channel(&ready, "General")).await;
    // Last: it stops the server.
    files_are_fetched_once(server, &key, &general).await;
}

async fn ready(generation: u64) -> ServerSnapshot {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            for event in setup::next_events(generation, 64).await.unwrap() {
                if let opencord_mobile::MobileEvent::Ready { snapshot } = event {
                    return snapshot;
                }
            }
        }
    })
    .await
    .expect("no Ready")
}

fn channel(ready: &ServerSnapshot, name: &str) -> String {
    ready
        .channels
        .iter()
        .find(|channel| channel.name == name)
        .unwrap_or_else(|| panic!("no channel {name}"))
        .id
        .clone()
}

async fn send_fetch_edit_delete(key: &str, channel: &str) {
    let sent = chat::message_send(
        key.to_owned(),
        channel.to_owned(),
        "one".to_owned(),
        "n1".to_owned(),
    )
    .await
    .unwrap();
    assert_eq!(sent.nonce.as_deref(), Some("n1"));
    for (text, nonce) in [("two", "n2"), ("three", "n3")] {
        chat::message_send(
            key.to_owned(),
            channel.to_owned(),
            text.to_owned(),
            nonce.to_owned(),
        )
        .await
        .unwrap();
    }
    let newest = chat::messages_fetch(key.to_owned(), channel.to_owned(), None, 2)
        .await
        .unwrap();
    let texts: Vec<_> = newest
        .iter()
        .map(|message| message.content.as_str())
        .collect();
    assert_eq!(texts, ["three", "two"], "newest first");
    let older = chat::messages_fetch(
        key.to_owned(),
        channel.to_owned(),
        Some(newest[1].id.clone()),
        10,
    )
    .await
    .unwrap();
    assert_eq!(older[0].content, "one");

    let edited = chat::message_edit(key.to_owned(), sent.id.clone(), "one!".to_owned())
        .await
        .unwrap();
    assert_eq!(edited.content, "one!");
    assert!(edited.edited_at_ms.is_some());
    chat::message_delete(key.to_owned(), sent.id.clone())
        .await
        .unwrap();
    let left = chat::messages_fetch(key.to_owned(), channel.to_owned(), None, 10)
        .await
        .unwrap();
    assert!(left.iter().all(|message| message.id != sent.id));
}

async fn files_are_fetched_once(server: TestServer, key: &str, channel: &str) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("picture.png");
    let png = png();
    std::fs::write(&path, &png).unwrap();
    let message = chat::message_send_files(
        key.to_owned(),
        channel.to_owned(),
        String::new(),
        "f1".to_owned(),
        vec![OutgoingFileRecord {
            path: path.to_string_lossy().into_owned(),
            filename: "picture.png".to_owned(),
        }],
    )
    .await
    .unwrap();
    let attachment = &message.attachments[0];
    assert!(attachment.has_thumbnail);
    let original = files::attachment_file(
        key.to_owned(),
        attachment.id.clone(),
        AttachmentVariant::Original,
    )
    .await
    .unwrap();
    let thumbnail = files::attachment_file(
        key.to_owned(),
        attachment.id.clone(),
        AttachmentVariant::Thumbnail,
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read(&original).unwrap(), png);
    let thumbnail_bytes = std::fs::read(&thumbnail).unwrap();
    assert!(!thumbnail_bytes.is_empty());

    // With the server gone, both come from the cache.
    server.handle.shutdown().await;
    let again = files::attachment_file(
        key.to_owned(),
        attachment.id.clone(),
        AttachmentVariant::Original,
    )
    .await
    .unwrap();
    assert_eq!(again, original);
    let again = files::attachment_file(
        key.to_owned(),
        attachment.id.clone(),
        AttachmentVariant::Thumbnail,
    )
    .await
    .unwrap();
    assert_eq!(std::fs::read(again).unwrap(), thumbnail_bytes);
}

async fn music_answers_on_a_fresh_server(key: &str, voice: &str) {
    let settings = music::music_settings(key.to_owned()).await.unwrap();
    assert!(settings.supported);
    let session = music::music_subscribe(key.to_owned(), voice.to_owned())
        .await
        .unwrap();
    assert_eq!(session.channel_id, voice);
    assert_eq!(session.state, MusicPlayState::Idle);
    assert!(session.now_playing.is_none());
    assert!(session.queue.is_empty());
    music::music_unsubscribe(key.to_owned(), voice.to_owned())
        .await
        .unwrap();
}
