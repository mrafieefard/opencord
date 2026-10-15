//! H.264 encoding through VideoToolbox (docs/plans/macos.md M3, plan
//! §7.9's macOS row): the hardware encoder (Apple silicon's media engine,
//! Intel's Quick Sync), and VideoToolbox's software encoder as the
//! fallback. Real-time settings as on Linux: no B-frames, rate control with
//! about a second of buffer, one keyframe at the start and then only on
//! request, every picture out of its own `encode` call, as Annex B NAL
//! units with the SPS and PPS before each IDR.

use std::ffi::c_void;
use std::sync::Mutex;

use objc2_core_foundation::{CFArray, CFBoolean, CFNumber, CFType};

use super::super::{CodecError, NAL_IDR, nal_type};
pub use super::super::{Encoded, EncoderConfig};
use super::ffi::{self, CMTime, Locked, Owned, Ref};
use crate::video::picture::{Picture, PixelFormat};

/// Pictures between keyframes the encoder would make by itself: an hour,
/// so in practice they come only on request.
const GOP: u32 = 30 * 60 * 60;

/// The encoders this system has, hardware first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// VideoToolbox's hardware encoder.
    VideoToolbox,
    /// VideoToolbox's software encoder.
    Software,
}

impl Backend {
    /// Hardware first, as the plan says.
    pub const ALL: [Backend; 2] = [Backend::VideoToolbox, Backend::Software];

    pub fn is_hardware(self) -> bool {
        self == Backend::VideoToolbox
    }
}

/// What the output callback hands back: each picture's NAL units, or the
/// status that stopped it.
type Outputs = Mutex<Vec<Result<Vec<Vec<u8>>, ffi::OSStatus>>>;

pub struct Encoder {
    backend: Backend,
    config: EncoderConfig,
    session: Owned,
    /// Whether the encoder is in VideoToolbox's low-latency mode.
    low_latency: bool,
    /// Where the callback puts what it was given; boxed, so its address
    /// stays put for the session.
    outputs: Box<Outputs>,
    /// Pictures encoded, the presentation time of the next.
    count: i64,
}

impl Encoder {
    /// The first of `backends` that opens for this configuration.
    pub fn open(config: EncoderConfig, backends: &[Backend]) -> Result<Self, CodecError> {
        let mut failure = CodecError::Unavailable("an H.264 encoder");
        for &backend in backends {
            // Low-latency rate control first; not every encoder has it.
            for low_latency in [true, false] {
                match Self::open_one(config, backend, low_latency) {
                    Ok(encoder) => return Ok(encoder),
                    Err(status) => {
                        tracing::debug!(?backend, low_latency, status, "an encoder did not open");
                        failure = CodecError::Unavailable(match backend {
                            Backend::VideoToolbox => "VideoToolbox's hardware H.264 encoder",
                            Backend::Software => "VideoToolbox's software H.264 encoder",
                        });
                    }
                }
            }
        }
        Err(failure)
    }

    fn open_one(
        config: EncoderConfig,
        backend: Backend,
        low_latency: bool,
    ) -> Result<Self, ffi::OSStatus> {
        let (Ok(width), Ok(height)) = (i32::try_from(config.width), i32::try_from(config.height))
        else {
            return Err(-1);
        };
        let yes: &CFType = CFBoolean::new(true).as_ref();
        let no: &CFType = CFBoolean::new(false).as_ref();
        // SAFETY: VideoToolbox's constant keys.
        let specification = unsafe {
            let mut pairs = vec![match backend {
                Backend::VideoToolbox => (
                    ffi::kVTVideoEncoderSpecification_RequireHardwareAcceleratedVideoEncoder,
                    yes,
                ),
                Backend::Software => (
                    ffi::kVTVideoEncoderSpecification_EnableHardwareAcceleratedVideoEncoder,
                    no,
                ),
            }];
            if low_latency {
                pairs.push((
                    ffi::kVTVideoEncoderSpecification_EnableLowLatencyRateControl,
                    yes,
                ));
            }
            ffi::dictionary(&pairs)
        };
        let format = CFNumber::new_i32(ffi::NV12_VIDEO_RANGE as i32);
        let (pixel_width, pixel_height) = (CFNumber::new_i32(width), CFNumber::new_i32(height));
        let surface = ffi::dictionary(&[]);
        // SAFETY: Core Video's constant keys.
        let sources = unsafe {
            ffi::dictionary(&[
                (ffi::kCVPixelBufferPixelFormatTypeKey, format.as_ref()),
                (ffi::kCVPixelBufferWidthKey, pixel_width.as_ref()),
                (ffi::kCVPixelBufferHeightKey, pixel_height.as_ref()),
                (ffi::kCVPixelBufferIOSurfacePropertiesKey, surface.as_ref()),
            ])
        };
        let outputs: Box<Outputs> = Box::default();
        let mut session: Ref = std::ptr::null_mut();
        // SAFETY: the callback gets `outputs`, which the encoder keeps for
        // as long as the session.
        let status = unsafe {
            ffi::VTCompressionSessionCreate(
                std::ptr::null_mut(),
                width,
                height,
                ffi::CODEC_H264,
                Some(&specification),
                Some(&sources),
                std::ptr::null_mut(),
                Some(on_encoded),
                std::ptr::from_ref::<Outputs>(&outputs).cast_mut().cast(),
                &mut session,
            )
        };
        let session = match Owned::new(session) {
            Some(session) if status == 0 => session,
            _ => return Err(status),
        };
        let mut encoder = Self {
            backend,
            config,
            session,
            low_latency,
            outputs,
            count: 0,
        };
        encoder.configure()?;
        Ok(encoder)
    }

    /// The real-time settings. Only `RealTime` and the bitrate must take;
    /// an encoder without the others is still usable.
    fn configure(&mut self) -> Result<(), ffi::OSStatus> {
        let session = self.session.get();
        let config = self.config;
        let bitrate = CFNumber::new_i64(i64::from(config.bitrate));
        let fps = CFNumber::new_i32(i32::try_from(config.fps).unwrap_or(30).max(1));
        let gop = CFNumber::new_i32(GOP as i32);
        let gop_seconds = CFNumber::new_f64(f64::from(GOP) / f64::from(config.fps.max(1)));
        // About a second of buffer: the bitrate's bytes over one second.
        let (bytes, second) = (
            CFNumber::new_i64(i64::from(config.bitrate / 8)),
            CFNumber::new_f64(1.0),
        );
        let limits = CFArray::from_retained_objects(&[bytes, second]);
        let yes: &CFType = CFBoolean::new(true).as_ref();
        let no: &CFType = CFBoolean::new(false).as_ref();
        // SAFETY: VideoToolbox's constant keys and profile.
        unsafe {
            for (key, value) in [
                (ffi::kVTCompressionPropertyKey_RealTime, yes),
                (
                    ffi::kVTCompressionPropertyKey_AverageBitRate,
                    bitrate.as_ref(),
                ),
            ] {
                let status = ffi::set_property(session, key, value);
                if status != 0 {
                    return Err(status);
                }
            }
            for (key, value) in [
                (ffi::kVTCompressionPropertyKey_AllowFrameReordering, no),
                (
                    ffi::kVTCompressionPropertyKey_ProfileLevel,
                    ffi::kVTProfileLevel_H264_High_AutoLevel.as_ref(),
                ),
                (
                    ffi::kVTCompressionPropertyKey_ExpectedFrameRate,
                    fps.as_ref(),
                ),
                (
                    ffi::kVTCompressionPropertyKey_MaxKeyFrameInterval,
                    gop.as_ref(),
                ),
                (
                    ffi::kVTCompressionPropertyKey_MaxKeyFrameIntervalDuration,
                    gop_seconds.as_ref(),
                ),
                (
                    ffi::kVTCompressionPropertyKey_DataRateLimits,
                    limits.as_ref(),
                ),
            ] {
                let status = ffi::set_property(session, key, value);
                if status != 0 {
                    tracing::debug!(%key, status, "an encoder setting did not take");
                }
            }
            ffi::VTCompressionSessionPrepareToEncodeFrames(session);
        }
        Ok(())
    }

    pub fn backend(&self) -> Backend {
        self.backend
    }

    pub fn config(&self) -> EncoderConfig {
        self.config
    }

    /// Whether VideoToolbox says the hardware encodes.
    pub fn is_hardware(&self) -> bool {
        // SAFETY: VideoToolbox's constant key.
        ffi::bool_property(self.session.get(), unsafe {
            ffi::kVTCompressionPropertyKey_UsingHardwareAcceleratedVideoEncoder
        })
    }

    /// Whether the encoder took VideoToolbox's low-latency rate control.
    pub fn is_low_latency(&self) -> bool {
        self.low_latency
    }

    /// Encodes a picture of the configured size; an IDR picture led by its
    /// SPS and PPS when `keyframe` is set. Every picture comes out of its
    /// own call.
    pub fn encode(
        &mut self,
        picture: &Picture,
        keyframe: bool,
    ) -> Result<Option<Encoded>, CodecError> {
        if (picture.width, picture.height) != (self.config.width, self.config.height) {
            return Err(CodecError::WrongPicture);
        }
        let buffer = self.pixel_buffer(picture)?;
        let yes: &CFType = CFBoolean::new(true).as_ref();
        // SAFETY: VideoToolbox's constant key.
        let force = keyframe.then(|| {
            ffi::dictionary(&[(unsafe { ffi::kVTEncodeFrameOptionKey_ForceKeyFrame }, yes)])
        });
        let fps = i32::try_from(self.config.fps).unwrap_or(30).max(1);
        let session = self.session.get();
        // SAFETY: a live session and pixel buffer; the callback runs before
        // `CompleteFrames` returns.
        let status = unsafe {
            let status = ffi::VTCompressionSessionEncodeFrame(
                session,
                buffer.get(),
                CMTime::new(self.count, fps),
                CMTime::INVALID,
                force.as_deref(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            if status == 0 {
                ffi::VTCompressionSessionCompleteFrames(session, CMTime::INVALID)
            } else {
                status
            }
        };
        self.count += 1;
        let outputs = std::mem::take(&mut *self.outputs.lock().unwrap_or_else(|e| e.into_inner()));
        if status != 0 {
            tracing::debug!(status, "VideoToolbox did not encode a picture");
            return Err(CodecError::Unavailable("VideoToolbox's encoder"));
        }
        let mut nal_units = Vec::new();
        for output in outputs {
            match output {
                Ok(units) => nal_units.extend(units),
                Err(status) => {
                    tracing::debug!(status, "VideoToolbox did not encode a picture");
                    return Err(CodecError::Unavailable("VideoToolbox's encoder"));
                }
            }
        }
        if nal_units.is_empty() {
            return Ok(None);
        }
        let keyframe = nal_units.iter().any(|unit| nal_type(unit) == Some(NAL_IDR));
        Ok(Some(Encoded {
            nal_units,
            keyframe,
        }))
    }

    /// The picture in one of the session's pixel buffers, as NV12.
    fn pixel_buffer(&self, picture: &Picture) -> Result<Owned, CodecError> {
        let mut buffer: Ref = std::ptr::null_mut();
        // SAFETY: a live session; its pool is the session's, not ours.
        let status = unsafe {
            let pool = ffi::VTCompressionSessionGetPixelBufferPool(self.session.get());
            if pool.is_null() {
                ffi::CVPixelBufferCreate(
                    std::ptr::null_mut(),
                    picture.width as usize,
                    picture.height as usize,
                    ffi::NV12_VIDEO_RANGE,
                    None,
                    &mut buffer,
                )
            } else {
                ffi::CVPixelBufferPoolCreatePixelBuffer(std::ptr::null_mut(), pool, &mut buffer)
            }
        };
        let buffer = Owned::new(buffer)
            .filter(|_| status == 0)
            .ok_or(CodecError::Unavailable("a pixel buffer"))?;
        let locked =
            Locked::new(buffer.get(), 0).ok_or(CodecError::Unavailable("a pixel buffer"))?;
        let (width, height) = (picture.width as usize, picture.height as usize);
        let (Some(luma), Some(chroma)) = (locked.plane(0), locked.plane(1)) else {
            return Err(CodecError::Unavailable("a pixel buffer"));
        };
        let planes = picture.planes();
        // SAFETY: each plane has `rows` rows of `stride` bytes, at least
        // the picture's width.
        unsafe {
            copy_rows(planes[0].0, width, luma, height);
            match picture.format {
                PixelFormat::Nv12 => copy_rows(planes[1].0, width, chroma, height / 2),
                PixelFormat::I420 => {
                    let (u, v) = (planes[1].0, planes[2].0);
                    let (base, stride, rows) = chroma;
                    for row in 0..(height / 2).min(rows) {
                        let out = std::slice::from_raw_parts_mut(base.add(row * stride), width);
                        let (u, v) = (&u[row * width / 2..], &v[row * width / 2..]);
                        for (pair, (u, v)) in
                            out.as_chunks_mut::<2>().0.iter_mut().zip(u.iter().zip(v))
                        {
                            *pair = [*u, *v];
                        }
                    }
                }
            }
        }
        drop(locked);
        Ok(buffer)
    }
}

/// Opens and closes a small hardware encoder once, on a thread of its own:
/// a process's first hardware encoder takes 0.3–0.7 s to open while
/// VideoToolbox loads it (§7, M3), which the first camera or screen share
/// would otherwise wait for. This crate's own tests wait for it, so the
/// ones that time the sender don't time VideoToolbox's loading.
pub(super) fn warm_up() {
    static WARM: std::sync::Once = std::sync::Once::new();
    WARM.call_once(|| {
        let warm = || {
            let config = EncoderConfig {
                width: 320,
                height: 180,
                fps: 30,
                bitrate: 150_000,
            };
            drop(Encoder::open(config, &[Backend::VideoToolbox]));
        };
        if cfg!(test) {
            warm();
        } else {
            let _ = std::thread::Builder::new()
                .name("opencord-videotoolbox".to_owned())
                .spawn(warm);
        }
    });
}

/// Copies rows of `width` bytes, packed in `from`, into a locked plane.
///
/// # Safety
///
/// The plane is locked, with `rows` rows of `stride` bytes each, `stride`
/// at least `width`.
unsafe fn copy_rows(
    from: &[u8],
    width: usize,
    (base, stride, rows): (*mut u8, usize, usize),
    height: usize,
) {
    for (row, line) in from.chunks_exact(width).take(height.min(rows)).enumerate() {
        // SAFETY: the caller's promise.
        unsafe { std::ptr::copy_nonoverlapping(line.as_ptr(), base.add(row * stride), width) };
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        // SAFETY: a live session; nothing calls back after this.
        unsafe { ffi::VTCompressionSessionInvalidate(self.session.get()) };
    }
}

/// VideoToolbox's output: one picture's AVCC sample, as Annex B NAL units,
/// with the SPS and PPS first on a keyframe.
unsafe extern "C" fn on_encoded(
    refcon: *mut c_void,
    _source_frame: *mut c_void,
    status: ffi::OSStatus,
    _flags: u32,
    sample: Ref,
) {
    // SAFETY: `refcon` is the encoder's boxed `Outputs`.
    let outputs = unsafe { &*refcon.cast::<Outputs>() };
    let output = if status != 0 {
        Err(status)
    } else if sample.is_null() {
        // Dropped: nothing for this picture.
        return;
    } else {
        // SAFETY: a valid sample buffer for the call.
        Ok(unsafe { nal_units_of(sample) })
    };
    outputs
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(output);
}

/// A sample's NAL units, its parameter sets first when it holds an IDR.
///
/// # Safety
///
/// `sample` is a valid sample buffer.
unsafe fn nal_units_of(sample: Ref) -> Vec<Vec<u8>> {
    // SAFETY: the caller's promise; these are the sample's own, not
    // retained.
    let (block, format) = unsafe {
        (
            ffi::CMSampleBufferGetDataBuffer(sample),
            ffi::CMSampleBufferGetFormatDescription(sample),
        )
    };
    if block.is_null() || format.is_null() {
        return Vec::new();
    }
    // SAFETY: a valid block buffer, copied into memory of its length.
    let data = unsafe {
        let length = ffi::CMBlockBufferGetDataLength(block);
        let mut data = vec![0u8; length];
        if ffi::CMBlockBufferCopyDataBytes(block, 0, length, data.as_mut_ptr().cast()) != 0 {
            return Vec::new();
        }
        data
    };
    // SAFETY: a valid format description.
    let (parameter_sets, length_size) = unsafe { parameter_sets(format) };
    let units = split_lengths(&data, length_size);
    if units.iter().any(|unit| nal_type(unit) == Some(NAL_IDR)) {
        parameter_sets.into_iter().chain(units).collect()
    } else {
        units
    }
}

/// The SPS and PPS of a format description, and the length of its NAL
/// units' length fields.
///
/// # Safety
///
/// `format` is a valid H.264 format description.
pub(super) unsafe fn parameter_sets(format: Ref) -> (Vec<Vec<u8>>, usize) {
    let (mut count, mut length_size) = (0usize, 4i32);
    // SAFETY: the caller's promise; the out-pointers are valid.
    unsafe {
        ffi::CMVideoFormatDescriptionGetH264ParameterSetAtIndex(
            format,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut count,
            &mut length_size,
        );
    }
    let sets = (0..count)
        .filter_map(|index| {
            let (mut pointer, mut size) = (std::ptr::null(), 0usize);
            // SAFETY: as above; the set belongs to the description.
            let status = unsafe {
                ffi::CMVideoFormatDescriptionGetH264ParameterSetAtIndex(
                    format,
                    index,
                    &mut pointer,
                    &mut size,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };
            // SAFETY: `size` bytes at `pointer`, while the description lives.
            (status == 0 && !pointer.is_null())
                .then(|| unsafe { std::slice::from_raw_parts(pointer, size) }.to_vec())
        })
        .collect();
    (sets, usize::try_from(length_size).unwrap_or(4).clamp(1, 4))
}

/// NAL units behind big-endian length fields of `length_size` bytes (AVCC).
pub(super) fn split_lengths(mut data: &[u8], length_size: usize) -> Vec<Vec<u8>> {
    let mut units = Vec::new();
    while data.len() >= length_size {
        let (field, rest) = data.split_at(length_size);
        let length = field
            .iter()
            .fold(0usize, |length, byte| length << 8 | usize::from(*byte));
        if length == 0 || length > rest.len() {
            break;
        }
        units.push(rest[..length].to_vec());
        data = &rest[length..];
    }
    units
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::video::codec::{NAL_PPS, NAL_SPS};
    use crate::video::pattern::scene;

    fn config(width: u32, height: u32, bitrate: u32) -> EncoderConfig {
        EncoderConfig {
            width,
            height,
            fps: 30,
            bitrate,
        }
    }

    /// Opens `backend`, or says why it cannot be tested here.
    fn open(backend: Backend, config: EncoderConfig) -> Option<Encoder> {
        match Encoder::open(config, &[backend]) {
            Ok(encoder) => Some(encoder),
            Err(error) => {
                eprintln!("{backend:?} cannot be tested here: {error}");
                None
            }
        }
    }

    fn types(encoded: &Encoded) -> Vec<u8> {
        encoded
            .nal_units
            .iter()
            .filter_map(|unit| nal_type(unit))
            .collect()
    }

    #[test]
    fn avcc_lengths_split_into_nal_units() {
        let data = [0, 0, 0, 2, 0x67, 1, 0, 0, 0, 1, 0x68, 0, 0, 0, 9];

        assert_eq!(split_lengths(&data, 4), [vec![0x67, 1], vec![0x68]]);
        assert_eq!(split_lengths(&[0, 2, 0x65, 7], 2), [vec![0x65, 7]]);
    }

    fn keyframes_on_request(backend: Backend) {
        let Some(mut encoder) = open(backend, config(640, 360, 500_000)) else {
            return;
        };
        eprintln!(
            "{backend:?}: hardware {}, low latency {}",
            encoder.is_hardware(),
            encoder.is_low_latency()
        );
        let start = Instant::now();
        let mut encode = |number, keyframe| {
            encoder
                .encode(&scene(640, 360, number, start), keyframe)
                .unwrap()
                .expect("every picture comes out at once")
        };

        let first = encode(0, true);
        assert!(first.keyframe);
        assert_eq!(
            types(&first)[..2],
            [NAL_SPS, NAL_PPS],
            "{:?}",
            types(&first)
        );
        assert!(types(&first).contains(&NAL_IDR), "{:?}", types(&first));
        for number in 1..20 {
            let delta = encode(number, false);
            assert!(!delta.keyframe, "picture {number}");
            assert!(!types(&delta).contains(&NAL_SPS));
        }
        let asked = encode(20, true);
        assert!(asked.keyframe);
        assert_eq!(types(&asked)[..2], [NAL_SPS, NAL_PPS]);
        assert_eq!(encoder.backend(), backend);
    }

    #[test]
    fn the_hardware_encoder_makes_keyframes_only_on_request() {
        keyframes_on_request(Backend::VideoToolbox);
    }

    #[test]
    fn the_software_encoder_makes_keyframes_only_on_request() {
        keyframes_on_request(Backend::Software);
    }

    #[test]
    fn i420_pictures_encode_too() {
        let Ok(mut encoder) = Encoder::open(config(320, 180, 300_000), &Backend::ALL) else {
            eprintln!("no encoder here");
            return;
        };
        let mut picture = Picture::black(PixelFormat::I420, 320, 180, Instant::now());
        picture.data[320 * 180..].fill(90);

        let encoded = encoder.encode(&picture, true).unwrap();

        assert!(encoded.is_some_and(|encoded| encoded.keyframe));
    }

    /// Within ±25 % of the target over 10 s of the test pattern at 720p
    /// (M3, step 7), for each encoder. Smaller layers of the pattern need
    /// less than their target from the hardware encoder (§7, M3).
    #[test]
    fn every_encoder_keeps_near_its_bitrate() {
        for backend in Backend::ALL {
            let Some(mut encoder) = open(backend, config(1280, 720, 1_500_000)) else {
                continue;
            };
            let start = Instant::now();
            let mut bytes = 0;
            for number in 0..300 {
                let encoded = encoder
                    .encode(&scene(1280, 720, number, start), number == 0)
                    .unwrap();
                bytes += encoded.map_or(0, |encoded| {
                    encoded.nal_units.iter().map(Vec::len).sum::<usize>()
                });
            }

            let bitrate = bytes * 8 / 10;
            eprintln!("{backend:?}: {bitrate} bit/s");
            assert!(
                (1_125_000..=1_875_000).contains(&bitrate),
                "{backend:?}: {bitrate} bit/s"
            );
        }
    }

    #[test]
    fn a_picture_of_another_size_is_refused() {
        let Ok(mut encoder) = Encoder::open(config(320, 180, 150_000), &Backend::ALL) else {
            eprintln!("no encoder here");
            return;
        };

        let wrong = encoder.encode(&scene(640, 360, 0, Instant::now()), true);

        assert!(matches!(wrong, Err(CodecError::WrongPicture)));
    }

    #[test]
    fn hardware_comes_first_where_there_is_some() {
        let Ok(encoder) = Encoder::open(config(1280, 720, 1_500_000), &Backend::ALL) else {
            eprintln!("no encoder here");
            return;
        };

        assert_eq!(encoder.backend().is_hardware(), encoder.is_hardware());
        eprintln!(
            "720p opened {:?} (hardware {})",
            encoder.backend(),
            encoder.is_hardware()
        );
    }
}
