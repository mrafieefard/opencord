//! Secrets at rest (Phase 3 plan §7.3, §20): certificate private keys, the
//! ACME account, DNS provider tokens and, later, Lavalink passwords, sealed
//! with XChaCha20-Poly1305 under `secrets.key` in the data directory. The
//! database alone gives none of them away.

use std::fs;
use std::io::Write;
use std::path::Path;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};

pub const SECRETS_FILE: &str = "secrets.key";
const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 24;

#[derive(Debug, thiserror::Error)]
pub enum SecretsError {
    #[error("could not read or write {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("{0} is not a 32-byte key")]
    BadKey(String),
    #[error("a secret could not be opened: the data or secrets.key changed")]
    Open,
}

/// The key, and what it seals.
pub struct Secrets {
    cipher: XChaCha20Poly1305,
}

impl std::fmt::Debug for Secrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secrets")
    }
}

impl Secrets {
    /// The key in `data_dir`, made on first start (readable by the owner
    /// only).
    pub fn load_or_generate(data_dir: &Path) -> Result<Self, SecretsError> {
        let path = data_dir.join(SECRETS_FILE);
        let io = |source| SecretsError::Io {
            path: path.display().to_string(),
            source,
        };
        let key: [u8; KEY_LEN] = match fs::read(&path) {
            Ok(bytes) => bytes
                .try_into()
                .map_err(|_| SecretsError::BadKey(path.display().to_string()))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let key = crate::random::bytes::<KEY_LEN>();
                let mut options = fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
                let mut file = options.open(&path).map_err(io)?;
                file.write_all(&key).map_err(io)?;
                file.sync_all().map_err(io)?;
                key
            }
            Err(error) => return Err(io(error)),
        };
        Ok(Self::from_key(&key))
    }

    pub fn from_key(key: &[u8; KEY_LEN]) -> Self {
        Self {
            cipher: XChaCha20Poly1305::new(key.into()),
        }
    }

    /// `plaintext` sealed, bound to `purpose` (what it is: a domain's key, a
    /// token), so one secret cannot stand in for another.
    pub fn seal(&self, purpose: &str, plaintext: &[u8]) -> Vec<u8> {
        let nonce_bytes = crate::random::bytes::<NONCE_LEN>();
        let nonce = XNonce::from(nonce_bytes);
        let sealed = self
            .cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext,
                    aad: purpose.as_bytes(),
                },
            )
            .expect("sealing a secret this size cannot fail");
        let mut out = nonce_bytes.to_vec();
        out.extend_from_slice(&sealed);
        out
    }

    pub fn open(&self, purpose: &str, sealed: &[u8]) -> Result<Vec<u8>, SecretsError> {
        if sealed.len() < NONCE_LEN {
            return Err(SecretsError::Open);
        }
        let (nonce, ciphertext) = sealed.split_at(NONCE_LEN);
        let nonce = XNonce::try_from(nonce).map_err(|_| SecretsError::Open)?;
        self.cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: ciphertext,
                    aad: purpose.as_bytes(),
                },
            )
            .map_err(|_| SecretsError::Open)
    }

    pub fn open_string(&self, purpose: &str, sealed: &[u8]) -> Result<String, SecretsError> {
        String::from_utf8(self.open(purpose, sealed)?).map_err(|_| SecretsError::Open)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_opens_only_with_its_key_and_purpose() {
        let secrets = Secrets::from_key(&[1; 32]);

        let sealed = secrets.seal("domain:1", b"private key");

        assert_eq!(secrets.open("domain:1", &sealed).unwrap(), b"private key");
        assert!(secrets.open("domain:2", &sealed).is_err());
        assert!(
            Secrets::from_key(&[2; 32])
                .open("domain:1", &sealed)
                .is_err()
        );
        assert!(!sealed.windows(11).any(|w| w == b"private key"));
    }

    #[test]
    fn the_key_is_made_once_and_kept_private() {
        let dir = tempfile::tempdir().unwrap();
        let first = Secrets::load_or_generate(dir.path()).unwrap();
        let sealed = first.seal("x", b"y");

        let again = Secrets::load_or_generate(dir.path()).unwrap();

        assert_eq!(again.open("x", &sealed).unwrap(), b"y");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(dir.path().join(SECRETS_FILE))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }
}
