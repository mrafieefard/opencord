//! The signed challenge a client answers in `Identify`.
//!
//! The signed payload is `"opencord-auth-v1" || server_id (16 bytes) ||
//! nonce (32 bytes) || timestamp_ms (u64 big-endian)`.

use ed25519_dalek::{Signature, Signer};
pub use ed25519_dalek::{SigningKey, VerifyingKey};

/// Domain separator, so these signatures can never be replayed elsewhere.
pub const AUTH_CONTEXT: &[u8; 16] = b"opencord-auth-v1";
pub const SERVER_ID_LEN: usize = 16;
pub const NONCE_LEN: usize = 32;
pub const PUBLIC_KEY_LEN: usize = 32;
pub const SIGNATURE_LEN: usize = 64;
/// How far the signed timestamp may be from the server's clock.
pub const MAX_CLOCK_SKEW_MS: u64 = 60_000;

const PAYLOAD_LEN: usize = AUTH_CONTEXT.len() + SERVER_ID_LEN + NONCE_LEN + 8;

pub type ServerId = [u8; SERVER_ID_LEN];
pub type Nonce = [u8; NONCE_LEN];

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AuthError {
    #[error("the public key is not a valid Ed25519 key")]
    InvalidPublicKey,
    #[error("the signature is malformed or does not match")]
    InvalidSignature,
    #[error("the timestamp is more than 60 seconds away from the server clock")]
    StaleTimestamp,
}

pub fn auth_payload(server_id: &ServerId, nonce: &Nonce, timestamp_ms: u64) -> [u8; PAYLOAD_LEN] {
    let mut payload = [0; PAYLOAD_LEN];
    let (context_part, rest) = payload.split_at_mut(AUTH_CONTEXT.len());
    let (server_part, rest) = rest.split_at_mut(SERVER_ID_LEN);
    let (nonce_part, timestamp_part) = rest.split_at_mut(NONCE_LEN);
    context_part.copy_from_slice(AUTH_CONTEXT);
    server_part.copy_from_slice(server_id);
    nonce_part.copy_from_slice(nonce);
    timestamp_part.copy_from_slice(&timestamp_ms.to_be_bytes());
    payload
}

pub fn sign(
    key: &SigningKey,
    server_id: &ServerId,
    nonce: &Nonce,
    timestamp_ms: u64,
) -> [u8; SIGNATURE_LEN] {
    key.sign(&auth_payload(server_id, nonce, timestamp_ms))
        .to_bytes()
}

/// Checks an `Identify` signature and returns the verified key.
pub fn verify(
    public_key: &[u8],
    signature: &[u8],
    server_id: &ServerId,
    nonce: &Nonce,
    timestamp_ms: u64,
    now_ms: u64,
) -> Result<VerifyingKey, AuthError> {
    let key_bytes: &[u8; PUBLIC_KEY_LEN] = public_key
        .try_into()
        .map_err(|_| AuthError::InvalidPublicKey)?;
    let key = VerifyingKey::from_bytes(key_bytes).map_err(|_| AuthError::InvalidPublicKey)?;
    let signature = Signature::from_slice(signature).map_err(|_| AuthError::InvalidSignature)?;
    if now_ms.abs_diff(timestamp_ms) > MAX_CLOCK_SKEW_MS {
        return Err(AuthError::StaleTimestamp);
    }
    key.verify_strict(&auth_payload(server_id, nonce, timestamp_ms), &signature)
        .map_err(|_| AuthError::InvalidSignature)?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SERVER_ID: ServerId = [1; SERVER_ID_LEN];
    const NONCE: Nonce = [2; NONCE_LEN];
    const NOW: u64 = 1_800_000_000_000;

    fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn public(key: &SigningKey) -> [u8; PUBLIC_KEY_LEN] {
        key.verifying_key().to_bytes()
    }

    #[test]
    fn payload_is_context_server_nonce_and_big_endian_timestamp() {
        let payload = auth_payload(&SERVER_ID, &NONCE, 0x0102_0304_0506_0708);

        assert_eq!(&payload[..16], b"opencord-auth-v1");
        assert_eq!(&payload[16..32], &SERVER_ID);
        assert_eq!(&payload[32..64], &NONCE);
        assert_eq!(&payload[64..], &[1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn accepts_a_valid_signature() {
        let signer = key(7);
        let signature = sign(&signer, &SERVER_ID, &NONCE, NOW);

        let verified = verify(&public(&signer), &signature, &SERVER_ID, &NONCE, NOW, NOW);

        assert_eq!(verified, Ok(signer.verifying_key()));
    }

    #[test]
    fn accepts_timestamps_within_the_allowed_skew() {
        let signer = key(7);
        for timestamp in [NOW - MAX_CLOCK_SKEW_MS, NOW + MAX_CLOCK_SKEW_MS] {
            let signature = sign(&signer, &SERVER_ID, &NONCE, timestamp);

            assert!(
                verify(
                    &public(&signer),
                    &signature,
                    &SERVER_ID,
                    &NONCE,
                    timestamp,
                    NOW
                )
                .is_ok()
            );
        }
    }

    #[test]
    fn rejects_a_signature_from_another_key() {
        let signature = sign(&key(7), &SERVER_ID, &NONCE, NOW);

        let result = verify(&public(&key(8)), &signature, &SERVER_ID, &NONCE, NOW, NOW);

        assert_eq!(result, Err(AuthError::InvalidSignature));
    }

    #[test]
    fn rejects_a_signature_for_another_nonce_or_server() {
        let signer = key(7);
        let signature = sign(&signer, &SERVER_ID, &NONCE, NOW);

        let other_nonce = verify(
            &public(&signer),
            &signature,
            &SERVER_ID,
            &[9; NONCE_LEN],
            NOW,
            NOW,
        );
        let other_server = verify(
            &public(&signer),
            &signature,
            &[9; SERVER_ID_LEN],
            &NONCE,
            NOW,
            NOW,
        );

        assert_eq!(other_nonce, Err(AuthError::InvalidSignature));
        assert_eq!(other_server, Err(AuthError::InvalidSignature));
    }

    #[test]
    fn rejects_expired_and_future_timestamps() {
        let signer = key(7);
        for timestamp in [NOW - MAX_CLOCK_SKEW_MS - 1, NOW + MAX_CLOCK_SKEW_MS + 1] {
            let signature = sign(&signer, &SERVER_ID, &NONCE, timestamp);

            let result = verify(
                &public(&signer),
                &signature,
                &SERVER_ID,
                &NONCE,
                timestamp,
                NOW,
            );

            assert_eq!(result, Err(AuthError::StaleTimestamp));
        }
    }

    #[test]
    fn rejects_malformed_keys_and_signatures() {
        let signer = key(7);
        let signature = sign(&signer, &SERVER_ID, &NONCE, NOW);

        let short_key = verify(&[0; 31], &signature, &SERVER_ID, &NONCE, NOW, NOW);
        let short_signature = verify(
            &public(&signer),
            &signature[..63],
            &SERVER_ID,
            &NONCE,
            NOW,
            NOW,
        );

        assert_eq!(short_key, Err(AuthError::InvalidPublicKey));
        assert_eq!(short_signature, Err(AuthError::InvalidSignature));
    }
}
