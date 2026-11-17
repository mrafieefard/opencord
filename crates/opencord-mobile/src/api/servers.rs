//! The saved servers.

use crate::MobileError;
use crate::convert;
use crate::mobile::{mobile, on_runtime};
use crate::types::{AddServerResult, OwnAvatarRecord, PresenceStatus, ServerEntry, UserRecord};

#[uniffi::export]
pub fn servers_list() -> Result<Vec<ServerEntry>, MobileError> {
    Ok(mobile()?
        .client()
        .servers()
        .into_iter()
        .map(convert::server_entry)
        .collect())
}

/// Adds a server from an invite link or `host:port`, optionally claiming it
/// with the token the server printed on its first start.
#[uniffi::export]
pub async fn server_add(
    link_or_address: String,
    claim_token: Option<String>,
) -> Result<AddServerResult, MobileError> {
    on_runtime(move |client| async move {
        let outcome = client.add_server(&link_or_address, claim_token).await?;
        Ok(convert::add_result(outcome))
    })
    .await
}

/// Pins a certificate the user confirmed; then add the server again.
#[uniffi::export]
pub fn server_trust(address: String, fingerprint: String) -> Result<(), MobileError> {
    Ok(mobile()?
        .client()
        .trust_fingerprint(&address, &fingerprint)?)
}

/// Tries a server again now instead of waiting, or after a failure.
#[uniffi::export]
pub async fn server_retry_now(server_key: String) -> Result<(), MobileError> {
    on_runtime(move |client| async move { Ok(client.retry_now(&server_key).await?) }).await
}

#[uniffi::export]
pub async fn server_remove(server_key: String) -> Result<(), MobileError> {
    on_runtime(move |client| async move { Ok(client.remove_server(&server_key).await?) }).await
}

/// Sends this person's display name to a server.
#[uniffi::export]
pub async fn profile_update(
    server_key: String,
    display_name: String,
) -> Result<UserRecord, MobileError> {
    on_runtime(move |client| async move {
        let user = client.update_profile(&server_key, display_name).await?;
        Ok(convert::user(user))
    })
    .await
}

#[uniffi::export]
pub async fn presence_update(
    server_key: String,
    status: PresenceStatus,
) -> Result<(), MobileError> {
    on_runtime(move |client| async move {
        Ok(client
            .update_presence(&server_key, convert::presence_status_to_core(status))
            .await?)
    })
    .await
}

/// The file of the profile picture or server icon named `hash`, fetched
/// from the server unless this device has it.
#[uniffi::export]
pub async fn avatar_file(server_key: String, hash: String) -> Result<String, MobileError> {
    on_runtime(move |client| async move { Ok(client.avatar_path(&server_key, &hash).await?) }).await
}

/// Makes this person's profile picture from the file at `source_path`, or
/// removes it (`None`); returns the new hash, `None` when removed.
#[uniffi::export]
pub fn avatar_set(source_path: Option<String>) -> Result<Option<String>, MobileError> {
    Ok(mobile()?.client().set_own_avatar(source_path.as_deref())?)
}

/// Sends this person's profile picture to a server.
#[uniffi::export]
pub async fn avatar_upload(server_key: String) -> Result<UserRecord, MobileError> {
    on_runtime(move |client| async move {
        let user = client.upload_avatar(&server_key).await?;
        Ok(convert::user(user))
    })
    .await
}

/// This person's profile picture's hash and local file path.
#[uniffi::export]
pub fn own_avatar() -> Result<OwnAvatarRecord, MobileError> {
    let client = mobile()?.client();
    Ok(OwnAvatarRecord {
        hash: client.own_avatar_hash(),
        file: client.own_avatar_file(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, ImageFormat, Rgba};
    use std::io::Cursor;

    fn png_2x2() -> Vec<u8> {
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_fn(2, 2, |x, y| {
            Rgba([(x * 100) as u8, (y * 100) as u8, 128, 255])
        });
        let mut bytes = Vec::new();
        img.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .unwrap();
        bytes
    }

    // Fails if setting an avatar doesn't compute and return its sha256 hash or update own_avatar.
    #[test]
    fn an_avatar_set_returns_its_hash() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path().join("data");
        let cache_dir = dir.path().join("cache");
        std::fs::create_dir_all(&data_dir).unwrap();
        std::fs::create_dir_all(&cache_dir).unwrap();
        let _ = crate::api::setup::core_init(
            data_dir.to_string_lossy().into_owned(),
            cache_dir.to_string_lossy().into_owned(),
        );
        let png_path = dir.path().join("test_avatar.png");
        std::fs::write(&png_path, png_2x2()).unwrap();

        let hash = avatar_set(Some(png_path.to_string_lossy().into_owned())).unwrap();
        assert!(hash.is_some());
        let hash = hash.unwrap();
        assert_eq!(hash.len(), 64);

        let own = own_avatar().unwrap();
        assert_eq!(own.hash, Some(hash));
        assert!(own.file.is_some());

        // Removing the avatar returns None.
        let removed = avatar_set(None).unwrap();
        assert_eq!(removed, None);
        assert_eq!(own_avatar().unwrap().hash, None);
    }
}
