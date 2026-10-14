//! H.264 decoding through VideoToolbox (docs/plans/macos.md M3): the SPS
//! and PPS from the stream make a format description and a session, which
//! decodes each access unit synchronously into NV12 at video range. A new
//! SPS (a screen share changing size) makes a new session.

use std::ffi::c_void;
use std::sync::Mutex;
use std::time::Instant;

use objc2_core_foundation::{CFBoolean, CFNumber, CFType};

use super::super::{CodecError, NAL_PPS, NAL_SPS, nal_type};
use super::ffi::{self, CMTime, Locked, Owned, Ref};
use crate::video::picture::{Picture, PixelFormat};

/// NAL unit types the decoder leaves out: access unit delimiters.
const NAL_AUD: u8 = 9;

/// What the output callback works with: the stamp to give pictures (the
/// access unit's), and the pictures, or the status that stopped one.
#[derive(Default)]
struct Outputs {
    captured: Mutex<Option<Instant>>,
    pictures: Mutex<Vec<Result<Picture, ffi::OSStatus>>>,
}

/// A session for one SPS and PPS.
struct Session {
    session: Owned,
    _format: Owned,
    sps: Vec<u8>,
    pps: Vec<u8>,
}

impl Drop for Session {
    fn drop(&mut self) {
        // SAFETY: a live session; nothing calls back after this.
        unsafe { ffi::VTDecompressionSessionInvalidate(self.session.get()) };
    }
}

pub struct Decoder {
    hardware: bool,
    session: Option<Session>,
    /// The parameter sets seen last, for the next session.
    sps: Option<Vec<u8>>,
    pps: Option<Vec<u8>>,
    /// Where the callback puts pictures; boxed, so its address stays put
    /// for the sessions.
    outputs: Box<Outputs>,
}

impl Decoder {
    /// A decoder; VideoToolbox's hardware decoder if `hardware` (and there
    /// is one), whichever VideoToolbox picks otherwise.
    pub fn open(hardware: bool) -> Result<Self, CodecError> {
        Ok(Self {
            hardware,
            session: None,
            sps: None,
            pps: None,
            outputs: Box::default(),
        })
    }

    /// Whether the hardware decodes: what VideoToolbox says once a session
    /// is open, what was asked until then.
    pub fn is_hardware(&self) -> bool {
        match &self.session {
            // SAFETY: VideoToolbox's constant key.
            Some(session) => ffi::bool_property(session.session.get(), unsafe {
                ffi::kVTDecompressionPropertyKey_UsingHardwareAcceleratedVideoDecoder
            }),
            None => self.hardware,
        }
    }

    /// Decodes one access unit (Annex B NAL units); the pictures that came
    /// out of it, stamped `captured`. An error means the stream is damaged
    /// until the next keyframe.
    pub fn decode(
        &mut self,
        nal_units: &[impl AsRef<[u8]>],
        captured: Instant,
    ) -> Result<Vec<Picture>, CodecError> {
        let mut sample = Vec::new();
        for unit in nal_units {
            let unit = unit.as_ref();
            match nal_type(unit) {
                Some(NAL_SPS) => self.sps = Some(unit.to_vec()),
                Some(NAL_PPS) => self.pps = Some(unit.to_vec()),
                Some(NAL_AUD) | None => {}
                Some(_) => {
                    let length = u32::try_from(unit.len()).map_err(|_| CodecError::WrongPicture)?;
                    sample.extend_from_slice(&length.to_be_bytes());
                    sample.extend_from_slice(unit);
                }
            }
        }
        self.renew_session()?;
        let Some(session) = &self.session else {
            // Nothing to decode with before the first keyframe.
            return Ok(Vec::new());
        };
        if sample.is_empty() {
            return Ok(Vec::new());
        }
        *self
            .outputs
            .captured
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(captured);
        // SAFETY: the format description lives as long as the session.
        let buffer = unsafe { sample_buffer(&sample, session._format.get()) }
            .ok_or(CodecError::Unavailable("a sample buffer"))?;
        let mut flags = 0u32;
        // SAFETY: a live session and sample buffer; without asynchronous
        // decoding the callback runs before this returns.
        let status = unsafe {
            let status = ffi::VTDecompressionSessionDecodeFrame(
                session.session.get(),
                buffer.get(),
                0,
                std::ptr::null_mut(),
                &mut flags,
            );
            ffi::VTDecompressionSessionWaitForAsynchronousFrames(session.session.get());
            status
        };
        let outputs = std::mem::take(
            &mut *self
                .outputs
                .pictures
                .lock()
                .unwrap_or_else(|e| e.into_inner()),
        );
        if status != 0 {
            tracing::debug!(status, "VideoToolbox did not decode an access unit");
            return Err(CodecError::Unavailable("VideoToolbox's decoder"));
        }
        // Without reordering every access unit gives a picture; one that
        // gives none was damaged (VideoToolbox conceals it without a word).
        if outputs.is_empty() {
            tracing::debug!(flags, "VideoToolbox gave no picture for an access unit");
            return Err(CodecError::Unavailable("VideoToolbox's decoder"));
        }
        outputs
            .into_iter()
            .map(|output| {
                output.map_err(|status| {
                    tracing::debug!(status, "VideoToolbox did not decode a picture");
                    CodecError::Unavailable("VideoToolbox's decoder")
                })
            })
            .collect()
    }

    /// A session for the parameter sets seen last, if they changed.
    fn renew_session(&mut self) -> Result<(), CodecError> {
        let (Some(sps), Some(pps)) = (&self.sps, &self.pps) else {
            return Ok(());
        };
        if self
            .session
            .as_ref()
            .is_some_and(|session| session.sps == *sps && session.pps == *pps)
        {
            return Ok(());
        }
        self.session = None;
        let format = format_description(sps, pps)?;
        let session = match self.open_session(&format, self.hardware) {
            Ok(session) => session,
            // Hardware asked but not to be had: VideoToolbox's choice.
            Err(status) if self.hardware => {
                tracing::debug!(status, "no hardware decoder; trying VideoToolbox's choice");
                self.open_session(&format, false).map_err(|status| {
                    tracing::debug!(status, "no decoder session");
                    CodecError::Unavailable("VideoToolbox's decoder")
                })?
            }
            Err(status) => {
                tracing::debug!(status, "no decoder session");
                return Err(CodecError::Unavailable("VideoToolbox's decoder"));
            }
        };
        self.session = Some(Session {
            session,
            _format: format,
            sps: sps.clone(),
            pps: pps.clone(),
        });
        Ok(())
    }

    fn open_session(&self, format: &Owned, hardware: bool) -> Result<Owned, ffi::OSStatus> {
        let yes: &CFType = CFBoolean::new(true).as_ref();
        // SAFETY: VideoToolbox's constant keys.
        let specification = unsafe {
            ffi::dictionary(&[if hardware {
                (
                    ffi::kVTVideoDecoderSpecification_RequireHardwareAcceleratedVideoDecoder,
                    yes,
                )
            } else {
                (
                    ffi::kVTVideoDecoderSpecification_EnableHardwareAcceleratedVideoDecoder,
                    yes,
                )
            }])
        };
        let nv12 = CFNumber::new_i32(ffi::NV12_VIDEO_RANGE as i32);
        // SAFETY: Core Video's constant key.
        let destination =
            unsafe { ffi::dictionary(&[(ffi::kCVPixelBufferPixelFormatTypeKey, nv12.as_ref())]) };
        let record = ffi::DecompressionOutputRecord {
            callback: on_decoded,
            refcon: std::ptr::from_ref::<Outputs>(&self.outputs)
                .cast_mut()
                .cast(),
        };
        let mut session: Ref = std::ptr::null_mut();
        // SAFETY: the callback gets the boxed `Outputs`, which the decoder
        // keeps for as long as its sessions.
        let status = unsafe {
            ffi::VTDecompressionSessionCreate(
                std::ptr::null_mut(),
                format.get(),
                Some(&specification),
                Some(&destination),
                &record,
                &mut session,
            )
        };
        let session = match Owned::new(session) {
            Some(session) if status == 0 => session,
            _ => return Err(status),
        };
        // SAFETY: VideoToolbox's constant key.
        ffi::set_property(
            session.get(),
            unsafe { ffi::kVTDecompressionPropertyKey_RealTime },
            yes,
        );
        Ok(session)
    }
}

/// A format description of these parameter sets, with four-byte lengths.
fn format_description(sps: &[u8], pps: &[u8]) -> Result<Owned, CodecError> {
    let pointers = [sps.as_ptr(), pps.as_ptr()];
    let sizes = [sps.len(), pps.len()];
    let mut format: Ref = std::ptr::null_mut();
    // SAFETY: two parameter sets of these sizes; the description copies
    // them.
    let status = unsafe {
        ffi::CMVideoFormatDescriptionCreateFromH264ParameterSets(
            std::ptr::null_mut(),
            2,
            pointers.as_ptr(),
            sizes.as_ptr(),
            4,
            &mut format,
        )
    };
    Owned::new(format).filter(|_| status == 0).ok_or_else(|| {
        tracing::debug!(status, "the SPS and PPS make no format description");
        CodecError::WrongPicture
    })
}

/// One sample of `avcc` (NAL units behind four-byte lengths).
///
/// # Safety
///
/// `format` is a valid format description.
unsafe fn sample_buffer(avcc: &[u8], format: Ref) -> Option<Owned> {
    let mut block: Ref = std::ptr::null_mut();
    // SAFETY: a new block of `avcc`'s length, then `avcc` copied in.
    let block = unsafe {
        let status = ffi::CMBlockBufferCreateWithMemoryBlock(
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            avcc.len(),
            std::ptr::null_mut(),
            std::ptr::null(),
            0,
            avcc.len(),
            ffi::ASSURE_MEMORY_NOW,
            &mut block,
        );
        let block = Owned::new(block).filter(|_| status == 0)?;
        if ffi::CMBlockBufferReplaceDataBytes(avcc.as_ptr().cast(), block.get(), 0, avcc.len()) != 0
        {
            return None;
        }
        block
    };
    let size = avcc.len();
    let mut sample: Ref = std::ptr::null_mut();
    // SAFETY: one sample of `size` bytes in `block`, no timing.
    let status = unsafe {
        ffi::CMSampleBufferCreateReady(
            std::ptr::null_mut(),
            block.get(),
            format,
            1,
            0,
            std::ptr::null(),
            1,
            &size,
            &mut sample,
        )
    };
    Owned::new(sample).filter(|_| status == 0)
}

/// VideoToolbox's output: a decoded image, copied into an NV12 picture.
unsafe extern "C" fn on_decoded(
    refcon: *mut c_void,
    _source_frame: *mut c_void,
    status: ffi::OSStatus,
    flags: u32,
    image: Ref,
    _presentation: CMTime,
    _duration: CMTime,
) {
    // SAFETY: `refcon` is the decoder's boxed `Outputs`.
    let outputs = unsafe { &*refcon.cast::<Outputs>() };
    let output = if status != 0 {
        Err(status)
    } else if image.is_null() || flags & ffi::FRAME_DROPPED != 0 {
        return;
    } else {
        let captured = outputs
            .captured
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .unwrap_or_else(Instant::now);
        // SAFETY: a valid image buffer for the call.
        match unsafe { picture_of(image, captured) } {
            Some(picture) => Ok(picture),
            None => return,
        }
    };
    outputs
        .pictures
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(output);
}

/// An NV12 pixel buffer as a packed picture, cut to even sizes.
///
/// # Safety
///
/// `image` is a valid pixel buffer.
pub(super) unsafe fn picture_of(image: Ref, captured: Instant) -> Option<Picture> {
    if image.is_null() {
        return None;
    }
    // SAFETY: a valid pixel buffer.
    let (format, planes, width, height) = unsafe {
        (
            ffi::CVPixelBufferGetPixelFormatType(image),
            ffi::CVPixelBufferGetPlaneCount(image),
            ffi::CVPixelBufferGetWidth(image) & !1,
            ffi::CVPixelBufferGetHeight(image) & !1,
        )
    };
    if format != ffi::NV12_VIDEO_RANGE || planes != 2 || width == 0 || height == 0 {
        return None;
    }
    let locked = Locked::new(image, ffi::LOCK_READ_ONLY)?;
    let mut data = Vec::with_capacity(Picture::bytes(width as u32, height as u32));
    for (plane, rows) in [(0, height), (1, height / 2)] {
        let (base, stride, available) = locked.plane(plane)?;
        for row in 0..rows.min(available) {
            // SAFETY: the plane is locked, with `available` rows of
            // `stride` bytes, each at least `width`.
            data.extend_from_slice(unsafe {
                std::slice::from_raw_parts(base.add(row * stride), width)
            });
        }
    }
    drop(locked);
    Picture::from_data(
        PixelFormat::Nv12,
        width as u32,
        height as u32,
        data,
        captured,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::codec::encoder::{Backend, Encoder, EncoderConfig};
    use crate::video::pattern::scene;

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

    fn round_trip(hardware: bool) {
        let Some((sources, units)) = encoded(640, 360, 30) else {
            return;
        };
        let mut decoder = Decoder::open(hardware).unwrap();
        let mut worst = f64::MAX;

        for (source, access_unit) in sources.iter().zip(&units) {
            let pictures = decoder.decode(access_unit, source.captured).unwrap();

            assert_eq!(pictures.len(), 1, "each picture at once");
            let picture = &pictures[0];
            assert_eq!((picture.width, picture.height), (640, 360));
            assert_eq!(picture.format, PixelFormat::Nv12);
            assert_eq!(picture.captured, source.captured);
            worst = worst.min(psnr(picture, source));
        }
        eprintln!(
            "hardware {hardware} ({}): worst {worst:.1} dB",
            decoder.is_hardware()
        );
        assert!(worst > 35.0, "{worst:.1} dB");
    }

    #[test]
    fn decoding_gives_back_each_picture_at_once_above_35_db() {
        round_trip(false);
    }

    #[test]
    fn hardware_decoding_gives_back_each_picture_at_once_above_35_db() {
        round_trip(true);
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

    #[test]
    fn a_size_change_mid_stream_decodes() {
        let (Some((small, small_units)), Some((large, large_units))) =
            (encoded(320, 180, 5), encoded(640, 360, 5))
        else {
            return;
        };
        let mut decoder = Decoder::open(false).unwrap();

        for (sources, units) in [(&small, &small_units), (&large, &large_units)] {
            for (source, access_unit) in sources.iter().zip(units) {
                let pictures = decoder.decode(access_unit, source.captured).unwrap();
                assert_eq!(pictures.len(), 1);
                assert_eq!(
                    (pictures[0].width, pictures[0].height),
                    (source.width, source.height)
                );
                assert!(psnr(&pictures[0], source) > 30.0);
            }
        }
    }

    /// Sizes a 16:10 screen gives (1214×758, 606×378: chroma rows of odd
    /// width), and a 1366×768 one: through the encoder, the decoder and
    /// the scaler to a tile.
    #[test]
    fn unaligned_sizes_decode_and_scale() {
        for (width, height) in [(1214, 758), (606, 378), (1366, 768), (1212, 756)] {
            for hardware in [false, true] {
                let Some((sources, units)) = encoded(width, height, 5) else {
                    return;
                };
                let mut decoder = Decoder::open(hardware).unwrap();
                let mut scaler = crate::video::codec::scale::Scaler::new();
                for (source, access_unit) in sources.iter().zip(&units) {
                    let pictures = decoder
                        .decode(access_unit, source.captured)
                        .unwrap_or_else(|error| panic!("{width}×{height} hw {hardware}: {error}"));
                    assert_eq!(pictures.len(), 1, "{width}×{height} hw {hardware}");
                    let picture = &pictures[0];
                    assert_eq!((picture.width, picture.height), (width, height));
                    let psnr = psnr(picture, source);
                    assert!(psnr > 30.0, "{width}×{height} hw {hardware}: {psnr:.1} dB");
                    let rgba = scaler
                        .rgba(picture, 640, 400)
                        .unwrap_or_else(|error| panic!("{width}×{height} scaling: {error}"));
                    assert_eq!(rgba.len(), 640 * 400 * 4);
                }
                eprintln!("{width}×{height} hardware {hardware}: ok");
            }
        }
    }
}
