//! The signed challenge a client answers in `Identify`.
//!
//! The signed payload is `"opencord-auth-v1" || server_id (16 bytes) ||
//! certificate fingerprint (32 bytes) || nonce (32 bytes) || timestamp_ms
//! (u64 big-endian)`. The certificate fingerprint is the SHA-256 of the TLS
//! certificate the client verified. It binds the signature to that TLS
//! endpoint, so a server cannot pass another server's challenge to its
//! users and replay their answers there.

use ed25519_dalek::{Signature, Signer};
pub use ed25519_dalek::{SigningKey, VerifyingKey};

use crate::address::Fingerprint;

/// Domain separator, so these signatures can never be replayed elsewhere.
pub const AUTH_CONTEXT: &[u8; 16] = b"opencord-auth-v1";
pub const SERVER_ID_LEN: usize = 16;
pub const NONCE_LEN: usize = 32;
pub const PUBLIC_KEY_LEN: usize = 32;
pub const SIGNATURE_LEN: usize = 64;
/// How far the signed timestamp may be from the server's clock.
pub const MAX_CLOCK_SKEW_MS: u64 = 60_000;

const PAYLOAD_LEN: usize = AUTH_CONTEXT.len() + SERVER_ID_LEN + 32 + NONCE_LEN + 8;

pub type ServerId = [u8; SERVER_ID_LEN];
pub type Nonce = [u8; NONCE_LEN];

/// Everything an `Identify` signature covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Challenge {
    pub server_id: ServerId,
    /// SHA-256 of the server's TLS certificate, as the client saw it.
    pub certificate: Fingerprint,
    pub nonce: Nonce,
    pub timestamp_ms: u64,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AuthError {
    #[error("the public key is not a valid Ed25519 key")]
    InvalidPublicKey,
    #[error("the signature is malformed or does not match")]
    InvalidSignature,
    #[error("the timestamp is more than 60 seconds away from the server clock")]
    StaleTimestamp,
}

pub fn auth_payload(challenge: &Challenge) -> [u8; PAYLOAD_LEN] {
    let mut payload = [0; PAYLOAD_LEN];
    let (context_part, rest) = payload.split_at_mut(AUTH_CONTEXT.len());
    let (server_part, rest) = rest.split_at_mut(SERVER_ID_LEN);
    let (certificate_part, rest) = rest.split_at_mut(challenge.certificate.len());
    let (nonce_part, timestamp_part) = rest.split_at_mut(NONCE_LEN);
    context_part.copy_from_slice(AUTH_CONTEXT);
    server_part.copy_from_slice(&challenge.server_id);
    certificate_part.copy_from_slice(&challenge.certificate);
    nonce_part.copy_from_slice(&challenge.nonce);
    timestamp_part.copy_from_slice(&challenge.timestamp_ms.to_be_bytes());
    payload
}

pub fn sign(key: &SigningKey, challenge: &Challenge) -> [u8; SIGNATURE_LEN] {
    key.sign(&auth_payload(challenge)).to_bytes()
}

/// Checks an `Identify` signature and returns the verified key.
pub fn verify(
    public_key: &[u8],
    signature: &[u8],
    challenge: &Challenge,
    now_ms: u64,
) -> Result<VerifyingKey, AuthError> {
    let key_bytes: &[u8; PUBLIC_KEY_LEN] = public_key
        .try_into()
        .map_err(|_| AuthError::InvalidPublicKey)?;
    let key = VerifyingKey::from_bytes(key_bytes).map_err(|_| AuthError::InvalidPublicKey)?;
    let signature = Signature::from_slice(signature).map_err(|_| AuthError::InvalidSignature)?;
    if now_ms.abs_diff(challenge.timestamp_ms) > MAX_CLOCK_SKEW_MS {
        return Err(AuthError::StaleTimestamp);
    }
    key.verify_strict(&auth_payload(challenge), &signature)
        .map_err(|_| AuthError::InvalidSignature)?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_800_000_000_000;

    fn challenge(timestamp_ms: u64) -> Challenge {
        Challenge {
            server_id: [1; SERVER_ID_LEN],
            certificate: [3; 32],
            nonce: [2; NONCE_LEN],
            timestamp_ms,
        }
    }

    fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn public(key: &SigningKey) -> [u8; PUBLIC_KEY_LEN] {
        key.verifying_key().to_bytes()
    }

    #[test]
    fn payload_is_context_server_certificate_nonce_and_big_endian_timestamp() {
        let payload = auth_payload(&challenge(0x0102_0304_0506_0708));

        assert_eq!(payload.len(), 104);
        assert_eq!(&payload[..16], b"opencord-auth-v1");
        assert_eq!(&payload[16..32], &[1; 16]);
        assert_eq!(&payload[32..64], &[3; 32]);
        assert_eq!(&payload[64..96], &[2; 32]);
        assert_eq!(&payload[96..], &[1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn accepts_a_valid_signature() {
        let signer = key(7);
        let signature = sign(&signer, &challenge(NOW));

        let verified = verify(&public(&signer), &signature, &challenge(NOW), NOW);

        assert_eq!(verified, Ok(signer.verifying_key()));
    }

    #[test]
    fn accepts_timestamps_within_the_allowed_skew() {
        let signer = key(7);
        for timestamp in [NOW - MAX_CLOCK_SKEW_MS, NOW + MAX_CLOCK_SKEW_MS] {
            let signature = sign(&signer, &challenge(timestamp));

            assert!(verify(&public(&signer), &signature, &challenge(timestamp), NOW).is_ok());
        }
    }

    #[test]
    fn rejects_a_signature_from_another_key() {
        let signature = sign(&key(7), &challenge(NOW));

        let result = verify(&public(&key(8)), &signature, &challenge(NOW), NOW);

        assert_eq!(result, Err(AuthError::InvalidSignature));
    }

    #[test]
    fn rejects_a_signature_for_another_nonce_or_server() {
        let signer = key(7);
        let signature = sign(&signer, &challenge(NOW));
        let other_nonce = Challenge {
            nonce: [9; NONCE_LEN],
            ..challenge(NOW)
        };
        let other_server = Challenge {
            server_id: [9; SERVER_ID_LEN],
            ..challenge(NOW)
        };

        assert_eq!(
            verify(&public(&signer), &signature, &other_nonce, NOW),
            Err(AuthError::InvalidSignature)
        );
        assert_eq!(
            verify(&public(&signer), &signature, &other_server, NOW),
            Err(AuthError::InvalidSignature)
        );
    }

    #[test]
    fn rejects_a_signature_made_for_another_certificate() {
        let signer = key(7);
        let relayed = Challenge {
            certificate: [4; 32],
            ..challenge(NOW)
        };
        let signature = sign(&signer, &relayed);

        let result = verify(&public(&signer), &signature, &challenge(NOW), NOW);

        assert_eq!(result, Err(AuthError::InvalidSignature));
    }

    #[test]
    fn rejects_expired_and_future_timestamps() {
        let signer = key(7);
        for timestamp in [NOW - MAX_CLOCK_SKEW_MS - 1, NOW + MAX_CLOCK_SKEW_MS + 1] {
            let signature = sign(&signer, &challenge(timestamp));

            let result = verify(&public(&signer), &signature, &challenge(timestamp), NOW);

            assert_eq!(result, Err(AuthError::StaleTimestamp));
        }
    }

    #[test]
    fn rejects_malformed_keys_and_signatures() {
        let signer = key(7);
        let signature = sign(&signer, &challenge(NOW));

        let short_key = verify(&[0; 31], &signature, &challenge(NOW), NOW);
        let short_signature = verify(&public(&signer), &signature[..63], &challenge(NOW), NOW);

        assert_eq!(short_key, Err(AuthError::InvalidPublicKey));
        assert_eq!(short_signature, Err(AuthError::InvalidSignature));
    }
}
