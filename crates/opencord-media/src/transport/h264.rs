//! H.264 over RTP (RFC 6184), packetization mode 1: a NAL unit that fits
//! goes in a packet of its own, a larger one in FU-A fragments. It works on
//! lists of NAL units rather than an Annex B stream, so a frame transform
//! (plan §13) can never create a start code the packetizer would split on.

/// NAL unit types of the RTP payload format.
const STAP_A: u8 = 24;
const FU_A: u8 = 28;
const TYPE_BITS: u8 = 0x1f;
/// The forbidden bit and NRI of a NAL header.
const HEADER_BITS: u8 = 0xe0;
const FU_START: u8 = 0x80;
const FU_END: u8 = 0x40;
/// FU indicator and FU header.
const FU_OVERHEAD: usize = 2;

/// The RTP payloads for one access unit, in order: each NAL unit whole when
/// it fits in `max_payload` bytes, in FU-A fragments otherwise.
pub fn packetize(nal_units: &[impl AsRef<[u8]>], max_payload: usize) -> Vec<Vec<u8>> {
    let mut packets = Vec::new();
    for unit in nal_units {
        let unit = unit.as_ref();
        let Some((&header, body)) = unit.split_first() else {
            continue;
        };
        if unit.len() <= max_payload {
            packets.push(unit.to_vec());
            continue;
        }
        let chunk = max_payload.saturating_sub(FU_OVERHEAD).max(1);
        let indicator = (header & HEADER_BITS) | FU_A;
        let count = body.len().div_ceil(chunk);
        for (index, piece) in body.chunks(chunk).enumerate() {
            let mut fu_header = header & TYPE_BITS;
            if index == 0 {
                fu_header |= FU_START;
            }
            if index + 1 == count {
                fu_header |= FU_END;
            }
            let mut packet = Vec::with_capacity(FU_OVERHEAD + piece.len());
            packet.extend([indicator, fu_header]);
            packet.extend_from_slice(piece);
            packets.push(packet);
        }
    }
    packets
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DepacketizeError {
    #[error("an empty RTP payload")]
    Empty,
    #[error("a fragmented NAL unit is incomplete")]
    Fragment,
    #[error("an aggregation packet is malformed")]
    Aggregation,
    #[error("NAL unit type {0} is not used in packetization mode 1")]
    Unsupported(u8),
}

/// The NAL units of one access unit, from its RTP payloads in sequence
/// order. Whether packets are missing is for the caller to know from the
/// sequence numbers; fragments carry no counter.
pub fn depacketize<'a>(
    payloads: impl Iterator<Item = &'a [u8]>,
) -> Result<Vec<Vec<u8>>, DepacketizeError> {
    let mut units = Vec::new();
    let mut fragment: Option<Vec<u8>> = None;
    for payload in payloads {
        let (&first, rest) = payload.split_first().ok_or(DepacketizeError::Empty)?;
        match first & TYPE_BITS {
            1..=23 => {
                if fragment.is_some() {
                    return Err(DepacketizeError::Fragment);
                }
                units.push(payload.to_vec());
            }
            STAP_A => {
                if fragment.is_some() {
                    return Err(DepacketizeError::Fragment);
                }
                aggregated(rest, &mut units)?;
            }
            FU_A => {
                let (&fu_header, piece) = rest.split_first().ok_or(DepacketizeError::Fragment)?;
                if fu_header & FU_START != 0 {
                    if fragment.is_some() {
                        return Err(DepacketizeError::Fragment);
                    }
                    fragment = Some(vec![(first & HEADER_BITS) | (fu_header & TYPE_BITS)]);
                }
                let unit = fragment.as_mut().ok_or(DepacketizeError::Fragment)?;
                unit.extend_from_slice(piece);
                if fu_header & FU_END != 0 {
                    units.extend(fragment.take());
                }
            }
            other => return Err(DepacketizeError::Unsupported(other)),
        }
    }
    if fragment.is_some() {
        return Err(DepacketizeError::Fragment);
    }
    Ok(units)
}

/// The NAL units of a STAP-A payload: each one after its 16-bit size.
fn aggregated(mut rest: &[u8], units: &mut Vec<Vec<u8>>) -> Result<(), DepacketizeError> {
    while !rest.is_empty() {
        let [high, low, tail @ ..] = rest else {
            return Err(DepacketizeError::Aggregation);
        };
        let size = usize::from(u16::from_be_bytes([*high, *low]));
        if size == 0 || size > tail.len() {
            return Err(DepacketizeError::Aggregation);
        }
        let (unit, after) = tail.split_at(size);
        units.push(unit.to_vec());
        rest = after;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX: usize = 1080;

    fn nal(header: u8, length: usize) -> Vec<u8> {
        let mut unit = vec![header];
        unit.extend((1..length).map(|i| (i * 7 % 251) as u8));
        unit
    }

    #[test]
    fn small_nal_units_go_whole_one_per_packet() {
        let units = [nal(0x67, 10), nal(0x68, 5), nal(0x65, 500)];

        let packets = packetize(&units, MAX);

        assert_eq!(packets, units.to_vec());
    }

    #[test]
    fn a_large_nal_unit_goes_in_fu_a_fragments() {
        let unit = nal(0x65, 3000);

        let packets = packetize(std::slice::from_ref(&unit), MAX);

        assert_eq!(packets.len(), 3);
        for (index, packet) in packets.iter().enumerate() {
            assert!(packet.len() <= MAX);
            // F and NRI from the NAL header, type 28 (FU-A).
            assert_eq!(packet[0], 0x60 | 28);
            let start = index == 0;
            let end = index == packets.len() - 1;
            assert_eq!(packet[1] & 0x80 != 0, start, "S bit of {index}");
            assert_eq!(packet[1] & 0x40 != 0, end, "E bit of {index}");
            assert_eq!(packet[1] & 0x1f, 5);
        }
        let body: Vec<u8> = packets.iter().flat_map(|p| p[2..].to_vec()).collect();
        assert_eq!(body, unit[1..]);
    }

    #[test]
    fn depacketizing_gives_back_the_nal_units() {
        let units = vec![
            nal(0x67, 12),
            nal(0x68, 4),
            nal(0x65, MAX),
            nal(0x65, MAX + 1),
            nal(0x41, 5000),
            nal(0x06, 1),
        ];

        let packets = packetize(&units, MAX);
        let back = depacketize(packets.iter().map(Vec::as_slice));

        assert_eq!(back, Ok(units));
    }

    #[test]
    fn depacketizing_reads_aggregation_packets() {
        let sps = nal(0x67, 6);
        let pps = nal(0x68, 3);
        let mut stap = vec![0x78];
        for unit in [&sps, &pps] {
            stap.extend((unit.len() as u16).to_be_bytes());
            stap.extend(unit.iter());
        }
        let idr = nal(0x65, 40);

        let back = depacketize([stap.as_slice(), idr.as_slice()].into_iter());

        assert_eq!(back, Ok(vec![sps, pps, idr]));
    }

    #[test]
    fn depacketizing_refuses_broken_packets() {
        let fragments = packetize(&[nal(0x65, 3000)], MAX);
        let missing_start = [fragments[1].as_slice(), fragments[2].as_slice()];
        let missing_end = [fragments[0].as_slice(), fragments[1].as_slice()];
        let short_stap = [0x78u8, 0x00, 0x09, 0x67];

        assert_eq!(
            depacketize(missing_start.into_iter()),
            Err(DepacketizeError::Fragment)
        );
        assert_eq!(
            depacketize(missing_end.into_iter()),
            Err(DepacketizeError::Fragment)
        );
        assert_eq!(
            depacketize([short_stap.as_slice()].into_iter()),
            Err(DepacketizeError::Aggregation)
        );
        assert_eq!(
            depacketize([[].as_slice()].into_iter()),
            Err(DepacketizeError::Empty)
        );
        assert_eq!(
            depacketize([[0x7d, 1, 2].as_slice()].into_iter()),
            Err(DepacketizeError::Unsupported(29))
        );
    }
}
