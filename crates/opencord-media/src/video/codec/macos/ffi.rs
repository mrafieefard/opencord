//! The VideoToolbox, Core Media and Core Video functions and keys the
//! codec uses, declared by hand: plain C APIs whose objc2 bindings are
//! spread over many features and deprecated aliases (docs/plans/macos.md
//! §7, M3's ruling). Objects are untyped Core Foundation references, held
//! by [`Owned`].

#![allow(non_upper_case_globals, non_snake_case)]

use std::ffi::c_void;
use std::ptr::NonNull;

use objc2_core_foundation::{CFDictionary, CFString};

pub type OSStatus = i32;
/// A Core Foundation, Core Media, Core Video or VideoToolbox object.
pub type Ref = *mut c_void;

/// `kCMVideoCodecType_H264`: 'avc1'.
pub const CODEC_H264: u32 = u32::from_be_bytes(*b"avc1");
/// `kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange`: NV12 at video range.
pub const NV12_VIDEO_RANGE: u32 = u32::from_be_bytes(*b"420v");
/// `kCMBlockBufferAssureMemoryNowFlag`.
pub const ASSURE_MEMORY_NOW: u32 = 1;
/// `kCVPixelBufferLock_ReadOnly`.
pub const LOCK_READ_ONLY: u64 = 1;
/// `kVTDecodeInfo_FrameDropped`.
pub const FRAME_DROPPED: u32 = 1 << 1;

/// A Core Media time: `value` / `timescale` seconds.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CMTime {
    pub value: i64,
    pub timescale: i32,
    pub flags: u32,
    pub epoch: i64,
}

impl CMTime {
    /// `kCMTimeInvalid`.
    pub const INVALID: Self = Self {
        value: 0,
        timescale: 0,
        flags: 0,
        epoch: 0,
    };

    /// `CMTimeMake`: valid (`kCMTimeFlags_Valid`).
    pub fn new(value: i64, timescale: i32) -> Self {
        Self {
            value,
            timescale,
            flags: 1,
            epoch: 0,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CMVideoDimensions {
    pub width: i32,
    pub height: i32,
}

pub type CompressionOutput = unsafe extern "C" fn(
    refcon: *mut c_void,
    source_frame_refcon: *mut c_void,
    status: OSStatus,
    info_flags: u32,
    sample_buffer: Ref,
);

pub type DecompressionOutput = unsafe extern "C" fn(
    refcon: *mut c_void,
    source_frame_refcon: *mut c_void,
    status: OSStatus,
    info_flags: u32,
    image_buffer: Ref,
    presentation_time: CMTime,
    presentation_duration: CMTime,
);

#[repr(C)]
pub struct DecompressionOutputRecord {
    pub callback: DecompressionOutput,
    pub refcon: *mut c_void,
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    pub fn CFRelease(object: Ref);
}

#[link(name = "VideoToolbox", kind = "framework")]
unsafe extern "C" {
    pub static kVTCompressionPropertyKey_RealTime: &'static CFString;
    pub static kVTCompressionPropertyKey_AllowFrameReordering: &'static CFString;
    pub static kVTCompressionPropertyKey_ProfileLevel: &'static CFString;
    pub static kVTCompressionPropertyKey_AverageBitRate: &'static CFString;
    pub static kVTCompressionPropertyKey_DataRateLimits: &'static CFString;
    pub static kVTCompressionPropertyKey_ExpectedFrameRate: &'static CFString;
    pub static kVTCompressionPropertyKey_MaxKeyFrameInterval: &'static CFString;
    pub static kVTCompressionPropertyKey_MaxKeyFrameIntervalDuration: &'static CFString;
    pub static kVTCompressionPropertyKey_UsingHardwareAcceleratedVideoEncoder: &'static CFString;
    pub static kVTProfileLevel_H264_High_AutoLevel: &'static CFString;
    pub static kVTEncodeFrameOptionKey_ForceKeyFrame: &'static CFString;
    pub static kVTVideoEncoderSpecification_EnableHardwareAcceleratedVideoEncoder:
        &'static CFString;
    pub static kVTVideoEncoderSpecification_RequireHardwareAcceleratedVideoEncoder:
        &'static CFString;
    pub static kVTVideoEncoderSpecification_EnableLowLatencyRateControl: &'static CFString;
    pub static kVTDecompressionPropertyKey_RealTime: &'static CFString;
    pub static kVTDecompressionPropertyKey_UsingHardwareAcceleratedVideoDecoder: &'static CFString;
    pub static kVTVideoDecoderSpecification_EnableHardwareAcceleratedVideoDecoder:
        &'static CFString;
    pub static kVTVideoDecoderSpecification_RequireHardwareAcceleratedVideoDecoder:
        &'static CFString;

    pub fn VTCompressionSessionCreate(
        allocator: Ref,
        width: i32,
        height: i32,
        codec_type: u32,
        encoder_specification: Option<&CFDictionary>,
        source_image_buffer_attributes: Option<&CFDictionary>,
        compressed_data_allocator: Ref,
        output_callback: Option<CompressionOutput>,
        output_callback_refcon: *mut c_void,
        session_out: *mut Ref,
    ) -> OSStatus;
    pub fn VTCompressionSessionPrepareToEncodeFrames(session: Ref) -> OSStatus;
    pub fn VTCompressionSessionGetPixelBufferPool(session: Ref) -> Ref;
    pub fn VTCompressionSessionEncodeFrame(
        session: Ref,
        image_buffer: Ref,
        presentation_time: CMTime,
        duration: CMTime,
        frame_properties: Option<&CFDictionary>,
        source_frame_refcon: *mut c_void,
        info_flags_out: *mut u32,
    ) -> OSStatus;
    pub fn VTCompressionSessionCompleteFrames(session: Ref, until: CMTime) -> OSStatus;
    pub fn VTCompressionSessionInvalidate(session: Ref);

    pub fn VTDecompressionSessionCreate(
        allocator: Ref,
        format_description: Ref,
        decoder_specification: Option<&CFDictionary>,
        destination_image_buffer_attributes: Option<&CFDictionary>,
        output_callback: *const DecompressionOutputRecord,
        session_out: *mut Ref,
    ) -> OSStatus;
    pub fn VTDecompressionSessionDecodeFrame(
        session: Ref,
        sample_buffer: Ref,
        decode_flags: u32,
        source_frame_refcon: *mut c_void,
        info_flags_out: *mut u32,
    ) -> OSStatus;
    pub fn VTDecompressionSessionWaitForAsynchronousFrames(session: Ref) -> OSStatus;
    pub fn VTDecompressionSessionInvalidate(session: Ref);

    pub fn VTSessionSetProperty(session: Ref, key: &CFString, value: Ref) -> OSStatus;
    pub fn VTSessionCopyProperty(
        session: Ref,
        key: &CFString,
        allocator: Ref,
        value_out: *mut Ref,
    ) -> OSStatus;
}

#[link(name = "CoreMedia", kind = "framework")]
unsafe extern "C" {
    pub fn CMVideoFormatDescriptionCreateFromH264ParameterSets(
        allocator: Ref,
        parameter_set_count: usize,
        parameter_set_pointers: *const *const u8,
        parameter_set_sizes: *const usize,
        nal_unit_header_length: i32,
        format_description_out: *mut Ref,
    ) -> OSStatus;
    pub fn CMVideoFormatDescriptionGetH264ParameterSetAtIndex(
        format_description: Ref,
        index: usize,
        parameter_set_out: *mut *const u8,
        parameter_set_size_out: *mut usize,
        parameter_set_count_out: *mut usize,
        nal_unit_header_length_out: *mut i32,
    ) -> OSStatus;

    pub fn CMBlockBufferCreateWithMemoryBlock(
        structure_allocator: Ref,
        memory_block: *mut c_void,
        block_length: usize,
        block_allocator: Ref,
        custom_block_source: *const c_void,
        offset_to_data: usize,
        data_length: usize,
        flags: u32,
        block_buffer_out: *mut Ref,
    ) -> OSStatus;
    pub fn CMBlockBufferReplaceDataBytes(
        source: *const c_void,
        destination: Ref,
        offset_into_destination: usize,
        data_length: usize,
    ) -> OSStatus;
    pub fn CMBlockBufferGetDataLength(buffer: Ref) -> usize;
    pub fn CMBlockBufferCopyDataBytes(
        buffer: Ref,
        offset: usize,
        length: usize,
        destination: *mut c_void,
    ) -> OSStatus;

    pub fn CMSampleBufferCreateReady(
        allocator: Ref,
        data_buffer: Ref,
        format_description: Ref,
        sample_count: isize,
        timing_entry_count: isize,
        timing: *const c_void,
        size_entry_count: isize,
        sizes: *const usize,
        sample_buffer_out: *mut Ref,
    ) -> OSStatus;
    /// Not retained.
    pub fn CMSampleBufferGetDataBuffer(sample_buffer: Ref) -> Ref;
    /// Not retained.
    pub fn CMSampleBufferGetImageBuffer(sample_buffer: Ref) -> Ref;
    /// Not retained; a CFArray of CFDictionaries, or null.
    pub fn CMSampleBufferGetSampleAttachmentsArray(
        sample_buffer: Ref,
        create_if_necessary: u8,
    ) -> *const std::ffi::c_void;
    pub fn CMVideoFormatDescriptionGetDimensions(format_description: Ref) -> CMVideoDimensions;
    /// Not retained.
    pub fn CMSampleBufferGetFormatDescription(sample_buffer: Ref) -> Ref;
}

#[link(name = "CoreVideo", kind = "framework")]
unsafe extern "C" {
    pub static kCVPixelBufferPixelFormatTypeKey: &'static CFString;
    pub static kCVPixelBufferWidthKey: &'static CFString;
    pub static kCVPixelBufferHeightKey: &'static CFString;
    pub static kCVPixelBufferIOSurfacePropertiesKey: &'static CFString;

    pub fn CVPixelBufferPoolCreatePixelBuffer(
        allocator: Ref,
        pool: Ref,
        pixel_buffer_out: *mut Ref,
    ) -> i32;
    pub fn CVPixelBufferCreate(
        allocator: Ref,
        width: usize,
        height: usize,
        pixel_format: u32,
        attributes: Option<&CFDictionary>,
        pixel_buffer_out: *mut Ref,
    ) -> i32;
    pub fn CVPixelBufferLockBaseAddress(pixel_buffer: Ref, flags: u64) -> i32;
    pub fn CVPixelBufferUnlockBaseAddress(pixel_buffer: Ref, flags: u64) -> i32;
    pub fn CVPixelBufferGetPixelFormatType(pixel_buffer: Ref) -> u32;
    pub fn CVPixelBufferGetWidth(pixel_buffer: Ref) -> usize;
    pub fn CVPixelBufferGetHeight(pixel_buffer: Ref) -> usize;
    pub fn CVPixelBufferGetPlaneCount(pixel_buffer: Ref) -> usize;
    pub fn CVPixelBufferGetBaseAddressOfPlane(pixel_buffer: Ref, plane: usize) -> *mut c_void;
    pub fn CVPixelBufferGetBytesPerRowOfPlane(pixel_buffer: Ref, plane: usize) -> usize;
    pub fn CVPixelBufferGetHeightOfPlane(pixel_buffer: Ref, plane: usize) -> usize;
}

/// A reference we own, released when dropped.
pub struct Owned(NonNull<c_void>);

impl Owned {
    /// Takes over `object`, a reference from a Create or Copy function.
    pub fn new(object: Ref) -> Option<Self> {
        NonNull::new(object).map(Self)
    }

    pub fn get(&self) -> Ref {
        self.0.as_ptr()
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: we hold one reference to it.
        unsafe { CFRelease(self.0.as_ptr()) };
    }
}

// SAFETY: Core Foundation objects may be retained and released from any
// thread; the sessions and buffers held this way are used by one thread at
// a time (`&mut self`).
unsafe impl Send for Owned {}

/// A pixel buffer's base addresses, while locked.
pub struct Locked {
    buffer: Ref,
    flags: u64,
}

impl Locked {
    /// Locks `buffer`, which must outlive the lock.
    pub fn new(buffer: Ref, flags: u64) -> Option<Self> {
        // SAFETY: the caller passes a live pixel buffer.
        (unsafe { CVPixelBufferLockBaseAddress(buffer, flags) } == 0)
            .then_some(Self { buffer, flags })
    }

    /// Plane `plane`'s rows: its base address, bytes per row and height.
    pub fn plane(&self, plane: usize) -> Option<(*mut u8, usize, usize)> {
        let buffer = self.buffer;
        // SAFETY: the buffer is locked.
        unsafe {
            let base = CVPixelBufferGetBaseAddressOfPlane(buffer, plane).cast::<u8>();
            (!base.is_null()).then(|| {
                (
                    base,
                    CVPixelBufferGetBytesPerRowOfPlane(buffer, plane),
                    CVPixelBufferGetHeightOfPlane(buffer, plane),
                )
            })
        }
    }
}

impl Drop for Locked {
    fn drop(&mut self) {
        // SAFETY: locked by `new` with these flags.
        unsafe { CVPixelBufferUnlockBaseAddress(self.buffer, self.flags) };
    }
}

/// A dictionary of these keys and values.
pub fn dictionary(
    pairs: &[(&CFString, &objc2_core_foundation::CFType)],
) -> objc2_core_foundation::CFRetained<CFDictionary> {
    let keys: Vec<&CFString> = pairs.iter().map(|(key, _)| *key).collect();
    let values: Vec<&objc2_core_foundation::CFType> =
        pairs.iter().map(|(_, value)| *value).collect();
    let typed =
        CFDictionary::<CFString, objc2_core_foundation::CFType>::from_slices(&keys, &values);
    // SAFETY: the same object, with its key and value types forgotten.
    unsafe { objc2_core_foundation::CFRetained::cast_unchecked(typed) }
}

/// Sets a VideoToolbox session's property.
pub fn set_property(
    session: Ref,
    key: &CFString,
    value: &objc2_core_foundation::CFType,
) -> OSStatus {
    let value: *const objc2_core_foundation::CFType = value;
    // SAFETY: a live session; the value is retained by the session as needed.
    unsafe { VTSessionSetProperty(session, key, value.cast_mut().cast()) }
}

/// A VideoToolbox session's Boolean property, false if it has none.
pub fn bool_property(session: Ref, key: &CFString) -> bool {
    let mut value: Ref = std::ptr::null_mut();
    // SAFETY: a live session; the copy is ours to release.
    let status = unsafe { VTSessionCopyProperty(session, key, std::ptr::null_mut(), &mut value) };
    let Some(value) = (status == 0).then(|| Owned::new(value)).flatten() else {
        return false;
    };
    // SAFETY: the property is a CFBoolean.
    unsafe { &*value.get().cast::<objc2_core_foundation::CFBoolean>() }.as_bool()
}
