//! The identity: a key kept in the system's keychain by the app.

use opencord_common::validation;
use opencord_core::identity::{self, Identity};

use crate::MobileError;
use crate::mobile::mobile;
use crate::types::{IdentityInfo, NewIdentity};

#[uniffi::export]
pub fn identity_generate() -> NewIdentity {
    let identity = Identity::generate();
    NewIdentity {
        secret: identity.secret().to_vec(),
        info: info(&identity),
    }
}

/// Checks a secret and display name as `identity_load` does, without using
/// them: the app saves an identity before putting it to use.
#[uniffi::export]
pub fn identity_check(secret: Vec<u8>, display_name: String) -> Result<IdentityInfo, MobileError> {
    let (identity, _) = checked(&secret, &display_name)?;
    Ok(info(&identity))
}

/// Uses this identity and name for every server, and connects to the saved
/// servers.
#[uniffi::export]
pub fn identity_load(secret: Vec<u8>, display_name: String) -> Result<IdentityInfo, MobileError> {
    let (identity, display_name) = checked(&secret, &display_name)?;
    let info = info(&identity);
    let client = mobile()?.client();
    client.set_identity(identity, display_name);
    client.connect_saved_servers();
    Ok(info)
}

/// The text of a backup file, as the desktop writes it. It holds the secret.
#[uniffi::export]
pub fn identity_backup_encode(secret: Vec<u8>) -> Result<String, MobileError> {
    identity::encode_backup(&secret).map_err(invalid)
}

/// The secret in a backup file's text.
#[uniffi::export]
pub fn identity_backup_decode(text: String) -> Result<Vec<u8>, MobileError> {
    identity::decode_backup(&text)
        .map(|secret| secret.to_vec())
        .map_err(invalid)
}

fn checked(secret: &[u8], display_name: &str) -> Result<(Identity, String), MobileError> {
    let identity = Identity::from_secret(secret).map_err(invalid)?;
    let display_name = validation::display_name(display_name).map_err(invalid)?;
    Ok((identity, display_name))
}

fn info(identity: &Identity) -> IdentityInfo {
    IdentityInfo {
        public_key_hex: hex::encode(identity.public_key()),
        fingerprint: identity.fingerprint(),
    }
}

fn invalid(error: impl std::fmt::Display) -> MobileError {
    MobileError::InvalidInput {
        message: error.to_string(),
    }
}
