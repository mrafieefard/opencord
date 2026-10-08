//! Cisco's OpenH264, the software encoder (plan §7.9). Cisco's own builds
//! are covered by its H.264 patent licence, so the library is loaded at run
//! time rather than built in: Cisco's build from the app's folder (checked
//! against the builds the bindings know), or, while developing, a system
//! copy of the same version.

use std::path::PathBuf;
use std::sync::OnceLock;

use openh264::OpenH264API;
use openh264::encoder::{
    BitRate, Encoder, EncoderConfig, FrameRate, IntraFramePeriod, RateControlMode, UsageType,
};
use openh264::formats::YUVBuffer;
use openh264_sys2::API;

use super::CodecError;
use crate::video::picture::{Picture, PixelFormat};

/// The version the bindings were made for.
const VERSION: (u32, u32) = (2, 6);
/// What a system copy is called.
const SYSTEM_LIBRARIES: [&str; 2] = ["libopenh264.so.8", "libopenh264.so.7"];

static LIBRARY_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Where the app keeps Cisco's library.
pub fn set_library_dir(dir: PathBuf) {
    let _ = LIBRARY_DIR.set(dir);
}

fn api() -> Result<OpenH264API, CodecError> {
    if let Some(dir) = LIBRARY_DIR.get()
        && let Ok(entries) = std::fs::read_dir(dir)
    {
        for path in entries.flatten().map(|entry| entry.path()) {
            let cisco = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("libopenh264-"));
            if cisco && let Ok(api) = OpenH264API::from_blob_path(&path) {
                return Ok(api);
            }
        }
    }
    for name in SYSTEM_LIBRARIES {
        // SAFETY: only the version is asked for before it is checked; that
        // function's signature has not changed since OpenH264 1.x.
        let Ok(api) = (unsafe { OpenH264API::from_blob_path_unchecked(name) }) else {
            continue;
        };
        // SAFETY: as above.
        let version = unsafe { api.WelsGetCodecVersion() };
        if (version.uMajor, version.uMinor) == VERSION {
            return Ok(api);
        }
    }
    Err(CodecError::Unavailable("OpenH264"))
}

/// One OpenH264 encoder, for one layer.
pub(super) struct Software {
    encoder: Encoder,
    width: u32,
    height: u32,
}

impl Software {
    pub(super) fn open(
        width: u32,
        height: u32,
        fps: u32,
        bitrate: u32,
    ) -> Result<Self, CodecError> {
        let config = EncoderConfig::new()
            .bitrate(BitRate::from_bps(bitrate))
            .max_frame_rate(FrameRate::from_hz(fps as f32))
            .usage_type(UsageType::CameraVideoRealTime)
            .rate_control_mode(RateControlMode::Bitrate)
            // It can only hold the bitrate if it may skip a picture.
            .skip_frames(true)
            // Only the first picture is an IDR; later ones come on request.
            .intra_frame_period(IntraFramePeriod::from_num_frames(0));
        let encoder = Encoder::with_api_config(api()?, config)
            .map_err(|error| CodecError::OpenH264(error.to_string()))?;
        Ok(Self {
            encoder,
            width,
            height,
        })
    }

    /// The picture as an Annex B byte stream.
    pub(super) fn encode(
        &mut self,
        picture: &Picture,
        keyframe: bool,
    ) -> Result<Vec<u8>, CodecError> {
        if (picture.width, picture.height) != (self.width, self.height) {
            return Err(CodecError::WrongPicture);
        }
        if keyframe {
            self.encoder.force_intra_frame();
        }
        let buffer =
            YUVBuffer::from_vec(to_i420(picture), self.width as usize, self.height as usize);
        let stream = self
            .encoder
            .encode(&buffer)
            .map_err(|error| CodecError::OpenH264(error.to_string()))?;
        Ok(stream.to_vec())
    }
}

fn to_i420(picture: &Picture) -> Vec<u8> {
    if picture.format == PixelFormat::I420 {
        return picture.data.clone();
    }
    let luma = picture.width as usize * picture.height as usize;
    let mut data = Vec::with_capacity(picture.data.len());
    data.extend_from_slice(&picture.data[..luma]);
    let chroma = &picture.data[luma..];
    data.extend(chroma.iter().step_by(2));
    data.extend(chroma.iter().skip(1).step_by(2));
    data
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn nv12_chroma_is_split_into_two_planes() {
        let mut data = vec![50; 8];
        data.extend([1, 2, 3, 4]);
        let nv12 = Picture::from_data(PixelFormat::Nv12, 4, 2, data, Instant::now()).unwrap();

        assert_eq!(&to_i420(&nv12)[8..], &[1, 3, 2, 4]);
    }
}
