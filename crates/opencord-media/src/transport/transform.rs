//! Where end-to-end encryption will go (plan §13): encoded frames pass
//! through a [`FrameTransform`] after the encoder and before packetizing,
//! and after depacketizing and before the decoder. The transform keeps codec
//! framing intact, as Discord's DAVE does: an Opus frame is transformed
//! whole, an H.264 frame NAL unit by NAL unit with each NAL header left in
//! the clear. The voice node only ever needs RTP headers and extensions.

use std::sync::Arc;

/// Changes encoded frames on their way out and back. The identity until
/// end-to-end encryption arrives.
pub trait FrameTransform: Send {
    /// An Opus frame on its way to the packetizer.
    fn outbound_audio(&mut self, frame: Vec<u8>) -> Vec<u8> {
        frame
    }

    /// `sender`'s Opus frame on its way to the decoder; `None` drops it.
    fn inbound_audio(&mut self, sender: i64, frame: Arc<[u8]>) -> Option<Arc<[u8]>> {
        let _ = sender;
        Some(frame)
    }

    /// An H.264 frame's NAL units on their way to the packetizer. Each NAL
    /// unit's first byte (its header) must stay as it is.
    fn outbound_video(&mut self, nal_units: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
        nal_units
    }

    /// `sender`'s NAL units on their way to the decoder; `None` drops the
    /// frame.
    fn inbound_video(&mut self, sender: i64, nal_units: Vec<Vec<u8>>) -> Option<Vec<Vec<u8>>> {
        let _ = sender;
        Some(nal_units)
    }
}

/// Frames as they are.
#[derive(Debug, Clone, Copy, Default)]
pub struct Identity;

impl FrameTransform for Identity {}

/// XORs every payload byte with a key (NAL headers stay in the clear): the
/// test-only transform that proves the voice node never needs payloads
/// (plan §13).
#[cfg(any(test, feature = "testing"))]
#[derive(Debug, Clone, Copy)]
pub struct XorTransform {
    pub key: u8,
}

#[cfg(any(test, feature = "testing"))]
impl XorTransform {
    fn apply(self, bytes: &mut [u8]) {
        for byte in bytes {
            *byte ^= self.key;
        }
    }

    fn apply_to_units(self, mut nal_units: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
        for unit in &mut nal_units {
            if let Some((_, body)) = unit.split_first_mut() {
                self.apply(body);
            }
        }
        nal_units
    }
}

#[cfg(any(test, feature = "testing"))]
impl FrameTransform for XorTransform {
    fn outbound_audio(&mut self, mut frame: Vec<u8>) -> Vec<u8> {
        self.apply(&mut frame);
        frame
    }

    fn inbound_audio(&mut self, _sender: i64, frame: Arc<[u8]>) -> Option<Arc<[u8]>> {
        let mut frame = frame.to_vec();
        self.apply(&mut frame);
        Some(frame.into())
    }

    fn outbound_video(&mut self, nal_units: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
        self.apply_to_units(nal_units)
    }

    fn inbound_video(&mut self, _sender: i64, nal_units: Vec<Vec<u8>>) -> Option<Vec<Vec<u8>>> {
        Some(self.apply_to_units(nal_units))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn units() -> Vec<Vec<u8>> {
        vec![vec![0x67, 1, 2, 3], vec![0x65, 0, 0, 1, 9]]
    }

    #[test]
    fn the_identity_changes_nothing() {
        let mut identity = Identity;
        let audio: Arc<[u8]> = vec![1, 2, 3].into();

        assert_eq!(identity.outbound_audio(vec![1, 2, 3]), vec![1, 2, 3]);
        assert_eq!(identity.inbound_audio(7, audio.clone()), Some(audio));
        assert_eq!(identity.outbound_video(units()), units());
        assert_eq!(identity.inbound_video(7, units()), Some(units()));
    }

    #[test]
    fn xor_changes_payloads_and_undoes_itself() {
        let mut xor = XorTransform { key: 0x5a };

        let sent_audio = xor.outbound_audio(vec![1, 2, 3]);
        let sent_video = xor.outbound_video(units());

        assert_ne!(sent_audio, vec![1, 2, 3]);
        assert_ne!(sent_video, units());
        assert_eq!(
            xor.inbound_audio(7, sent_audio.into()).as_deref(),
            Some(&[1u8, 2, 3][..])
        );
        assert_eq!(xor.inbound_video(7, sent_video), Some(units()));
    }

    #[test]
    fn xor_leaves_nal_headers_in_the_clear() {
        let mut xor = XorTransform { key: 0xff };

        let sent = xor.outbound_video(units());

        assert_eq!(sent[0][0], 0x67);
        assert_eq!(sent[1][0], 0x65);
        assert_eq!(sent[1][1..], [0xff, 0xff, 0xfe, 0xf6]);
    }
}
