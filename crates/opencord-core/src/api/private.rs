//! Private messages, friends, message requests, blocking and settings
//! (private messages plan, Milestone E4), for the Flutter app.

use crate::frb_generated::StreamSink;

use super::client::{client, on_runtime, runtime};
use super::types::{
    CoreError, KeyWords, OwnDevice, PrivacySettingsData, PrivateContact, PrivateEvent,
    PrivateMessage, PrivateOpened, PrivateSettings, SharedServer,
};

/// Opens private messages for the identity using the master key from the
/// keychain. Not 32 bytes returns `CoreError::InvalidInput`.
pub async fn private_open(key: Vec<u8>) -> Result<PrivateOpened, CoreError> {
    let key: [u8; 32] = key.try_into().map_err(|_| CoreError::InvalidInput {
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

/// MANAGE_SERVER: whether the server relays private messages, and only
/// between friends. `None` leaves a setting as it is.
pub async fn update_private_settings(
    server_key: String,
    enabled: Option<bool>,
    friends_only: Option<bool>,
) -> Result<PrivateSettings, CoreError> {
    on_runtime(move |client| async move {
        client
            .update_private_settings(&server_key, enabled, friends_only)
            .await
    })
    .await
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
    on_runtime(move |client| async move { client.private_edit(&peer, &message_id, content).await })
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
    on_runtime(
        move |client| async move { client.private_open_conversation(&server_key, user_id).await },
    )
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

// Chat keys (design 2026-11-01).

/// This person's other devices with chat keys of their own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnDevices {
    pub devices: Vec<OwnDevice>,
    /// This device has private chats.
    pub has_chats: bool,
}

/// The 16 words for the chat with `peer` and the keys they were made from;
/// with `new_keys`, for the other chat key their devices offer.
pub async fn private_words(peer: String, new_keys: bool) -> Result<KeyWords, CoreError> {
    on_runtime(move |client| async move { client.private_words(&peer, new_keys).await }).await
}

/// They match or They don't match, for the chat with `peer`, on the words
/// made from `keys`.
pub async fn private_verify(
    peer: String,
    new_keys: bool,
    matched: bool,
    keys: String,
) -> Result<(), CoreError> {
    on_runtime(
        move |client| async move { client.private_verify(&peer, new_keys, matched, &keys).await },
    )
    .await
}

/// Use new keys anyway.
pub async fn private_accept_new_keys(peer: String) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.private_accept_new_keys(&peer).await }).await
}

pub async fn private_own_devices() -> Result<OwnDevices, CoreError> {
    on_runtime(move |client| async move {
        let (devices, has_chats) = client.private_own_devices().await?;
        Ok(OwnDevices { devices, has_chats })
    })
    .await
}

/// The 16 words this device and `device_id` both show, and the keys they
/// were made from.
pub async fn private_pairing_words(device_id: String) -> Result<KeyWords, CoreError> {
    on_runtime(move |client| async move { client.private_pairing_words(&device_id).await }).await
}

/// They match, on the device giving its chat key to `device_id`, on the
/// words made from `keys`.
pub async fn private_approve_device(device_id: String, keys: String) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.private_approve_device(&device_id, &keys).await })
        .await
}

/// They match or They don't match, for the chat key `device_id` offered, on
/// the words made from `keys`.
pub async fn private_accept_keys_from(
    device_id: String,
    matched: bool,
    keys: String,
) -> Result<(), CoreError> {
    on_runtime(move |client| async move {
        client
            .private_accept_keys_from(&device_id, matched, &keys)
            .await
    })
    .await
}

/// Not mine.
pub async fn private_reject_device(device_id: String) -> Result<(), CoreError> {
    on_runtime(move |client| async move { client.private_reject_device(&device_id).await }).await
}
