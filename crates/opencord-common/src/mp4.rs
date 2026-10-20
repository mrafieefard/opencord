//! The tracks of an MP4 file (ISO base media): whether it holds a picture.
//! Sound alone often comes under a video brand (`isom`, `mp42`), as phones
//! and FFmpeg write `.m4a` files, so the brand can't tell audio from video.

/// A box's header: its type, how long the header is, and the whole box's
/// size, or `None` when it runs to the end of the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoxHeader {
    pub kind: [u8; 4],
    pub header_len: u64,
    pub size: Option<u64>,
}

/// The header of the box `bytes` start with (16 bytes cover any header);
/// `None` when it is cut short or its size is smaller than itself.
pub fn box_header(bytes: &[u8]) -> Option<BoxHeader> {
    let size = u32::from_be_bytes(bytes.get(0..4)?.try_into().ok()?);
    let kind: [u8; 4] = bytes.get(4..8)?.try_into().ok()?;
    let (header_len, size) = match size {
        0 => (8, None),
        1 => (
            16,
            Some(u64::from_be_bytes(bytes.get(8..16)?.try_into().ok()?)),
        ),
        size => (8, Some(u64::from(size))),
    };
    if size.is_some_and(|size| size < header_len) {
        return None;
    }
    Some(BoxHeader {
        kind,
        header_len,
        size,
    })
}

/// The boxes inside a box's contents, as (type, contents); stops at the
/// first one that doesn't fit.
fn children(mut contents: &[u8]) -> impl Iterator<Item = ([u8; 4], &[u8])> {
    std::iter::from_fn(move || {
        let header = box_header(contents)?;
        let size = usize::try_from(header.size.unwrap_or(contents.len() as u64)).ok()?;
        let inner = contents.get(header.header_len as usize..size)?;
        contents = &contents[size..];
        Some((header.kind, inner))
    })
}

/// Whether the contents of a `moov` box hold a video track; `None` when
/// they hold no track that can be read.
pub fn has_video_track(moov: &[u8]) -> Option<bool> {
    let mut handlers = children(moov)
        .filter(|(kind, _)| kind == b"trak")
        .flat_map(|(_, trak)| children(trak))
        .filter(|(kind, _)| kind == b"mdia")
        .flat_map(|(_, mdia)| children(mdia))
        .filter(|(kind, _)| kind == b"hdlr")
        // Version and flags, then a reserved word, then the handler.
        .filter_map(|(_, hdlr)| hdlr.get(8..12))
        .peekable();
    handlers.peek()?;
    Some(handlers.any(|handler| handler == b"vide"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boxed(kind: &[u8; 4], contents: &[u8]) -> Vec<u8> {
        let mut out = (8 + contents.len() as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(contents);
        out
    }

    fn track(handler: &[u8; 4]) -> Vec<u8> {
        let mut hdlr = vec![0u8; 8];
        hdlr.extend_from_slice(handler);
        hdlr.extend_from_slice(&[0u8; 13]);
        let mdia = [boxed(b"mdhd", &[0u8; 24]), boxed(b"hdlr", &hdlr)].concat();
        boxed(
            b"trak",
            &[boxed(b"tkhd", &[0u8; 84]), boxed(b"mdia", &mdia)].concat(),
        )
    }

    #[test]
    fn a_picture_track_is_found_among_the_others() {
        let audio = [boxed(b"mvhd", &[0u8; 100]), track(b"soun")].concat();
        assert_eq!(has_video_track(&audio), Some(false));
        let video = [audio.clone(), track(b"vide")].concat();
        assert_eq!(has_video_track(&video), Some(true));
        assert_eq!(has_video_track(&boxed(b"mvhd", &[0u8; 100])), None);
    }

    #[test]
    fn headers_take_every_size_form() {
        let mut large = 1u32.to_be_bytes().to_vec();
        large.extend_from_slice(b"mdat");
        large.extend_from_slice(&(1u64 << 33).to_be_bytes());
        assert_eq!(
            box_header(&large),
            Some(BoxHeader {
                kind: *b"mdat",
                header_len: 16,
                size: Some(1 << 33),
            })
        );
        let to_the_end = [0, 0, 0, 0, b'm', b'd', b'a', b't'];
        assert_eq!(box_header(&to_the_end).unwrap().size, None);
        // Smaller than its own header: not a box.
        assert_eq!(box_header(&[0, 0, 0, 4, b'f', b'r', b'e', b'e']), None);
        assert_eq!(box_header(&[0, 0, 0]), None);
    }

    #[test]
    fn a_cut_short_track_is_not_read_past_its_end() {
        let mut moov = track(b"vide");
        moov.truncate(moov.len() - 10);
        assert_eq!(has_video_track(&moov), None);
    }
}
