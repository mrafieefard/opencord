//! A hardware-decoded picture handed on without copying it (plan §7.11,
//! zero-copy): its VA-API surface exported as DMA-BUF planes, luma and
//! chroma, which the app's GPU imports and draws from. The decoder's
//! surface stays out of its pool for as long as the picture lives.

use std::ffi::{c_int, c_uint, c_void};
use std::os::fd::{FromRawFd, OwnedFd};
use std::time::Instant;

use ff::ffi;
use ffmpeg_next as ff;

use super::CodecError;

/// libva's `VA_SURFACE_ATTRIB_MEM_TYPE_DRM_PRIME_2`.
const MEM_TYPE_DRM_PRIME_2: c_uint = 0x4000_0000;
/// `VA_EXPORT_SURFACE_READ_ONLY | VA_EXPORT_SURFACE_SEPARATE_LAYERS`.
const EXPORT_READ_SEPARATE: c_uint = 0x0001 | 0x0004;
/// DRM's `R8` and `GR88` fourccs: NV12's two planes, one layer each.
pub const DRM_FORMAT_R8: u32 = u32::from_le_bytes(*b"R8  ");
pub const DRM_FORMAT_GR88: u32 = u32::from_le_bytes(*b"GR88");

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Object {
    fd: c_int,
    size: u32,
    modifier: u64,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Layer {
    drm_format: u32,
    num_planes: u32,
    object_index: [u32; 4],
    offset: [u32; 4],
    pitch: [u32; 4],
}

/// libva's `VADRMPRIMESurfaceDescriptor`.
#[repr(C)]
#[derive(Default)]
struct Descriptor {
    fourcc: u32,
    width: u32,
    height: u32,
    num_objects: u32,
    objects: [Object; 4],
    num_layers: u32,
    layers: [Layer; 4],
}

#[link(name = "va")]
unsafe extern "C" {
    fn vaSyncSurface(display: *mut c_void, surface: c_uint) -> c_int;
    fn vaExportSurfaceHandle(
        display: *mut c_void,
        surface: c_uint,
        mem_type: c_uint,
        flags: c_uint,
        descriptor: *mut c_void,
    ) -> c_int;
}

/// One plane: a buffer, where in it the plane starts, and its row length.
pub struct Plane {
    pub fd: OwnedFd,
    pub fourcc: u32,
    pub offset: u32,
    pub pitch: u32,
    /// The buffer's layout (tiling), as DRM names it.
    pub modifier: u64,
}

/// A decoded picture on the GPU, as DMA-BUF planes: luma, then chroma.
pub struct DmaBufPicture {
    pub width: u32,
    pub height: u32,
    pub planes: [Plane; 2],
    pub captured: Instant,
    /// The decoder's surface, kept until the picture is dropped.
    _frame: ff::frame::Video,
}

/// Exports a decoder's VA-API picture; `frame` must hold a VA surface.
pub(crate) fn export(
    frame: ff::frame::Video,
    captured: Instant,
) -> Result<DmaBufPicture, CodecError> {
    // SAFETY: a VAAPI frame's data[3] is its VASurfaceID, and its
    // hw_frames_ctx leads to the device context whose hwctx is an
    // AVVAAPIDeviceContext; both live as long as `frame`.
    let (display, surface) = unsafe {
        let raw = frame.as_ptr();
        if (*raw).format != ffi::AVPixelFormat::AV_PIX_FMT_VAAPI as c_int
            || (*raw).hw_frames_ctx.is_null()
        {
            return Err(CodecError::Unavailable("a VA-API picture"));
        }
        let frames = (*(*raw).hw_frames_ctx)
            .data
            .cast::<ffi::AVHWFramesContext>();
        let device = (*frames).device_ctx;
        let vaapi = (*device).hwctx.cast::<ffi::AVVAAPIDeviceContext>();
        ((*vaapi).display, (*raw).data[3] as usize as c_uint)
    };
    let mut descriptor = Descriptor::default();
    // SAFETY: `display` and `surface` are live (above); `descriptor` is the
    // DRM PRIME 2 layout libva fills for that memory type.
    let exported = unsafe {
        vaSyncSurface(display, surface) == 0
            && vaExportSurfaceHandle(
                display,
                surface,
                MEM_TYPE_DRM_PRIME_2,
                EXPORT_READ_SEPARATE,
                std::ptr::from_mut(&mut descriptor).cast(),
            ) == 0
    };
    if !exported {
        return Err(CodecError::Unavailable("an exported VA-API surface"));
    }
    // The buffers are ours now, whatever happens next.
    let objects: Vec<OwnedFd> = descriptor.objects[..descriptor.num_objects.min(4) as usize]
        .iter()
        // SAFETY: libva hands over these descriptors, open, for us to close.
        .map(|object| unsafe { OwnedFd::from_raw_fd(object.fd) })
        .collect();
    let plane = |layer: &Layer| -> Option<Plane> {
        let index = layer.object_index[0] as usize;
        Some(Plane {
            fd: objects.get(index)?.try_clone().ok()?,
            fourcc: layer.drm_format,
            offset: layer.offset[0],
            pitch: layer.pitch[0],
            modifier: descriptor.objects.get(index)?.modifier,
        })
    };
    let layers = &descriptor.layers[..descriptor.num_layers.min(4) as usize];
    let (Some(luma), Some(chroma)) = (
        layers
            .first()
            .filter(|layer| layer.drm_format == DRM_FORMAT_R8)
            .and_then(plane),
        layers
            .get(1)
            .filter(|layer| layer.drm_format == DRM_FORMAT_GR88)
            .and_then(plane),
    ) else {
        return Err(CodecError::Unavailable(
            "an NV12 surface in R8 and GR88 layers",
        ));
    };
    Ok(DmaBufPicture {
        width: frame.width(),
        height: frame.height(),
        planes: [luma, chroma],
        captured,
        _frame: frame,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::codec::EncoderConfig;
    use crate::video::codec::decoder::{Decoded, Decoder};
    use crate::video::codec::encoder::{Backend, Encoder};
    use crate::video::picture::{Picture, PixelFormat};

    #[test]
    fn a_hardware_decoded_picture_leaves_as_luma_and_chroma_planes() {
        crate::video::codec::needs_codec!();
        let config = EncoderConfig {
            width: 1280,
            height: 720,
            fps: 30,
            bitrate: 1_500_000,
        };
        let Ok(mut encoder) = Encoder::open(config, &Backend::ALL) else {
            return;
        };
        let mut decoder = Decoder::open(true).unwrap();
        if !decoder.is_hardware() {
            eprintln!("skipped: no hardware decoder here");
            return;
        }
        let picture = Picture::black(PixelFormat::Nv12, 1280, 720, Instant::now());
        let mut gpu = Vec::new();
        for number in 0..10 {
            let Some(encoded) = encoder.encode(&picture, number == 0).unwrap() else {
                continue;
            };
            for decoded in decoder
                .decode_gpu(&encoded.nal_units, Instant::now())
                .unwrap()
            {
                if let Decoded::Gpu(picture) = decoded {
                    gpu.push(picture);
                }
            }
        }

        assert!(!gpu.is_empty(), "no picture stayed on the GPU");
        let first = &gpu[0];
        assert_eq!((first.width, first.height), (1280, 720));
        assert_eq!(first.planes[0].fourcc, DRM_FORMAT_R8);
        assert_eq!(first.planes[1].fourcc, DRM_FORMAT_GR88);
        assert!(first.planes.iter().all(|plane| plane.pitch >= 1280));
        // Held pictures keep their surfaces; the decoder keeps going.
        assert!(gpu.len() >= 5, "{} pictures", gpu.len());
    }
}
