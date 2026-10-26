//! Messages: history, sending, editing, deleting and typing.

use crate::MobileError;
use crate::convert;
use crate::ids::parse_id;
use crate::mobile::{mobile, on_runtime};
use crate::types::{MessageRecord, OutgoingFileRecord};

/// The newest `limit` (1–100) messages before `before`, or the newest of
/// all; newest first, as the core returns them.
#[uniffi::export]
pub async fn messages_fetch(
    server_key: String,
    channel_id: String,
    before: Option<String>,
    limit: u32,
) -> Result<Vec<MessageRecord>, MobileError> {
    let channel_id = parse_id(&channel_id)?;
    let before = before.as_deref().map(parse_id).transpose()?;
    on_runtime(move |client| async move {
        let messages = client
            .fetch_messages(&server_key, channel_id, before, limit.clamp(1, 100))
            .await?;
        Ok(messages.into_iter().map(convert::message).collect())
    })
    .await
}

/// Sends a message; its `MessageCreated` echo carries `nonce`.
#[uniffi::export]
pub async fn message_send(
    server_key: String,
    channel_id: String,
    content: String,
    nonce: String,
) -> Result<MessageRecord, MobileError> {
    let channel_id = parse_id(&channel_id)?;
    on_runtime(move |client| async move {
        let message = client
            .send_message(&server_key, channel_id, content, nonce)
            .await?;
        Ok(convert::message(message))
    })
    .await
}

/// Sends a message with files; `SendProgress` events follow under `nonce`.
#[uniffi::export]
pub async fn message_send_files(
    server_key: String,
    channel_id: String,
    content: String,
    nonce: String,
    files: Vec<OutgoingFileRecord>,
) -> Result<MessageRecord, MobileError> {
    let channel_id = parse_id(&channel_id)?;
    let files = files.into_iter().map(convert::outgoing_file).collect();
    on_runtime(move |client| async move {
        let message = client
            .send_message_with_files(&server_key, channel_id, content, nonce, files)
            .await?;
        Ok(convert::message(message))
    })
    .await
}

/// Stops a send that is still uploading; it then fails with `Cancelled`.
#[uniffi::export]
pub fn send_cancel(server_key: String, nonce: String) -> Result<(), MobileError> {
    mobile()?.client().cancel_send(&server_key, &nonce);
    Ok(())
}

#[uniffi::export]
pub async fn message_edit(
    server_key: String,
    message_id: String,
    content: String,
) -> Result<MessageRecord, MobileError> {
    let message_id = parse_id(&message_id)?;
    on_runtime(move |client| async move {
        let message = client
            .edit_message(&server_key, message_id, content)
            .await?;
        Ok(convert::message(message))
    })
    .await
}

#[uniffi::export]
pub async fn message_delete(server_key: String, message_id: String) -> Result<(), MobileError> {
    let message_id = parse_id(&message_id)?;
    on_runtime(
        move |client| async move { Ok(client.delete_message(&server_key, message_id).await?) },
    )
    .await
}

#[uniffi::export]
pub async fn typing_start(server_key: String, channel_id: String) -> Result<(), MobileError> {
    let channel_id = parse_id(&channel_id)?;
    on_runtime(move |client| async move { Ok(client.start_typing(&server_key, channel_id).await?) })
        .await
}
