// SPDX-License-Identifier: MPL-2.0

//! The 16 words two people compare to check a private chat (design
//! 2026-11-01 §9), and the ones two of a person's devices compare before
//! one gives the other its chat key.
//!
//! The list is BIP-39's English one (`bitcoin/bips`, `bip-0039/english.txt`,
//! SHA-256 2f5eed53…24dbda): 2048 words, no two sharing their first four
//! letters.

use std::sync::LazyLock;

use sha2::{Digest, Sha512};

/// Words shown, in a 4×4 grid.
pub const WORD_COUNT: usize = 16;
const BITS_PER_WORD: usize = 11;
const CHAT_CONTEXT: &[u8] = b"opencord-words-v1";
const PAIRING_CONTEXT: &[u8] = b"opencord-own-words-v1";

static LIST: LazyLock<Vec<&'static str>> =
    LazyLock::new(|| include_str!("words/english.txt").lines().collect());

fn list() -> &'static [&'static str] {
    &LIST
}

/// A chat's words, from both people's chat keys in either order.
pub fn chat_words(a: &[u8; 32], b: &[u8; 32]) -> [&'static str; WORD_COUNT] {
    words(CHAT_CONTEXT, a, b)
}

/// The words two of a person's devices show, from their Ed25519 keys.
pub fn pairing_words(a: &[u8; 32], b: &[u8; 32]) -> [&'static str; WORD_COUNT] {
    words(PAIRING_CONTEXT, a, b)
}

fn words(context: &[u8], a: &[u8; 32], b: &[u8; 32]) -> [&'static str; WORD_COUNT] {
    let (lower, higher) = if a <= b { (a, b) } else { (b, a) };
    let digest = Sha512::new()
        .chain_update(context)
        .chain_update(lower)
        .chain_update(higher)
        .finalize();
    std::array::from_fn(|word| {
        let index = (0..BITS_PER_WORD).fold(0, |index, bit| {
            let at = word * BITS_PER_WORD + bit;
            (index << 1) | usize::from((digest[at / 8] >> (7 - at % 8)) & 1)
        });
        list()[index]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_are_the_same_in_either_order() {
        assert_eq!(
            chat_words(&[1; 32], &[2; 32]),
            chat_words(&[2; 32], &[1; 32])
        );
        assert_eq!(
            pairing_words(&[1; 32], &[2; 32]),
            pairing_words(&[2; 32], &[1; 32])
        );
    }

    #[test]
    fn another_key_gives_other_words() {
        assert_ne!(
            chat_words(&[1; 32], &[2; 32]),
            chat_words(&[1; 32], &[3; 32])
        );
    }

    #[test]
    fn pairing_words_differ_from_chat_words() {
        assert_ne!(
            chat_words(&[1; 32], &[2; 32]),
            pairing_words(&[1; 32], &[2; 32])
        );
    }

    #[test]
    fn the_word_list_is_bip39_english() {
        let list = list();
        assert_eq!(list.len(), 2048);
        assert_eq!(list[0], "abandon");
        assert_eq!(list[2047], "zoo");
        let mut sorted = list.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 2048);
    }

    #[test]
    fn words_take_eleven_bits_each_from_the_digest() {
        use sha2::{Digest, Sha512};
        let digest = Sha512::new()
            .chain_update(b"opencord-words-v1")
            .chain_update([1; 32])
            .chain_update([2; 32])
            .finalize();
        let first = (usize::from(digest[0]) << 3) | usize::from(digest[1] >> 5);
        let second = ((usize::from(digest[1]) & 0x1f) << 6) | usize::from(digest[2] >> 2);
        let words = chat_words(&[2; 32], &[1; 32]);
        assert_eq!(words[0], list()[first]);
        assert_eq!(words[1], list()[second]);
    }

    #[test]
    fn the_words_are_pinned() {
        assert_eq!(chat_words(&[1; 32], &[2; 32]), PINNED);
    }

    const PINNED: [&str; WORD_COUNT] = [
        "chaos", "avocado", "cushion", "erode", "ranch", "hire", "sadness", "captain", "trust",
        "fiber", "fossil", "right", "pink", "jacket", "enjoy", "fiscal",
    ];
}
