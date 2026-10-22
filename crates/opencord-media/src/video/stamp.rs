//! A picture that says when it was made, for measuring how long video takes
//! from capture to the screen (Phase 2 plan §16.1). The virtual screen
//! draws the wall-clock time, in milliseconds, as a row of black and white
//! blocks across its top; whoever decodes the picture reads them back.
//! Blocks are a fraction of the picture's size, so they survive scaling and
//! H.264 at any layer.

/// Blocks across the picture: 32 bits of the time, then the low 8 bits
/// again inverted, which a misread breaks.
const BLOCKS: u32 = 40;
/// The band's height, as a share of the picture's.
const BAND: u32 = 16;

fn bits(milliseconds: u32) -> impl Iterator<Item = bool> {
    let check = !milliseconds & 0xff;
    (0..32)
        .map(move |bit| milliseconds >> (31 - bit) & 1 == 1)
        .chain((0..8).map(move |bit| check >> (7 - bit) & 1 == 1))
}

/// The wall-clock time now, in milliseconds, as the stamp carries it.
pub fn now_ms() -> u32 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u32)
}

/// Draws `milliseconds` into BGRx pixels, `stride` bytes a row.
pub fn draw_bgrx(pixels: &mut [u8], width: u32, height: u32, stride: usize, milliseconds: u32) {
    let band = (height / BAND).max(1);
    for (index, on) in bits(milliseconds).enumerate() {
        let shade = if on { 255 } else { 0 };
        let (left, right) = block(index as u32, width);
        for row in 0..band {
            let start = row as usize * stride + left as usize * 4;
            let end = row as usize * stride + right as usize * 4;
            if let Some(span) = pixels.get_mut(start..end) {
                span.fill(shade);
            }
        }
    }
}

/// The time a picture's luma plane (`width` bytes a row) carries, if it
/// carries one.
pub fn read_luma(luma: &[u8], width: u32, height: u32) -> Option<u32> {
    let row = (height / BAND / 2) as usize * width as usize;
    let mut read = 0u64;
    for index in 0..BLOCKS {
        let (left, right) = block(index, width);
        let middle = row + ((left + right) / 2) as usize;
        read = read << 1 | u64::from(*luma.get(middle)? > 128);
    }
    let milliseconds = (read >> 8) as u32;
    (read as u32 & 0xff == !milliseconds & 0xff).then_some(milliseconds)
}

/// Where block `index` starts and ends across `width`.
fn block(index: u32, width: u32) -> (u32, u32) {
    (index * width / BLOCKS, (index + 1) * width / BLOCKS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stamp_reads_back_and_a_plain_picture_has_none() {
        let (width, height) = (640u32, 360u32);
        let mut bgrx = vec![128u8; (width * height * 4) as usize];
        draw_bgrx(&mut bgrx, width, height, width as usize * 4, 1_234_567_890);
        // Luma from the blue byte is enough: the blocks are grey.
        let luma: Vec<u8> = bgrx.chunks(4).map(|pixel| pixel[0]).collect();

        assert_eq!(read_luma(&luma, width, height), Some(1_234_567_890));
        assert_eq!(read_luma(&vec![128; luma.len()], width, height), None);
    }
}
