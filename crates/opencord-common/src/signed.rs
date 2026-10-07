//! Payloads signed with Ed25519 under a domain label: tokens one server
//! issues and itself or a voice node checks later. A sealed token is the
//! payload followed by the 64-byte signature over `domain || payload`.

use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};

use crate::auth::SIGNATURE_LEN;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SignedError {
    #[error("the token is too short")]
    Truncated,
    #[error("the token's signature does not match")]
    BadSignature,
}

/// `payload || signature`.
pub fn seal(key: &SigningKey, domain: &[u8], payload: &[u8]) -> Vec<u8> {
    let signature = key.sign(&signed_message(domain, payload));
    let mut token = Vec::with_capacity(payload.len() + SIGNATURE_LEN);
    token.extend_from_slice(payload);
    token.extend_from_slice(&signature.to_bytes());
    token
}

/// The payload of a token sealed by `key`'s owner under `domain`.
pub fn open<'a>(
    key: &VerifyingKey,
    domain: &[u8],
    token: &'a [u8],
) -> Result<&'a [u8], SignedError> {
    let split = token
        .len()
        .checked_sub(SIGNATURE_LEN)
        .ok_or(SignedError::Truncated)?;
    let (payload, signature) = token.split_at(split);
    let signature = Signature::from_slice(signature).map_err(|_| SignedError::BadSignature)?;
    key.verify_strict(&signed_message(domain, payload), &signature)
        .map_err(|_| SignedError::BadSignature)?;
    Ok(payload)
}

fn signed_message(domain: &[u8], payload: &[u8]) -> Vec<u8> {
    [domain, payload].concat()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOMAIN: &[u8] = b"opencord-test-token-v1";

    fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    #[test]
    fn sealed_payloads_open_with_the_matching_key() {
        let token = seal(&key(1), DOMAIN, b"claims");

        assert_eq!(
            open(&key(1).verifying_key(), DOMAIN, &token),
            Ok(&b"claims"[..])
        );
    }

    #[test]
    fn another_key_or_domain_is_refused() {
        let token = seal(&key(1), DOMAIN, b"claims");

        assert_eq!(
            open(&key(2).verifying_key(), DOMAIN, &token),
            Err(SignedError::BadSignature)
        );
        assert_eq!(
            open(&key(1).verifying_key(), b"opencord-other-v1", &token),
            Err(SignedError::BadSignature)
        );
    }

    #[test]
    fn changed_payloads_are_refused() {
        let mut token = seal(&key(1), DOMAIN, b"claims");
        token[0] ^= 1;

        assert_eq!(
            open(&key(1).verifying_key(), DOMAIN, &token),
            Err(SignedError::BadSignature)
        );
    }

    #[test]
    fn short_tokens_are_refused() {
        assert_eq!(
            open(&key(1).verifying_key(), DOMAIN, &[0; 63]),
            Err(SignedError::Truncated)
        );
    }
}
