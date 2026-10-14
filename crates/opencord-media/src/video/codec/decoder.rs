//! H.264 decoding (plan §7.9): FFmpeg's decoder, in hardware for large
//! layers where this computer has a device for it — VA-API on Linux,
//! D3D11VA on Windows (docs/plans/windows.md W3) — and in software
//! otherwise, so many small tiles do not use up hardware decoder sessions.
//! Every picture comes out as soon as its access unit is in.

use std::time::Instant;

use ff::ffi;
use ff::format::Pixel;
use ffmpeg_next as ff;

#[cfg(windows)]
use super::d3d11va as hardware_device;
#[cfg(target_os = "linux")]
use super::vaapi as hardware_device;
use super::{CodecError, annexb, ffmpeg};
use crate::video::picture::Picture;

/// The format a hardware-decoded picture arrives in, and the one
/// `prefer_hardware` asks the decoder for.
#[cfg(target_os = "linux")]
const SURFACE: ffi::AVPixelFormat = ffi::AVPixelFormat::AV_PIX_FMT_VAAPI;
#[cfg(windows)]
const SURFACE: ffi::AVPixelFormat = ffi::AVPixelFormat::AV_PIX_FMT_D3D11;

pub struct Decoder {
    decoder: ff::decoder::Video,
    hardware: bool,
    frame: ff::frame::Video,
}

impl Decoder {
    /// A decoder; in hardware if `hardware` and this computer has a device
    /// for it, in software otherwise.
    pub fn open(hardware: bool) -> Result<Self, CodecError> {
        ffmpeg::init();
        let codec = ff::decoder::find(ff::codec::Id::H264)
            .ok_or(CodecError::Unavailable("an H.264 decoder"))?;
        let mut context = ff::codec::context::Context::new_with_codec(codec);
        context.set_flags(ff::codec::Flags::LOW_DELAY);
        // Frame threads would hold pictures back.
        context.set_threading(ff::threading::Config::count(1));
        // Damage has to come back as an error, because that is what makes the
        // receiver ask the sender for a keyframe (`receiver.rs`); concealing
        // it silently leaves the viewer looking at rubbish until the next one
        // happens along.
        //
        // Only on Windows: Linux's FFmpeg already reports this damage, and
        // W3 must not change how Linux behaves. Measured here — truncating
        // every NAL unit of one picture decoded as `Ok` with one picture out,
        // and as `Invalid data found when processing input` with these flags
        // (docs/plans/windows.md W3).
        #[cfg(windows)]
        // SAFETY: a field of the context this decoder owns, set before it
        // opens.
        unsafe {
            (*context.as_mut_ptr()).err_recognition = ffi::AV_EF_EXPLODE | ffi::AV_EF_BITSTREAM;
        }
        let device = if hardware {
            hardware_device::device()
        } else {
            None
        };
        if let Some(device) = device {
            // SAFETY: fields of the context this decoder owns, set before it
            // opens; it takes the new device reference.
            unsafe {
                let raw = context.as_mut_ptr();
                (*raw).hw_device_ctx = device.reference();
                (*raw).get_format = Some(prefer_hardware);
            }
        }
        let decoder = context
            .decoder()
            .open_as(codec)
            .and_then(|opened| opened.video())
            .map_err(ffmpeg::error)?;
        Ok(Self {
            decoder,
            hardware: device.is_some(),
            frame: ff::frame::Video::empty(),
        })
    }

    pub fn is_hardware(&self) -> bool {
        self.hardware
    }

    /// Decodes one access unit; the pictures that came out of it, stamped
    /// `captured`. An error means the stream is damaged until the next
    /// keyframe.
    pub fn decode(
        &mut self,
        nal_units: &[impl AsRef<[u8]>],
        captured: Instant,
    ) -> Result<Vec<Picture>, CodecError> {
        let packet = ff::Packet::copy(&annexb::join(nal_units));
        self.decoder.send_packet(&packet).map_err(ffmpeg::error)?;
        let mut pictures = Vec::new();
        loop {
            match self.decoder.receive_frame(&mut self.frame) {
                Ok(()) => {
                    let picture = if self.frame.format() == Pixel::from(SURFACE) {
                        download(&self.frame).and_then(|frame| ffmpeg::from_frame(&frame, captured))
                    } else {
                        ffmpeg::from_frame(&self.frame, captured)
                    };
                    pictures.extend(picture);
                }
                Err(ff::Error::Other { errno }) if errno == ff::error::EAGAIN => break,
                Err(error) => return Err(ffmpeg::error(error)),
            }
        }
        Ok(pictures)
    }
}

/// A hardware surface's picture in memory.
fn download(surface: &ff::frame::Video) -> Option<ff::frame::Video> {
    let mut frame = ff::frame::Video::empty();
    // SAFETY: `frame` is empty, so FFmpeg allocates it in the surface's
    // software format; `surface` is a decoded hardware frame.
    let result = unsafe { ffi::av_hwframe_transfer_data(frame.as_mut_ptr(), surface.as_ptr(), 0) };
    (result >= 0).then_some(frame)
}

/// Picks this computer's hardware format when the decoder offers it,
/// FFmpeg's own choice otherwise.
unsafe extern "C" fn prefer_hardware(
    context: *mut ffi::AVCodecContext,
    formats: *const ffi::AVPixelFormat,
) -> ffi::AVPixelFormat {
    // SAFETY: FFmpeg passes a list ended by AV_PIX_FMT_NONE.
    unsafe {
        let mut at = formats;
        while *at != ffi::AVPixelFormat::AV_PIX_FMT_NONE {
            if *at == SURFACE {
                return *at;
            }
            at = at.add(1);
        }
        ffi::avcodec_default_get_format(context, formats)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::codec::encoder::{Backend, Encoder, EncoderConfig};
    use crate::video::pattern::scene;
    use crate::video::picture::PixelFormat;

    /// The pictures encoded, and each one's NAL units.
    type Stream = (Vec<Picture>, Vec<Vec<Vec<u8>>>);

    fn encoded(width: u32, height: u32, count: u64) -> Option<Stream> {
        let config = EncoderConfig {
            width,
            height,
            fps: 30,
            bitrate: 1_500_000,
        };
        let Ok(mut encoder) = Encoder::open(config, &Backend::ALL) else {
            eprintln!("no encoder here");
            return None;
        };
        let start = Instant::now();
        let mut sources = Vec::new();
        let mut units = Vec::new();
        for number in 0..count {
            let picture = scene(width, height, number, start);
            if let Some(encoded) = encoder.encode(&picture, number == 0).unwrap() {
                sources.push(picture);
                units.push(encoded.nal_units);
            }
        }
        Some((sources, units))
    }

    /// Peak signal-to-noise ratio of the luma planes, in dB.
    fn psnr(a: &Picture, b: &Picture) -> f64 {
        let error: f64 = a
            .y()
            .iter()
            .zip(b.y())
            .map(|(x, y)| (f64::from(*x) - f64::from(*y)).powi(2))
            .sum::<f64>()
            / a.y().len() as f64;
        10.0 * (255.0 * 255.0 / error.max(1e-9)).log10()
    }

    #[test]
    fn software_decoding_gives_back_each_picture_at_once() {
        let Some((sources, units)) = encoded(640, 360, 30) else {
            return;
        };
        let mut decoder = Decoder::open(false).unwrap();

        for (source, access_unit) in sources.iter().zip(&units) {
            let pictures = decoder.decode(access_unit, source.captured).unwrap();

            assert_eq!(pictures.len(), 1, "each picture at once");
            let picture = &pictures[0];
            assert_eq!((picture.width, picture.height), (640, 360));
            assert_eq!(picture.format, PixelFormat::I420);
            let quality = psnr(picture, source);
            assert!(quality > 30.0, "{quality:.1} dB");
        }
        assert!(!decoder.is_hardware());
    }

    #[test]
    fn hardware_decoding_gives_nv12_pictures_where_there_is_a_device() {
        let Some((sources, units)) = encoded(1280, 720, 10) else {
            return;
        };
        let mut decoder = Decoder::open(true).unwrap();
        if !decoder.is_hardware() {
            eprintln!("no hardware decoding here");
            return;
        }

        for (source, access_unit) in sources.iter().zip(&units) {
            let pictures = decoder.decode(access_unit, source.captured).unwrap();

            assert_eq!(pictures.len(), 1);
            assert_eq!(pictures[0].format, PixelFormat::Nv12);
            assert!(psnr(&pictures[0], source) > 30.0);
        }
    }

    #[test]
    fn nothing_comes_out_before_a_keyframe_and_damage_is_survived() {
        let Some((sources, units)) = encoded(320, 180, 12) else {
            return;
        };
        let mut decoder = Decoder::open(false).unwrap();
        let now = Instant::now();

        // Joining after the keyframe: nothing to show yet.
        for access_unit in &units[1..4] {
            let pictures = decoder.decode(access_unit, now).unwrap_or_default();
            assert!(pictures.is_empty());
        }
        // Garbage is an error or nothing, never a panic.
        let garbage = vec![vec![0x65, 0xff, 0x00, 0x13, 0x37]];
        assert!(decoder.decode(&garbage, now).unwrap_or_default().is_empty());
        // A keyframe brings it back.
        let pictures = decoder.decode(&units[0], now).unwrap();
        assert_eq!(pictures.len(), 1);
        assert!(psnr(&pictures[0], &sources[0]) > 30.0);
    }
}
