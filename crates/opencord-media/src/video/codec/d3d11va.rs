//! A D3D11VA device for the decoder (docs/plans/windows.md W3), beside
//! `vaapi.rs`. Windows picks the adapter itself: unlike `/dev/dri`, where a
//! hybrid laptop has one render node per GPU and the right one has to be
//! found, `av_hwdevice_ctx_create` with no device string takes the default
//! adapter, which is the one the display is on.
//!
//! Only decoding uses it. The encoders take their pictures in memory, as
//! NV12; D3D11 frames without a copy are V10's.

use std::ptr;
use std::sync::OnceLock;

use ffmpeg_next::ffi;

/// One D3D11 device, shared by every decoder.
pub(super) struct Device {
    context: *mut ffi::AVBufferRef,
}

// SAFETY: the device context is reference-counted atomically, and FFmpeg
// creates the D3D11 device with multithread protection on, so one device may
// be used from several threads.
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

/// The default adapter's D3D11 device, once per process.
pub(super) fn device() -> Option<&'static Device> {
    static DEVICE: OnceLock<Option<Device>> = OnceLock::new();
    DEVICE.get_or_init(open).as_ref()
}

fn open() -> Option<Device> {
    super::ffmpeg::init();
    let mut context = ptr::null_mut();
    // SAFETY: a null device string asks for the default adapter, and a null
    // options dictionary for no options; `context` is only read on success.
    let result = unsafe {
        ffi::av_hwdevice_ctx_create(
            &mut context,
            ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_D3D11VA,
            ptr::null(),
            ptr::null_mut(),
            0,
        )
    };
    let device = (result >= 0 && !context.is_null()).then_some(Device { context });
    match &device {
        Some(_) => tracing::info!("D3D11VA decoding"),
        None => tracing::debug!(result, "D3D11VA did not open"),
    }
    device
}

impl Device {
    /// A new reference, for a codec context to own.
    pub(super) fn reference(&self) -> *mut ffi::AVBufferRef {
        // SAFETY: `self.context` is live.
        unsafe { ffi::av_buffer_ref(self.context) }
    }
}
