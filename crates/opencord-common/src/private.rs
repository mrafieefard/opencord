// SPDX-License-Identifier: MPL-2.0

//! Signing payloads, key verification, and limits for private messages.

use ed25519_dalek::{Signature, Signer};
pub use ed25519_dalek::{SigningKey, VerifyingKey};

pub const DEVICE_ID_LEN: usize = 16;
pub const KEY_LEN: usize = 32;
pub const SIGNATURE_LEN: usize = 64;
pub const SEND_ID_LEN: usize = 16;
pub const MESSAGE_ID_LEN: usize = 16;

pub const MAX_DEVICES_PER_USER: usize = 10;
pub const ONE_TIME_KEYS_TARGET: usize = 20;
pub const MAX_STORED_ONE_TIME_KEYS: usize = 100;

pub const MAX_CIPHERTEXTS: usize = 20;
pub const MAX_CIPHERTEXT_BYTES: usize = 32_768;

pub const MAX_FETCH_DEVICES_USERS: usize = 2;
pub const MAX_CLAIM_DEVICES: usize = 20;
pub const MAX_ACK_IDS: usize = 500;

pub const PENDING_PAGE_EVENTS: u32 = 100;
pub const PENDING_PAGE_BYTES: usize = 786_432;

pub const MAX_OUTGOING_FRIEND_REQUESTS: i64 = 100;
pub const MAX_BLOCKED_IDS: usize = 10_000;

pub const SPAM_AFTER_DECLINES: u32 = 3;

pub const RECENT_SEND_TTL_MS: i64 = 86_400_000;
pub const FALLBACK_ROTATION_MS: i64 = 2_592_000_000;
pub const DEVICE_LIST_TTL_MS: i64 = 600_000;
pub const UNSENT_RESEND_MS: i64 = 86_400_000;
pub const RECEIVED_RECORD_TTL_MS: i64 = 3_024_000_000;
pub const TYPING_INTERVAL_MS: i64 = 8_000;

const DEVICE_CONTEXT: &[u8; 18] = b"opencord-device-v1";
const PREKEY_CONTEXT: &[u8; 18] = b"opencord-prekey-v1";
const SLIP_CONTEXT: &[u8; 20] = b"opencord-dm-allow-v1";

pub fn device_payload(device_id: &[u8; 16], curve25519: &[u8; 32], ed25519: &[u8; 32]) -> [u8; 98] {
    let mut payload = [0u8; 98];
    payload[..18].copy_from_slice(DEVICE_CONTEXT);
    payload[18..34].copy_from_slice(device_id);
    payload[34..66].copy_from_slice(curve25519);
    payload[66..98].copy_from_slice(ed25519);
    payload
}

pub fn prekey_payload(device_id: &[u8; 16], key: &[u8; 32], fallback: bool) -> [u8; 67] {
    let mut payload = [0u8; 67];
    payload[..18].copy_from_slice(PREKEY_CONTEXT);
    payload[18..34].copy_from_slice(device_id);
    payload[34..66].copy_from_slice(key);
    payload[66] = if fallback { 1 } else { 0 };
    payload
}

pub fn slip_payload(granter: &[u8; 32], grantee: &[u8; 32]) -> [u8; 84] {
    let mut payload = [0u8; 84];
    payload[..20].copy_from_slice(SLIP_CONTEXT);
    payload[20..52].copy_from_slice(granter);
    payload[52..84].copy_from_slice(grantee);
    payload
}

pub fn sign_device(
    identity: &SigningKey,
    device_id: &[u8; 16],
    curve25519: &[u8; 32],
    ed25519: &[u8; 32],
) -> [u8; 64] {
    identity
        .sign(&device_payload(device_id, curve25519, ed25519))
        .to_bytes()
}

pub fn sign_prekey(
    device_ed25519: &SigningKey,
    device_id: &[u8; 16],
    key: &[u8; 32],
    fallback: bool,
) -> [u8; 64] {
    device_ed25519
        .sign(&prekey_payload(device_id, key, fallback))
        .to_bytes()
}

pub fn sign_slip(identity: &SigningKey, grantee: &[u8; 32]) -> [u8; 64] {
    let granter = identity.verifying_key().to_bytes();
    identity.sign(&slip_payload(&granter, grantee)).to_bytes()
}

pub fn verify_device(
    identity_public_key: &[u8],
    device_id: &[u8],
    curve25519: &[u8],
    ed25519: &[u8],
    signature: &[u8],
) -> bool {
    let Ok(key_bytes) = identity_public_key.try_into() else {
        return false;
    };
    let Ok(device_id_bytes) = device_id.try_into() else {
        return false;
    };
    let Ok(curve25519_bytes) = curve25519.try_into() else {
        return false;
    };
    let Ok(ed25519_bytes) = ed25519.try_into() else {
        return false;
    };
    let Ok(sig_bytes) = signature.try_into() else {
        return false;
    };
    let Ok(key) = VerifyingKey::from_bytes(key_bytes) else {
        return false;
    };
    let sig = Signature::from_bytes(sig_bytes);
    let payload = device_payload(device_id_bytes, curve25519_bytes, ed25519_bytes);
    key.verify_strict(&payload, &sig).is_ok()
}

pub fn verify_prekey(
    device_ed25519: &[u8],
    device_id: &[u8],
    key: &[u8],
    fallback: bool,
    signature: &[u8],
) -> bool {
    let Ok(key_bytes) = device_ed25519.try_into() else {
        return false;
    };
    let Ok(device_id_bytes) = device_id.try_into() else {
        return false;
    };
    let Ok(prekey_bytes) = key.try_into() else {
        return false;
    };
    let Ok(sig_bytes) = signature.try_into() else {
        return false;
    };
    let Ok(verifying_key) = VerifyingKey::from_bytes(key_bytes) else {
        return false;
    };
    let sig = Signature::from_bytes(sig_bytes);
    let payload = prekey_payload(device_id_bytes, prekey_bytes, fallback);
    verifying_key.verify_strict(&payload, &sig).is_ok()
}

pub fn verify_slip(granter: &[u8], grantee: &[u8], slip: &[u8]) -> bool {
    let Ok(granter_bytes) = granter.try_into() else {
        return false;
    };
    let Ok(grantee_bytes) = grantee.try_into() else {
        return false;
    };
    let Ok(sig_bytes) = slip.try_into() else {
        return false;
    };
    let Ok(key) = VerifyingKey::from_bytes(granter_bytes) else {
        return false;
    };
    let sig = Signature::from_bytes(sig_bytes);
    let payload = slip_payload(granter_bytes, grantee_bytes);
    key.verify_strict(&payload, &sig).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    #[test]
    fn payload_layouts_are_fixed() {
        let dev_id = [1u8; 16];
        let curve = [2u8; 32];
        let ed = [3u8; 32];
        let dev_p = device_payload(&dev_id, &curve, &ed);
        assert_eq!(dev_p.len(), 98);
        assert_eq!(&dev_p[..18], b"opencord-device-v1");
        assert_eq!(&dev_p[18..34], &dev_id);
        assert_eq!(&dev_p[34..66], &curve);
        assert_eq!(&dev_p[66..98], &ed);

        let pre_p_true = prekey_payload(&dev_id, &curve, true);
        assert_eq!(pre_p_true.len(), 67);
        assert_eq!(&pre_p_true[..18], b"opencord-prekey-v1");
        assert_eq!(&pre_p_true[18..34], &dev_id);
        assert_eq!(&pre_p_true[34..66], &curve);
        assert_eq!(pre_p_true[66], 1);

        let pre_p_false = prekey_payload(&dev_id, &curve, false);
        assert_eq!(pre_p_false.len(), 67);
        assert_eq!(pre_p_false[66], 0);

        let granter = [4u8; 32];
        let grantee = [5u8; 32];
        let slip_p = slip_payload(&granter, &grantee);
        assert_eq!(slip_p.len(), 84);
        assert_eq!(&slip_p[..20], b"opencord-dm-allow-v1");
        assert_eq!(&slip_p[20..52], &granter);
        assert_eq!(&slip_p[52..84], &grantee);
    }

    #[test]
    fn device_signatures_bind_every_field() {
        let id_key = key(10);
        let id_pub = id_key.verifying_key().to_bytes();
        let dev_id = [1u8; 16];
        let curve = [2u8; 32];
        let ed = [3u8; 32];

        let sig = sign_device(&id_key, &dev_id, &curve, &ed);
        assert!(verify_device(&id_pub, &dev_id, &curve, &ed, &sig));

        // Flip one byte of device id
        let mut bad_dev_id = dev_id;
        bad_dev_id[0] ^= 0xff;
        assert!(!verify_device(&id_pub, &bad_dev_id, &curve, &ed, &sig));

        // Flip one byte of curve key
        let mut bad_curve = curve;
        bad_curve[0] ^= 0xff;
        assert!(!verify_device(&id_pub, &dev_id, &bad_curve, &ed, &sig));

        // Flip one byte of ed key
        let mut bad_ed = ed;
        bad_ed[0] ^= 0xff;
        assert!(!verify_device(&id_pub, &dev_id, &curve, &bad_ed, &sig));

        // Flip one byte of signature
        let mut bad_sig = sig;
        bad_sig[0] ^= 0xff;
        assert!(!verify_device(&id_pub, &dev_id, &curve, &ed, &bad_sig));

        // Another identity's key fails
        let other_key = key(11);
        let other_pub = other_key.verifying_key().to_bytes();
        assert!(!verify_device(&other_pub, &dev_id, &curve, &ed, &sig));
    }

    #[test]
    fn prekey_signatures_bind_the_fallback_flag() {
        let dev_key = key(20);
        let dev_pub = dev_key.verifying_key().to_bytes();
        let dev_id = [1u8; 16];
        let key_bytes = [2u8; 32];

        // Sign as one-time key (fallback = false)
        let sig_onetime = sign_prekey(&dev_key, &dev_id, &key_bytes, false);
        assert!(verify_prekey(
            &dev_pub,
            &dev_id,
            &key_bytes,
            false,
            &sig_onetime
        ));
        // A one-time key's signature fails as a fallback key
        assert!(!verify_prekey(
            &dev_pub,
            &dev_id,
            &key_bytes,
            true,
            &sig_onetime
        ));

        // Sign as fallback key (fallback = true)
        let sig_fallback = sign_prekey(&dev_key, &dev_id, &key_bytes, true);
        assert!(verify_prekey(
            &dev_pub,
            &dev_id,
            &key_bytes,
            true,
            &sig_fallback
        ));
        // A fallback key's signature fails as a one-time key
        assert!(!verify_prekey(
            &dev_pub,
            &dev_id,
            &key_bytes,
            false,
            &sig_fallback
        ));

        // Another device id fails
        let mut other_dev_id = dev_id;
        other_dev_id[0] ^= 0xff;
        assert!(!verify_prekey(
            &dev_pub,
            &other_dev_id,
            &key_bytes,
            false,
            &sig_onetime
        ));
    }

    #[test]
    fn slips_name_both_people_in_order() {
        let a_key = key(30);
        let a_pub = a_key.verifying_key().to_bytes();
        let b_key = key(31);
        let b_pub = b_key.verifying_key().to_bytes();
        let c_key = key(32);
        let c_pub = c_key.verifying_key().to_bytes();

        let a_slip_for_b = sign_slip(&a_key, &b_pub);
        // Verifies as (A, B)
        assert!(verify_slip(&a_pub, &b_pub, &a_slip_for_b));
        // Not as (B, A)
        assert!(!verify_slip(&b_pub, &a_pub, &a_slip_for_b));
        // Not as (A, C)
        assert!(!verify_slip(&a_pub, &c_pub, &a_slip_for_b));
    }

    #[test]
    fn wrong_lengths_never_verify() {
        let id_key = key(40);
        let id_pub = id_key.verifying_key().to_bytes();
        let dev_id = [1u8; 16];
        let curve = [2u8; 32];
        let ed = [3u8; 32];
        let sig = sign_device(&id_key, &dev_id, &curve, &ed);

        // 31- or 33-byte keys
        assert!(!verify_device(&id_pub[..31], &dev_id, &curve, &ed, &sig));
        let mut key33 = vec![0u8; 33];
        key33[..32].copy_from_slice(&id_pub);
        assert!(!verify_device(&key33, &dev_id, &curve, &ed, &sig));

        assert!(!verify_device(&id_pub, &dev_id, &curve[..31], &ed, &sig));
        assert!(!verify_device(&id_pub, &dev_id, &curve, &ed[..31], &sig));

        // 15-byte device id
        assert!(!verify_device(&id_pub, &dev_id[..15], &curve, &ed, &sig));

        // 63-byte signature
        assert!(!verify_device(&id_pub, &dev_id, &curve, &ed, &sig[..63]));

        // verify_prekey wrong lengths
        let sig_prekey = sign_prekey(&id_key, &dev_id, &curve, false);
        assert!(!verify_prekey(
            &id_pub[..31],
            &dev_id,
            &curve,
            false,
            &sig_prekey
        ));
        assert!(!verify_prekey(
            &id_pub,
            &dev_id[..15],
            &curve,
            false,
            &sig_prekey
        ));
        assert!(!verify_prekey(
            &id_pub,
            &dev_id,
            &curve[..31],
            false,
            &sig_prekey
        ));
        assert!(!verify_prekey(
            &id_pub,
            &dev_id,
            &curve,
            false,
            &sig_prekey[..63]
        ));

        // verify_slip wrong lengths
        let slip = sign_slip(&id_key, &curve);
        assert!(!verify_slip(&id_pub[..31], &curve, &slip));
        assert!(!verify_slip(&id_pub, &curve[..31], &slip));
        assert!(!verify_slip(&id_pub, &curve, &slip[..63]));
    }
}
