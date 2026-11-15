# Phone P7: Camera and Screen Sharing — Outline

> **This isn't a task-level plan yet.** It stops where the owner's decision (design §9 Q6) changes what gets built. After the answer, Claude writes a design of its own (`docs/superpowers/specs/…-phone-video-design.md`), then turns this outline into tasks. Nothing here is to be built as written.

**Goal:** On the phone, turn the camera on in a call, share the phone's screen, and watch other people's cameras and streams, with the desktop's limits and permissions.

## What exists

- **One H.264 API for every system** (`crates/opencord-media/src/video/codec/mod.rs`): `supported()`, `encoder::{Backend, Encoder}`, `decoder::Decoder`, `scale::Scaler`, `raw::RawDecoder`. Linux and Windows use FFmpeg and OpenH264; macOS uses VideoToolbox (`codec/macos/`).
- **Camera and screen back ends per system** (`video/camera/`, `video/screen/`), each with an `unsupported.rs` the phones build today, so `video_support()` reports nothing and the phone hides video.
- **The receiver, layers and priorities** (`video/receiver.rs`, `layers.rs`, `priorities.rs`) are system-independent and already build for the phones.
- **The desktop draws frames through Flutter textures.** The phone has no such path: React Native needs native views fed from the core without crossing JavaScript.
- **The facade drops every video event** (`VideoTrackAdded/Removed`, `VideoSizeChanged`, `CameraStopped`, `ScreenShareStopped`, `Stream*`).

## The choice (design §7.2)

| | A. Rust through the systems' codecs (Claude's pick) | B. Native Kotlin and Swift pipelines | C. OpenH264 in software |
|---|---|---|---|
| Encode and decode | `MediaCodec` through the NDK (`AMediaCodec`); VideoToolbox on iOS, sharing most of `codec/macos/` | in Kotlin and Swift; Rust sends and receives RTP only | OpenH264 for both |
| Camera | Camera2 NDK (`ACameraManager`); AVFoundation on iOS, sharing `camera/avfoundation.rs` | CameraX; AVFoundation in Swift | as A |
| Drawing | a native view per tile (`SurfaceView` / `AVSampleBufferDisplayLayer`) that the core writes into directly, through a handle the Expo module registers | the native pipeline draws itself | as A |
| Cost | the most Rust; one pipeline to keep in step | two more pipelines beside the desktop's | quickest; hot phones, no hardware decode |

## Stages after the decision (A assumed)

1. **Gates first** (as plugins M4's Stage A): `MediaCodec` encode and decode at 720p30 on the emulator and a mid-range Android phone; VideoToolbox on an iPhone (owner's Mac); frame latency from the core to a native view; the APK's growth. Any failed gate stops the plan for the owner.
2. **Watching:** decoders, the native tile view (`mobile/modules/video-view`), the facade's video events, tiles in the call grid with the desktop's priorities (who's speaking, pinned), and streams opened full screen with the stream volume.
3. **Camera:** the camera back ends, front and back switching, the camera permission's words, stopping on background (both systems stop the camera there), the desktop's limits (`max_camera_participants`).
4. **Screen sharing on Android:** a `MediaProjection` foreground service with its notification, the system's own consent dialog, the stream's audio where Android 10+ allows it.
5. **Screen sharing on iOS:** a Broadcast Upload Extension (a separate process with about 50 MB of memory) that encodes itself and hands packets to the app through an App Group. It has its own signing and its own hand checks on a real iPhone.
6. **Live checks, docs, the D-entry and the pull request.**

## Questions for the owner (with design §9 Q6)

1. A, B or C?
2. Is a stream from the phone allowed at the desktop's limits (`screen_share_max_resolution`, `screen_share_max_fps`), or capped lower on phones to save battery? Claude's pick: capped at 720p30 on phones.
3. Is iOS screen sharing in this plan, or after it? It's the slowest part and can only be tested on a real iPhone. Claude's pick: after.
