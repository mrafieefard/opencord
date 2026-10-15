//! H.264 encoders (plan §7.9): on Linux VA-API on Intel and AMD and NVENC on
//! NVIDIA; on Windows NVENC, AMF, Quick Sync and Media Foundation
//! (docs/plans/windows.md W3); Cisco's OpenH264 in software on both.
//! Real-time settings throughout: no B-frames, rate control with about a
//! second of buffer, one keyframe at the start and then only on request,
//! every picture out as soon as it is in.

use ffmpeg_next as ff;

use super::openh264::Software;
#[cfg(target_os = "linux")]
use super::vaapi::{self, Frames};
use super::{CodecError, NAL_IDR, annexb, ffmpeg, nal_type};
pub use super::{Encoded, EncoderConfig};
use crate::video::picture::{Picture, PixelFormat};

/// Pictures between keyframes the encoder would make by itself: an hour,
/// so in practice they come only on request.
const GOP: u32 = 30 * 60 * 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    #[cfg(target_os = "linux")]
    VaApi,
    Nvenc,
    #[cfg(windows)]
    Amf,
    #[cfg(windows)]
    Qsv,
    #[cfg(windows)]
    MediaFoundation,
    OpenH264,
}

impl Backend {
    /// Hardware first, as the plan says.
    ///
    /// [`Backend::MediaFoundation`] is deliberately **not** here, though it
    /// opens and holds its bitrate: it keeps 16 pictures before letting the
    /// first one out, which a call cannot afford, and windows.md W3 step 2
    /// says such an encoder is dropped. It stays openable by name, and
    /// `media_foundation_holds_pictures_back` keeps the measurement that
    /// decided it.
    #[cfg(target_os = "linux")]
    pub const ALL: [Backend; 3] = [Backend::VaApi, Backend::Nvenc, Backend::OpenH264];
    #[cfg(windows)]
    pub const ALL: [Backend; 4] = [
        Backend::Nvenc,
        Backend::Amf,
        Backend::Qsv,
        Backend::OpenH264,
    ];

    pub fn is_hardware(self) -> bool {
        self != Backend::OpenH264
    }

    /// The FFmpeg encoder it is; `None` for the one that isn't FFmpeg's.
    fn ffmpeg_name(self) -> Option<&'static str> {
        Some(match self {
            #[cfg(target_os = "linux")]
            Self::VaApi => "h264_vaapi",
            Self::Nvenc => "h264_nvenc",
            #[cfg(windows)]
            Self::Amf => "h264_amf",
            #[cfg(windows)]
            Self::Qsv => "h264_qsv",
            #[cfg(windows)]
            Self::MediaFoundation => "h264_mf",
            Self::OpenH264 => return None,
        })
    }
}

pub struct Encoder {
    backend: Backend,
    config: EncoderConfig,
    inner: Inner,
}

enum Inner {
    Ffmpeg(Hardware),
    OpenH264(Box<Software>),
}

struct Hardware {
    encoder: ff::encoder::video::Encoder,
    /// VA-API's input surfaces; every other encoder takes frames in memory.
    /// D3D11 frames without a copy are V10's.
    #[cfg(target_os = "linux")]
    frames: Option<Frames>,
    packet: ff::Packet,
    pts: i64,
}

impl Encoder {
    /// The first of `backends` that opens for this configuration.
    pub fn open(config: EncoderConfig, backends: &[Backend]) -> Result<Self, CodecError> {
        let mut failure = CodecError::Unavailable("an H.264 encoder");
        for &backend in backends {
            let opened = match backend {
                Backend::OpenH264 => {
                    Software::open(config.width, config.height, config.fps, config.bitrate)
                        .map(|software| Inner::OpenH264(Box::new(software)))
                }
                hardware => open_hardware(hardware, config).map(Inner::Ffmpeg),
            };
            match opened {
                Ok(inner) => {
                    return Ok(Self {
                        backend,
                        config,
                        inner,
                    });
                }
                Err(error) => {
                    tracing::debug!(?backend, %error, "an encoder did not open");
                    failure = error;
                }
            }
        }
        Err(failure)
    }

    pub fn backend(&self) -> Backend {
        self.backend
    }

    pub fn config(&self) -> EncoderConfig {
        self.config
    }

    /// Encodes a picture of the configured size; an IDR picture when
    /// `keyframe` is set.
    pub fn encode(
        &mut self,
        picture: &Picture,
        keyframe: bool,
    ) -> Result<Option<Encoded>, CodecError> {
        if (picture.width, picture.height) != (self.config.width, self.config.height) {
            return Err(CodecError::WrongPicture);
        }
        let stream = match &mut self.inner {
            Inner::Ffmpeg(hardware) => hardware.encode(picture, keyframe)?,
            Inner::OpenH264(software) => software.encode(picture, keyframe)?,
        };
        let nal_units = annexb::split(&stream);
        if nal_units.is_empty() {
            return Ok(None);
        }
        let keyframe = nal_units.iter().any(|unit| nal_type(unit) == Some(NAL_IDR));
        Ok(Some(Encoded {
            nal_units,
            keyframe,
        }))
    }
}

fn open_hardware(backend: Backend, config: EncoderConfig) -> Result<Hardware, CodecError> {
    ffmpeg::init();
    let name = backend
        .ffmpeg_name()
        .ok_or(CodecError::Unavailable("that encoder"))?;
    let codec = ff::encoder::find_by_name(name).ok_or(CodecError::Unavailable("that encoder"))?;
    let mut video = ff::codec::context::Context::new_with_codec(codec)
        .encoder()
        .video()
        .map_err(ffmpeg::error)?;
    let fps = i32::try_from(config.fps).unwrap_or(30).max(1);
    video.set_width(config.width);
    video.set_height(config.height);
    video.set_time_base((1, fps));
    video.set_frame_rate(Some((fps, 1)));
    video.set_gop(GOP);
    video.set_max_b_frames(0);
    video.set_bit_rate(config.bitrate as usize);
    video.set_max_bit_rate(config.bitrate as usize);
    // SAFETY: a field of the context this encoder owns, set before opening.
    unsafe {
        (*video.as_mut_ptr()).rc_buffer_size = i32::try_from(config.bitrate).unwrap_or(i32::MAX);
    }
    let mut options = ff::Dictionary::new();
    // VA-API is the only one that wants its input in hardware surfaces.
    #[cfg(target_os = "linux")]
    let frames = match backend {
        Backend::VaApi => {
            options.set("profile", "high");
            let device = vaapi::device().ok_or(CodecError::Unavailable("VA-API"))?;
            let frames = device.frames(config.width, config.height)?;
            video.set_format(ff::format::Pixel::VAAPI);
            // SAFETY: the context takes the new reference and releases it
            // when it is freed.
            unsafe { (*video.as_mut_ptr()).hw_frames_ctx = frames.reference() };
            options.set("rc_mode", "VBR");
            // One picture in flight: out as soon as it is in.
            options.set("async_depth", "1");
            Some(frames)
        }
        _ => {
            video.set_format(ff::format::Pixel::NV12);
            nvenc(&mut options);
            None
        }
    };
    #[cfg(windows)]
    {
        video.set_format(ff::format::Pixel::NV12);
        match backend {
            Backend::Nvenc => nvenc(&mut options),
            // AMF's own low-latency usecase, constant bitrate, speed over
            // quality, and the parameter sets in front of every IDR, which
            // AMF leaves out by default.
            Backend::Amf => {
                options.set("profile", "high");
                options.set("usage", "ultralowlatency");
                options.set("rc", "cbr");
                options.set("quality", "speed");
                options.set("latency", "1");
                options.set("bf", "0");
                options.set("forced_idr", "1");
                options.set("header_spacing", "0");
                // One picture in flight; AMF's default of 16 would hold
                // fifteen of them back.
                options.set("async_depth", "1");
            }
            // Quick Sync: the fastest preset, no look-ahead (it delays by
            // its depth), one picture in flight, and the bitrate obeyed
            // frame by frame rather than on average.
            Backend::Qsv => {
                options.set("profile", "high");
                options.set("preset", "veryfast");
                options.set("look_ahead", "0");
                options.set("async_depth", "1");
                options.set("bf", "0");
                options.set("forced_idr", "1");
                options.set("low_delay_brc", "1");
                options.set("idr_interval", "0");
            }
            // Media Foundation, which is both the last hardware encoder and
            // the way to Microsoft's software one. `hw_encoding` is left
            // alone so it takes whichever it has; asking for hardware on a
            // machine without it fails the open instead of falling back.
            //
            // No `profile`: `h264_mf` has no such option, and FFmpeg then
            // tries to parse "high" as the generic enum and refuses to open
            // the encoder at all. Measured — it is why this encoder reported
            // "Invalid argument" the first time round.
            Backend::MediaFoundation => {
                options.set("rate_control", "cbr");
                options.set("scenario", "video_conference");
            }
            Backend::OpenH264 => return Err(CodecError::Unavailable("that encoder")),
        }
    }
    let encoder = video.open_as_with(codec, options).map_err(ffmpeg::error)?;
    Ok(Hardware {
        encoder,
        #[cfg(target_os = "linux")]
        frames,
        packet: ff::Packet::empty(),
        pts: 0,
    })
}

/// NVENC's settings, the same on both systems.
fn nvenc(options: &mut ff::Dictionary) {
    options.set("profile", "high");
    options.set("preset", "p4");
    options.set("tune", "ull");
    options.set("zerolatency", "1");
    options.set("delay", "0");
    options.set("rc", "cbr");
    options.set("forced-idr", "1");
}

impl Hardware {
    fn encode(&mut self, picture: &Picture, keyframe: bool) -> Result<Vec<u8>, CodecError> {
        let mut frame = if picture.format == PixelFormat::Nv12 {
            ffmpeg::to_frame(picture)
        } else {
            ffmpeg::to_frame(&ffmpeg::to_nv12(picture))
        };
        #[cfg(target_os = "linux")]
        let mut input = match &self.frames {
            Some(frames) => frames.upload(&frame)?,
            None => std::mem::replace(&mut frame, ff::frame::Video::empty()),
        };
        #[cfg(windows)]
        let mut input = std::mem::replace(&mut frame, ff::frame::Video::empty());
        input.set_pts(Some(self.pts));
        self.pts += 1;
        input.set_kind(if keyframe {
            ff::picture::Type::I
        } else {
            ff::picture::Type::None
        });
        self.encoder.send_frame(&input).map_err(ffmpeg::error)?;
        let mut stream = Vec::new();
        loop {
            match self.encoder.receive_packet(&mut self.packet) {
                Ok(()) => stream.extend_from_slice(self.packet.data().unwrap_or_default()),
                Err(ff::Error::Other { errno }) if errno == ff::error::EAGAIN => break,
                Err(error) => return Err(ffmpeg::error(error)),
            }
        }
        Ok(stream)
    }
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

    fn keyframes_on_request(backend: Backend) {
        let Some(mut encoder) = open(backend, config(640, 360, 500_000)) else {
            return;
        };
        let start = Instant::now();
        let mut encode = |number, keyframe| {
            encoder
                .encode(&scene(640, 360, number, start), keyframe)
                .unwrap()
                .expect("every picture comes out at once")
        };

        let first = encode(0, true);
        assert!(first.keyframe);
        for parameter_set in [NAL_SPS, NAL_PPS, NAL_IDR] {
            assert!(
                types(&first).contains(&parameter_set),
                "{:?}",
                types(&first)
            );
        }
        for number in 1..20 {
            let delta = encode(number, false);
            assert!(!delta.keyframe, "picture {number}");
            assert!(!types(&delta).contains(&NAL_SPS));
        }
        let asked = encode(20, true);
        assert!(asked.keyframe);
        assert!(types(&asked).contains(&NAL_SPS));
        assert_eq!(encoder.backend(), backend);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn vaapi_makes_keyframes_only_on_request() {
        keyframes_on_request(Backend::VaApi);
    }

    #[test]
    fn nvenc_makes_keyframes_only_on_request() {
        keyframes_on_request(Backend::Nvenc);
    }

    #[cfg(windows)]
    #[test]
    fn amf_makes_keyframes_only_on_request() {
        keyframes_on_request(Backend::Amf);
    }

    #[cfg(windows)]
    #[test]
    fn quick_sync_makes_keyframes_only_on_request() {
        keyframes_on_request(Backend::Qsv);
    }

    /// Why Media Foundation is not in [`Backend::ALL`] (windows.md W3 step
    /// 2: an encoder that holds pictures back is dropped). It is kept as a
    /// test rather than a comment so that if Microsoft ever makes it answer
    /// at once, this fails and says so.
    #[cfg(windows)]
    #[test]
    fn media_foundation_holds_pictures_back() {
        let Some(mut encoder) = open(Backend::MediaFoundation, config(640, 360, 500_000)) else {
            return;
        };
        let start = Instant::now();

        let held = (0..30)
            .map(|number| {
                encoder
                    .encode(&scene(640, 360, number, start), number == 0)
                    .unwrap()
            })
            .take_while(Option::is_none)
            .count();

        eprintln!("Media Foundation held {held} pictures before the first came out");
        assert!(
            held > 0,
            "Media Foundation answers at once now; it can go back into Backend::ALL"
        );
    }

    /// The other half of the same rule: nothing in [`Backend::ALL`] may hold
    /// a picture back, because the sender hands the transport whatever each
    /// call returns and a delayed encoder is latency nobody asked for.
    #[test]
    fn no_encoder_in_all_holds_a_picture_back() {
        for backend in Backend::ALL {
            let Some(mut encoder) = open(backend, config(640, 360, 500_000)) else {
                continue;
            };
            let start = Instant::now();

            // Software may skip a picture to hold the bitrate, so only the
            // first one has to be immediate — that is what "no delay" means.
            let first = encoder.encode(&scene(640, 360, 0, start), true).unwrap();

            assert!(first.is_some(), "{backend:?} held its first picture back");
        }
    }

    #[test]
    fn openh264_makes_keyframes_only_on_request() {
        keyframes_on_request(Backend::OpenH264);
    }

    #[test]
    fn every_encoder_keeps_near_its_bitrate() {
        for backend in Backend::ALL {
            let Some(mut encoder) = open(backend, config(640, 360, 500_000)) else {
                continue;
            };
            let start = Instant::now();
            let mut bytes = 0;
            for number in 0..90 {
                // Software may skip a picture to hold the bitrate.
                let encoded = encoder
                    .encode(&scene(640, 360, number, start), number == 0)
                    .unwrap();
                bytes += encoded.map_or(0, |encoded| {
                    encoded.nal_units.iter().map(Vec::len).sum::<usize>()
                });
            }

            let bitrate = bytes * 8 / 3;
            eprintln!("{backend:?}: {bitrate} bit/s");
            assert!(
                (200_000..=650_000).contains(&bitrate),
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

    /// Whichever backend `ALL` settles on, every hardware one before it in
    /// the list must have refused to open — that is all "hardware first"
    /// can be checked to mean without asking each system what it has.
    #[test]
    fn hardware_comes_first_where_there_is_some() {
        let Ok(encoder) = Encoder::open(config(320, 180, 150_000), &Backend::ALL) else {
            eprintln!("no encoder here");
            return;
        };
        let chosen = encoder.backend();

        for earlier in Backend::ALL {
            if earlier == chosen {
                break;
            }
            assert!(
                Encoder::open(config(320, 180, 150_000), &[earlier]).is_err(),
                "{earlier:?} opens, so {chosen:?} should not have been chosen"
            );
        }
    }
}
