//! Private messages, friends, message requests, blocking and settings
//! (private messages plan, Milestone E4), for the Flutter app.

use crate::frb_generated::StreamSink;

use super::client::{client, on_runtime, runtime};
use super::types::{
    CoreError, PrivacySettingsData, PrivateContact, PrivateEvent, PrivateMessage, PrivateOpened,
    SharedServer,
};

/// Opens private messages for the identity using the master key from the
/// keychain. Not 32 bytes returns `CoreError::InvalidInput`.
pub async fn private_open(key: Vec<u8>) -> Result<PrivateOpened, CoreError> {
    let key: [u8; 32] = key
        .try_into()
        .map_err(|_| CoreError::InvalidInput {
            message: "private message key must be 32 bytes".to_owned(),
        })?;
    on_runtime(move |client| async move { client.private_open(key).await }).await
}

/// Forwards private events into the sink on the runtime. Opening it again
/// replaces the earlier stream. Nothing is kept while none is open; the app
/// reloads after subscribing.
pub fn private_event_stream(sink: StreamSink<PrivateEvent>) -> Result<(), CoreError> {
    let client = client()?;
    let runtime = runtime()?;
    let mut rx = client.private_events();
    runtime.spawn(async move {
        while let Some(event) = rx.recv().await {
            if sink.add(event).is_err() {
                break;
            }
        }
    });
    Ok(())
}

pub async fn private_contacts() -> Result<Vec<PrivateContact>, CoreError> {
    on_runtime(move |client| async move { client.private_contacts().await }).await
}

pub async fn private_messages(
    peer: String,
    before: Option<String>,
    limit: u32,
) -> Result<Vec<PrivateMessage>, CoreError> {
    on_runtime(move |client| async move { client.private_messages(&peer, before, limit).await })
        .await
}

pub async fn private_send(peer: String, content: String) -> Result<PrivateMessage, CoreError> {
    on_runtime(move |client| async move { client.private_send(&peer, content).await }).await
}

pub async fn private_edit(
    peer: String,
    message_id: String,
    content: String,
) -> Result<(), CoreError> {
    on_runtime(
        move |client| async move { client.private_edit(&peer, &message_id, content).await },
    )
    .await
}

pub async fn private_delete(peer: String, message_id: String) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.private_delete(&peer, &message_id).await }).await
}

pub async fn private_retry(message_id: String) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.private_retry(&message_id).await }).await
}

pub async fn friend_request_send(server_key: String, user_id: i64) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.friend_request_send(&server_key, user_id).await })
        .await
}

pub async fn friend_request_answer(peer: String, accept: bool) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.friend_request_answer(&peer, accept).await }).await
}

pub async fn friend_request_cancel(peer: String) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.friend_request_cancel(&peer).await }).await
}

pub async fn friend_remove(peer: String) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.friend_remove(&peer).await }).await
}

pub async fn message_request_accept(peer: String) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.message_request_accept(&peer).await }).await
}

pub async fn message_request_ignore(peer: String) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.message_request_ignore(&peer).await }).await
}

pub async fn private_block(peer: String) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.private_block(&peer).await }).await
}

pub async fn private_unblock(peer: String) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.private_unblock(&peer).await }).await
}

pub async fn privacy_settings() -> Result<PrivacySettingsData, CoreError> {
    on_runtime(move |client| async move { client.privacy_settings().await }).await
}

pub async fn set_privacy_settings(settings: PrivacySettingsData) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.set_privacy_settings(settings).await }).await
}

pub async fn private_open_conversation(
    server_key: String,
    user_id: i64,
) -> Result<PrivateContact, CoreError> {
    on_runtime(move |client| async move {
        client
            .private_open_conversation(&server_key, user_id)
            .await
    })
    .await
}

pub async fn private_shared_servers(peer: String) -> Result<Vec<SharedServer>, CoreError> {
    on_runtime(move |client| async move { Ok(client.private_shared_servers(&peer)) }).await
}

pub async fn private_move(peer: String, server_key: String) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.private_move(&peer, &server_key).await }).await
}

pub async fn private_move_too(peer: String, message_id: String) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.private_move_too(&peer, &message_id).await }).await
}

pub async fn private_typing(peer: String) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.private_typing(&peer).await }).await
}

pub async fn private_mark_read(peer: String) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.private_mark_read(&peer).await }).await
}

pub async fn private_set_muted(peer: String, muted: bool) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.private_set_muted(&peer, muted).await }).await
}

pub async fn private_hide(peer: String) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.private_hide(&peer).await }).await
}
