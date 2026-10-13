# macOS plan (Phase 2, platform work)

> For the Claude Code session on the Mac, and for the person setting the Mac up.
> It covers the macOS half of Phase 2 V3, V5, V6 and V7: everything that the
> Linux machine built but could not build or test for macOS. Phase 2's plan
> ([phase-2.md](phase-2.md)) stays the spec; this file says what to build on
> macOS, in what order, how to test it, and what was decided.
> Section numbers like §9.3 refer to phase-2.md.
>
> **A Windows session works at the same time** ([windows.md](windows.md)).
> Read [platforms.md](platforms.md) first: it says which files are yours,
> which are shared, and how the work gets merged without the two sessions
> colliding. Where this file and platforms.md disagree about who may change a
> file, platforms.md wins.

## 0. Working rules

- **Branch:** `phase2-macos`, made from `phase2-remaining`.
  - Start each milestone with `git fetch && git merge origin/phase2-remaining`.
  - Push at the end of each milestone (the user asked for pushes from the
    Mac).
  - Never merge `phase2-windows`, and never commit to `main` or `pre`.
- **Your files:** change only the files [platforms.md](platforms.md) §3 gives
  to the Mac. Anything else follows §4 there: a minimal `shared: …` commit
  with the reason in the log. Linux code isn't yours: the Linux session
  merges each push and runs the Linux tests and live checks, which the Mac
  can't.
- **Wait for L1:** M0 and M1 can start now. M2 onwards need the Linux
  session's seams (platforms.md §2): the stub files you fill in, the shared
  code already calling them, and every Rust↔Dart API change this plan needs.
  Check with
  `git log origin/phase2-remaining --oneline --grep 'platform seams (L1)'`.
- **Don't start V9.** Don't use or mention the Remotion connector.
- **No questions mid-work.** Make the call, record it as a ruling (below), and
  go on. Some steps need the user at the Mac: permission prompts, picking a
  window, turning speakers on. Batch those and say exactly what to click.
- **Say where you are:** every progress message names the plan position, for
  example "macOS plan M3, step 3.2".
- **Rulings and measurements:** record each in §7 of this file, rulings as
  `Mn: Ruling: … — cost if wrong: …`. Don't edit `docs/decisions.md`,
  `docs/performance.md` or `phase-2.md`; the Linux session moves your log
  into them when it merges (platforms.md §4).
- **Style:** match the code around you. Read the Linux module you're mirroring
  first and copy its doc comments, naming and plan references. Use the same
  public API, so callers compile unchanged.
- **Checks before each commit:**
  - `cargo fmt --all --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`. Before L1, the
    dead-code warnings in shared files are expected (platforms.md §4); don't
    fix them.
  - `cargo test --workspace --lib --bins --tests`
  - In `app/`: `flutter analyze` and `flutter test`
  - Build with `CARGO_INCREMENTAL=0`.
  - Golden tests (`test/golden/`) were rendered on Linux, so they may fail on
    the Mac. That's expected; never regenerate them there.
  - A shared test that fails on macOS for a platform reason goes in the log.
    Don't edit it.

## 1. Install on the Mac

```sh
# Xcode: the full app from the App Store, not just the Command Line Tools.
sudo xcode-select -s /Applications/Xcode.app/Contents/Developer
sudo xcodebuild -license accept
xcodebuild -runFirstLaunch

# Homebrew (https://brew.sh), then:
brew install git gh cocoapods sox switchaudio-osx
# Two virtual audio devices: the tests' stand-in for PipeWire null sinks.
# This asks for your password. If they don't appear in Sound settings,
# run: sudo killall coreaudiod
brew install --cask blackhole-2ch blackhole-16ch

# Rust. Release builds are universal (Apple silicon + Intel).
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
rustup target add aarch64-apple-darwin x86_64-apple-darwin

# Flutter 3.44.0, the version CI pins.
git clone https://github.com/flutter/flutter.git -b 3.44.0 ~/flutter
echo 'export PATH="$HOME/flutter/bin:$HOME/.cargo/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc && flutter doctor

# The repository.
gh auth login
git clone https://github.com/mrafieefard/opencord.git && cd opencord
git switch phase2-remaining && git switch -c phase2-macos
```

What else is already there or isn't needed:
- `flutter_rust_bridge_codegen`: this session doesn't change the Rust↔Dart
  API (platforms.md §4).
- `python3` comes with Xcode; the check scripts use it.
- `afplay` is built in; the tests use it as "an app playing music".
- protoc isn't needed: the build uses protox.
- FFmpeg isn't needed on macOS (decision B in §3).

**A stable code signature.** Debug builds are signed ad hoc (`CODE_SIGN_IDENTITY
= "-"`). macOS ties camera, microphone, screen and input permissions to the
signature, so an ad-hoc build asks again after every rebuild. Screen Recording
also needs a relaunch each time. To avoid that, create a signing identity
once, either way:
- Xcode → Settings → Accounts → add your Apple ID (gives a free "Apple
  Development" certificate), or
- Keychain Access → Certificate Assistant → Create a Certificate…, named
  "Opencord Dev", identity type Self Signed Root, certificate type Code
  Signing.

M0 makes Debug builds use that identity without committing it.

## 2. What you will be asked to allow

macOS asks once per app. Tests that Claude runs with `cargo test` count as
your terminal app: whichever app runs Claude Code (Terminal, iTerm, VS Code).

| Permission (System Settings → Privacy & Security) | Needed for | Allow for | When |
|---|---|---|---|
| Microphone | voice | Opencord | M1 |
| Input Monitoring | global keybinds | Opencord, terminal app | M2 |
| Accessibility | tests that press keys for you | terminal app | M2 |
| Camera | camera | Opencord, terminal app | M5 |
| Screen & System Audio Recording | screen share, its sound | Opencord, terminal app | M6, M7 |

The user also has to be at the Mac to:
- pick a window in the system picker (M6, M7);
- turn the speakers on for the echo test (M1);
- for the hardware checklist (M8): plug in AirPods and a second monitor, put
  the Mac to sleep, and have an iPhone nearby for Continuity Camera
  (optional);
- start the Linux machine for a call between the two.

## 3. Where macOS stands, and the decisions

### What the code does on macOS today
- **Works, but has never been checked on a Mac:** the voice transport, the
  audio engine (cpal's Core Audio host), the processing chain, the soundboard
  and all the UI. The macOS CI job builds `main`/`pre` and PRs, never this
  branch. V7 and the keybinds have never been built on a Mac; the Linux
  session type-checked their not-Linux paths with a FreeBSD target on
  2026-10-13.
- **Linux-only, gated behind `cfg(target_os = "linux")`:**

  | Feature | Where | What macOS gets today |
  |---|---|---|
  | Cameras | `opencord-media/src/video/camera/` | `CameraError::NotSupported` |
  | H.264 codec, scaling and conversion | `video/codec/` (FFmpeg, OpenH264, VA-API) | not compiled |
  | Video sender, receiver and screen capture | `video/{sender,receiver,screen}` | not compiled |
  | Screen share sound | `share_audio/`, the whole module | not compiled |
  | Global hotkeys | `hotkeys/mod.rs::set_on_this_system` | `FocusedOnly` |
  | Flutter textures | `opencord-core/src/video/flutter.rs`; irondash deps are Linux-only in `opencord-core/Cargo.toml` | none |
  | Core screen share | `opencord-core/src/video/screen.rs` (written against the portal and PipeWire) | "not supported" |

  L1 turns each of these into a macOS stub file that reports "not
  supported", already called by the shared code (platforms.md §2). From M2 on
  you fill in those files and nothing else.

  Today, in `api/client.rs`:
  - `video_supported()` is `cfg!(linux)`, which hides the camera and Screen
    buttons.
  - `screen_audio_support()` reports "not available on this system yet".
- **The app bundle:**
  - `Info.plist` has no `NSMicrophoneUsageDescription`. Joining voice from the
    `.app` most likely crashes or records silence.
  - The entitlements turn the sandbox on but have no
    `com.apple.security.device.audio-input` or `device.camera`.
  - The deployment target is 10.15 (the plan says 13).
  - The podspec sets `s.platform = :osx, '10.11'`.
- **Frameworks need to be listed in the podspec.** The Rust library ships as a
  static archive, which carries no framework dependencies. Every framework the
  Rust code uses must be added to `OTHER_LDFLAGS` in
  `app/rust_builder/macos/opencord_core.podspec`, as CoreAudio and
  AudioToolbox already are. Otherwise `cargo test` passes and the app fails
  to link. Frameworks with APIs newer than 13.0 are linked weakly
  (`-weak_framework`), as CoreAudio is for process taps.

### Decisions (copy each into §7 as a ruling when you start)

**A. Bindings.**
- Use the `objc2` framework crates, on the same 0.3 line as the
  `objc2-core-audio` 0.3.2 that cpal already puts in `Cargo.lock`:
  - `objc2-av-foundation`, `objc2-screen-capture-kit`, `objc2-video-toolbox`
  - `objc2-core-media`, `objc2-core-video`, `objc2-core-graphics`
  - `objc2-core-foundation`, `objc2-foundation`, `objc2-app-kit`
  - `dispatch2`, `block2`
- Delegates go through `define_class!`.
- Declare the few HIToolbox (keyboard layout) functions by hand.
- Use Swift or Objective-C shims only where objc2 really can't do something
  (§9.3 allows them).
- Check OS versions at run time with `objc2::available!`.

**B. Video codec: VideoToolbox directly, no FFmpeg on macOS.**
- Why:
  - The release build is universal. Linking FFmpeg would mean building it
    for two architectures and bundling LGPL dylibs.
  - VideoToolbox is a system framework, and it's the macOS row in §7.9's
    table.
  - Its CVPixelBuffers lead straight to V10's IOSurface textures.
- Cost if wrong: about 600–900 lines of encoder/decoder glue that FFmpeg's
  `h264_videotoolbox` would have given us.

**C. Scaling and colour conversion in portable Rust, in the macOS codec
files.** Windows uses FFmpeg's swscale (its decision B), so nothing here is
shared with it.
- `codec/convert.rs` already has `bgrx_to_nv12`, `rgbx_to_nv12`,
  `yuyv_to_nv12` and `halve_nv12`, all in BT.601 video range.
- Add an NV12 scaler and NV12/I420 → RGBA, with the same matrix and range as
  `scale.rs`, either hand-written or with the `yuv` (yuvutils-rs) and
  `pic-scale` crates. Choose by measurement: 1080p NV12 → 640×360 RGBA must
  take under 2 ms on Apple silicon.
- Linux keeps swscale.

**D. Minimum macOS version for each feature.**

| Feature | Needs | Why |
|---|---|---|
| Voice, keybinds, camera, receiving video | 13.0 | the new deployment target |
| Screen share | 14.0 | the system picker, `SCContentSharingPicker` |
| Screen share sound | 14.2 | Core Audio process taps |

- On older versions the switch or button is disabled and says what's needed.
- Not built unless a Mac on 13 or 14.0–14.1 turns up:
  - Opencord's own Screens/Windows picker for macOS 13 (§9.1). The Windows
    session builds that picker UI (W6); macOS 13 could later use it through
    `screen_sources()` with `own_picker`.
  - ScreenCaptureKit audio for 13–14.1.

**E. Keep the app sandbox.**
- These work inside the sandbox: listen-only event taps (with Input
  Monitoring), ScreenCaptureKit and AVFoundation.
- Process taps: check this in M7. If the sandbox blocks them, turn it off
  only in `DebugProfile.entitlements`, record that, and leave Release to the
  user.
- Release distribution (Developer ID, hardened runtime, notarization) is the
  user's decision; see M8.

**F. Test stand-ins, like the Linux ones.**
- BlackHole 2ch/16ch in place of PipeWire null sinks.
- `SwitchAudioSource` in place of `pactl`.
- `sox -t coreaudio "BlackHole 2ch" …` in place of `parec`.
- `afplay` as "another app playing".
- `OPENCORD_SCREEN_DISPLAY=main` (new, M6, read only in
  `video/screen/macos.rs`) in place of `OPENCORD_SCREEN_NODE`: shares the
  main display without the picker.
- **Check scripts and integration tests** go in `app/tool/macos/` and
  `app/integration_test/macos/`. Scripts may `source ../e2e_server.sh`, which
  is portable, but don't change it or the shared tests (platforms.md §4).
- The real camera, with the user present, in place of the virtual PipeWire
  camera.
- Voicebots measure what was received, exactly as on Linux. That part
  doesn't depend on the platform.

## 4. Milestones

Order: M0 → M1 → (L1) → M2 → M3 → M4 → M5 → M6 → M7 → M8. Each one ends
with a commit, a push, and a line in §7. From M2 on, every file named
"stub" below is one the Linux session laid down in L1; it already has the
contract in its doc comments and is already called by the shared code.

### M0: Setup and baseline
1. Install (§1). Record these in §7: `sw_vers`, the chip (`uname -m`), the
   Xcode version, and `flutter --version`.
2. Make Debug builds use a stable signing identity without committing
   anyone's certificate or team ID:
   - `app/macos/Runner/Configs/Debug.xcconfig` gets an optional
     `#include? "Signing.local.xcconfig"`, and that file is gitignored.
   - If the target's hard-coded `CODE_SIGN_IDENTITY = "-"` in
     `project.pbxproj` overrides the xcconfig, move that setting into the
     xcconfig.
3. Check that the workspace builds and passes:
   - `cargo build --workspace`, then `cargo test --workspace --lib --bins
     --tests`, then clippy.
   - On 2026-10-13 the Linux session type-checked the not-Linux code paths
     with a FreeBSD target: they compile, with dead-code warnings in shared
     files. Anything that still fails on macOS (a C dependency, the linker)
     gets a minimal `shared:` fix and a line in the log.
4. In `app/`: `flutter pub get`, `flutter analyze`, `flutter test`.
5. Run the app: `flutter run -d macos`. Point it at a local server:
   `cargo run -p opencord-server`, or source `app/tool/e2e_server.sh`, which
   is portable. Sign in, open a text channel and send a message.
6. **Accept:** everything compiles; Rust and Flutter tests pass (goldens
   excepted); the app runs and chats.

### M1: Voice works on the Mac, measured (§7.6, V2, V3)
1. **`Info.plist` usage strings:**
   - `NSMicrophoneUsageDescription`: "Opencord uses your microphone in voice
     channels."
   - `NSCameraUsageDescription`: "Opencord uses your camera when you turn it
     on in a voice channel."
   - `NSAudioCaptureUsageDescription`: "Opencord records other apps' sound
     when you share your screen with sound."
2. **Entitlements** (DebugProfile and Release): `com.apple.security.device.audio-input`
   and `com.apple.security.device.camera`.
3. **Deployment target 13.0:**
   - `MACOSX_DEPLOYMENT_TARGET`: all three entries in `project.pbxproj`.
   - The podspec's `s.platform`. cargokit hands Xcode's value to cargo.
4. Join voice from the app: macOS asks for the microphone. Then:
   - a voicebot hears the app;
   - the app hears a voicebot;
   - the level meter moves;
   - device lists name the Mac's devices.
5. **Port `app/tool/check_user_audio.sh` and its integration test to macOS,**
   as new files: `app/tool/macos/check_user_audio.sh` and
   `app/integration_test/macos/user_audio_test.dart` (a copy of the shared
   test that takes the BlackHole device ids):
   - Before launching, set the default output to BlackHole 2ch and the
     default input to BlackHole 16ch with `SwitchAudioSource`. Restore both in
     the exit trap.
   - Record what the app plays with `sox -t coreaudio "BlackHole 2ch"`.
   - **Accept:** the same numbers as Linux, within 0.5 dB:

     | Setting | Expected |
     |---|---|
     | 50 % | −6 dB |
     | 200 % | +6 dB |
     | Mute | silence (< −90 dBFS) |
     | Mute soundboard | the sound silent, the voice unchanged |

6. **Microphone path:**
   - `afplay` a tone into BlackHole 16ch (`SwitchAudioSource -t output`); the
     app uses that as its microphone; a listening voicebot measures the tone.
   - Repeat in push-to-talk and in voice activity mode.
7. **Echo test** (V3's acceptance; needs the user, with the speakers on):
   - The app uses the MacBook's own speakers and microphone.
   - A Talker voicebot plays 440 Hz; a Listener voicebot measures 440 Hz in
     what the app sends.
   - Record the number.
   - **Accept:** Listener measures under −50 dBFS while the app plays the
     tone at about −12 dBFS.
8. Run the shared integration tests `v2`, `v3` and `v8` with `-d macos`. Log
   any that fail for a platform reason; don't edit them.

### M2: Global keybinds (§7.13, V3, the keybinds work in 937448f)
1. **`crates/opencord-media/src/hotkeys/macos.rs`** (the L1 stub) becomes
   an event tap:
   - `CGEventTapCreate(kCGSessionEventTap, kCGHeadInsertEventTap,
     kCGEventTapOptionListenOnly, keyDown | keyUp | flagsChanged)`.
   - Listen-only: Opencord never swallows a key.
   - It runs on its own thread, with a `CFRunLoop`.
   - When it gets `kCGEventTapDisabledByTimeout` or `…ByUserInput`, it
     re-enables the tap.
2. **Reuse the platform-neutral parts of `hotkeys/mod.rs`:**
   - the accelerator parsing;
   - `action_of` and `shortcut_ids`;
   - `Held`, which counts keys per action, so two push-to-talk keys hold
     until both are up.

   `x11.rs` shows how a backend uses them.
3. **Keys:**
   - Accelerators use X keysym names (`CTRL+SHIFT+m`, `Control_R`, `F13`,
     `KP_5`).
   - Printable keys: build the keycode ↔ character table from the current
     layout with `TISCopyCurrentKeyboardLayoutInputSource` and
     `UCKeyTranslate`. Rebuild it when the input source changes.
   - Function, arrow, keypad and modifier keys come from a fixed `kVK_*`
     table.
   - Modifier names: CTRL = Control, SHIFT = Shift, ALT = Option,
     LOGO = Command.
   - A modifier on its own (Right Ctrl, `kVK_RightControl`) comes from
     `flagsChanged`. Tell left from right by the device-dependent flag bits.
4. **Permission:**
   - If `CGPreflightListenEventAccess()` is false, call
     `CGRequestListenEventAccess()` once, which shows the system prompt.
   - Then return `FocusedOnly` with the reason "Allow Opencord under System
     Settings → Privacy & Security → Input Monitoring" and `settings_url` =
     `x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent`.
   - The app calls `hotkeys_set` again when its window gains focus, so a
     later grant takes effect. Check whether macOS also needs a relaunch, and
     say so in the reason if it does.
5. **App:** nothing to build. Since L1 the Keybinds page shows the reason
   and an "Open settings" button whenever `settings_url` is set. Check that
   the link opens the right pane.
6. **Known limit, for the log:** while any app has Secure Event Input on (a
   password field is focused), no event tap gets keys.
7. **Tests:**
   - Unit tests: the keysym → keycode table, flag parsing, left/right
     modifiers.
   - An `#[ignore]`d live test that posts real key events
     (`CGEventCreateKeyboardEvent` + `CGEventPost`). It needs Accessibility
     and Input Monitoring for the terminal app. Mirror the Xvfb test:
     - press and release reach the action;
     - two push-to-talk keys hold until both are up;
     - Right Ctrl alone works;
     - a toggle fires once per press.
8. **Accept:**
   - Push-to-talk works while TextEdit has focus: the listening voicebot hears
     the BlackHole tone only while the posted key is held.
   - Settings say the keybinds are global.

### M3: The H.264 codec on macOS (§7.9, V5)
1. **Same public API as Linux,** so that `video/sender.rs`,
   `video/receiver.rs` and their callers compile unchanged. Read the Linux
   modules first and mirror their signatures exactly:
   - `codec::encoder::{Backend, Encoder, EncoderConfig, Encoded}`: open with
     a backend list, `encode(picture, keyframe)`, `backend()`, `config()`.
   - `codec::decoder::Decoder`: `open(hardware)`, `is_hardware()`,
     `decode`.
   - `codec::scale::Scaler`: `new`, `picture(.., PixelFormat::Nv12)`,
     `rgba`.
   - `codec::raw::RawDecoder`: `new`, `picture(&RawFrame)`.

   They go in `video/codec/macos/` (the L1 stubs), which `codec/mod.rs`
   already selects on macOS. Set its `SUPPORTED` to true when they work. Don't
   touch the FFmpeg files: they are shared with Windows (platforms.md §3).
2. **Encoder (VideoToolbox):**
   - Open it with `VTCompressionSessionCreate`, H.264, at the configured
     size, with `kVTVideoEncoderSpecification_EnableLowLatencyRateControl`.
   - Session properties:

     | Property | Value |
     |---|---|
     | `RealTime` | true |
     | `AllowFrameReordering` | false (no B-frames) |
     | `ProfileLevel` | High, auto level (whatever low-latency mode accepts) |
     | `AverageBitRate` | the bitrate |
     | `DataRateLimits` | about 1 s of buffer |
     | `ExpectedFrameRate` | the frame rate |
     | `MaxKeyFrameInterval` | an hour's worth (the GOP the Linux encoders use) |

   - A requested keyframe sets `kVTEncodeFrameOptionKey_ForceKeyFrame`.
   - **Every picture comes out of its own encode call.** After each
     `VTCompressionSessionEncodeFrame`, call `VTCompressionSessionCompleteFrames`
     (the Linux tests check this).
   - **The bitstream:**
     - VideoToolbox gives AVCC (length-prefixed). Split it into NAL units.
     - On every keyframe, put the SPS and PPS first, from
       `CMVideoFormatDescriptionGetH264ParameterSetAtIndex`. The transport and
       every receiver expect parameter sets in-band before an IDR (`NAL_SPS`,
       `NAL_PPS`).
   - **Backends:** the macOS `Backend` enum lives in your files.
     `Backend::VideoToolbox` = hardware, opened with
     `EnableHardwareAcceleratedVideoEncoder`. VideoToolbox's software encoder
     is the fallback (§7.9's macOS row). `is_hardware()` reports
     `UsingHardwareAcceleratedVideoEncoder`.
   - **Bitrate changes:** change the properties in place if the Linux API
     changes bitrate without reopening.
3. **Decoder (VideoToolbox):**
   - Collect the SPS and PPS from the Annex B input.
   - `CMVideoFormatDescriptionCreateFromH264ParameterSets` →
     `VTDecompressionSessionCreate`, with output NV12 video range
     (`kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange`).
   - Decode synchronously.
   - Make a new session when the SPS changes: a screen share changes size.
   - Turn Annex B into AVCC before decoding.
   - `open(hardware)`: hardware requires the hardware decoder; otherwise
     VideoToolbox chooses.
4. **Scaler and RawDecoder,** in portable Rust (decision C):
   - RawDecoder:
     - NV12/I420: repack.
     - BGRx/RGBx: through `convert.rs`.
     - YUYV: through `convert.rs`.
     - MJPEG: `CodecError::Unavailable`, since AVFoundation delivers NV12.
   - The scaler matches Linux's colours:
     - BT.601 video range;
     - `halve_nv12` as the fast path for exact halves.
5. The sender and receiver already compile on macOS (L1) and pick the
   codec up by themselves.
6. **Voicebot:** since L1 its video code compiles everywhere and skips at run
   time where the codec isn't supported. Once `SUPPORTED` is true, two
   voicebots should send each other video through a local server on the Mac.
7. **Tests** (port the Linux ones that don't depend on a platform):
   - the encoder opens, and every picture comes out of its encode call;
   - a keyframe on request is an IDR led by the SPS and PPS;
   - the bitrate stays within ±25 % of target over 10 s of `video/pattern.rs`;
   - an encode → decode round trip keeps PSNR > 35 dB;
   - a size change mid-stream decodes;
   - the scaler's colour patches match the expected BT.601 values.
8. **Accept:**
   - `app/tool/macos/check_v4.sh` (the `check_v4.sh` flow) passes on the Mac:
     a voicebot camera → a watching voicebot at 360p, every picture intact.
   - Encode cost per layer (720p30, 360p30, 180p15) is in the log.

### M4: Seeing video in the app (§7.11)
1. Since L1 the textures (irondash) compile on macOS, and `video_support()`
   reports `receive` from your codec's `SUPPORTED`. Check that they work:
   - irondash takes RGBA on macOS and converts it itself;
   - check the colours with the test pattern: red must look red.
2. **Test:** `app/integration_test/macos/receive_video_test.dart`, after the
   `v5_test.dart` flow, receive half. A voicebot's test-pattern camera appears
   in the app, and `video_texture_stats` shows pictures both presented and
   drawn.
5. **Accept:**
   - The app on the Mac shows a voicebot's camera and a voicebot's screen
     stream.
   - Watching opens focus mode.
   - Hidden tiles stop receiving (`video_set_wants`).

### M5: Camera (§8, V5)
1. **`video/camera/avfoundation.rs`** (the L1 stub):
   - **`cameras()`:**
     - Use `AVCaptureDeviceDiscoverySession` for video.
     - Device types on 14+: built-in wide-angle, `External` and
       `ContinuityCamera`. On 13: `ExternalUnknown`.
     - `id` = `uniqueID`; `name` = `localizedName`.
   - **Access:** check `authorizationStatusForMediaType`.
     - Not determined: call `requestAccessForMediaType` and wait for it.
     - Denied or restricted: `CameraError::Denied`.
   - **`Capture::start`:**
     - Pick the device's best format up to 1280×720 that does 30 fps, and set
       `activeFormat` and the frame durations under `lockForConfiguration`.
     - Use `AVCaptureVideoDataOutput` with NV12 video range and
       `alwaysDiscardsLateVideoFrames`.
     - The delegate runs on a serial dispatch queue. It locks the pixel
       buffer, packs the planes by their `bytesPerRow` (use `camera::pack`),
       and `try_send`s a `RawFrame` (Nv12). A full queue drops the frame.
     - `mode()` reports the format in use.
   - **Unplugged** (`AVCaptureDeviceWasDisconnectedNotification`, a session
     runtime error): close the frame queue. The core then shows that the
     camera went away, as on Linux.
2. `camera::Capture` already has its macOS variant (L1). Make the stub's
   `support()` say supported; the camera buttons then show on macOS.
3. **Tests:**
   - An `#[ignore]`d real-camera test, run with the user present: the camera
     is listed, the first frame arrives within 1 s at the chosen mode, and
     frames stop after stopping.
   - Live, `app/tool/macos/check_v5.sh` (the `check_v5.sh` flow): the app
     turns the camera on, and a watching voicebot decodes every picture.
4. **Accept** (§8 and V5):
   - Camera on and off each take under 1 s (measured).
   - Three layers sent.
   - The preview is mirrored; what's sent isn't.
   - CPU in the log.
   - With an iPhone nearby, Continuity Camera is listed and works (optional).

### M6: Screen share (§9, V6)
1. **`video/screen/macos.rs`** (the L1 stub, plus files beside it):
   ScreenCaptureKit. It implements the contract of platforms.md §2.3:
   `support()` (with `own_picker: false`) and `start(request, frames)`, which
   picks and then captures.
   - **Picking, on 14.0+:**
     - Use `SCContentSharingPicker.sharedPicker`.
     - Configuration: allowed modes "one display" and "one window";
       `excludedBundleIDs` = Opencord's own, so its windows aren't offered.
     - An observer (`define_class!`) handles three cases:
       - `didUpdateWithFilter`: a source was picked.
       - `didCancelForStream`: `ScreenError::Cancelled`.
       - `StartDidFailWithError`: `Failed`.
     - Set `active` and call `present`.
     - Keep the filter; its kind (`filter.style`: display or window) goes in
       `Picked.kind`.
   - **Capture:**
     - `SCStreamConfiguration`:
       - width and height from §9.2's rule: the preset's pixel budget,
         applied to `contentRect` × `pointPixelScale`;
       - `minimumFrameInterval` = 1/fps;
       - `pixelFormat` = `420v` (NV12; no conversion needed);
       - `showsCursor` on;
       - `queueDepth` 5.
     - Output callback: take only frames whose `SCStreamFrameInfoStatus` is
       complete, and pack them into a `RawFrame`.
     - `size` and `finished()` behave as Linux `ScreenCapture`'s do.
   - **When the size changes** (a window resized, a resolution change),
     update the configuration. The core already reshapes the layers.
   - **When the stream stops** (`stream:didStopWithError:`: the window closed,
     the display was unplugged, or the user stopped sharing from the menu
     bar), close the queue. The core then shows "The shared window was
     closed" or ends the share.
   - **Minimized windows deliver nothing.** Check that the sender keeps
     sending the last picture at 1 fps (§9.4). The "<app> is minimized"
     notice is optional.
   - **For tests:** `OPENCORD_SCREEN_DISPLAY=main` shares the main display
     through `SCShareableContent` without the picker. It needs Screen
     Recording for the app or the terminal app.
2. **Core:** nothing to change. Since L1 the core's `video/screen.rs` calls
   `start` and `support` and no longer knows about the portal or PipeWire.
3. **Permission:** find out whether the system picker path (14+) still needs
   the Screen Recording permission (§9.3 asks). Record the answer.
4. **macOS 13:** `ScreenError::NotSupported` with "Screen sharing needs macOS
   14 or later", shown by the app.
5. **App:** nothing to build. Since L1 the share dialog says a system picker
   follows whenever `own_picker` is false. Check that the wording fits macOS;
   if it doesn't, log it (the dialog is the Windows session's file).
6. **Tests:**
   - `app/tool/macos/check_v6.sh` (the `check_v6.sh` flow), using the
     environment variable: the app shares
     the main display at 720p30 with something moving on screen; a watching
     voicebot decodes every picture at the right size and about 30 fps;
     stopping ends the share for everyone.
   - A window share through the real picker, with the user picking.
7. **Accept:**
   - V6's acceptance on macOS.
   - CPU at 1080p30 in the log.
   - Optional: with an XDR display, HDR content isn't washed out (§9.4).
     Record what you see.

### M7: Screen share sound (§10.1, §10.3, §10.7, V7)
1. **Already done in L1:** `Source`, `AppInfo` and `Captured` live in
   `share_audio/mod.rs`; `sender.rs` is shared; the core calls the OS's
   `Capture`, `available`, `apps_with_audio` and `support`.
2. **`share_audio/coreaudio.rs`** (the L1 stub): Core Audio process taps,
   14.2+.
   - **Opencord's own process object:**
     `kAudioHardwarePropertyTranslatePIDToProcessObject` of `getpid()`.
   - **What the tap covers:**

     | Source | Tap |
     |---|---|
     | `Everything` | `CATapDescription initStereoGlobalTapButExcludeProcesses:[own]` |
     | `App(name)` or `Process(pid)` | `initStereoMixdownOfProcesses:` with every process object from `kAudioHardwarePropertyProcessObjectList` whose `kAudioProcessPropertyBundleID` starts with the app's bundle ID (browser and Electron helpers), or that is the process or its child |
     | `Nothing` | no tap |

   - **The tap:**
     - Settings: `private`, `muteBehavior` = `CATapUnmuted` (the user still
       hears it), and a name.
     - Create it with `AudioHardwareCreateProcessTap`.
     - Read `kAudioTapPropertyUID` and `kAudioTapPropertyFormat`.
   - **A private aggregate device:**
     - Create it with `AudioHardwareCreateAggregateDevice`.
     - Settings: a unique UID; private; `TapAutoStart`; the default output as
       main sub-device; `TapList` = the tap's UID, with drift compensation.
     - Start it with `AudioDeviceCreateIOProcIDWithBlock` then
       `AudioDeviceStart`.
   - **In the IOProc:**
     - Make it stereo f32.
     - Resample to 48 kHz with rubato if the tap isn't at 48 kHz.
     - Send `Captured { samples, position, captured }`, exactly as
       `pipewire.rs` does (read what it means by `position`).
   - **`share(source)` while live:** set the tap's `kAudioTapPropertyDescription`
     to the new processes, or rebuild the tap.
   - **New helpers in app mode:** a browser tab can start playing after the
     share starts. Listen for `ProcessObjectList` changes and update the tap.
   - **Stopping:** in this order: `AudioDeviceStop`,
     `AudioDeviceDestroyIOProcID`, `AudioHardwareDestroyAggregateDevice`,
     `AudioHardwareDestroyProcessTap`.
   - **`available()` and `support()`:** supported on macOS 14.2+. Otherwise
     the reason is "Sharing sound needs macOS 14.2 or later".
   - **`apps_with_audio()`:** process objects with
     `kAudioProcessPropertyIsRunningOutput`, without Opencord, grouped by app.
     Names come from `NSRunningApplication`.
   - **Permission:** the first tap triggers the prompt
     (`NSAudioCaptureUsageDescription`).
     - A refusal produces silence, not an error.
     - Log when the tap stays all-zero for 2 s while `apps_with_audio` isn't
       empty. Record that it can't be told apart otherwise.
   - **The sandbox:** check that taps work in it (decision E).
3. **Which app a window belongs to:**
   - Read it from the picker's filter if the SDK has it (check
     `SCContentFilter`'s included applications or windows on 15.x, with the
     owning app's bundle ID and pid), and return it in `Picked.audio`.
   - Otherwise use the app's existing "Sound from…" dialog, as on Wayland. It
     is already wired through `screen_audio_apps` and
     `screen_share_audio_from`.
4. **Echo:** the processor already feeds the shared sound to the echo
   canceller (§10.6). Check that this works on macOS; there's nothing to
   build.
5. **Tests:**
   - `#[ignore]`d Rust tests in `crates/opencord-media/tests/share_audio_macos.rs`,
     mirroring `share_audio.rs`:
     - A global tap captures an `afplay` tone, while a tone the test process
       plays itself is absent.
     - A tap of one `afplay`'s process captures only its frequency while a
       second `afplay` plays another.
   - Since L1 the voicebot's `tests/screen_audio.rs` runs on macOS by
     itself; check that it passes.
6. **Accept** (§10.7, the "no double audio" test), with every device on
   BlackHole so nothing is audible. Port `check_v7.sh` as
   `app/tool/macos/check_v7.sh`:
   - The app shares the screen with sound.
   - A Talker voicebot talks in the call, which the app plays.
   - `afplay` plays music.
   - A Watcher voicebot measures: the music in the stream at about −12 dBFS
     on both channels; Talker below −55 dBFS in the stream.
   - **Mutation check:** without excluding Opencord, the check must fail.
   - **Window test**, with the user picking Safari's window: only Safari's
     sound is in the stream while `afplay` plays another tone.

### M8: Platform checklist, CI, packaging, docs
1. **The §15 checklist on the Mac,** with the user:
   - two monitors with mixed DPI (Retina plus an external one);
   - sleep and wake during a call;
   - AirPods connected and removed during a call: the device fallback toast;
   - an XDR display;
   - Continuity Camera;
   - a call between the Mac app and the Linux app through one server on the
     LAN, talking both ways (V2's two machines); with the Windows PC as
     well once it gets there (platforms.md §5).

   Record each result.
2. **CI:** the macOS job must keep building. It needs no Homebrew packages,
   only system frameworks. Before pushing M3, M5, M6 and M7:
   - Check locally with `flutter build macos --release`. This is the
     universal build and needs the `x86_64-apple-darwin` target.
   - Change only the `desktop` job's macOS steps. Propose a `cargo test` job
     on `macos-latest` (for the tests that need no hardware) in the log; the
     Linux session adds it (platforms.md §4).
3. **Packaging that macOS needs now:**
   - The usage strings and entitlements are done in M1. Check that Release
     matches Debug.
   - Developer ID signing, the hardened runtime and notarization need a paid
     Apple Developer account. That's the user's decision.
   - Until then the release zip stays ad-hoc signed. Its permission prompts
     still work, but they are asked again after each update. Put the
     sentence for the release notes in the log; the `release` job is
     shared.
4. **Log:** every measurement, ruling and known limit (Secure Event Input,
   the minimum versions) in §7. The Linux session moves them into
   `docs/decisions.md`, `docs/performance.md` and `docs/architecture.md`.

## 5. Out of scope on macOS
- "Use the operating system's processing" (Voice Processing I/O, §7.4); it's
  off by default.
- Lowering other apps' volume: macOS hides it (§12 table).
- Zero-copy IOSurface textures and capture → encoder without copying (V10).
- Opencord's own picker for macOS 13, and ScreenCaptureKit audio for 13–14.1
  (decision D).
- V9.

## 6. What the Linux machine does meanwhile
- It pulls each push of `phase2-macos`, then runs:
  - `cargo test`;
  - clippy;
  - `flutter test`;
  - the live checks a change could affect (`check_v6.sh` and `check_v7.sh`
    after M6/M7's refactors).
- It reports or fixes any Linux regression on its own branch.
- `phase2-macos` is merged into `phase2-remaining` when M8 is done, or
  earlier if both sides agree.

## 7. Log (the Mac session fills this in)

| Milestone | Commit | Result, measurements, what's not done |
|---|---|---|
| M0 | | |
| M1 | | |
| M2 | | |
| M3 | | |
| M4 | | |
| M5 | | |
| M6 | | |
| M7 | | |
| M8 | | |

Rulings (`Mn: Ruling: … — cost if wrong: …`):
- (none yet)

`shared:` commits (file, why):
- (none yet)
