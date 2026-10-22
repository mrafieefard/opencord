//! H.264 byte streams (ITU-T H.264 Annex B): NAL units behind start codes.
//! Encoders write them; the transport carries bare NAL units.

/// The NAL units in a byte stream, without their start codes. A NAL unit
/// never ends in a zero byte, so zeros before a start code are padding.
pub fn split(stream: &[u8]) -> Vec<Vec<u8>> {
    let mut units = Vec::new();
    let mut start = None;
    let mut index = 0;
    while index + 3 <= stream.len() {
        if stream[index] == 0 && stream[index + 1] == 0 && stream[index + 2] == 1 {
            if let Some(begin) = start {
                push_unit(&mut units, &stream[begin..index]);
            }
            index += 3;
            start = Some(index);
        } else {
            index += 1;
        }
    }
    if let Some(begin) = start {
        push_unit(&mut units, &stream[begin..]);
    }
    units
}

fn push_unit(units: &mut Vec<Vec<u8>>, unit: &[u8]) {
    let end = unit
        .iter()
        .rposition(|&byte| byte != 0)
        .map_or(0, |at| at + 1);
    if end > 0 {
        units.push(unit[..end].to_vec());
    }
}

/// A byte stream of these NAL units, each behind a four-byte start code.
pub fn join(units: &[impl AsRef<[u8]>]) -> Vec<u8> {
    let length: usize = units.iter().map(|unit| unit.as_ref().len() + 4).sum();
    let mut stream = Vec::with_capacity(length);
    for unit in units {
        stream.extend_from_slice(&[0, 0, 0, 1]);
        stream.extend_from_slice(unit.as_ref());
    }
    stream
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_are_split_at_three_and_four_byte_start_codes() {
        let stream = [
            0, 0, 0, 1, 0x67, 1, 2, //
            0, 0, 1, 0x68, 3, //
            0, 0, 0, 1, 0x65, 4, 5, 6,
        ];

        assert_eq!(
            split(&stream),
            vec![vec![0x67, 1, 2], vec![0x68, 3], vec![0x65, 4, 5, 6]]
        );
    }

    #[test]
    fn trailing_zeros_before_a_start_code_are_not_part_of_a_unit() {
        let stream = [0, 0, 1, 0x41, 7, 0, 0, 0, 0, 0, 1, 0x41, 8];

        assert_eq!(split(&stream), vec![vec![0x41, 7], vec![0x41, 8]]);
    }

    #[test]
    fn emulation_prevention_bytes_stay_in_place() {
        let stream = [0, 0, 1, 0x41, 0, 0, 3, 1, 9];

        assert_eq!(split(&stream), vec![vec![0x41, 0, 0, 3, 1, 9]]);
    }

    #[test]
    fn nothing_before_the_first_start_code_is_a_unit() {
        assert_eq!(split(&[9, 9, 0, 0, 1, 0x41]), vec![vec![0x41]]);
        assert!(split(&[]).is_empty());
        assert!(split(&[0, 0, 1]).is_empty());
    }

    #[test]
    fn joined_units_split_back_into_the_same_units() {
        let units = vec![vec![0x67, 1], vec![0x68, 2, 0], vec![0x65, 3]];

        let stream = join(&units);

        assert_eq!(&stream[..4], &[0, 0, 0, 1]);
        assert_eq!(
            split(&stream),
            vec![vec![0x67, 1], vec![0x68, 2], vec![0x65, 3]]
        );
    }
}
