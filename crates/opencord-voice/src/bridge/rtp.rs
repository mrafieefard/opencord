//! What arrives on the bridge's UDP port (Phase 3 plan §13.3): Discord's
//! IP discovery, and RTP packets in the `rtpsize` encryption modes. In
//! those, the RTP header up to and including any extension's preamble is
//! sent in the clear and authenticated; the rest is encrypted, and a
//! 32-bit nonce follows the tag.

use aes_gcm::Aes256Gcm;
use aes_gcm::aead::{AeadCore, AeadInOut, KeyInit, Nonce, Tag};
use chacha20poly1305::XChaCha20Poly1305;

/// The encryption modes offered, most preferred first: Lavalink takes the
/// first one it knows. XChaCha20 is the one Discord requires everywhere.
pub const MODES: [Mode; 2] = [Mode::Aes256Gcm, Mode::XChaCha20Poly1305];

/// Datagrams over this size are dropped unread.
pub const MAX_DATAGRAM: usize = 1500;
const HEADER: usize = 12;
const TAG: usize = 16;
const NONCE: usize = 4;
/// IP discovery's request and response, both exactly this long.
pub const DISCOVERY_LEN: usize = 74;
const DISCOVERY_REQUEST: u16 = 1;
const DISCOVERY_RESPONSE: u16 = 2;
/// The length field: what follows type and length.
const DISCOVERY_BODY: u16 = 70;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Aes256Gcm,
    XChaCha20Poly1305,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Self::Aes256Gcm => "aead_aes256_gcm_rtpsize",
            Self::XChaCha20Poly1305 => "aead_xchacha20_poly1305_rtpsize",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        MODES.into_iter().find(|mode| mode.name() == name)
    }
}

/// A decrypted packet: Opus, and where it sits in the sender's stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opened {
    pub ssrc: u32,
    pub sequence: u16,
    pub timestamp: u32,
    pub payload_type: u8,
    pub opus: Vec<u8>,
}

/// One connection's key, ready to open its packets.
pub enum Opener {
    Aes(Box<Aes256Gcm>),
    XChaCha(Box<XChaCha20Poly1305>),
}

impl Opener {
    pub fn new(mode: Mode, key: &[u8; 32]) -> Self {
        match mode {
            Mode::Aes256Gcm => Self::Aes(Box::new(Aes256Gcm::new(key.into()))),
            Mode::XChaCha20Poly1305 => Self::XChaCha(Box::new(XChaCha20Poly1305::new(key.into()))),
        }
    }

    /// The Opus in `packet`, if it is RTP that this key sealed.
    pub fn open(&self, packet: &[u8]) -> Option<Opened> {
        let header = Header::parse(packet)?;
        let clear = header.clear_len;
        let sealed_end = packet.len().checked_sub(NONCE)?;
        let tag_start = sealed_end.checked_sub(TAG)?;
        if tag_start < clear {
            return None;
        }
        let aad = &packet[..clear];
        let counter: [u8; NONCE] = packet[sealed_end..].try_into().ok()?;
        let mut body = packet[clear..tag_start].to_vec();
        let tag = &packet[tag_start..sealed_end];
        match self {
            Self::Aes(cipher) => decrypt(cipher.as_ref(), counter, aad, &mut body, tag)?,
            Self::XChaCha(cipher) => decrypt(cipher.as_ref(), counter, aad, &mut body, tag)?,
        }
        // The extension's body was encrypted with the payload.
        let skip = header.extension_words * 4;
        if body.len() < skip {
            return None;
        }
        body.drain(..skip);
        Some(Opened {
            ssrc: header.ssrc,
            sequence: header.sequence,
            timestamp: header.timestamp,
            payload_type: header.payload_type,
            opus: body,
        })
    }
}

/// The nonce: the packet's counter in its first four bytes, the rest zero,
/// as Discord's `rtpsize` modes build it.
fn decrypt<C: AeadInOut>(
    cipher: &C,
    counter: [u8; NONCE],
    aad: &[u8],
    body: &mut [u8],
    tag: &[u8],
) -> Option<()> {
    let mut nonce = Nonce::<C>::default();
    nonce[..NONCE].copy_from_slice(&counter);
    let tag = Tag::<C>::try_from(tag).ok()?;
    cipher
        .decrypt_inout_detached(&nonce, aad, body.into(), &tag)
        .ok()
}

#[derive(Debug, Clone, Copy)]
struct Header {
    payload_type: u8,
    sequence: u16,
    timestamp: u32,
    ssrc: u32,
    /// The header, CSRCs and extension preamble: sent in the clear.
    clear_len: usize,
    /// The extension's length, in 32-bit words, from its preamble.
    extension_words: usize,
}

impl Header {
    fn parse(packet: &[u8]) -> Option<Self> {
        if packet.len() < HEADER || packet[0] >> 6 != 2 {
            return None;
        }
        let csrcs = usize::from(packet[0] & 0x0f);
        let extension = packet[0] & 0x10 != 0;
        let mut clear_len = HEADER + csrcs * 4;
        let mut extension_words = 0;
        if extension {
            let preamble = packet.get(clear_len..clear_len + 4)?;
            extension_words = usize::from(u16::from_be_bytes([preamble[2], preamble[3]]));
            clear_len += 4;
        }
        Some(Self {
            payload_type: packet[1] & 0x7f,
            sequence: u16::from_be_bytes([packet[2], packet[3]]),
            timestamp: u32::from_be_bytes(packet[4..8].try_into().ok()?),
            ssrc: u32::from_be_bytes(packet[8..12].try_into().ok()?),
            clear_len,
            extension_words,
        })
    }
}

/// The SSRC an IP discovery request asks about, if `packet` is one.
pub fn discovery_request(packet: &[u8]) -> Option<u32> {
    if packet.len() != DISCOVERY_LEN
        || u16::from_be_bytes([packet[0], packet[1]]) != DISCOVERY_REQUEST
    {
        return None;
    }
    Some(u32::from_be_bytes(packet[4..8].try_into().ok()?))
}

/// The answer to IP discovery: the address and port the request came from,
/// the address as a NUL-terminated string.
pub fn discovery_response(ssrc: u32, from: std::net::SocketAddr) -> [u8; DISCOVERY_LEN] {
    let mut response = [0u8; DISCOVERY_LEN];
    response[..2].copy_from_slice(&DISCOVERY_RESPONSE.to_be_bytes());
    response[2..4].copy_from_slice(&DISCOVERY_BODY.to_be_bytes());
    response[4..8].copy_from_slice(&ssrc.to_be_bytes());
    let ip = from.ip().to_canonical().to_string();
    // 64 bytes of address: an IPv6 one is at most 45 characters.
    response[8..8 + ip.len()].copy_from_slice(ip.as_bytes());
    response[72..74].copy_from_slice(&from.port().to_be_bytes());
    response
}

/// Seals Opus as Lavalink's voice client does. For tests: a stand-in for
/// Lavalink, and the bridge's own conformance tests.
#[doc(hidden)]
pub fn seal(
    mode: Mode,
    key: &[u8; 32],
    counter: u32,
    header: [u8; HEADER],
    opus: &[u8],
) -> Vec<u8> {
    fn sealed<C: AeadInOut + AeadCore>(
        cipher: &C,
        counter: [u8; NONCE],
        aad: &[u8],
        body: &mut [u8],
    ) -> Vec<u8> {
        let mut nonce = Nonce::<C>::default();
        nonce[..NONCE].copy_from_slice(&counter);
        let tag = cipher
            .encrypt_inout_detached(&nonce, aad, body.into())
            .expect("a packet this small seals");
        tag.to_vec()
    }
    let counter = counter.to_le_bytes();
    let mut body = opus.to_vec();
    let tag = match mode {
        Mode::Aes256Gcm => sealed(&Aes256Gcm::new(key.into()), counter, &header, &mut body),
        Mode::XChaCha20Poly1305 => sealed(
            &XChaCha20Poly1305::new(key.into()),
            counter,
            &header,
            &mut body,
        ),
    };
    let mut packet = header.to_vec();
    packet.extend_from_slice(&body);
    packet.extend_from_slice(&tag);
    packet.extend_from_slice(&counter);
    packet
}

/// An RTP header as Lavalink writes it: version 2, no padding, extension
/// or CSRCs.
#[doc(hidden)]
pub fn header(payload_type: u8, sequence: u16, timestamp: u32, ssrc: u32) -> [u8; HEADER] {
    let mut header = [0u8; HEADER];
    header[0] = 0x80;
    header[1] = payload_type & 0x7f;
    header[2..4].copy_from_slice(&sequence.to_be_bytes());
    header[4..8].copy_from_slice(&timestamp.to_be_bytes());
    header[8..12].copy_from_slice(&ssrc.to_be_bytes());
    header
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; 32] = [7; 32];
    const OPUS: &[u8] = &[0xfc, 0xff, 0xfe, 1, 2, 3, 4, 5];

    #[test]
    fn both_modes_open_what_lavalink_seals() {
        for mode in MODES {
            let packet = seal(mode, &KEY, 5, header(120, 65535, 960, 42), OPUS);
            let opened = Opener::new(mode, &KEY).open(&packet).unwrap();
            assert_eq!(
                opened,
                Opened {
                    ssrc: 42,
                    sequence: 65535,
                    timestamp: 960,
                    payload_type: 120,
                    opus: OPUS.to_vec(),
                }
            );
        }
    }

    #[test]
    fn tampering_or_another_key_opens_nothing() {
        for mode in MODES {
            let packet = seal(mode, &KEY, 1, header(120, 1, 0, 42), OPUS);
            assert!(Opener::new(mode, &[8; 32]).open(&packet).is_none());
            // The header is authenticated, not only the payload.
            let mut moved = packet.clone();
            moved[11] ^= 1;
            assert!(Opener::new(mode, &KEY).open(&moved).is_none());
            let mut flipped = packet.clone();
            flipped[HEADER] ^= 1;
            assert!(Opener::new(mode, &KEY).open(&flipped).is_none());
            assert!(Opener::new(mode, &KEY).open(&packet[..20]).is_none());
        }
    }

    #[test]
    fn an_extension_body_is_dropped_after_decrypting() {
        let mut clear = header(120, 9, 0, 42).to_vec();
        clear[0] |= 0x10;
        // Preamble: profile 0xBEDE, one word of extension.
        clear.extend_from_slice(&[0xbe, 0xde, 0, 1]);
        let mut body = vec![0x10, 0xff, 0, 0];
        body.extend_from_slice(OPUS);
        let cipher = Aes256Gcm::new((&KEY).into());
        let mut nonce = Nonce::<Aes256Gcm>::default();
        nonce[..4].copy_from_slice(&3u32.to_le_bytes());
        let tag = cipher
            .encrypt_inout_detached(&nonce, &clear, body.as_mut_slice().into())
            .unwrap();
        let mut packet = clear;
        packet.extend_from_slice(&body);
        packet.extend_from_slice(&tag);
        packet.extend_from_slice(&3u32.to_le_bytes());

        let opened = Opener::new(Mode::Aes256Gcm, &KEY).open(&packet).unwrap();

        assert_eq!(opened.opus, OPUS);
    }

    #[test]
    fn discovery_answers_with_the_address_seen() {
        let mut request = [0u8; DISCOVERY_LEN];
        request[..2].copy_from_slice(&1u16.to_be_bytes());
        request[2..4].copy_from_slice(&70u16.to_be_bytes());
        request[4..8].copy_from_slice(&77u32.to_be_bytes());
        assert_eq!(discovery_request(&request), Some(77));
        assert_eq!(discovery_request(&request[..73]), None);

        let response = discovery_response(77, "203.0.113.9:50123".parse().unwrap());

        // Read as Lavalink's voice client reads it.
        let ip: Vec<u8> = response[8..]
            .iter()
            .take_while(|b| **b != 0)
            .copied()
            .collect();
        assert_eq!(ip, b"203.0.113.9");
        assert_eq!(u16::from_be_bytes([response[72], response[73]]), 50123);
        assert_eq!(&response[..2], &2u16.to_be_bytes());
        assert_eq!(discovery_request(&response), None);
    }

    #[test]
    fn modes_are_named_as_discord_names_them() {
        assert_eq!(
            Mode::from_name("aead_xchacha20_poly1305_rtpsize"),
            Some(Mode::XChaCha20Poly1305)
        );
        assert_eq!(Mode::from_name("xsalsa20_poly1305"), None);
    }
}
