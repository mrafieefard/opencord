//! The user's identity: an Ed25519 key pair that never leaves the device.

use opencord_common::auth::{self, Challenge, SigningKey};
use sha2::{Digest, Sha256};

pub const SECRET_LEN: usize = 32;

const BACKUP_PREFIX: &str = "opencord-identity-v1";
const CHECKSUM_LEN: usize = 4;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IdentityError {
    #[error("an identity secret is exactly 32 bytes")]
    InvalidSecret,
    #[error("this is not an Opencord identity backup")]
    NotABackup,
    #[error("the identity backup is damaged")]
    DamagedBackup,
}

#[derive(Clone)]
pub struct Identity {
    key: SigningKey,
}

impl std::fmt::Debug for Identity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Identity")
            .field("fingerprint", &self.fingerprint())
            .finish_non_exhaustive()
    }
}

impl Identity {
    pub fn generate() -> Self {
        let mut secret = [0; SECRET_LEN];
        // The OS random source failing leaves nothing secure to fall back to.
        getrandom::fill(&mut secret).expect("the operating system's random source is unavailable");
        Self {
            key: SigningKey::from_bytes(&secret),
        }
    }

    pub fn from_secret(secret: &[u8]) -> Result<Self, IdentityError> {
        let secret: [u8; SECRET_LEN] = secret
            .try_into()
            .map_err(|_| IdentityError::InvalidSecret)?;
        Ok(Self {
            key: SigningKey::from_bytes(&secret),
        })
    }

    pub fn secret(&self) -> [u8; SECRET_LEN] {
        self.key.to_bytes()
    }

    pub fn public_key(&self) -> [u8; 32] {
        self.key.verifying_key().to_bytes()
    }

    /// Short, human-readable form of the public key, like `ABCD-EFGH-IJKL-MNOP`.
    pub fn fingerprint(&self) -> String {
        public_key_fingerprint(&self.public_key())
    }

    pub fn sign_identify(&self, challenge: &Challenge) -> [u8; 64] {
        auth::sign(&self.key, challenge)
    }
}

/// First 80 bits of the key's SHA-256 in base32, grouped by four.
pub fn public_key_fingerprint(public_key: &[u8]) -> String {
    let digest = Sha256::digest(public_key);
    let encoded: Vec<char> = data_encoding::BASE32_NOPAD
        .encode(&digest[..10])
        .chars()
        .collect();
    encoded
        .chunks(4)
        .map(|group| group.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("-")
}

/// Text that can be saved to a file and imported later. Contains the secret.
pub fn encode_backup(secret: &[u8]) -> Result<String, IdentityError> {
    let secret: [u8; SECRET_LEN] = secret
        .try_into()
        .map_err(|_| IdentityError::InvalidSecret)?;
    Ok(format!(
        "{BACKUP_PREFIX}:{}:{}",
        hex::encode(secret),
        hex::encode(checksum(&secret))
    ))
}

pub fn decode_backup(text: &str) -> Result<[u8; SECRET_LEN], IdentityError> {
    let rest = text
        .trim()
        .strip_prefix(BACKUP_PREFIX)
        .and_then(|rest| rest.strip_prefix(':'))
        .ok_or(IdentityError::NotABackup)?;
    let (secret_hex, checksum_hex) = rest.split_once(':').ok_or(IdentityError::DamagedBackup)?;
    let mut secret = [0; SECRET_LEN];
    hex::decode_to_slice(secret_hex, &mut secret).map_err(|_| IdentityError::DamagedBackup)?;
    let mut stored_checksum = [0; CHECKSUM_LEN];
    hex::decode_to_slice(checksum_hex, &mut stored_checksum)
        .map_err(|_| IdentityError::DamagedBackup)?;
    if checksum(&secret) == stored_checksum {
        Ok(secret)
    } else {
        Err(IdentityError::DamagedBackup)
    }
}

fn checksum(secret: &[u8; SECRET_LEN]) -> [u8; CHECKSUM_LEN] {
    let digest = Sha256::digest(secret);
    let mut checksum = [0; CHECKSUM_LEN];
    checksum.copy_from_slice(&digest[..CHECKSUM_LEN]);
    checksum
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_identities_differ() {
        assert_ne!(
            Identity::generate().public_key(),
            Identity::generate().public_key()
        );
    }

    #[test]
    fn loads_from_its_secret() {
        let original = Identity::generate();

        let loaded = Identity::from_secret(&original.secret()).unwrap();

        assert_eq!(loaded.public_key(), original.public_key());
        assert_eq!(
            Identity::from_secret(&[1; 31]).unwrap_err(),
            IdentityError::InvalidSecret
        );
    }

    #[test]
    fn fingerprint_is_four_groups_of_four() {
        let fingerprint = public_key_fingerprint(&[7; 32]);

        let groups: Vec<&str> = fingerprint.split('-').collect();
        assert_eq!(groups.len(), 4);
        assert!(groups.iter().all(|group| group.len() == 4));
        assert!(
            fingerprint
                .chars()
                .all(|c| c == '-' || c.is_ascii_uppercase() || c.is_ascii_digit())
        );
        assert_eq!(fingerprint, public_key_fingerprint(&[7; 32]));
        assert_ne!(fingerprint, public_key_fingerprint(&[8; 32]));
    }

    #[test]
    fn backups_round_trip() {
        let identity = Identity::generate();

        let text = encode_backup(&identity.secret()).unwrap();

        assert!(text.starts_with("opencord-identity-v1:"));
        assert_eq!(
            decode_backup(&format!("  {text}\n")).unwrap(),
            identity.secret()
        );
    }

    #[test]
    fn damaged_backups_are_rejected() {
        let text = encode_backup(&[5; 32]).unwrap();
        let flipped = text.replacen('0', "1", 1).replacen("05", "06", 1);

        assert_eq!(decode_backup("hello"), Err(IdentityError::NotABackup));
        assert_eq!(decode_backup(&flipped), Err(IdentityError::DamagedBackup));
        assert_eq!(
            decode_backup("opencord-identity-v1:zz:00000000"),
            Err(IdentityError::DamagedBackup)
        );
    }
}
