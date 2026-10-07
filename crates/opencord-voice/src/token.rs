//! Voice tokens: the main server vouches for a user joining a channel, and
//! the voice node checks the token without a database. Tokens are single
//! use and expire after [`VOICE_TOKEN_LIFETIME`].

use std::collections::HashMap;

pub use ed25519_dalek::{SigningKey, VerifyingKey};
pub use opencord_common::limits::VOICE_TOKEN_LIFETIME;
use opencord_common::signed;
use opencord_proto::internal::v1::VoiceTokenClaims;
use prost::Message as _;

pub const VOICE_TOKEN_DOMAIN: &[u8] = b"opencord-voice-token-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TokenError {
    #[error("the voice token is not valid")]
    Invalid,
    #[error("the voice token has expired")]
    Expired,
    #[error("the voice token is for someone or somewhere else")]
    Mismatch,
    #[error("the voice token has already been used")]
    Used,
}

/// Who presents a token, as they say in `Identify`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Presenter<'a> {
    pub user_id: i64,
    pub session_id: &'a str,
    pub channel_id: i64,
}

/// Signs `claims` into a token.
pub fn issue(key: &SigningKey, claims: &VoiceTokenClaims) -> Vec<u8> {
    signed::seal(key, VOICE_TOKEN_DOMAIN, &claims.encode_to_vec())
}

/// The claims of a token signed by `key`, if it is unexpired and was issued
/// to `presenter`. Does not consume it; see [`UsedTokens`].
pub fn verify(
    key: &VerifyingKey,
    token: &[u8],
    presenter: Presenter<'_>,
    now_ms: i64,
) -> Result<VoiceTokenClaims, TokenError> {
    let payload = signed::open(key, VOICE_TOKEN_DOMAIN, token).map_err(|_| TokenError::Invalid)?;
    let claims = VoiceTokenClaims::decode(payload).map_err(|_| TokenError::Invalid)?;
    if now_ms > claims.expires_at_ms {
        return Err(TokenError::Expired);
    }
    let issued_to = Presenter {
        user_id: claims.user_id,
        session_id: &claims.session_id,
        channel_id: claims.channel_id,
    };
    if issued_to != presenter {
        return Err(TokenError::Mismatch);
    }
    Ok(claims)
}

/// Token ids already accepted, kept until their tokens expire.
#[derive(Debug, Default)]
pub struct UsedTokens {
    expiry_by_id: HashMap<Vec<u8>, i64>,
}

impl UsedTokens {
    /// Marks the token as used. Fails if it was used before.
    pub fn redeem(&mut self, claims: &VoiceTokenClaims, now_ms: i64) -> Result<(), TokenError> {
        self.expiry_by_id
            .retain(|_, expires_at_ms| *expires_at_ms >= now_ms);
        if self.expiry_by_id.contains_key(&claims.token_id) {
            return Err(TokenError::Used);
        }
        self.expiry_by_id
            .insert(claims.token_id.clone(), claims.expires_at_ms);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000_000;
    const USER: i64 = 7;
    const CHANNEL: i64 = 42;
    const SESSION: &str = "session-1";

    fn key() -> SigningKey {
        SigningKey::from_bytes(&[3; 32])
    }

    fn claims(token_id: u8, expires_at_ms: i64) -> VoiceTokenClaims {
        VoiceTokenClaims {
            token_id: vec![token_id; 16],
            user_id: USER,
            channel_id: CHANNEL,
            session_id: SESSION.to_owned(),
            permissions: 1 << 16,
            expires_at_ms,
            ..Default::default()
        }
    }

    fn presenter() -> Presenter<'static> {
        Presenter {
            user_id: USER,
            session_id: SESSION,
            channel_id: CHANNEL,
        }
    }

    #[test]
    fn a_fresh_token_for_the_presenter_is_valid() {
        let issued = claims(1, NOW + 60_000);
        let token = issue(&key(), &issued);

        assert_eq!(
            verify(&key().verifying_key(), &token, presenter(), NOW),
            Ok(issued)
        );
    }

    #[test]
    fn expired_tokens_are_refused() {
        let token = issue(&key(), &claims(1, NOW));

        assert_eq!(
            verify(&key().verifying_key(), &token, presenter(), NOW + 1),
            Err(TokenError::Expired)
        );
    }

    #[test]
    fn tokens_for_another_channel_session_or_user_are_refused() {
        let token = issue(&key(), &claims(1, NOW + 60_000));
        let verifying = key().verifying_key();
        let elsewhere = [
            Presenter {
                channel_id: CHANNEL + 1,
                ..presenter()
            },
            Presenter {
                session_id: "session-2",
                ..presenter()
            },
            Presenter {
                user_id: USER + 1,
                ..presenter()
            },
        ];

        for presenter in elsewhere {
            assert_eq!(
                verify(&verifying, &token, presenter, NOW),
                Err(TokenError::Mismatch)
            );
        }
    }

    #[test]
    fn tokens_signed_by_another_key_or_garbled_are_refused() {
        let token = issue(&SigningKey::from_bytes(&[4; 32]), &claims(1, NOW + 60_000));

        assert_eq!(
            verify(&key().verifying_key(), &token, presenter(), NOW),
            Err(TokenError::Invalid)
        );
        assert_eq!(
            verify(&key().verifying_key(), b"short", presenter(), NOW),
            Err(TokenError::Invalid)
        );
    }

    #[test]
    fn each_token_is_accepted_once() {
        let mut used = UsedTokens::default();
        let first = claims(1, NOW + 60_000);

        assert_eq!(used.redeem(&first, NOW), Ok(()));
        assert_eq!(used.redeem(&first, NOW + 1), Err(TokenError::Used));
        assert_eq!(used.redeem(&claims(2, NOW + 60_000), NOW), Ok(()));
    }

    #[test]
    fn used_ids_are_forgotten_once_their_tokens_expire() {
        let mut used = UsedTokens::default();
        used.redeem(&claims(1, NOW + 10), NOW).unwrap();

        used.redeem(&claims(2, NOW + 60_000), NOW + 11).unwrap();

        assert_eq!(used.expiry_by_id.len(), 1);
    }
}
