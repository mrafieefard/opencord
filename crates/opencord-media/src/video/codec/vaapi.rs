//! A VA-API device for the encoders (plan §7.9). The driver is chosen per
//! render node from its kernel driver rather than read from
//! `LIBVA_DRIVER_NAME`, which hybrid laptops often point at NVIDIA's
//! decode-only driver for every node.

use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::OnceLock;

use ffmpeg_next::ffi;

use super::CodecError;

/// Surfaces an encoder's input pool holds.
const POOL: i32 = 8;

/// One VA display, shared by every encoder.
pub(super) struct Device {
    context: *mut ffi::AVBufferRef,
}

// SAFETY: the device context is reference-counted atomically, and libva
// allows one display to be used from several threads.
unsafe impl Send for Device {}
// SAFETY: as above; nothing in `Device` is mutated after it is opened.
unsafe impl Sync for Device {}

impl Drop for Device {
    fn drop(&mut self) {
        // SAFETY: `context` came from `av_hwdevice_ctx_create` and is owned
        // by this device.
        unsafe { ffi::av_buffer_unref(&mut self.context) };
    }
}

/// The first render node whose VA-API driver opens, once per process.
pub(super) fn device() -> Option<&'static Device> {
    static DEVICE: OnceLock<Option<Device>> = OnceLock::new();
    DEVICE.get_or_init(open_any).as_ref()
}

fn open_any() -> Option<Device> {
    super::ffmpeg::init();
    let mut nodes: Vec<PathBuf> = std::fs::read_dir("/dev/dri")
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("renderD"))
        })
        .collect();
    nodes.sort();
    nodes.into_iter().find_map(|node| {
        let driver = kernel_driver(&node).and_then(|kernel| va_driver(&kernel))?;
        let device = open(&node, driver);
        match &device {
            Some(_) => tracing::info!(node = %node.display(), driver, "VA-API encoding"),
            None => tracing::debug!(node = %node.display(), driver, "VA-API did not open"),
        }
        device
    })
}

/// The VA-API driver for a kernel driver; `None` where VA-API cannot
/// encode (NVIDIA's VA-API driver only decodes; NVENC covers those GPUs).
fn va_driver(kernel: &str) -> Option<&'static str> {
    match kernel {
        "i915" | "xe" => Some("iHD"),
        "amdgpu" | "radeon" => Some("radeonsi"),
        _ => None,
    }
}

fn kernel_driver(node: &Path) -> Option<String> {
    let name = node.file_name()?;
    let link =
        std::fs::read_link(Path::new("/sys/class/drm").join(name).join("device/driver")).ok()?;
    Some(link.file_name()?.to_str()?.to_owned())
}

fn open(node: &Path, driver: &str) -> Option<Device> {
    let path = CString::new(node.to_str()?).ok()?;
    let key = CString::new("driver").ok()?;
    let value = CString::new(driver).ok()?;
    let mut options = ptr::null_mut();
    let mut context = ptr::null_mut();
    // SAFETY: the strings outlive the calls (FFmpeg copies them), and the
    // dictionary is freed whatever the result.
    let result = unsafe {
        ffi::av_dict_set(&mut options, key.as_ptr(), value.as_ptr(), 0);
        let result = ffi::av_hwdevice_ctx_create(
            &mut context,
            ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI,
            path.as_ptr(),
            options,
            0,
        );
        ffi::av_dict_free(&mut options);
        result
    };
    (result >= 0 && !context.is_null()).then_some(Device { context })
}

/// A pool of NV12 surfaces of one size, for an encoder's input.
pub(super) struct Frames(*mut ffi::AVBufferRef);

// SAFETY: frames contexts are reference-counted atomically; each encoder
// uses its own from one thread at a time.
unsafe impl Send for Frames {}

impl Drop for Frames {
    fn drop(&mut self) {
        // SAFETY: owned reference from `av_hwframe_ctx_alloc`.
        unsafe { ffi::av_buffer_unref(&mut self.0) };
    }
}

impl Device {
    /// A new reference, for a codec context to own.
    pub(super) fn reference(&self) -> *mut ffi::AVBufferRef {
        // SAFETY: `self.context` is live.
        unsafe { ffi::av_buffer_ref(self.context) }
    }

    pub(super) fn frames(&self, width: u32, height: u32) -> Result<Frames, CodecError> {
        let failed = || CodecError::Ffmpeg("VA-API surfaces could not be set up".to_owned());
        // SAFETY: `self.context` is a live device context; the frames
        // context is filled in before `av_hwframe_ctx_init`, and released
        // if that fails.
        unsafe {
            let mut frames = ffi::av_hwframe_ctx_alloc(self.context);
            if frames.is_null() {
                return Err(failed());
            }
            let context = (*frames).data.cast::<ffi::AVHWFramesContext>();
            (*context).format = ffi::AVPixelFormat::AV_PIX_FMT_VAAPI;
            (*context).sw_format = ffi::AVPixelFormat::AV_PIX_FMT_NV12;
            (*context).width = i32::try_from(width).map_err(|_| failed())?;
            (*context).height = i32::try_from(height).map_err(|_| failed())?;
            (*context).initial_pool_size = POOL;
            if ffi::av_hwframe_ctx_init(frames) < 0 {
                ffi::av_buffer_unref(&mut frames);
                return Err(failed());
            }
            Ok(Frames(frames))
        }
    }
}

impl Frames {
    /// A new reference, for a codec context to own.
    pub(super) fn reference(&self) -> *mut ffi::AVBufferRef {
        // SAFETY: `self.0` is live.
        unsafe { ffi::av_buffer_ref(self.0) }
    }

    /// Uploads a frame in system memory to a surface from the pool.
    pub(super) fn upload(
        &self,
        frame: &ffmpeg_next::frame::Video,
    ) -> Result<ffmpeg_next::frame::Video, CodecError> {
        let mut surface = ffmpeg_next::frame::Video::empty();
        // SAFETY: `surface` is an empty frame that `av_hwframe_get_buffer`
        // fills from this pool; `frame` is a valid NV12 frame of the pool's
        // size.
        let result = unsafe {
            let result = ffi::av_hwframe_get_buffer(self.0, surface.as_mut_ptr(), 0);
            if result < 0 {
                result
            } else {
                ffi::av_hwframe_transfer_data(surface.as_mut_ptr(), frame.as_ptr(), 0)
            }
        };
        if result < 0 {
            return Err(CodecError::Ffmpeg(
                ffmpeg_next::Error::from(result).to_string(),
            ));
        }
        Ok(surface)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intel_and_amd_get_their_drivers_and_nvidia_none() {
        assert_eq!(va_driver("i915"), Some("iHD"));
        assert_eq!(va_driver("xe"), Some("iHD"));
        assert_eq!(va_driver("amdgpu"), Some("radeonsi"));
        assert_eq!(va_driver("nvidia"), None);
        assert_eq!(va_driver("nouveau"), None);
    }
}
