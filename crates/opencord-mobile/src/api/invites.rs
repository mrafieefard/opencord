//! Invites to servers.

use crate::MobileError;
use crate::convert;
use crate::mobile::on_runtime;
use crate::types::InviteRecord;

#[uniffi::export]
pub async fn invite_create(
    server_key: String,
    max_uses: Option<u32>,
    expires_in_s: Option<u32>,
) -> Result<InviteRecord, MobileError> {
    on_runtime(move |client| async move {
        let invite = client
            .create_invite(&server_key, max_uses, expires_in_s, None)
            .await?;
        Ok(convert::invite(invite))
    })
    .await
}

#[uniffi::export]
pub async fn invites_fetch(server_key: String) -> Result<Vec<InviteRecord>, MobileError> {
    on_runtime(move |client| async move {
        let invites = client.fetch_invites(&server_key).await?;
        Ok(invites.into_iter().map(convert::invite).collect())
    })
    .await
}

#[uniffi::export]
pub async fn invite_revoke(server_key: String, code: String) -> Result<(), MobileError> {
    on_runtime(move |client| async move { Ok(client.revoke_invite(&server_key, code).await?) })
        .await
}
