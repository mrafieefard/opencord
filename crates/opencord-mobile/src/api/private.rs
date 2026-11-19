//! Private messages and friends (phone P3): one call per `Client` method,
//! named the same. A `peer` is a public key in lowercase hex and a message
//! or device id is hex; the core checks them. A `keys` string is the one the
//! last words call returned, handed back unchanged.

use crate::MobileError;
use crate::convert;
use crate::ids::parse_id;
use crate::mobile::{mobile, on_runtime};
use crate::private_types::{
    KeyWordsRecord, OwnDevicesRecord, PrivacySettingsRecord, PrivateContactRecord,
    PrivateMessageRecord, PrivateOpenedRecord, SharedServerRecord,
};

/// The most messages one `private_messages` call returns.
const MESSAGES_PAGE: u32 = 100;

/// The private database's master key: exactly 32 bytes.
fn master_key(key: Vec<u8>) -> Result<[u8; 32], MobileError> {
    let length = key.len();
    key.try_into().map_err(|_| MobileError::InvalidInput {
        message: format!("the private key must be 32 bytes, not {length}"),
    })
}

/// A new master key: 32 bytes from the operating system's random source.
/// The phone keeps it in its keystore; JavaScript has no random source of
/// its own it can be sure of.
#[uniffi::export]
pub fn private_key_generate() -> Result<Vec<u8>, MobileError> {
    let mut key = [0u8; 32];
    getrandom::fill(&mut key).map_err(|error| MobileError::Other {
        message: format!("the operating system's random source failed: {error}"),
    })?;
    Ok(key.to_vec())
}

/// Opens the private database with its master key, after the identity.
/// Its events come through `next_events` from then on, also after a
/// JavaScript reload calls this again.
#[uniffi::export]
pub async fn private_open(key: Vec<u8>) -> Result<PrivateOpenedRecord, MobileError> {
    let key = master_key(key)?;
    mobile()?.private_open(key).await
}

#[uniffi::export]
pub async fn private_contacts() -> Result<Vec<PrivateContactRecord>, MobileError> {
    on_runtime(move |client| async move {
        let contacts = client.private_contacts().await?;
        Ok(contacts.into_iter().map(convert::private_contact).collect())
    })
    .await
}

/// The newest `limit` (at most 100) messages before the message `before`
/// (hex), or the newest of all.
#[uniffi::export]
pub async fn private_messages(
    peer: String,
    before: Option<String>,
    limit: u32,
) -> Result<Vec<PrivateMessageRecord>, MobileError> {
    let limit = limit.min(MESSAGES_PAGE);
    on_runtime(move |client| async move {
        let messages = client.private_messages(&peer, before, limit).await?;
        Ok(messages.into_iter().map(convert::private_message).collect())
    })
    .await
}

#[uniffi::export]
pub async fn private_send(
    peer: String,
    content: String,
) -> Result<PrivateMessageRecord, MobileError> {
    on_runtime(move |client| async move {
        Ok(convert::private_message(
            client.private_send(&peer, content).await?,
        ))
    })
    .await
}

#[uniffi::export]
pub async fn private_edit(
    peer: String,
    message_id: String,
    content: String,
) -> Result<(), MobileError> {
    on_runtime(
        move |client| async move { Ok(client.private_edit(&peer, &message_id, content).await?) },
    )
    .await
}

#[uniffi::export]
pub async fn private_delete(peer: String, message_id: String) -> Result<(), MobileError> {
    on_runtime(move |client| async move { Ok(client.private_delete(&peer, &message_id).await?) })
        .await
}

#[uniffi::export]
pub async fn private_retry(message_id: String) -> Result<(), MobileError> {
    on_runtime(move |client| async move { Ok(client.private_retry(&message_id).await?) }).await
}

/// `user_id` is their decimal id on `server_key`.
#[uniffi::export]
pub async fn friend_request_send(server_key: String, user_id: String) -> Result<(), MobileError> {
    let user_id = parse_id(&user_id)?;
    on_runtime(
        move |client| async move { Ok(client.friend_request_send(&server_key, user_id).await?) },
    )
    .await
}

#[uniffi::export]
pub async fn friend_request_answer(peer: String, accept: bool) -> Result<(), MobileError> {
    on_runtime(move |client| async move { Ok(client.friend_request_answer(&peer, accept).await?) })
        .await
}

#[uniffi::export]
pub async fn friend_request_cancel(peer: String) -> Result<(), MobileError> {
    on_runtime(move |client| async move { Ok(client.friend_request_cancel(&peer).await?) }).await
}

#[uniffi::export]
pub async fn friend_remove(peer: String) -> Result<(), MobileError> {
    on_runtime(move |client| async move { Ok(client.friend_remove(&peer).await?) }).await
}

#[uniffi::export]
pub async fn message_request_accept(peer: String) -> Result<(), MobileError> {
    on_runtime(move |client| async move { Ok(client.message_request_accept(&peer).await?) }).await
}

#[uniffi::export]
pub async fn message_request_ignore(peer: String) -> Result<(), MobileError> {
    on_runtime(move |client| async move { Ok(client.message_request_ignore(&peer).await?) }).await
}

#[uniffi::export]
pub async fn private_block(peer: String) -> Result<(), MobileError> {
    on_runtime(move |client| async move { Ok(client.private_block(&peer).await?) }).await
}

#[uniffi::export]
pub async fn private_unblock(peer: String) -> Result<(), MobileError> {
    on_runtime(move |client| async move { Ok(client.private_unblock(&peer).await?) }).await
}

#[uniffi::export]
pub async fn privacy_settings() -> Result<PrivacySettingsRecord, MobileError> {
    on_runtime(move |client| async move {
        Ok(convert::privacy_settings(client.privacy_settings().await?))
    })
    .await
}

/// The core's `set_privacy_settings`.
#[uniffi::export]
pub async fn privacy_settings_set(settings: PrivacySettingsRecord) -> Result<(), MobileError> {
    let settings = convert::core_privacy_settings(settings);
    on_runtime(move |client| async move { Ok(client.set_privacy_settings(settings).await?) }).await
}

/// The chat with the user `user_id` (decimal) of `server_key`, made if
/// there is none.
#[uniffi::export]
pub async fn private_open_conversation(
    server_key: String,
    user_id: String,
) -> Result<PrivateContactRecord, MobileError> {
    let user_id = parse_id(&user_id)?;
    on_runtime(move |client| async move {
        Ok(convert::private_contact(
            client
                .private_open_conversation(&server_key, user_id)
                .await?,
        ))
    })
    .await
}

/// The servers both of you are on, in the server list's order.
#[uniffi::export]
pub async fn private_shared_servers(peer: String) -> Result<Vec<SharedServerRecord>, MobileError> {
    on_runtime(move |client| async move {
        let servers = client.private_shared_servers(&peer);
        Ok(servers.into_iter().map(convert::shared_server).collect())
    })
    .await
}

#[uniffi::export]
pub async fn private_move(peer: String, server_key: String) -> Result<(), MobileError> {
    on_runtime(move |client| async move { Ok(client.private_move(&peer, &server_key).await?) })
        .await
}

#[uniffi::export]
pub async fn private_move_too(peer: String, message_id: String) -> Result<(), MobileError> {
    on_runtime(move |client| async move { Ok(client.private_move_too(&peer, &message_id).await?) })
        .await
}

#[uniffi::export]
pub async fn private_typing(peer: String) -> Result<(), MobileError> {
    on_runtime(move |client| async move { Ok(client.private_typing(&peer).await?) }).await
}

#[uniffi::export]
pub async fn private_mark_read(peer: String) -> Result<(), MobileError> {
    on_runtime(move |client| async move { Ok(client.private_mark_read(&peer).await?) }).await
}

#[uniffi::export]
pub async fn private_set_muted(peer: String, muted: bool) -> Result<(), MobileError> {
    on_runtime(move |client| async move { Ok(client.private_set_muted(&peer, muted).await?) }).await
}

#[uniffi::export]
pub async fn private_hide(peer: String) -> Result<(), MobileError> {
    on_runtime(move |client| async move { Ok(client.private_hide(&peer).await?) }).await
}

/// This person's other devices with other chat keys, and whether this
/// device has any chats.
#[uniffi::export]
pub async fn private_own_devices() -> Result<OwnDevicesRecord, MobileError> {
    on_runtime(move |client| async move {
        let (devices, has_chats) = client.private_own_devices().await?;
        Ok(convert::own_devices(devices, has_chats))
    })
    .await
}

/// The words to compare with another device of yours.
#[uniffi::export]
pub async fn private_pairing_words(device_id: String) -> Result<KeyWordsRecord, MobileError> {
    on_runtime(move |client| async move {
        Ok(convert::key_words(
            client.private_pairing_words(&device_id).await?,
        ))
    })
    .await
}

/// They match, on the device that has the chat key: gives it to
/// `device_id`. `keys` is the one `private_pairing_words` returned.
#[uniffi::export]
pub async fn private_approve_device(device_id: String, keys: String) -> Result<(), MobileError> {
    on_runtime(
        move |client| async move { Ok(client.private_approve_device(&device_id, &keys).await?) },
    )
    .await
}

/// The answer on the device being offered a chat key. `keys` is the one
/// `private_pairing_words` returned.
#[uniffi::export]
pub async fn private_accept_keys_from(
    device_id: String,
    matched: bool,
    keys: String,
) -> Result<(), MobileError> {
    on_runtime(move |client| async move {
        Ok(client
            .private_accept_keys_from(&device_id, matched, &keys)
            .await?)
    })
    .await
}

/// The words to compare with `peer`; with `new_keys`, for their new keys.
#[uniffi::export]
pub async fn private_words(peer: String, new_keys: bool) -> Result<KeyWordsRecord, MobileError> {
    on_runtime(move |client| async move {
        Ok(convert::key_words(
            client.private_words(&peer, new_keys).await?,
        ))
    })
    .await
}

/// The answer to `private_words`. `keys` is the one it returned.
#[uniffi::export]
pub async fn private_verify(
    peer: String,
    new_keys: bool,
    matched: bool,
    keys: String,
) -> Result<(), MobileError> {
    on_runtime(move |client| async move {
        Ok(client
            .private_verify(&peer, new_keys, matched, &keys)
            .await?)
    })
    .await
}

/// Use new keys anyway, without comparing words.
#[uniffi::export]
pub async fn private_accept_new_keys(peer: String) -> Result<(), MobileError> {
    on_runtime(move |client| async move { Ok(client.private_accept_new_keys(&peer).await?) }).await
}

#[uniffi::export]
pub async fn private_reject_device(device_id: String) -> Result<(), MobileError> {
    on_runtime(move |client| async move { Ok(client.private_reject_device(&device_id).await?) })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fails if a master key of the wrong length reaches the core, which
    // would open (or start fresh) the private database with a wrong key.
    #[tokio::test]
    async fn a_short_master_key_is_refused() {
        for length in [0, 16, 31, 33, 64] {
            match private_open(vec![7; length]).await {
                Err(MobileError::InvalidInput { message }) => {
                    assert!(message.contains("32 bytes"), "{length}: {message}");
                }
                other => panic!("{length} bytes gave {other:?}"),
            }
        }
        assert_eq!(master_key(vec![7; 32]).unwrap(), [7; 32]);
    }

    // Fails if a new key is the wrong length (private_open refuses it) or
    // isn't random (every phone would seal its chats with one key).
    #[test]
    fn a_new_master_key_is_32_random_bytes() {
        let first = private_key_generate().unwrap();
        let second = private_key_generate().unwrap();
        assert!(master_key(first.clone()).is_ok());
        assert_ne!(first, second);
        assert_ne!(first, vec![0; 32]);
    }
}
