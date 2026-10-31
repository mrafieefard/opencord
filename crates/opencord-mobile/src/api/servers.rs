//! The saved servers.

use crate::MobileError;
use crate::convert;
use crate::mobile::{mobile, on_runtime};
use crate::types::{AddServerResult, ServerEntry, UserRecord};

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

/// The file of the profile picture or server icon named `hash`, fetched
/// from the server unless this device has it.
#[uniffi::export]
pub async fn avatar_file(server_key: String, hash: String) -> Result<String, MobileError> {
    on_runtime(move |client| async move { Ok(client.avatar_path(&server_key, &hash).await?) }).await
}
