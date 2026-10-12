//! Ogg Opus files (RFC 3533, RFC 7845), the format soundboard sounds are
//! stored in. The reader is strict, because the server checks what a client
//! uploads with it: every page's checksum, one logical stream, Opus with one
//! or two channels at 48 kHz, and a duration counted both from the final
//! granule position and from the packets themselves, so a file cannot claim
//! to be shorter than it plays.

const CAPTURE: &[u8; 4] = b"OggS";
const HEADER_LEN: usize = 27;
const CONTINUED: u8 = 0x01;
const FIRST: u8 = 0x02;
const LAST: u8 = 0x04;
/// Opus always decodes at 48 kHz; granule positions count its samples.
pub const SAMPLE_RATE: u32 = 48_000;
/// Opus allows no longer packet.
const MAX_PACKET_SAMPLES: u64 = 5_760;
/// Data put on one page before a new one starts (writer).
const PAGE_DATA: usize = 4_096;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum OggError {
    #[error("the file ends in the middle of a page")]
    Truncated,
    #[error("the file is not an Ogg stream")]
    NotOgg,
    #[error("a page's checksum is wrong")]
    Checksum,
    #[error("the file holds more than one stream")]
    Streams,
    #[error("pages are missing or out of order")]
    Sequence,
    #[error("the stream is not Opus")]
    NotOpus,
    #[error("Opus streams with {0} channels are not accepted")]
    Channels(u8),
    #[error("the sample rate must be 48 kHz, not {0} Hz")]
    SampleRate(u32),
    #[error("a packet is not valid Opus")]
    Packet,
    #[error("the stream's positions do not match its packets")]
    Granule,
    #[error("the stream has no end")]
    Unfinished,
}

/// What the headers and pages say about a stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpusInfo {
    pub channels: u8,
    /// Samples the decoder drops at the start.
    pub pre_skip: u16,
    /// Playing time from the final granule position.
    pub duration_ms: u32,
    /// Playing time if every packet's samples are played, which is what a
    /// decoder ignoring end trimming does; never shorter than `duration_ms`.
    pub decoded_ms: u32,
}

/// A checked stream: its header facts and its audio packets in order.
#[derive(Debug, Clone)]
pub struct OpusFile {
    pub info: OpusInfo,
    pub packets: Vec<Vec<u8>>,
    /// The final granule position: pre-skip plus the samples to play.
    pub final_granule: u64,
}

impl OpusFile {
    /// A stored sound: everything checked, the input sample rate included.
    pub fn read(bytes: &[u8]) -> Result<Self, OggError> {
        Self::read_with(bytes, true)
    }

    /// A file someone picked to make a sound from: the header's input
    /// sample rate is only what the audio was before encoding, so any is
    /// fine.
    pub fn read_any_rate(bytes: &[u8]) -> Result<Self, OggError> {
        Self::read_with(bytes, false)
    }

    fn read_with(bytes: &[u8], only_48_khz: bool) -> Result<Self, OggError> {
        let pages = pages(bytes)?;
        let mut packets = packets(&pages)?.into_iter();
        let head = packets.next().ok_or(OggError::NotOpus)?;
        let (channels, pre_skip) = opus_head(&head, only_48_khz)?;
        let tags = packets.next().ok_or(OggError::NotOpus)?;
        if !tags.starts_with(b"OpusTags") {
            return Err(OggError::NotOpus);
        }
        let packets: Vec<Vec<u8>> = packets.collect();
        let mut samples = 0u64;
        for packet in &packets {
            samples += packet_samples(packet)?;
        }
        let final_granule = pages.last().map_or(0, |page| page.granule);
        let pre_skip_samples = u64::from(pre_skip);
        if final_granule < pre_skip_samples || final_granule > samples {
            return Err(OggError::Granule);
        }
        Ok(Self {
            info: OpusInfo {
                channels,
                pre_skip,
                duration_ms: millis(final_granule - pre_skip_samples),
                decoded_ms: millis(samples.saturating_sub(pre_skip_samples)),
            },
            packets,
            final_granule,
        })
    }
}

fn millis(samples: u64) -> u32 {
    u32::try_from(samples.div_ceil(u64::from(SAMPLE_RATE / 1_000))).unwrap_or(u32::MAX)
}

struct Page<'a> {
    flags: u8,
    granule: u64,
    lacing: &'a [u8],
    data: &'a [u8],
}

/// Every page, checked: checksums, one stream, sequence, first and last.
fn pages(mut bytes: &[u8]) -> Result<Vec<Page<'_>>, OggError> {
    if !bytes.starts_with(CAPTURE) {
        return Err(OggError::NotOgg);
    }
    let mut pages: Vec<Page<'_>> = Vec::new();
    let mut serial = None;
    while !bytes.is_empty() {
        if bytes.len() < HEADER_LEN {
            return Err(OggError::Truncated);
        }
        if &bytes[..4] != CAPTURE || bytes[4] != 0 {
            return Err(OggError::NotOgg);
        }
        let segments = usize::from(bytes[26]);
        let header = HEADER_LEN + segments;
        if bytes.len() < header {
            return Err(OggError::Truncated);
        }
        let lacing = &bytes[HEADER_LEN..header];
        let length = header + lacing.iter().map(|&n| usize::from(n)).sum::<usize>();
        if bytes.len() < length {
            return Err(OggError::Truncated);
        }
        let (page, rest) = bytes.split_at(length);
        let stored = u32::from_le_bytes([page[22], page[23], page[24], page[25]]);
        if checksum(page) != stored {
            return Err(OggError::Checksum);
        }
        let flags = page[5];
        let this_serial = u32::from_le_bytes([page[14], page[15], page[16], page[17]]);
        let sequence = u32::from_le_bytes([page[18], page[19], page[20], page[21]]);
        if *serial.get_or_insert(this_serial) != this_serial {
            return Err(OggError::Streams);
        }
        let first = pages.is_empty();
        if (flags & FIRST != 0) != first {
            return Err(OggError::Streams);
        }
        if usize::try_from(sequence).ok() != Some(pages.len()) {
            return Err(OggError::Sequence);
        }
        if pages.last().is_some_and(|page| page.flags & LAST != 0) {
            return Err(OggError::Streams);
        }
        pages.push(Page {
            flags,
            granule: u64::from_le_bytes(page[6..14].try_into().unwrap_or_default()),
            lacing,
            data: &page[header..],
        });
        bytes = rest;
    }
    if pages.last().is_none_or(|page| page.flags & LAST == 0) {
        return Err(OggError::Unfinished);
    }
    Ok(pages)
}

/// The packets the pages carry, joined across page boundaries.
fn packets(pages: &[Page<'_>]) -> Result<Vec<Vec<u8>>, OggError> {
    let mut packets = Vec::new();
    let mut partial: Option<Vec<u8>> = None;
    for page in pages {
        if (page.flags & CONTINUED != 0) != partial.is_some() {
            return Err(OggError::Sequence);
        }
        let mut offset = 0;
        for &length in page.lacing {
            let length = usize::from(length);
            partial
                .get_or_insert_with(Vec::new)
                .extend_from_slice(&page.data[offset..offset + length]);
            offset += length;
            if length < 255 {
                packets.extend(partial.take());
            }
        }
    }
    if partial.is_some() {
        return Err(OggError::Unfinished);
    }
    Ok(packets)
}

/// Channels and pre-skip from the identification header (RFC 7845 §5.1).
fn opus_head(packet: &[u8], only_48_khz: bool) -> Result<(u8, u16), OggError> {
    if packet.len() < 19 || !packet.starts_with(b"OpusHead") || packet[8] >> 4 != 0 {
        return Err(OggError::NotOpus);
    }
    let channels = packet[9];
    let pre_skip = u16::from_le_bytes([packet[10], packet[11]]);
    let sample_rate = u32::from_le_bytes([packet[12], packet[13], packet[14], packet[15]]);
    let mapping_family = packet[18];
    if !(1..=2).contains(&channels) || mapping_family != 0 {
        return Err(OggError::Channels(channels));
    }
    if only_48_khz && sample_rate != SAMPLE_RATE {
        return Err(OggError::SampleRate(sample_rate));
    }
    Ok((channels, pre_skip))
}

/// A packet's samples at 48 kHz, from its table-of-contents byte
/// (RFC 6716 §3.1).
pub fn packet_samples(packet: &[u8]) -> Result<u64, OggError> {
    let toc = *packet.first().ok_or(OggError::Packet)?;
    let config = toc >> 3;
    let per_frame: u64 = match config {
        // SILK: 10, 20, 40, 60 ms.
        0..=11 => [480, 960, 1_920, 2_880][usize::from(config % 4)],
        // Hybrid: 10, 20 ms.
        12..=15 => [480, 960][usize::from(config % 2)],
        // CELT: 2.5, 5, 10, 20 ms.
        _ => [120, 240, 480, 960][usize::from(config % 4)],
    };
    let frames = match toc & 0x03 {
        0 => 1,
        1 | 2 => 2,
        _ => u64::from(packet.get(1).ok_or(OggError::Packet)? & 0x3F),
    };
    let samples = per_frame * frames;
    if frames == 0 || samples > MAX_PACKET_SAMPLES {
        return Err(OggError::Packet);
    }
    Ok(samples)
}

/// Writes Opus packets as an Ogg Opus file. `samples` is how many samples
/// to play after the pre-skip; the final page's granule position trims the
/// last packet to it.
pub fn write(channels: u8, pre_skip: u16, packets: &[Vec<u8>], samples: u64) -> Vec<u8> {
    let serial = 0x4F43_5244; // "OCRD"
    let mut writer = Writer {
        out: Vec::new(),
        serial,
        sequence: 0,
    };
    let mut head = b"OpusHead".to_vec();
    head.extend_from_slice(&[1, channels]);
    head.extend_from_slice(&pre_skip.to_le_bytes());
    head.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    head.extend_from_slice(&0i16.to_le_bytes());
    head.push(0);
    writer.page(FIRST, 0, &[&head]);
    let vendor = b"Opencord";
    let mut tags = b"OpusTags".to_vec();
    tags.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    tags.extend_from_slice(vendor);
    tags.extend_from_slice(&0u32.to_le_bytes());
    writer.page(0, 0, &[&tags]);

    let end = u64::from(pre_skip) + samples;
    let mut granule = u64::from(pre_skip);
    let mut page: Vec<&[u8]> = Vec::new();
    let mut size = 0;
    for (index, packet) in packets.iter().enumerate() {
        page.push(packet);
        size += packet.len();
        granule += packet_samples(packet).unwrap_or(0);
        let last = index + 1 == packets.len();
        let segments: usize = page.iter().map(|packet| packet.len() / 255 + 1).sum();
        if last || size >= PAGE_DATA || segments > 200 {
            let flags = if last { LAST } else { 0 };
            writer.page(flags, granule.min(end), &page);
            page.clear();
            size = 0;
        }
    }
    if packets.is_empty() {
        writer.page(LAST, end, &[]);
    }
    writer.out
}

struct Writer {
    out: Vec<u8>,
    serial: u32,
    sequence: u32,
}

impl Writer {
    /// One page holding whole packets.
    fn page(&mut self, flags: u8, granule: u64, packets: &[&[u8]]) {
        let mut lacing = Vec::new();
        for packet in packets {
            lacing.extend(std::iter::repeat_n(255u8, packet.len() / 255));
            lacing.push((packet.len() % 255) as u8);
        }
        let start = self.out.len();
        self.out.extend_from_slice(CAPTURE);
        self.out.extend_from_slice(&[0, flags]);
        self.out.extend_from_slice(&granule.to_le_bytes());
        self.out.extend_from_slice(&self.serial.to_le_bytes());
        self.out.extend_from_slice(&self.sequence.to_le_bytes());
        self.out.extend_from_slice(&[0; 4]);
        self.out.push(lacing.len() as u8);
        self.out.extend_from_slice(&lacing);
        for packet in packets {
            self.out.extend_from_slice(packet);
        }
        let crc = checksum(&self.out[start..]);
        self.out[start + 22..start + 26].copy_from_slice(&crc.to_le_bytes());
        self.sequence += 1;
    }
}

/// Ogg's CRC-32: polynomial 0x04C11DB7, no reflection, the checksum field
/// counted as zero.
fn checksum(page: &[u8]) -> u32 {
    let mut crc = 0u32;
    for (index, &byte) in page.iter().enumerate() {
        let byte = if (22..26).contains(&index) { 0 } else { byte };
        crc ^= u32::from(byte) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ 0x04C1_1DB7
            } else {
                crc << 1
            };
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 20 ms CELT fullband packet (config 31, one frame); the payload
    /// only needs to be there.
    fn packet(fill: u8) -> Vec<u8> {
        let mut packet = vec![31 << 3];
        packet.extend(std::iter::repeat_n(fill, 60));
        packet
    }

    fn file(seconds: f64) -> Vec<u8> {
        let samples = (seconds * f64::from(SAMPLE_RATE)) as u64;
        let packets: Vec<Vec<u8>> = (0..samples.div_ceil(960) + 1)
            .map(|n| packet(n as u8))
            .collect();
        write(2, 312, &packets, samples)
    }

    #[test]
    fn what_is_written_reads_back() {
        let bytes = file(4.99);
        let read = OpusFile::read(&bytes).unwrap();

        assert_eq!(read.info.channels, 2);
        assert_eq!(read.info.pre_skip, 312);
        assert_eq!(read.info.duration_ms, 4_990);
        assert!(read.info.decoded_ms >= read.info.duration_ms);
        assert_eq!(read.packets.len(), 251);
        assert_eq!(read.packets[3], packet(3));
    }

    #[test]
    fn a_long_packet_is_split_across_pages() {
        let big: Vec<Vec<u8>> = (0..3)
            .map(|n| {
                let mut packet = packet(n);
                packet.extend(std::iter::repeat_n(n, 5_000));
                packet
            })
            .collect();
        let read = OpusFile::read(&write(1, 0, &big, 2_880)).unwrap();

        assert_eq!(read.packets, big);
    }

    #[test]
    fn a_truncated_file_is_refused() {
        let bytes = file(1.0);

        for cut in [10, bytes.len() / 2, bytes.len() - 1] {
            let result = OpusFile::read(&bytes[..cut]);
            assert!(
                matches!(result, Err(OggError::Truncated | OggError::Unfinished)),
                "cut at {cut}: {result:?}"
            );
        }
    }

    #[test]
    fn a_changed_byte_fails_the_checksum() {
        let mut bytes = file(1.0);
        let middle = bytes.len() / 2;
        bytes[middle] ^= 0x40;

        assert_eq!(OpusFile::read(&bytes).unwrap_err(), OggError::Checksum);
    }

    #[test]
    fn only_48_khz_mono_or_stereo_opus_is_accepted() {
        let at_44 = rewrite_head(&file(1.0), |head| {
            head[12..16].copy_from_slice(&44_100u32.to_le_bytes());
        });
        assert_eq!(
            OpusFile::read(&at_44).unwrap_err(),
            OggError::SampleRate(44_100)
        );
        assert!(OpusFile::read_any_rate(&at_44).is_ok());

        let surround = rewrite_head(&file(1.0), |head| head[9] = 6);
        assert_eq!(
            OpusFile::read(&surround).unwrap_err(),
            OggError::Channels(6)
        );

        let vorbis = rewrite_head(&file(1.0), |head| head[..8].copy_from_slice(b"vorbisXX"));
        assert_eq!(OpusFile::read(&vorbis).unwrap_err(), OggError::NotOpus);
    }

    #[test]
    fn a_short_final_granule_cannot_hide_a_long_sound() {
        let samples = 6 * u64::from(SAMPLE_RATE);
        let packets: Vec<Vec<u8>> = (0..samples / 960).map(|n| packet(n as u8)).collect();
        // Claims one second; its packets play for six.
        let lying = write(2, 0, &packets, u64::from(SAMPLE_RATE));
        let read = OpusFile::read(&lying).unwrap();

        assert_eq!(read.info.duration_ms, 1_000);
        assert_eq!(read.info.decoded_ms, 6_000);
    }

    #[test]
    fn a_granule_past_the_packets_is_refused() {
        let packets: Vec<Vec<u8>> = (0..10).map(packet).collect();
        let bytes = write(2, 0, &packets, 10 * 960);
        // Ten packets of 20 ms, but the last page claims twenty.
        let forged = rewrite_last_granule(&bytes, 20 * 960);

        assert!(OpusFile::read(&bytes).is_ok());
        assert_eq!(OpusFile::read(&forged).unwrap_err(), OggError::Granule);
    }

    #[test]
    fn packet_lengths_come_from_the_toc() {
        // SILK 60 ms, hybrid 10 ms, CELT 2.5 ms, two CELT 20 ms frames,
        // code 3 with five 20 ms frames.
        assert_eq!(packet_samples(&[3 << 3]).unwrap(), 2_880);
        assert_eq!(packet_samples(&[12 << 3]).unwrap(), 480);
        assert_eq!(packet_samples(&[16 << 3]).unwrap(), 120);
        assert_eq!(packet_samples(&[(31 << 3) | 1]).unwrap(), 1_920);
        assert_eq!(packet_samples(&[(31 << 3) | 3, 5]).unwrap(), 4_800);
        // Seven 20 ms frames are 140 ms: more than Opus allows.
        assert_eq!(packet_samples(&[(31 << 3) | 3, 7]), Err(OggError::Packet));
        assert_eq!(packet_samples(&[]), Err(OggError::Packet));
    }

    /// Sets the last page's granule position and fixes its checksum.
    fn rewrite_last_granule(bytes: &[u8], granule: u64) -> Vec<u8> {
        let mut bytes = bytes.to_vec();
        let mut start = 0;
        loop {
            let header = HEADER_LEN + usize::from(bytes[start + 26]);
            let lacing = &bytes[start + HEADER_LEN..start + header];
            let length = header + lacing.iter().map(|&n| usize::from(n)).sum::<usize>();
            if start + length == bytes.len() {
                bytes[start + 6..start + 14].copy_from_slice(&granule.to_le_bytes());
                let crc = checksum(&bytes[start..]);
                bytes[start + 22..start + 26].copy_from_slice(&crc.to_le_bytes());
                return bytes;
            }
            start += length;
        }
    }

    /// Changes the identification header and fixes the first page's
    /// checksum, as a modified client would.
    fn rewrite_head(bytes: &[u8], change: impl Fn(&mut [u8])) -> Vec<u8> {
        let mut bytes = bytes.to_vec();
        let segments = usize::from(bytes[26]);
        let start = HEADER_LEN + segments;
        change(&mut bytes[start..start + 19]);
        let length = start + usize::from(bytes[HEADER_LEN]);
        let crc = checksum(&bytes[..length]);
        bytes[22..26].copy_from_slice(&crc.to_le_bytes());
        bytes
    }
}
