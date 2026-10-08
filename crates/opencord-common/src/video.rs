//! What clients and voice nodes agree on for video (Phase 2 plan §6, §8,
//! §9.2), without depending on each other.

/// RTP payload type of H.264 on every voice connection.
pub const H264_PAYLOAD_TYPE: u8 = 102;
/// Its retransmissions (RFC 4588).
pub const H264_RTX_PAYLOAD_TYPE: u8 = 103;
/// What the node advertises for H.264. The node never reads payloads, so
/// this only tells clients how to packetize.
pub const H264_FMTP: &str =
    "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f";
pub const VIDEO_CLOCK_RATE: u32 = 90_000;

/// Simulcast layer names, lowest first; a layer's index is its position.
pub const LAYER_RIDS: [&str; 3] = ["l", "m", "h"];

/// The index of the layer called `rid`, 0 for the lowest.
pub fn layer_index(rid: &str) -> Option<u8> {
    LAYER_RIDS
        .iter()
        .position(|known| *known == rid)
        .and_then(|index| u8::try_from(index).ok())
}

/// The media a client sends a track's layers on, named after the SSRC of
/// its first layer.
pub fn video_mid(ssrc: u32) -> String {
    format!("v{ssrc:08x}")
}

/// What a video track shows; ceilings and limits differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VideoKind {
    Camera,
    Screen,
}

/// Rows of the bitrate ceiling table: the largest pixel count a row covers,
/// and its ceilings at up to 15, 30 and 60 fps, in bits per second (plan
/// §9.2, with rows below 480p from §8's camera layers and §9.2's low layer).
const CEILINGS: [(u32, [u32; 3]); 7] = [
    (320 * 180, [150_000, 250_000, 400_000]),
    (640 * 360, [300_000, 500_000, 800_000]),
    (854 * 480, [600_000, 1_000_000, 1_600_000]),
    (1280 * 720, [1_200_000, 2_000_000, 3_500_000]),
    (1920 * 1080, [2_500_000, 4_000_000, 6_500_000]),
    (2560 * 1440, [4_000_000, 6_500_000, 10_000_000]),
    (3840 * 2160, [6_000_000, 10_000_000, 16_000_000]),
];

/// A camera's 720p layer may go up to §8's 2.5 Mbps.
const CAMERA_720P30: u32 = 2_500_000;

/// The most a layer of this size and frame rate may send, in bits per
/// second. Sizes go by pixel count, so ultrawide and portrait video are
/// treated fairly; frame rates round up to 15, 30 or 60.
pub fn layer_ceiling(kind: VideoKind, width: u32, height: u32, fps: u32) -> u32 {
    let pixels = width.saturating_mul(height);
    let row = CEILINGS
        .iter()
        .position(|(most, _)| pixels <= *most)
        .unwrap_or(CEILINGS.len() - 1);
    let column = match fps {
        0..=15 => 0,
        16..=30 => 1,
        _ => 2,
    };
    let ceiling = CEILINGS[row].1[column];
    if kind == VideoKind::Camera && row == 3 && column == 1 {
        CAMERA_720P30
    } else {
        ceiling
    }
}

/// URI of Opencord's frame-marking RTP header extension. Clients and nodes
/// map it to [`FRAME_MARKING_ID`] without negotiating.
pub const FRAME_MARKING_URI: &str = "http://opencord.dev/rtp-hdrext/frame-marking";
pub const FRAME_MARKING_ID: u8 = 9;

/// What the voice node needs to know about a video packet, in an RTP
/// header extension so it never reads the payload (plan §6, §13).
///
/// One byte: bit 7 keyframe, bit 6 first packet of a frame, bits 0-1 the
/// layer index; bits 2-5 are zero. A keyframe's first packet adds the
/// frame's width and height (big-endian `u16` each). The RTP marker bit
/// ends a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameMarking {
    /// The frame decodes on its own.
    pub keyframe: bool,
    /// The frame's first packet.
    pub start: bool,
    /// 0 for the lowest layer.
    pub layer: u8,
    /// Width and height, on a keyframe's first packet.
    pub size: Option<(u16, u16)>,
}

const KEYFRAME_BIT: u8 = 0x80;
const START_BIT: u8 = 0x40;
const LAYER_BITS: u8 = 0x03;

impl FrameMarking {
    /// The most bytes it takes.
    pub const MAX_LEN: usize = 5;

    /// Writes the extension's bytes; returns how many, 0 when `buffer` is
    /// too small.
    pub fn write(&self, buffer: &mut [u8]) -> usize {
        let length = if self.size.is_some() { 5 } else { 1 };
        if buffer.len() < length {
            return 0;
        }
        let mut first = self.layer & LAYER_BITS;
        if self.keyframe {
            first |= KEYFRAME_BIT;
        }
        if self.start {
            first |= START_BIT;
        }
        buffer[0] = first;
        if let Some((width, height)) = self.size {
            buffer[1..3].copy_from_slice(&width.to_be_bytes());
            buffer[3..5].copy_from_slice(&height.to_be_bytes());
        }
        length
    }

    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let (first, size) = match bytes {
            [first] => (*first, None),
            [first, w0, w1, h0, h1] => (
                *first,
                Some((
                    u16::from_be_bytes([*w0, *w1]),
                    u16::from_be_bytes([*h0, *h1]),
                )),
            ),
            _ => return None,
        };
        Some(Self {
            keyframe: first & KEYFRAME_BIT != 0,
            start: first & START_BIT != 0,
            layer: first & LAYER_BITS,
            size,
        })
    }
}

/// Reads and writes [`FrameMarking`] in str0m's extension values (as a
/// user value).
#[cfg(feature = "str0m")]
#[derive(Debug, Clone, Copy)]
pub struct FrameMarkingSerializer;

#[cfg(feature = "str0m")]
impl str0m::rtp::ExtensionSerializer for FrameMarkingSerializer {
    fn write_to(&self, buffer: &mut [u8], values: &str0m::rtp::ExtensionValues) -> usize {
        values
            .user_values
            .get::<FrameMarking>()
            .map_or(0, |marking| marking.write(buffer))
    }

    fn parse_value(&self, bytes: &[u8], values: &mut str0m::rtp::ExtensionValues) -> bool {
        let Some(marking) = FrameMarking::parse(bytes) else {
            return false;
        };
        values.user_values.set(marking);
        true
    }

    fn is_audio(&self) -> bool {
        false
    }

    fn is_video(&self) -> bool {
        true
    }
}

/// The extension, for `RtcConfig::set_extension(FRAME_MARKING_ID, ...)`.
#[cfg(feature = "str0m")]
pub fn frame_marking_extension() -> str0m::rtp::Extension {
    str0m::rtp::Extension::with_serializer(FRAME_MARKING_URI, FrameMarkingSerializer)
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "str0m")]
    #[test]
    fn str0m_reads_and_writes_the_extension() {
        use str0m::rtp::{ExtensionSerializer as _, ExtensionValues};

        let marking = FrameMarking {
            keyframe: true,
            start: true,
            layer: 1,
            size: Some((640, 360)),
        };
        let mut values = ExtensionValues::default();
        values.user_values.set(marking);
        let serializer = FrameMarkingSerializer;
        let mut buffer = [0u8; 16];

        let written = serializer.write_to(&mut buffer, &values);
        let mut parsed = ExtensionValues::default();

        assert_eq!(written, 5);
        assert!(serializer.parse_value(&buffer[..written], &mut parsed));
        assert_eq!(parsed.user_values.get::<FrameMarking>(), Some(&marking));
        assert!(serializer.is_video() && !serializer.is_audio());
        assert_eq!(
            serializer.write_to(&mut buffer, &ExtensionValues::default()),
            0
        );
    }

    use super::*;

    #[test]
    fn camera_ceilings_follow_the_camera_table() {
        assert_eq!(layer_ceiling(VideoKind::Camera, 1280, 720, 30), 2_500_000);
        assert_eq!(layer_ceiling(VideoKind::Camera, 640, 360, 30), 500_000);
        assert_eq!(layer_ceiling(VideoKind::Camera, 320, 180, 15), 150_000);
    }

    #[test]
    fn screen_ceilings_follow_the_screen_share_table() {
        assert_eq!(layer_ceiling(VideoKind::Screen, 854, 480, 15), 600_000);
        assert_eq!(layer_ceiling(VideoKind::Screen, 1280, 720, 30), 2_000_000);
        assert_eq!(layer_ceiling(VideoKind::Screen, 1920, 1080, 60), 6_500_000);
        assert_eq!(layer_ceiling(VideoKind::Screen, 2560, 1440, 30), 6_500_000);
        assert_eq!(layer_ceiling(VideoKind::Screen, 3840, 2160, 60), 16_000_000);
        // The low layer: at most 640x360 at 15 fps, about 300 kbps.
        assert_eq!(layer_ceiling(VideoKind::Screen, 640, 360, 15), 300_000);
    }

    #[test]
    fn a_ceiling_goes_by_pixel_count_and_the_next_frame_rate_up() {
        // An ultrawide 2560x1080 has fewer pixels than 1440p.
        assert_eq!(layer_ceiling(VideoKind::Screen, 2560, 1080, 30), 6_500_000);
        // 1.5 MP is above 720p, so the 1080p row.
        assert_eq!(layer_ceiling(VideoKind::Screen, 1500, 1000, 30), 4_000_000);
        // 24 fps counts as 30, 5 fps as 15.
        assert_eq!(layer_ceiling(VideoKind::Screen, 1280, 720, 24), 2_000_000);
        assert_eq!(layer_ceiling(VideoKind::Screen, 1280, 720, 5), 1_200_000);
        // Tiny layers get the smallest row.
        assert_eq!(layer_ceiling(VideoKind::Camera, 160, 90, 15), 150_000);
    }

    #[test]
    fn frame_marking_round_trips() {
        let markings = [
            FrameMarking {
                keyframe: true,
                start: true,
                layer: 2,
                size: Some((1280, 720)),
            },
            FrameMarking {
                keyframe: false,
                start: true,
                layer: 0,
                size: None,
            },
            FrameMarking {
                keyframe: true,
                start: false,
                layer: 1,
                size: None,
            },
        ];
        for marking in markings {
            let mut buffer = [0u8; FrameMarking::MAX_LEN];

            let written = marking.write(&mut buffer);

            assert_eq!(written, if marking.size.is_some() { 5 } else { 1 });
            assert_eq!(FrameMarking::parse(&buffer[..written]), Some(marking));
        }
    }

    #[test]
    fn frame_marking_has_a_fixed_layout() {
        let marking = FrameMarking {
            keyframe: true,
            start: true,
            layer: 2,
            size: Some((0x0500, 0x02d0)),
        };
        let mut buffer = [0u8; FrameMarking::MAX_LEN];

        marking.write(&mut buffer);

        assert_eq!(buffer, [0b1100_0010, 0x05, 0x00, 0x02, 0xd0]);
    }

    #[test]
    fn frame_marking_refuses_what_it_cannot_read() {
        assert_eq!(FrameMarking::parse(&[]), None);
        assert_eq!(FrameMarking::parse(&[0x80, 0x05]), None);
        assert_eq!(FrameMarking::parse(&[0x80, 0, 0, 0, 0, 0]), None);
        let mut small = [0u8; 4];
        let sized = FrameMarking {
            keyframe: true,
            start: true,
            layer: 0,
            size: Some((320, 180)),
        };
        assert_eq!(sized.write(&mut small), 0);
    }

    #[test]
    fn rids_name_layers_lowest_first() {
        assert_eq!(layer_index("l"), Some(0));
        assert_eq!(layer_index("m"), Some(1));
        assert_eq!(layer_index("h"), Some(2));
        assert_eq!(layer_index("x"), None);
        assert_eq!(LAYER_RIDS[2], "h");
    }

    #[test]
    fn video_mids_fit_a_mid_and_differ_from_audio_mids() {
        let mid = video_mid(u32::MAX);

        assert_eq!(mid, "vffffffff");
        assert!(mid.len() <= 16);
        assert_ne!(video_mid(7), crate::voice::receive_mid(7));
    }
}
