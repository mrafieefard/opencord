//! Randomness from the operating system.

const INVITE_ALPHABET: &[u8; 62] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
/// Largest multiple of 62 that fits in a byte; higher bytes are skipped so
/// every character is equally likely.
const UNBIASED_LIMIT: u8 = 248;

pub fn bytes<const N: usize>() -> [u8; N] {
    let mut buffer = [0; N];
    // The OS random source failing leaves nothing secure to fall back to.
    getrandom::fill(&mut buffer).expect("the operating system's random source is unavailable");
    buffer
}

/// Base62 code of `len` characters.
pub fn code(len: usize) -> String {
    let mut code = String::with_capacity(len);
    while code.len() < len {
        for byte in bytes::<32>() {
            if byte < UNBIASED_LIMIT && code.len() < len {
                code.push(char::from(INVITE_ALPHABET[usize::from(byte % 62)]));
            }
        }
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_have_the_requested_length_and_alphabet() {
        let code = code(8);

        assert_eq!(code.len(), 8);
        assert!(code.bytes().all(|byte| byte.is_ascii_alphanumeric()));
        assert_ne!(code, super::code(8));
    }
}
