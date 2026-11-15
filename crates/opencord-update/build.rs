//! Refuses to build a launcher whose update key can't be read: a broken
//! key would leave every user's launcher unable to update. Only the shape
//! is checked here; whether the bytes are a point on the curve is checked
//! at run time (`signature::built_in_key`), which refuses to update
//! rather than panic.

use data_encoding::BASE64;

const KEY_VAR: &str = "OPENCORD_UPDATE_PUBLIC_KEY";
/// SubjectPublicKeyInfo for Ed25519, as `openssl pkey -pubout -outform DER`
/// writes it, before the 32 raw bytes.
const DER_PREFIX: [u8; 12] = [
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
];

fn main() {
    println!("cargo:rerun-if-env-changed={KEY_VAR}");
    println!("cargo:rerun-if-changed=build.rs");
    let Ok(key) = std::env::var(KEY_VAR) else {
        return;
    };
    if key.trim().is_empty() {
        return;
    }
    if !is_key_shaped(&key) {
        panic!("{KEY_VAR} is not a base64 Ed25519 public key (raw 32 bytes or DER)");
    }
}

/// Base64 of the 32 raw bytes, or of the 44-byte DER that ends in them.
fn is_key_shaped(text: &str) -> bool {
    match BASE64.decode(text.trim().as_bytes()) {
        Ok(bytes) => bytes.len() == 32 || (bytes.len() == 44 && bytes[..12] == DER_PREFIX),
        Err(_) => false,
    }
}
