//! Encrypted blob storage format.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};

pub const MAGIC: [u8; 4] = *b"OCA1";
pub const HEADER_LEN: u64 = 8;
pub const SEGMENT_PLAIN: u64 = 65_536;
pub const SEGMENT_OVERHEAD: u64 = 40;
pub const SEGMENT_SEALED: u64 = 65_576;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlobKind {
    File = 0,
    Thumbnail = 1,
    Cache = 2,
}

#[derive(Clone)]
pub struct BlobKey([u8; 32]);

impl BlobKey {
    pub fn random() -> Self {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).expect("the operating system's random source is unavailable");
        Self(bytes)
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Debug for BlobKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BlobKey(..)")
    }
}

pub fn header() -> [u8; 8] {
    [b'O', b'C', b'A', b'1', 0, 0, 0, 0]
}

pub fn segment_count(plain_len: u64) -> u64 {
    if plain_len == 0 {
        1
    } else {
        plain_len.div_ceil(SEGMENT_PLAIN)
    }
}

pub fn sealed_len(plain_len: u64) -> u64 {
    HEADER_LEN + segment_count(plain_len) * SEGMENT_OVERHEAD + plain_len
}

fn make_aad(kind: BlobKind, blob_id: i64, index: u64, last: bool) -> [u8; 22] {
    let mut aad = [0u8; 22];
    aad[0..4].copy_from_slice(&MAGIC);
    aad[4] = kind as u8;
    aad[5..13].copy_from_slice(&blob_id.to_be_bytes());
    aad[13..21].copy_from_slice(&index.to_be_bytes());
    aad[21] = last as u8;
    aad
}

pub fn seal_segment(
    key: &BlobKey,
    kind: BlobKind,
    blob_id: i64,
    index: u64,
    last: bool,
    plain: &[u8],
) -> Vec<u8> {
    let mut nonce_bytes = [0u8; 24];
    getrandom::fill(&mut nonce_bytes).expect("the operating system's random source is unavailable");
    let nonce = XNonce::from(nonce_bytes);
    let cipher = XChaCha20Poly1305::new(key.as_bytes().into());
    let aad = make_aad(kind, blob_id, index, last);
    let ciphertext = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: plain,
                aad: &aad,
            },
        )
        .expect("sealing a segment cannot fail");
    let mut out = Vec::with_capacity(24 + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    out
}

pub fn open_segment(
    key: &BlobKey,
    kind: BlobKind,
    blob_id: i64,
    index: u64,
    last: bool,
    sealed: &[u8],
) -> Result<Vec<u8>, BlobError> {
    if sealed.len() < (24 + 16) {
        return Err(BlobError::Segment);
    }
    let (nonce_bytes, ciphertext) = sealed.split_at(24);
    let nonce = XNonce::try_from(nonce_bytes).map_err(|_| BlobError::Segment)?;
    let cipher = XChaCha20Poly1305::new(key.as_bytes().into());
    let aad = make_aad(kind, blob_id, index, last);
    cipher
        .decrypt(
            &nonce,
            Payload {
                msg: ciphertext,
                aad: &aad,
            },
        )
        .map_err(|_| BlobError::Segment)
}

pub fn seal_all(key: &BlobKey, kind: BlobKind, blob_id: i64, plain: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(sealed_len(plain.len() as u64) as usize);
    out.extend_from_slice(&header());
    if plain.is_empty() {
        out.extend_from_slice(&seal_segment(key, kind, blob_id, 0, true, &[]));
        return out;
    }
    let total_chunks = plain.len().div_ceil(SEGMENT_PLAIN as usize);
    for (index, chunk) in plain.chunks(SEGMENT_PLAIN as usize).enumerate() {
        let last = index + 1 == total_chunks;
        out.extend_from_slice(&seal_segment(key, kind, blob_id, index as u64, last, chunk));
    }
    out
}

pub fn open_all(
    key: &BlobKey,
    kind: BlobKind,
    blob_id: i64,
    sealed: &[u8],
) -> Result<Vec<u8>, BlobError> {
    if sealed.len() < HEADER_LEN as usize || sealed[0..4] != MAGIC || sealed[4..8] != [0, 0, 0, 0] {
        return Err(BlobError::Header);
    }
    if sealed.len() < (HEADER_LEN + SEGMENT_OVERHEAD) as usize {
        return Err(BlobError::Truncated);
    }
    let mut offset = HEADER_LEN as usize;
    let mut index = 0u64;
    let mut plain = Vec::new();
    while offset < sealed.len() {
        let remaining = sealed.len() - offset;
        if remaining < SEGMENT_OVERHEAD as usize {
            return Err(BlobError::Truncated);
        }
        let (seg_len, last) = if remaining <= SEGMENT_SEALED as usize {
            (remaining, true)
        } else {
            (SEGMENT_SEALED as usize, false)
        };
        let seg_bytes = &sealed[offset..offset + seg_len];
        let seg_plain = open_segment(key, kind, blob_id, index, last, seg_bytes)?;
        plain.extend_from_slice(&seg_plain);
        offset += seg_len;
        index += 1;
        if last {
            break;
        }
    }
    if offset != sealed.len() {
        return Err(BlobError::Truncated);
    }
    Ok(plain)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SealedSpan {
    pub first_segment: u64,
    pub last_segment: u64,
    pub sealed_start: u64,
    pub sealed_end: u64,
    pub skip: u64,
    pub take: u64,
}

pub fn locate(plain_len: u64, start: u64, end_inclusive: u64) -> SealedSpan {
    let first_segment = start / SEGMENT_PLAIN;
    let last_segment = end_inclusive / SEGMENT_PLAIN;
    let sealed_start = HEADER_LEN + first_segment * SEGMENT_SEALED;
    let total_segments = segment_count(plain_len);
    let sealed_end = if last_segment + 1 >= total_segments {
        sealed_len(plain_len)
    } else {
        HEADER_LEN + (last_segment + 1) * SEGMENT_SEALED
    };
    let skip = start - first_segment * SEGMENT_PLAIN;
    let take = end_inclusive - start + 1;
    SealedSpan {
        first_segment,
        last_segment,
        sealed_start,
        sealed_end,
        skip,
        take,
    }
}

pub fn open_span(
    key: &BlobKey,
    kind: BlobKind,
    blob_id: i64,
    plain_len: u64,
    span: &SealedSpan,
    sealed: &[u8],
) -> Result<Vec<u8>, BlobError> {
    let expected_sealed_len = (span.sealed_end - span.sealed_start) as usize;
    if sealed.len() < expected_sealed_len {
        return Err(BlobError::Truncated);
    }
    let total_segments = segment_count(plain_len);
    let mut offset = 0usize;
    let mut decrypted = Vec::new();
    for seg_idx in span.first_segment..=span.last_segment {
        let is_final = seg_idx + 1 >= total_segments;
        let seg_plain_len = if is_final {
            plain_len.saturating_sub(seg_idx * SEGMENT_PLAIN)
        } else {
            SEGMENT_PLAIN
        };
        let seg_sealed_len = (seg_plain_len + SEGMENT_OVERHEAD) as usize;
        if offset + seg_sealed_len > sealed.len() {
            return Err(BlobError::Truncated);
        }
        let seg_bytes = &sealed[offset..offset + seg_sealed_len];
        let plain_seg = open_segment(key, kind, blob_id, seg_idx, is_final, seg_bytes)?;
        decrypted.extend_from_slice(&plain_seg);
        offset += seg_sealed_len;
    }
    let skip = span.skip as usize;
    let take = span.take as usize;
    if decrypted.len() < skip + take {
        return Err(BlobError::Truncated);
    }
    Ok(decrypted[skip..skip + take].to_vec())
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BlobError {
    #[error("the file is not an Opencord blob")]
    Header,
    #[error("a part of the file could not be opened")]
    Segment,
    #[error("the file is shorter than its size")]
    Truncated,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_every_size() {
        assert_eq!(sealed_len(0), 48);
        assert_eq!(sealed_len(65_536), 65_584);

        let key = BlobKey::random();
        let sizes = [0, 1, 65_535, 65_536, 65_537, 3 * 65_536 + 5];
        for size in sizes {
            let plain: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
            let sealed = seal_all(&key, BlobKind::File, 42, &plain);
            assert_eq!(sealed.len() as u64, sealed_len(size as u64));
            let opened = open_all(&key, BlobKind::File, 42, &sealed).unwrap();
            assert_eq!(opened, plain);
        }
    }

    #[test]
    fn tampering_is_detected() {
        let key = BlobKey::random();
        let plain = b"hello encrypted world";
        let mut sealed = seal_all(&key, BlobKind::File, 1, plain);
        let last = sealed.len() - 1;
        sealed[last] ^= 0x01;
        assert_eq!(
            open_all(&key, BlobKind::File, 1, &sealed),
            Err(BlobError::Segment)
        );
    }

    #[test]
    fn truncation_at_a_segment_boundary_is_detected() {
        let key = BlobKey::random();
        let plain = vec![0xaa; 2 * 65_536];
        let sealed = seal_all(&key, BlobKind::File, 10, &plain);
        let truncated = &sealed[..8 + 65_576];
        assert!(open_all(&key, BlobKind::File, 10, truncated).is_err());
    }

    #[test]
    fn segments_cannot_be_swapped_between_files_or_kinds() {
        let key = BlobKey::random();
        let plain = b"segment data";
        let sealed_file_1 = seal_segment(&key, BlobKind::File, 1, 0, true, plain);
        assert_eq!(
            open_segment(&key, BlobKind::File, 2, 0, true, &sealed_file_1),
            Err(BlobError::Segment)
        );
        assert_eq!(
            open_segment(&key, BlobKind::Thumbnail, 1, 0, true, &sealed_file_1),
            Err(BlobError::Segment)
        );
    }

    #[test]
    fn locate_and_open_span_return_exact_ranges() {
        let key = BlobKey::random();
        let plain_len = 200_000usize;
        let plain: Vec<u8> = (0..plain_len).map(|i| ((i * 31) % 256) as u8).collect();
        let sealed = seal_all(&key, BlobKind::File, 123, &plain);

        let cases = [(0, 0), (65_535, 65_536), (199_999, 199_999), (10, 150_000)];
        for (start, end) in cases {
            let span = locate(plain_len as u64, start, end);
            let slice = &sealed[span.sealed_start as usize..span.sealed_end as usize];
            let opened =
                open_span(&key, BlobKind::File, 123, plain_len as u64, &span, slice).unwrap();
            assert_eq!(
                opened,
                &plain[start as usize..=end as usize],
                "range ({start}, {end}) failed"
            );
        }
    }
}
