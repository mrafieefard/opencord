//! Media tokens: bearer tokens for the media HTTP endpoints (Phase 2 plan
//! §4.4), valid for [`MEDIA_TOKEN_LIFETIME`].

use data_encoding::BASE64URL_NOPAD;
use ed25519_dalek::{SigningKey, VerifyingKey};
use opencord_common::auth::ServerId;
pub use opencord_common::limits::MEDIA_TOKEN_LIFETIME;
use opencord_common::signed;
use opencord_proto::internal::v1::MediaTokenClaims;
use opencord_proto::v1 as proto;
use prost::Message as _;

pub const MEDIA_TOKEN_DOMAIN: &[u8] = b"opencord-media-token-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum MediaTokenError {
    #[error("the media token is not valid")]
    Invalid,
    #[error("the media token has expired")]
    Expired,
}

pub fn issue(
    key: &SigningKey,
    user_id: i64,
    server_id: &ServerId,
    now_ms: i64,
) -> proto::MediaToken {
    let lifetime = i64::try_from(MEDIA_TOKEN_LIFETIME.as_millis()).unwrap_or(i64::MAX);
    let claims = MediaTokenClaims {
        user_id,
        server_id: server_id.to_vec(),
        expires_at_ms: now_ms.saturating_add(lifetime),
    };
    let sealed = signed::seal(key, MEDIA_TOKEN_DOMAIN, &claims.encode_to_vec());
    proto::MediaToken {
        token: BASE64URL_NOPAD.encode(&sealed),
        expires_at_ms: claims.expires_at_ms,
    }
}

/// The user a token was issued to.
pub fn verify(
    key: &VerifyingKey,
    token: &str,
    server_id: &ServerId,
    now_ms: i64,
) -> Result<i64, MediaTokenError> {
    let sealed = BASE64URL_NOPAD
        .decode(token.as_bytes())
        .map_err(|_| MediaTokenError::Invalid)?;
    let payload =
        signed::open(key, MEDIA_TOKEN_DOMAIN, &sealed).map_err(|_| MediaTokenError::Invalid)?;
    let claims = MediaTokenClaims::decode(payload).map_err(|_| MediaTokenError::Invalid)?;
    if claims.server_id != server_id {
        return Err(MediaTokenError::Invalid);
    }
    if now_ms > claims.expires_at_ms {
        return Err(MediaTokenError::Expired);
    }
    Ok(claims.user_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000_000;
    const SERVER: ServerId = [9; 16];

    fn key() -> SigningKey {
        SigningKey::from_bytes(&[5; 32])
    }

    #[test]
    fn a_token_names_its_user_until_it_expires() {
        let token = issue(&key(), 42, &SERVER, NOW);
        let lifetime = i64::try_from(MEDIA_TOKEN_LIFETIME.as_millis()).unwrap();

        assert_eq!(token.expires_at_ms, NOW + lifetime);
        assert_eq!(
            verify(
                &key().verifying_key(),
                &token.token,
                &SERVER,
                NOW + lifetime
            ),
            Ok(42)
        );
        assert_eq!(
            verify(
                &key().verifying_key(),
                &token.token,
                &SERVER,
                NOW + lifetime + 1
            ),
            Err(MediaTokenError::Expired)
        );
    }

    #[test]
    fn tokens_from_another_server_or_key_are_refused() {
        let token = issue(&key(), 42, &SERVER, NOW).token;

        assert_eq!(
            verify(&key().verifying_key(), &token, &[8; 16], NOW),
            Err(MediaTokenError::Invalid)
        );
        assert_eq!(
            verify(
                &SigningKey::from_bytes(&[6; 32]).verifying_key(),
                &token,
                &SERVER,
                NOW
            ),
            Err(MediaTokenError::Invalid)
        );
    }

    #[test]
    fn garbage_is_refused() {
        for token in ["", "not base64!", "AAAA"] {
            assert_eq!(
                verify(&key().verifying_key(), token, &SERVER, NOW),
                Err(MediaTokenError::Invalid)
            );
        }
    }
}
