//! Keys for the private database, and sealing its columns (design
//! 2026-10-22 §11). The master key from the keychain derives two keys: one
//! encrypts vodozemac pickles, the other seals text, names and outboxes,
//! each bound to its column and row.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hmac::{Hmac, Mac};
use sha2::Sha256;

use super::PrivateError;

const NONCE_LEN: usize = 24;
const PICKLE_LABEL: &[u8] = b"opencord-private-pickle-v1";
const SEAL_LABEL: &[u8] = b"opencord-private-seal-v1";
const AAD_PREFIX: &[u8] = b"opencord-private:";

/// The two keys derived from the master key.
pub struct DerivedKeys {
    pub pickle: [u8; 32],
    pub seal: [u8; 32],
}

/// HMAC-SHA256 of each key's label, keyed with `master`.
pub fn derive_keys(master: &[u8; 32]) -> DerivedKeys {
    DerivedKeys {
        pickle: hmac_sha256(master, PICKLE_LABEL),
        seal: hmac_sha256(master, SEAL_LABEL),
    }
}

fn hmac_sha256(key: &[u8; 32], message: &[u8]) -> [u8; 32] {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC takes keys of any length");
    mac.update(message);
    mac.finalize().into_bytes().into()
}

/// Seals and opens values with the seal key.
pub struct Sealer {
    cipher: XChaCha20Poly1305,
}

impl std::fmt::Debug for Sealer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Sealer")
    }
}

impl Sealer {
    pub fn new(key: &[u8; 32]) -> Self {
        Self {
            cipher: XChaCha20Poly1305::new(key.into()),
        }
    }

    /// `nonce ‖ ciphertext+tag`, bound to the column (`label`) and the row,
    /// so a value can't be moved to another column or row and still open.
    pub fn seal(&self, label: &str, row: &[u8], plaintext: &[u8]) -> Vec<u8> {
        let mut nonce_bytes = [0u8; NONCE_LEN];
        getrandom::fill(&mut nonce_bytes)
            .expect("the operating system's random source is unavailable");
        let nonce = XNonce::from(nonce_bytes);
        let aad = associated_data(label, row);
        let sealed = self
            .cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext,
                    aad: &aad,
                },
            )
            .expect("sealing a value this size cannot fail");
        let mut out = Vec::with_capacity(NONCE_LEN + sealed.len());
        out.extend_from_slice(&nonce_bytes);
        out.extend_from_slice(&sealed);
        out
    }

    pub fn open(&self, label: &str, row: &[u8], sealed: &[u8]) -> Result<Vec<u8>, PrivateError> {
        let unreadable = || PrivateError::Crypto(format!("a sealed {label} value doesn't open"));
        if sealed.len() < NONCE_LEN {
            return Err(unreadable());
        }
        let (nonce, ciphertext) = sealed.split_at(NONCE_LEN);
        let nonce = XNonce::try_from(nonce).map_err(|_| unreadable())?;
        let aad = associated_data(label, row);
        self.cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: ciphertext,
                    aad: &aad,
                },
            )
            .map_err(|_| unreadable())
    }
}

fn associated_data(label: &str, row: &[u8]) -> Vec<u8> {
    let mut aad = Vec::with_capacity(AAD_PREFIX.len() + label.len() + 1 + row.len());
    aad.extend_from_slice(AAD_PREFIX);
    aad.extend_from_slice(label.as_bytes());
    aad.push(b':');
    aad.extend_from_slice(row);
    aad
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sealed_values_open_only_with_their_key_label_and_row() {
        let sealer = Sealer::new(&[1; 32]);
        let sealed = sealer.seal("messages.body", b"row-1", b"hello");

        assert_eq!(
            sealer.open("messages.body", b"row-1", &sealed).unwrap(),
            b"hello"
        );
        assert!(
            Sealer::new(&[2; 32])
                .open("messages.body", b"row-1", &sealed)
                .is_err()
        );
        assert!(sealer.open("contacts.name", b"row-1", &sealed).is_err());
        assert!(sealer.open("messages.body", b"row-2", &sealed).is_err());
        assert!(sealer.open("messages.body", b"", &sealed).is_err());
        for index in [0, NONCE_LEN, sealed.len() - 1] {
            let mut flipped = sealed.clone();
            flipped[index] ^= 1;
            assert!(
                sealer.open("messages.body", b"row-1", &flipped).is_err(),
                "byte {index}"
            );
        }
        assert!(
            sealer
                .open("messages.body", b"row-1", &sealed[..sealed.len() - 1])
                .is_err()
        );
        assert!(
            sealer
                .open("messages.body", b"row-1", &sealed[..NONCE_LEN - 1])
                .is_err()
        );
        assert!(sealer.open("messages.body", b"row-1", &[]).is_err());
    }

    #[test]
    fn every_seal_uses_a_new_nonce() {
        let sealer = Sealer::new(&[1; 32]);

        let first = sealer.seal("messages.body", b"row", b"same text");
        let second = sealer.seal("messages.body", b"row", b"same text");

        assert_ne!(first[..NONCE_LEN], second[..NONCE_LEN]);
        assert_ne!(first, second);
        assert_eq!(
            sealer.open("messages.body", b"row", &first).unwrap(),
            b"same text"
        );
        assert_eq!(
            sealer.open("messages.body", b"row", &second).unwrap(),
            b"same text"
        );
    }

    #[test]
    fn derived_keys_differ_and_repeat() {
        let first = derive_keys(&[7; 32]);
        let again = derive_keys(&[7; 32]);
        let other = derive_keys(&[8; 32]);

        assert_eq!(first.pickle, again.pickle);
        assert_eq!(first.seal, again.seal);
        assert_ne!(first.pickle, first.seal);
        assert_ne!(first.pickle, [7; 32]);
        assert_ne!(first.pickle, other.pickle);
        assert_ne!(first.seal, other.seal);
    }
}
