//! `release.json.sig`: CI signs `release.json` with an Ed25519 key kept in
//! the repository's secrets, and the launcher checks it with the public
//! key it was built with. `release.json` holds every file's SHA-256, so
//! the signature covers the downloads too. A launcher built without a key
//! trusts HTTPS to github.com alone.

use data_encoding::BASE64;
use ed25519_dalek::{Signature, VerifyingKey};

/// The public key this launcher was built with, if any.
pub fn built_in_key() -> Option<VerifyingKey> {
    let text = option_env!("OPENCORD_UPDATE_PUBLIC_KEY").filter(|key| !key.trim().is_empty())?;
    // A key that doesn't read is a broken build, not a reason to accept
    // unsigned releases.
    Some(parse_key(text).expect("OPENCORD_UPDATE_PUBLIC_KEY is not a base64 Ed25519 key"))
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
}
