# Phone P7: Camera and Screen Sharing — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** On the phone, watch other people's cameras and streams in a call, turn your own camera on, and share your screen, with the desktop's limits and permissions, at the lowest cost to the phone's battery and heat.

**Architecture (design §7.2: B, chosen by the owner for performance):**
- **Frames never leave the phone's hardware path.** The camera or the screen feeds the hardware encoder's input surface directly; the hardware decoder draws straight into the tile's view. Android: Camera2, `MediaProjection`, `MediaCodec` with input and output `Surface`s, `SurfaceView`. iOS: `AVCaptureSession`, `VTCompressionSession` (camera pixel buffers are IOSurface-backed, so no copy), `AVSampleBufferDisplayLayer` (it decodes H.264 itself). No frame is copied to the CPU, converted, or passed to Rust or JavaScript.
- **Rust keeps the transport and its decisions.** Encoded H.264 goes into the transport as `VideoFrame`s (`crates/opencord-media/src/transport/mod.rs:149`) and comes out as `ReceivedVideo` (`:163`). The transport's `Encode` events (which layers, bitrates, frame-rate and size scales), its `KeyframeRequested` events, and the tile wants (`VideoWant`) that choose layers stay as they are.
- **The seam is `Video::on_event`** (`crates/opencord-core/src/video.rs:298`). On the desktop, `VoiceEvent::Video` goes to a `VideoReceiver` that decodes, and `Encode` and `KeyframeRequested` go to a `VideoSender` that encodes. On phones they go to **native** receivers and senders through the bridge.
- **The bridge is a small C interface** in `crates/opencord-mobile` (`src/video_bridge.rs`, header `mobile/modules/video-view/cpp/opencord_video.h`): native code calls Rust to hand over encoded frames and register views; Rust calls native code through function pointers registered once. Kotlin reaches it through a JNI shim; Swift calls it directly. It's in the same library as the core, so it shares the core's state. JavaScript does only what it does on the desktop: start and stop, and say which tiles show which tracks at which sizes.
- **Lighter defaults on phones** (Claude's calls, for the owner to check):
  - a phone's camera sends two layers, 180p and 360p (`l` and `m` of `LAYERS` in `crates/opencord-media/src/video/layers.rs`), never 720p;
  - a phone's screen share is capped at 720p and 30 fps, under the server's own limits;
  - at most 4 hardware decoders run at once (`VIDEO_DECODERS_MAX`); tiles beyond them show the person's avatar;
  - in the background, decoding and the camera stop; a screen share keeps going.

**Tech Stack:** Rust 2024 (`opencord-core`, `opencord-media`, `opencord-mobile`), Kotlin (Android SDK; the NDK for the JNI shim), Swift (VideoToolbox, AVFoundation, ReplayKit), the phone app (Expo 58, React Native 0.88). **No new libraries:** every codec and capture API is the system's own.

**Spec:** design §3 (P7), §7.2; the desktop's video plan, Phase 2 §7.8–7.11 and §8 (`docs/plans/phase-2.md`); `docs/voice.md`. Read `crates/opencord-core/src/video.rs` and `crates/opencord-media/src/video/{sender,receiver,layers,priorities}.rs` before Task 1.

## Global Constraints

- **Branch:** `feat/mobile-features`, the one branch and **one pull request into `beta`** for every phone plan (the owner, 2026-11-15). It already holds the design and the plans. Work on it in order, P0 first; never on a branch of its own. Never merge; never push to `beta`, `canary` or `main`. The pull request is open already (P0 opened it): push to it, add this plan's section to its description, and read its checks at each stage end.
- **Scope:** this plan **does change the core**: `crates/opencord-core/src/video.rs` and a new `video/external.rs`, and `crates/opencord-media/src/video/layers.rs`, all gated to `target_os = "android"` and `"ios"`, so nothing changes on the desktop (`cargo test -p opencord-core -p opencord-media` on Linux and the desktop app's tests prove it). No server or protocol change.
- **Gates need real phones.** The emulator's codecs are software, so performance is measured on the owner's Android phone (over `adb`) and on an iPhone through the Mac session (git bundles, as the phone apps plan did). Stage A stops for the owner if a gate fails or a phone isn't available.
- **Never log** frames or packet contents: log sizes, rates, layer names and error kinds.
- P0's Global Constraints from "Ids are decimal strings" on apply, and P0's Lessons.

## Working Rules

P0's apply word for word. **Ledger:** `.superpowers/sdd/phone-p7/progress.md`; gate numbers in `.superpowers/sdd/phone-p7/gates.md`.

Scores (the owner, 2026-11-15): **0** no code; **1–3 Gemini**; **4–5 Claude**, handed over with claude-bridge `delegate_task`, each a cold session, so each task's section is self-contained. The rubric is P0's "Task owners". Most of this plan is Claude's: by the rubric, real-time video on two systems' hardware scores 5.

| Task | Score | Owner | Why |
|---|---|---|---|
| 1 The C bridge | 5 | Claude | FFI with function pointers called from Rust threads into native code; buffer lifetimes across the boundary |
| 2 The core's native video back end | 5 | Claude | Changes the core's real-time video routing; concurrency between the transport's events and native callbacks |
| 3 Android: the tile view and decoding | 5 | Claude | `MediaCodec` on a `Surface`, its async callbacks, the surface's life inside React Native views |
| 4 iOS: the tile view and decoding | 5 | Claude | `AVSampleBufferDisplayLayer` with format descriptions from SPS and PPS, timing, the view's life |
| 5 The gates | 5 | Claude | Measurements on real phones that decide whether the plan goes on |
| 6 Facade, JavaScript core and fake | 3 | Gemini | Records and events in the facade's pattern, and a fake. Focus: `texture_id` becomes the view id; no `u64` reaches JavaScript |
| 7 Tiles in the call and the stream page | 3 | Gemini | Layout and wants from tile sizes, testable on the fake. Focus: a hidden tile wants nothing; rotation |
| 8 Decoder budget and the background | 4 | Claude | Hardware decoder limits; stopping and resuming with keyframes across app states |
| 9 Android: the camera | 5 | Claude | A Camera2 session feeding two encoders' surfaces and the preview; bitrate and keyframe control |
| 10 iOS: the camera | 5 | Claude | `AVCaptureSession` into two `VTCompressionSession`s; rotation; control |
| 11 The camera in the call | 3 | Gemini | Buttons, permission words, the server's limits. Focus: both `ControlBar` files; `camera_allowed` |
| 12 Android: sharing the screen | 5 | Claude | `MediaProjection` consent and Android 14's foreground service rules; `VirtualDisplay` to the encoder; the screen's sound |
| 13 iOS: sharing the screen | 5 | Claude | A Broadcast Upload Extension (a separate process with about 50 MB) that encodes itself and hands packets to the app |
| 14 Sharing in the call | 3 | Gemini | Start and stop, the banner, your own stream tile. Focus: a share the system ended says why |
| 15 Live checks, docs, D-entry, PR | 4 | Claude | Performance on real phones, judged against the gates; two systems against the desktop |

**Order:** Stage A: 1, 2, 3, 4, 5 (all Claude's). Stage B: **8**, then 6, 7. Stage C: **9, 10**, then 11. Stage D: **12, 13**, then 14. Stage E: **15**.

### Commands

As P0, plus:

| What | Command, after `source mobile/scripts/env.sh &&` |
|---|---|
| The core's and media's tests, desktop | `cargo test -p opencord-core -p opencord-media` |
| The phone build of the facade (compile only) | `cargo ndk -t arm64-v8a check -p opencord-mobile` |
| The bridge's tests | `cargo test -p opencord-mobile video_bridge` |
| A real Android phone | `adb -s <serial> …` (the owner gives the serial); CPU: `adb shell top -b -n 1 -p <pid>`; heat: `adb shell dumpsys thermalservice` |

### Stages

| Stage | Tasks | At its end |
|---|---|---|
| A — The bridge, proven | 1–5 | Gates pasted in `gates.md`; full checks; push; draft PR; checks read. **Stop for the owner if a gate fails.** |
| B — Watching | 6–8 | Full checks; push; **review of A–B** |
| C — Camera | 9–11 | Full checks; push |
| D — Screen sharing | 12–14 | Full checks; push; **review of C–D** |
| E — Live | 15 | Full checks; push; review round; report |

## Review Focus

1. **A tile's view goes away while its decoder is writing** (scrolling the grid, closing the call sheet, rotating). Expected: the decoder stops before its surface is released; no crash, nothing drawn into a dead surface, and the view's id is unregistered from Rust. Pinned in Task 3 (`a released surface stops its decoder first`) and Task 4 (`a removed layer stops its feed`).
2. **The app goes to the background in a call.** Expected: every decoder and the camera stop (both systems take hardware codecs and cameras away there) while the track stays (others see its last frame, then the avatar). Back in the foreground, decoders restart from a requested keyframe within a second. A screen share keeps going. Pinned in Task 8: `the background stops decoders and the camera, and the foreground asks for keyframes`.
3. **A fifth tile.** Expected: at most `VIDEO_DECODERS_MAX` = 4 decoders; the fifth tile shows the avatar and gets a decoder when another tile goes, without the others stalling. Pinned in Task 8: `a fifth tile waits for a decoder`.
4. **The transport asks for fewer layers or a keyframe while encoding** (someone stops watching; a viewer joins). Expected: the encoder of a layer nobody wants stops within a second; a keyframe request is honoured on the next frame. Pinned in Task 2 (`encode and keyframe requests reach the native sender`) and Task 9 (`an unwanted layer's encoder stops`).
5. **The system ends a screen share** (Android's Stop in the notification; iOS's red status bar). Expected: the stream ends on the server, `ScreenShareStopped` reaches the app with its reason, and the UI goes back to not sharing. Pinned in Task 12 (`the system stopping the projection ends the stream`) and Task 13 (`the extension finishing ends the stream`).

---

## Stage A — The bridge, proven

### Task 1: The C bridge

**Files:** create `crates/opencord-mobile/src/video_bridge.rs`, `mobile/modules/video-view/cpp/opencord_video.h`; modify `crates/opencord-mobile/src/lib.rs`.

**Interfaces (produces):** a C ABI: every function `#[unsafe(no_mangle)] pub extern "C"`, every pointer argument documented with who owns it and for how long.
- **Native → Rust:**
  - `oc_video_register(callbacks: *const OcVideoCallbacks) -> i32`, once at start; 0 on success, refused the second time.
  - `oc_video_view_ready(view_id: i64)`, `oc_video_view_gone(view_id: i64)`: a tile view's surface exists, or went.
  - `oc_video_push_encoded(sender_id: i64, layer: u8, keyframe: bool, width: u16, height: u16, captured_ns: i64, data: *const u8, len: usize)`: one encoded picture of one layer, in Annex B. Rust splits it into NAL units with `opencord_media::video::codec::annexb::split` (it builds on every system). `data` is borrowed for the call only, so Rust copies it.
  - `oc_video_sender_stopped(sender_id: i64, reason: i32, message: *const c_char)`: the camera or the projection ended by itself.
- **Rust → native,** in `OcVideoCallbacks` (C function pointers plus `context: *mut c_void`):
  - `create_view(context) -> i64` (a new tile view's id, for the core's `Textures::create`) and `release_view(context, view_id)`;
  - `decode(context, view_id, keyframe: bool, timestamp_90k: u32, data: *const u8, len: usize)`: one access unit in Annex B (`annexb::join`), borrowed for the call;
  - `start_sender(context, sender_id, kind: i32 /* camera | screen */, layers: *const OcLayer, count: usize) -> i32`, `encode(context, sender_id, layer_mask: u32, bitrates: *const u32, count: usize, fps_scale: f32, size_scale: f32)`, `keyframe(context, sender_id, layer: u8)`, `stop_sender(context, sender_id)`.
- Callbacks are called on Rust's threads. Native code returns at once and hands the work to its own threads. Rust never holds a lock while calling one.
- [ ] Tests (Rust, with the callbacks written in Rust as `extern "C" fn`): `a_pushed_frame_reaches_the_sink_split_into_nal_units`, `callbacks_are_called_without_a_lock_held` (a callback that calls back into `oc_video_push_encoded` doesn't deadlock), `register_twice_is_refused`, `a_null_message_is_an_empty_string`. Run (fail), implement, run (pass); clippy; commit `feat(mobile): a C bridge for native video`.

### Task 2: The core's native video back end

**Files:** create `crates/opencord-core/src/video/external.rs`; modify `crates/opencord-core/src/video.rs`, `crates/opencord-media/src/video/layers.rs` (add `phone_camera_layers(width, height)`, giving only `l` and `m`), `crates/opencord-core/src/client.rs` (on phones, `video_support()` answers from what the native side registered).

**Interfaces:**
- **Consumes** Task 1's callbacks through a Rust trait, so the core doesn't depend on the facade: `pub trait NativeVideo: Send + Sync { fn create_view(&self) -> Option<i64>; fn release_view(&self, id: i64); fn decode(&self, view_id: i64, video: &ReceivedVideo); fn start_sender(&self, id: i64, kind: SenderKind, layers: &[Layer]) -> Result<(), CoreError>; fn encode(&self, id: i64, rids: &[String], bitrates: &[u32], fps_scale: f32, size_scale: f32); fn keyframe(&self, id: i64, layer: u8); fn stop_sender(&self, id: i64); }`, set with `Client::set_native_video(Arc<dyn NativeVideo>)`. The facade implements it over the C callbacks.
- **Produces,** on Android and iOS only (`#[cfg(any(target_os = "android", target_os = "ios"))]`):
  - `Video` uses `external.rs`'s `ExternalReceiver` (its `push` calls `decode` for the track's view; its `set_size` does nothing, since the view scales) and `ExternalSender` (its `encode` and `keyframe` call the native side).
  - Frames come back through `Video::push_encoded(sender_id, …)`, which builds a `VideoFrame` with the sender's `track_id` and calls the connection's `send_video`.
  - `start_camera` asks for `start_sender(kind: Camera, layers: phone_camera_layers(…))`; `screen_share_start` asks for `start_sender(kind: Screen, layers: screen_layers(…))`, with the size capped at 1280×720 and 30 fps.
  - `VideoTrackAdded`'s `texture_id` is the native view's id from `create_view`, so the phone's tiles follow the desktop's flow unchanged.
- [ ] Tests (a `NativeVideo` fake recording its calls): `encode and keyframe requests reach the native sender`, `a received picture goes to its track's view`, `a native frame goes out on the connection with the sender's track`, `phone camera layers are l and m only`, `a phone screen share is capped at 720p30`, `the desktop path is unchanged` (the existing `video.rs` tests pass on Linux). Run (fail), implement, run (pass); `cargo test -p opencord-core -p opencord-media` on Linux and the `cargo ndk … check`; commit `feat(core): video through the phone's own codecs`.

### Task 3: Android: the tile view and decoding

**Files:** create `mobile/modules/video-view/`, an Expo module: `expo-module.config.json`, `package.json`, `android/src/main/java/dev/opencord/videoview/{VideoViewModule,VideoTileView,TileDecoder,Bridge}.kt`, `android/src/main/cpp/bridge_jni.cpp`, `CMakeLists.txt`, `src/index.ts`. Modify `mobile/package.json`, `mobile/app.config.ts`, `crates/opencord-mobile/src/video_bridge.rs` (the `NativeVideo` implementation over the callbacks).

- `VideoTileView`, a `SurfaceView`, is created by React Native with a `viewId` prop. On `surfaceCreated` it calls `oc_video_view_ready`; on `surfaceDestroyed` it stops its `TileDecoder` **first**, then calls `oc_video_view_gone`.
- `TileDecoder`: `MediaCodec` for `video/avc` in async mode on its own `HandlerThread`, configured with the view's `Surface` and reconfigured on a new SPS (a size change). Access units are queued in arrival order, with `presentationTimeUs` from the 90 kHz timestamp; output is released with `render = true`.
- The JNI shim keeps one global reference to `Bridge` and turns each callback into a call on it; `decode`'s bytes are copied into direct `ByteBuffer`s from a small pool, with no allocation per frame.
- [ ] Tests: instrumented (`android/src/androidTest`): `a released surface stops its decoder first`, `a size change reconfigures`; a JNI unit test with a fake bridge: `decode reaches the view's decoder`. Run on the emulator (it proves function, not speed). Commit `feat(mobile): video tiles decoded by MediaCodec on Android`.

### Task 4: iOS: the tile view and decoding

**Files:** `mobile/modules/video-view/ios/{VideoViewModule,VideoTileView,TileFeed,Bridge}.swift`, and a module map for `opencord_video.h`.

- `VideoTileView` hosts an `AVSampleBufferDisplayLayer` (`videoGravity = .resizeAspect`). `TileFeed` builds a `CMVideoFormatDescription` from each keyframe's SPS and PPS (`CMVideoFormatDescriptionCreateFromH264ParameterSets`), converts each access unit from Annex B to length-prefixed, wraps it in a `CMSampleBuffer` with `kCMSampleAttachmentKey_DisplayImmediately`, and enqueues it. A failed layer (`status == .failed`) is flushed, asks for a keyframe, and waits for it.
- [ ] Tests (XCTest, run by CI's iOS job): `a keyframe builds a format description`, `annex b becomes length prefixed`, `a removed layer stops its feed`. Commit `feat(mobile): video tiles on iOS through AVSampleBufferDisplayLayer`.

### Task 5: The gates

**Files:** create `mobile/src/app/dev/video.tsx` (a dev screen: N tiles of a desktop's camera, a frames-drawn counter per tile, and a camera switch once Stage C exists), `.superpowers/sdd/phone-p7/gates.md`.

With the desktop sending a 720p camera into a call on a local server:
- **G1, the bridge:** 4 tiles at 360p and 30 fps each, for 10 minutes, on the owner's Android phone: no frame lost at the bridge (Rust's count of frames handed over equals the native decoders' count of frames queued).
- **G2, the cost of watching:** in the same run, the app's CPU averages at most 25 % of one core (`top`), the thermal status stays `NONE` or `LIGHT`, and every tile draws at least 28 fps.
- **G3, iOS:** the same on an iPhone through the Mac session: no loss, at least 28 fps, and Xcode's energy gauge never at "Very high".
- **G4, size:** the APK grows by less than 1 MB (no libraries are added; the bridge and the views are small).
- If any gate fails: stop, write the numbers in `gates.md`, and ask the owner.
- [ ] Measure; paste every number and how it was taken; commit `test(mobile): the video gates`. Stage A's end.

## Stage B — Watching

### Task 6: Facade, JavaScript core and fake

**Files:** `crates/opencord-mobile/src/api/video.rs` (new), `src/events.rs`, `src/types.rs`; `mobile/src/core/{api,native}.ts`, `fake/FakeCore.ts`.
- **Calls:** `video_set_wants(Vec<VideoWantRecord { track_id, width, height }>)`, `stream_watch(stream_key, watching)`, `set_stream_volume`, `set_stream_mute`, `video_support() → VideoSupportRecord`.
- **Events** leave the dropped list: `VideoTrackAdded { server_key, channel_id, user_id, track_id, kind, view_id: Option<String>, width, height }` (`view_id` from `texture_id`), `VideoTrackRemoved`, `VideoSizeChanged`, `StreamCreated`, `StreamUpdated`, `StreamDeleted`, `StreamViewers`.
- **The fake** adds two camera tracks and one stream to its call; their view ids draw a moving test pattern in the native view's fake mode (a flag the module takes under `yarn android:fake`).
- [ ] Tests: `video_events_reach_javascript`, `a_view_id_is_a_string`; jest: `the fake has tracks with view ids`. Commit `feat(mobile): video calls and events in the facade`.

### Task 7: Tiles in the call and the stream page

**Files:** modify `mobile/src/features/voice/{VoicePane,ParticipantTile,tileGrid}.tsx|ts`; create `features/voice/VideoTile.tsx`, `features/voice/StreamPage.tsx`, route `app/server/[serverKey]/stream/[streamKey].tsx`.
- A participant with a camera track shows `VideoTile` (the native view, with `viewId` from the event) instead of the avatar, shaped by the track's width and height. On every layout change, `videoSetWants` gets each visible tile's size in pixels (`PixelRatio`); a tile scrolled out or hidden wants `0×0`, as on the desktop.
- A stream shows as a tile with the sharer's name and `LIVE`. A tap opens `StreamPage` full screen (landscape allowed there only), with the stream's volume and mute, and `streamWatch(key, true)` while it's open.
- [ ] Tests: `wants follow tile sizes`, `a hidden tile wants nothing`, `opening a stream watches it and closing stops`. Screenshots (the fake, phone and tablet, both themes). Commit `feat(mobile): video tiles and streams in the call`.

### Task 8: Decoder budget and the background

**Files:** modify `mobile/modules/video-view/{android,ios}` (a `DecoderBudget` of `VIDEO_DECODERS_MAX` = 4 per process); create `mobile/src/features/voice/useVideoLifecycle.ts`.
- A tile view takes a decoder from the budget when its surface is ready. Without one it reports `waiting`, and React Native shows the avatar over it. A freed decoder goes to the waiting tile that became visible first.
- When `AppState` leaves `active`, every decoder and the camera sender stop (Tasks 9 and 10 honour it). Back to `active`, decoders start, and the wants are sent again so the core asks each watched track for a keyframe.
- [ ] Tests: `a fifth tile waits for a decoder`, `the background stops decoders and the camera, and the foreground asks for keyframes`. Commit `feat(mobile): a decoder budget and video in the background`. Stage B's end and review.

## Stage C — Camera

### Task 9: Android: the camera

**Files:** `mobile/modules/video-view/android/.../{CameraSender,LayerEncoder}.kt`.
- A Camera2 session with three outputs: the preview's `SurfaceView` and two `LayerEncoder` input surfaces (`MediaCodec.createInputSurface()`), 640×360 at 30 fps and 320×180 at 15 fps. If the device refuses three outputs at those sizes, one 640×360 output goes through a GL fan-out (a `SurfaceTexture` drawn into both encoder surfaces); write a ruling naming the device.
- Each `LayerEncoder`: H.264 Baseline (the desktop decodes it everywhere), CBR at the bitrate `encode` gives (`setParameters` with `KEY_VIDEO_BITRATE`), a keyframe on `keyframe` (`PARAMETER_KEY_REQUEST_SYNC_FRAME`), stopped when its layer leaves the mask. Output goes to `oc_video_push_encoded`, with SPS and PPS before each keyframe.
- Front and back cameras; the picture is rotated through the encoder's input orientation, never by copying.
- [ ] Tests (instrumented): `an unwanted layer's encoder stops`, `a keyframe request makes the next frame a keyframe`, `switching cameras keeps the track`. On the owner's phone: camera on for 10 minutes with the desktop watching both layers; CPU and heat in `gates.md`, against G2's thresholds. Commit `feat(mobile): the camera on Android`.

### Task 10: iOS: the camera

**Files:** `mobile/modules/video-view/ios/{CameraSender,LayerEncoder}.swift`.
- `AVCaptureSession` at the nearest 16:9 preset to 640×360; `AVCaptureVideoDataOutput` hands IOSurface-backed `CVPixelBuffer`s straight to two `VTCompressionSession`s: 640×360 at 30 fps and 320×180 at 15 fps (every second buffer), with `kVTCompressionPropertyKey_RealTime`, Baseline, `AverageBitRate` from `encode`, and `kVTEncodeFrameOptionKey_ForceKeyFrame` on `keyframe`. `AVCaptureVideoPreviewLayer` draws the preview. The device's orientation goes into the capture connection, not the pixels.
- [ ] Tests (XCTest): `an unwanted layer's session is invalidated`, `force keyframe is set on request`. Measured on an iPhone through the Mac session. Commit `feat(mobile): the camera on iOS`.

### Task 11: The camera in the call

**Files:** modify `features/voice/ControlBar.tsx` and `ControlBar.ios.tsx` (a camera button, shown when `videoSupport().camera`, the server's `camera_allowed` and P0's access give `video`); create `features/voice/SelfPreview.tsx`; modify `app.config.ts` (the camera permission sentence, new on the phone: `Opencord uses the camera when you turn it on in a call.`).
- Camera on: `cameraStart()`, and the self preview as a small tile in the corner (drag to move, tap to flip). Off: `cameraStop()`. A refusal for `max_camera_participants` shows the server's words as a toast.
- [ ] Tests: `the button shows only when allowed`, `on and off reach the core`, `a refusal shows the server's words`. Screenshots. Commit `feat(mobile): your camera in a call`.

## Stage D — Screen sharing

### Task 12: Android: sharing the screen

**Files:** `mobile/modules/video-view/android/.../ScreenSender.kt`; modify `mobile/modules/voice-service` (its service gains `mediaProjection` in `foregroundServiceType`, started with that type only after consent, as Android 14 requires) and `app.config.ts` (`FOREGROUND_SERVICE_MEDIA_PROJECTION`).
- The system's consent dialog (`MediaProjectionManager.createScreenCaptureIntent()`); a `VirtualDisplay` rendering into the main layer's encoder surface, and the low layer's when `screen_layers` gives two; the size from the display, capped at 1280×720. The projection's `onStop` ends the stream through `oc_video_sender_stopped`.
- The screen's sound (Android 10 and up): `AudioPlaybackCaptureConfiguration` into the core's existing screen-audio path, when the share asked for sound. Not offered below Android 10.
- [ ] Tests (instrumented where the system allows; otherwise a fake projection): `the system stopping the projection ends the stream`, `the service starts its projection type only after consent`. On the owner's phone: share for 10 minutes with the desktop watching; numbers in `gates.md`. Commit `feat(mobile): screen sharing on Android`.

### Task 13: iOS: sharing the screen

**Files:** a Broadcast Upload Extension target added by a config plugin (`mobile/plugins/withBroadcastExtension.ts`) and its `SampleHandler.swift`; an App Group (`group.dev.opencord.opencord`); `mobile/modules/video-view/ios/ScreenSender.swift` (the app's side).
- The extension encodes with its own `VTCompressionSession` (720p cap, 30 fps, the bitrate the app last wrote to the group's shared settings) and writes each encoded picture to the app through a Unix socket in the App Group's container. It never links the Rust core (memory). The app's `ScreenSender` reads the socket and calls `oc_video_push_encoded`; keyframe requests go back on the same socket. The extension finishing (`broadcastFinished`, or the user tapping the red status bar) ends the stream.
- The start button is an `RPSystemBroadcastPickerView` with the extension preselected.
- [ ] Tests (XCTest): `a picture crosses the socket whole`, `the extension finishing ends the stream`; the extension's memory measured under 45 MB through the Mac session. Commit `feat(mobile): screen sharing on iOS`.

### Task 14: Sharing in the call

**Files:** modify both `ControlBar` files (a share button, shown with the `screenshare` permission and within the server's limits); create `features/voice/SharingBanner.tsx`.
- Start: Android's consent or iOS's picker. While sharing, a banner `You're sharing your screen` with `Stop` (new on the phone), and your own stream tile marked `LIVE`. A share the system ended shows the reason from `ScreenShareStopped` as a toast.
- [ ] Tests: `stop ends the share`, `a share ended by the system shows why`. Screenshots. Commit `feat(mobile): screen sharing in the call`. Stage D's end and review.

## Stage E — Live

### Task 15: Live checks, docs, D-entry, PR

- [ ] On the owner's Android phone and an iPhone, with the desktop in the call: watch 4 cameras, turn the camera on, share the screen, go to the background and back, rotate, add a fifth tile. Measure the gates again and judge them against Stage A's numbers.
- [ ] `docs/mobile.md` ("Not there yet" loses camera and screen sharing; how video works on phones); `docs/voice.md` (the native path); the iPhone and real-phone hand checks.
- [ ] The next D-number: B and why, the bridge, the phone's layer and decoder limits. From `git diff`.
- [ ] `.github/check.sh`; push; PR description; `gh pr checks` green; final review round; report.
