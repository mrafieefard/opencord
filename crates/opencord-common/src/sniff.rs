//! Magic-byte file sniffing.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sniffed {
    Png,
    Jpeg,
    Gif,
    WebP,
    Mp4,
    QuickTime,
    WebM,
    Matroska,
    M4a,
    Ogg,
    Mp3,
    Flac,
    Wav,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SniffKind {
    Image,
    Video,
    Audio,
    File,
}

impl Sniffed {
    pub fn content_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Gif => "image/gif",
            Self::WebP => "image/webp",
            Self::Mp4 => "video/mp4",
            Self::QuickTime => "video/quicktime",
            Self::WebM => "video/webm",
            Self::Matroska => "video/x-matroska",
            Self::M4a => "audio/mp4",
            Self::Ogg => "audio/ogg",
            Self::Mp3 => "audio/mpeg",
            Self::Flac => "audio/flac",
            Self::Wav => "audio/wav",
            Self::Unknown => "application/octet-stream",
        }
    }

    pub fn kind(self) -> SniffKind {
        match self {
            Self::Png | Self::Jpeg | Self::Gif | Self::WebP => SniffKind::Image,
            Self::QuickTime | Self::Mp4 | Self::WebM | Self::Matroska => SniffKind::Video,
            Self::M4a | Self::Ogg | Self::Mp3 | Self::Flac | Self::Wav => SniffKind::Audio,
            Self::Unknown => SniffKind::File,
        }
    }
}

pub fn sniff(head: &[u8]) -> Sniffed {
    // 1. starts 89 50 4E 47 0D 0A 1A 0A -> Png
    if head.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Sniffed::Png;
    }

    // 2. starts FF D8 FF -> Jpeg
    if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Sniffed::Jpeg;
    }

    // 3. starts GIF87a or GIF89a -> Gif
    if head.starts_with(b"GIF87a") || head.starts_with(b"GIF89a") {
        return Sniffed::Gif;
    }

    // 4. [0..4]=RIFF and [8..12]=WEBP -> WebP
    if head.len() >= 12 && &head[0..4] == b"RIFF" && &head[8..12] == b"WEBP" {
        return Sniffed::WebP;
    }

    // 5. [0..4]=RIFF and [8..12]=WAVE -> Wav
    if head.len() >= 12 && &head[0..4] == b"RIFF" && &head[8..12] == b"WAVE" {
        return Sniffed::Wav;
    }

    // 6. [4..8]=ftyp -> M4a / QuickTime / Mp4
    if head.len() >= 12 && &head[4..8] == b"ftyp" {
        let brand = &head[8..12];
        if matches!(brand, b"M4A " | b"M4B " | b"M4P " | b"F4A ") {
            return Sniffed::M4a;
        }
        if brand == b"qt  " {
            return Sniffed::QuickTime;
        }
        return Sniffed::Mp4;
    }

    // 7. starts 1A 45 DF A3 and webm appears in first 64 bytes -> WebM; else Matroska
    if head.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        let search_len = head.len().min(64);
        if head[..search_len].windows(4).any(|w| w == b"webm") {
            return Sniffed::WebM;
        }
        return Sniffed::Matroska;
    }

    // 8. starts OggS -> Ogg
    if head.starts_with(b"OggS") {
        return Sniffed::Ogg;
    }

    // 9. starts fLaC -> Flac
    if head.starts_with(b"fLaC") {
        return Sniffed::Flac;
    }

    // 10. starts ID3, or b[0]==0xFF && b[1]&0xE0==0xE0 && b[1]&0x06!=0 -> Mp3
    if head.starts_with(b"ID3")
        || (head.len() >= 2 && head[0] == 0xFF && (head[1] & 0xE0) == 0xE0 && (head[1] & 0x06) != 0)
    {
        return Sniffed::Mp3;
    }

    // 11. anything else -> Unknown
    Sniffed::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_rows_sniff_correctly() {
        // Png
        let png = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        assert_eq!(sniff(&png), Sniffed::Png);
        assert_eq!(Sniffed::Png.content_type(), "image/png");
        assert_eq!(Sniffed::Png.kind(), SniffKind::Image);

        // Jpeg
        let jpeg = [0xFF, 0xD8, 0xFF, 0xE0];
        assert_eq!(sniff(&jpeg), Sniffed::Jpeg);
        assert_eq!(Sniffed::Jpeg.content_type(), "image/jpeg");
        assert_eq!(Sniffed::Jpeg.kind(), SniffKind::Image);

        // Gif
        assert_eq!(sniff(b"GIF87a..."), Sniffed::Gif);
        assert_eq!(sniff(b"GIF89a..."), Sniffed::Gif);
        assert_eq!(Sniffed::Gif.content_type(), "image/gif");
        assert_eq!(Sniffed::Gif.kind(), SniffKind::Image);

        // WebP
        let mut webp = [0u8; 16];
        webp[0..4].copy_from_slice(b"RIFF");
        webp[8..12].copy_from_slice(b"WEBP");
        assert_eq!(sniff(&webp), Sniffed::WebP);
        assert_eq!(Sniffed::WebP.content_type(), "image/webp");
        assert_eq!(Sniffed::WebP.kind(), SniffKind::Image);

        // Wav
        let mut wav = [0u8; 16];
        wav[0..4].copy_from_slice(b"RIFF");
        wav[8..12].copy_from_slice(b"WAVE");
        assert_eq!(sniff(&wav), Sniffed::Wav);
        assert_eq!(Sniffed::Wav.content_type(), "audio/wav");
        assert_eq!(Sniffed::Wav.kind(), SniffKind::Audio);

        // M4a brands
        for brand in [b"M4A ", b"M4B ", b"M4P ", b"F4A "] {
            let mut m4a = [0u8; 16];
            m4a[4..8].copy_from_slice(b"ftyp");
            m4a[8..12].copy_from_slice(brand);
            assert_eq!(sniff(&m4a), Sniffed::M4a);
        }
        assert_eq!(Sniffed::M4a.content_type(), "audio/mp4");
        assert_eq!(Sniffed::M4a.kind(), SniffKind::Audio);

        // QuickTime
        let mut qt = [0u8; 16];
        qt[4..8].copy_from_slice(b"ftyp");
        qt[8..12].copy_from_slice(b"qt  ");
        assert_eq!(sniff(&qt), Sniffed::QuickTime);
        assert_eq!(Sniffed::QuickTime.content_type(), "video/quicktime");
        assert_eq!(Sniffed::QuickTime.kind(), SniffKind::Video);

        // Mp4 (other brand)
        let mut mp4 = [0u8; 16];
        mp4[4..8].copy_from_slice(b"ftyp");
        mp4[8..12].copy_from_slice(b"isom");
        assert_eq!(sniff(&mp4), Sniffed::Mp4);
        assert_eq!(Sniffed::Mp4.content_type(), "video/mp4");
        assert_eq!(Sniffed::Mp4.kind(), SniffKind::Video);

        // WebM
        let mut webm = vec![0x1A, 0x45, 0xDF, 0xA3];
        webm.extend_from_slice(b"....webm....");
        assert_eq!(sniff(&webm), Sniffed::WebM);
        assert_eq!(Sniffed::WebM.content_type(), "video/webm");
        assert_eq!(Sniffed::WebM.kind(), SniffKind::Video);

        // Matroska
        let matroska = [0x1A, 0x45, 0xDF, 0xA3, 0x00, 0x00];
        assert_eq!(sniff(&matroska), Sniffed::Matroska);
        assert_eq!(Sniffed::Matroska.content_type(), "video/x-matroska");
        assert_eq!(Sniffed::Matroska.kind(), SniffKind::Video);

        // Ogg
        assert_eq!(sniff(b"OggS...."), Sniffed::Ogg);
        assert_eq!(Sniffed::Ogg.content_type(), "audio/ogg");
        assert_eq!(Sniffed::Ogg.kind(), SniffKind::Audio);

        // Flac
        assert_eq!(sniff(b"fLaC...."), Sniffed::Flac);
        assert_eq!(Sniffed::Flac.content_type(), "audio/flac");
        assert_eq!(Sniffed::Flac.kind(), SniffKind::Audio);

        // Mp3
        assert_eq!(sniff(b"ID3...."), Sniffed::Mp3);
        let mp3_sync = [0xFF, 0xFB, 0x90, 0x64];
        assert_eq!(sniff(&mp3_sync), Sniffed::Mp3);
        assert_eq!(Sniffed::Mp3.content_type(), "audio/mpeg");
        assert_eq!(Sniffed::Mp3.kind(), SniffKind::Audio);

        // Unknown
        assert_eq!(sniff(b"random text file"), Sniffed::Unknown);
        assert_eq!(Sniffed::Unknown.content_type(), "application/octet-stream");
        assert_eq!(Sniffed::Unknown.kind(), SniffKind::File);
    }

    #[test]
    fn short_input_is_unknown_and_empty_does_not_panic() {
        assert_eq!(sniff(&[]), Sniffed::Unknown);
        assert_eq!(sniff(b"a"), Sniffed::Unknown);
        assert_eq!(sniff(b"ab"), Sniffed::Unknown);
        assert_eq!(sniff(b"abc"), Sniffed::Unknown);
    }
}
