//! `release.json.sig`: CI signs `release.json` with an Ed25519 key kept in
//! the repository's secrets, and the launcher checks it with the public
//! key it was built with. `release.json` holds every file's SHA-256, so
//! the signature covers the downloads too. A launcher built without a key
//! trusts HTTPS to github.com alone.

use std::fmt;

use data_encoding::BASE64;
use ed25519_dalek::{Signature, VerifyingKey};

/// The launcher was built with a key it can't read. `build.rs` refuses
/// keys of the wrong shape, so this is 32 bytes that aren't a point on
/// the curve.
#[derive(Debug, PartialEq)]
pub struct KeyError;

impl fmt::Display for KeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the built-in update key can't be read")
    }
}

/// The public key this launcher was built with: `None` without one, and
/// `Err` when it can't be read. A key that doesn't read means no updates,
/// never unsigned ones.
pub fn built_in_key() -> Result<Option<VerifyingKey>, KeyError> {
    let key = key_from(option_env!("OPENCORD_UPDATE_PUBLIC_KEY"));
    if let Err(error) = &key {
        eprintln!("{error}");
    }
    key
}

/// The key `text` (`OPENCORD_UPDATE_PUBLIC_KEY` at build time) names.
fn key_from(text: Option<&str>) -> Result<Option<VerifyingKey>, KeyError> {
    match text.filter(|key| !key.trim().is_empty()) {
        None => Ok(None),
        Some(text) => parse_key(text).map(Some).ok_or(KeyError),
    }
}

/// A base64 Ed25519 public key: the 32 raw bytes, or the 44-byte DER
/// `openssl pkey -pubout -outform DER` writes, which ends in them.
pub fn parse_key(text: &str) -> Option<VerifyingKey> {
    let bytes = BASE64.decode(text.trim().as_bytes()).ok()?;
    let raw: [u8; 32] = bytes.get(bytes.len().checked_sub(32)?..)?.try_into().ok()?;
    if bytes.len() != 32 && bytes.len() != 44 {
        return None;
    }
    VerifyingKey::from_bytes(&raw).ok()
}

/// Whether `signature` (base64, as `release.json.sig` holds it) signs
/// `message` with `key`.
pub fn verify(key: &VerifyingKey, message: &[u8], signature: &[u8]) -> bool {
    let text = String::from_utf8_lossy(signature);
    let Ok(bytes) = BASE64.decode(text.trim().as_bytes()) else {
        return false;
    };
    let Ok(raw) = <[u8; 64]>::try_from(bytes.as_slice()) else {
        return false;
    };
    key.verify_strict(message, &Signature::from_bytes(&raw))
        .is_ok()
}

#[cfg(test)]
mod tests {
    use ed25519_dalek::{Signer, SigningKey};

    use super::*;

    fn signing_key() -> SigningKey {
        SigningKey::from_bytes(&[7; 32])
    }

    #[test]
    fn a_signature_by_the_key_holds() {
        let key = signing_key();
        let signature = BASE64.encode(&key.sign(b"release").to_bytes());
        assert!(verify(
            &key.verifying_key(),
            b"release",
            signature.as_bytes()
        ));
        // As `openssl base64` writes it, with a newline.
        let signature = format!("{signature}\n");
        assert!(verify(
            &key.verifying_key(),
            b"release",
            signature.as_bytes()
        ));
    }

    #[test]
    fn another_message_or_key_or_garbage_fails() {
        let key = signing_key();
        let signature = BASE64.encode(&key.sign(b"release").to_bytes());
        assert!(!verify(
            &key.verifying_key(),
            b"Release",
            signature.as_bytes()
        ));
        let other = SigningKey::from_bytes(&[8; 32]).verifying_key();
        assert!(!verify(&other, b"release", signature.as_bytes()));
        assert!(!verify(&key.verifying_key(), b"release", b"not base64!"));
        assert!(!verify(&key.verifying_key(), b"release", b""));
    }

    #[test]
    fn keys_read_raw_or_as_der() {
        let key = signing_key().verifying_key();
        let raw = BASE64.encode(key.as_bytes());
        assert_eq!(parse_key(&raw), Some(key));
        // SubjectPublicKeyInfo for Ed25519: a fixed 12-byte prefix.
        let mut der = vec![
            0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
        ];
        der.extend_from_slice(key.as_bytes());
        assert_eq!(parse_key(&BASE64.encode(&der)), Some(key));
        assert_eq!(parse_key("short"), None);
    }

    /// 32 bytes, so `build.rs` lets them through, that aren't a point on
    /// the curve.
    fn not_a_point() -> String {
        let mut bytes = [0; 32];
        bytes[0] = 2;
        assert!(
            VerifyingKey::from_bytes(&bytes).is_err(),
            "pick other bytes"
        );
        BASE64.encode(&bytes)
    }

    // Fails if `key_from` panics on a key it can't read (as
    // `built_in_key`'s old `expect` did), or if it answers `Ok(None)` and so
    // trusts unsigned releases.
    #[test]
    fn a_key_that_is_not_a_point_refuses_updates_without_panicking() {
        let key = key_from(Some(&not_a_point()));
        assert_eq!(key, Err(KeyError));
    }

    // Fails if a blank or missing key stops being "built without a key",
    // or a good key stops reading.
    #[test]
    fn no_key_is_none_and_a_good_key_reads() {
        assert_eq!(key_from(None), Ok(None));
        assert_eq!(key_from(Some("  ")), Ok(None));
        let key = signing_key().verifying_key();
        assert_eq!(
            key_from(Some(&BASE64.encode(key.as_bytes()))),
            Ok(Some(key))
        );
    }
}
