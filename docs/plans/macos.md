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

## Next phase: M2 to M8 (from 2026-10-13)

**Done:** M0 and M1 (merged), and the Linux session's seams, L1
(platforms.md §2). Start with `git fetch && git merge origin/phase2-remaining`
and check that `git log --oneline --grep 'platform seams (L1)'` prints a
commit.

**This phase is everything left on macOS: M2 to M8 in §4, in order.** The
Windows session waits until you finish M8.

| Milestone | What you build | Your files (all L1 stubs or your own folders) |
|---|---|---|
| M2 | Global keybinds: a listen-only event tap, Input Monitoring | `hotkeys/macos.rs` |
| M3 | VideoToolbox H.264 encoder and decoder; Rust scaling and conversion | `video/codec/macos/` |
| M4 | Others' video shown in the app (irondash textures) | none new: check them |
| M5 | Camera through AVFoundation | `video/camera/avfoundation.rs` |
| M6 | Screen share through ScreenCaptureKit and the system picker | `video/screen/macos.rs` (+ files beside it) |
| M7 | Screen share sound through Core Audio process taps | `share_audio/coreaudio.rs` |
| M8 | Hardware checklist, CI steps, packaging notes, the log | `app/macos/**`, your CI steps, §7 |

Paths are under `crates/opencord-media/src/`. Checks and integration tests go
in `app/tool/macos/` and `app/integration_test/macos/`.

**L1 already did, so don't build these:**
- Every Rust↔Dart API change this plan needs: `video_support()`,
  `settings_url` on `HotkeySupport`, `ScreenShareStarted.audio_app`.
- In the app:
  - "Open settings" on the Keybinds page, and binding again when the window
    is focused;
  - the share dialog's "your desktop asks" line where the system picks;
  - not asking "Sound from" when `Picked.sound` names the app.
- Shared code that calls your files.
- Run-time skips for tests that need a codec (`needs_codec!()`).

**Adjusted to this Mac** (from M0 and M1):
- **The Mac:** an Intel MacBook Pro (2019, i7-9750H, macOS 15.8.1), not
  Apple silicon.
  - VideoToolbox's hardware H.264 runs on Intel's Quick Sync here; record
    which encoder opened.
  - The macOS 13 and 14.0–14.1 paths can only be checked by reading the code.
- **Tools:** MacPorts, not Homebrew (`/opt/local/bin`).
- **One BlackHole device (2ch).** Wherever §4 says BlackHole 16ch or two
  devices, use BlackHole 2ch in both directions, as `check_microphone.sh`
  does.
- **Permissions:** checks run with `flutter test -d macos` make the terminal
  the one asking. Check the app's own prompt once by hand for each new
  permission (`open --env OPENCORD_PROFILE=…`): Input Monitoring (M2), Camera
  (M5), Screen & System Audio Recording (M6, M7).
- **The sandbox** won't let the app start processes, such as `sox`, so
  recordings run from the scripts, as in M1.
- **`opencord-voice`'s `tests/video.rs`** can miss its timing on a loaded
  machine. It's a Linux-owned file: rerun it alone, log it, and don't let it
  block a milestone.

**When the user has to be at the Mac:**

| Milestone | What for |
|---|---|
| M2 | Input Monitoring, for Opencord and the terminal; Accessibility, for the terminal (posted keys) |
| M5 | Camera access; the camera on; an iPhone nearby (optional) |
| M6 | Screen & System Audio Recording; picking a window in the system picker |
| M7 | The audio capture prompt; picking Safari's window for the window test |
| M8 | A second monitor, AirPods, sleep and wake, and the Linux machine (later the Windows PC) for a call |

Batch these, and say exactly what to click.

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
- **L1 has landed** (commit `refactor: platform seams (L1)`, platforms.md §2):
  the stub files you fill in from M2 on exist, the shared code already
  calls them, and every Rust↔Dart API change this plan needs is made.
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
  - `cargo clippy --workspace --all-targets -- -D warnings`. The off-Linux
    dead-code warnings L1 removed must not come back; a warning in a shared
    file goes in the log.
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
   already selects on macOS. Make its `supported()` return true when they work. Don't
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
   time where the codec isn't supported. Once `supported()` is true, two
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
   reports `receive` from your codec's `supported()`. Check that they work:
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
2. `camera::Capture` already wraps your backend's `Capture` (L1). Set the
   file's `SUPPORTED` to true; the camera buttons then show on macOS once
   the codec works too (`video_support().camera` needs both).
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
   ScreenCaptureKit. It implements the contract in `screen/mod.rs`:
   `support()` (with `own_picker: false`), `sources()` (empty here) and
   `start(request, frames) -> (Capture, Picked)`, which picks and then
   captures. Your `Capture` has `size()` and `finished()`.
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
     owning app's bundle ID and pid), and return it in `Picked.sound` (a
     `WindowSound { source, app }`); the app then skips "Sound from".
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
| M0 | `M0: macOS setup and baseline` (+ f6aff9a `shared:`) | **Mac:** MacBook Pro 2019, Intel Core i7-9750H (`uname -m` x86_64, 6 cores), macOS 15.8.1 (24H32), Xcode 26.3 (17C529), Flutter 3.44.0 / Dart 3.12.0 (the CI pin; the Mac also has 3.47.5), rustc 1.99.0. **Rust:** `cargo build --workspace` passes (54 s, only the expected off-Linux warnings); `cargo test --workspace --lib --bins --tests` 625 passed, 0 failed, 4 ignored over 24 binaries (one timing flake on the first run, below); `cargo fmt --check` clean; clippy as in the ruling below, clean. **Flutter:** `flutter analyze` no issues; `flutter test` +731 ~1 −19: the 16 goldens and 3 D-Bus tests, below. **App:** Debug build signed "Apple Development" (team 56D9H34Y6B) through `Signing.local.xcconfig`. Through `app/tool/macos/run_check.sh`: `m6` (sign in, join, chat live, follow a new restriction), `m5` (TOFU join, restart, still connected) and `v0` (voice states live, moved, disconnected) all pass on macOS. Not done: Homebrew, sox, SwitchAudioSource and BlackHole are not installed (needs the user; M1). |
| M1 | `M1: microphone and camera usage, entitlements, macOS 13; Mac voice checks` and the three M1 commits after it | **Bundle:** the three usage strings and `device.audio-input` + `device.camera` (Debug and Release) are in the built app; `LSMinimumSystemVersion` 13.0, `minos 13.0` for the app and `opencord_core` (the "building for macOS-10.15" linker warnings are gone). **Prompt (with the user):** Opencord launched on its own asked for the microphone on joining General; after Allow it is listed under Privacy & Security → Microphone and uses the MacBook's microphone. **`app/tool/macos/check_user_audio.sh`:** passes. Talker 440 Hz at 100 % −12.2 dBFS; 50 % −18.3 (−6.0 dB); 200 % −6.2 (+6.0 dB); muted −106.1; unmuted −12.2; Player's 880 Hz sound −14.7; soundboard muted −200 (nothing); Talker meanwhile −12.2; soundboard unmuted −14.8 — Linux's numbers within 0.5 dB. Run from BlackHole at 44.1 kHz, which the script moves to 48 kHz. **`check_microphone.sh`:** passes. 1 kHz from `afplay` through BlackHole as the microphone, in what a listening voicebot got: voice activity −8.6 dBFS (225 packets), push-to-talk held −12.6 dBFS (221), released nothing; level meter 577 readings, loudest −3.8 dBFS; device lists: inputs Mohammad Microphone, BlackHole 2ch, MacBook Pro Microphone; outputs BlackHole 2ch, MacBook Pro Speakers. **`check_echo.sh` (V3's echo acceptance, user present, MacBook Pro Speakers + Microphone, Talker at −12.2 dBFS):** passes. 440 Hz in what the app sent: **−110.1 dBFS** with its processing (echo cancellation on, noise suppression standard), −19.1 dBFS with both off. **Shared integration tests with `-d macos`** (`run_check.sh`): `v2` passes; `v3` passes (noise suppression standard by benchmark, hotkeys `FocusedOnly`, 11 levels, loudest −40.1 dBFS); `v8` passes; none failed for a platform reason. **Not done:** nothing in M1. L1 had not landed when M1 ended (checked after the last step). |
| M2 | `M2: global keybinds through a listen-only event tap` (57c2d60) | **`hotkeys/macos.rs`:** a `kCGSessionEventTap`, `kCGHeadInsertEventTap`, listen-only (keyDown, keyUp, flagsChanged) on its own thread and run loop, re-enabled on `TapDisabledByTimeout`/`…ByUserInput`; printable keys from the current layout (`TISCopyCurrentKeyboardLayoutInputSource` + `UCKeyTranslate`, unshifted and shifted, rebuilt on `kTISNotifySelectedKeyboardInputSourceChanged`), the rest from a fixed `kVK_*` table; modifier keys alone from flagsChanged, their side from the device flags. New macOS deps: `objc2-core-graphics` 0.3.2, `objc2-core-foundation` 0.3.2, `dispatch2` 0.3.1. The podspec links CoreGraphics and Carbon. **Unit tests:** 12 new (keysym → keycode table, the current layout's letters, left/right flags, matcher cases); `hotkeys` 16 passed, 0 failed. **Live, posted keys** (`--ignored`, iTerm with Input Monitoring + Accessibility, TextEdit focused): 4 passed, 0 failed, 9.6 s — CTRL+F13 press and release; F14 + Right Ctrl hold until both are up; Right Ctrl alone (Left Ctrl does nothing); a toggle fires once over three auto-repeated downs. **Accept, `app/tool/macos/check_hotkeys.sh`:** passes. Push-to-talk on F13, TextEdit focused throughout, 1 kHz from `afplay` as the microphone, what a Listener voicebot got: not held nothing; F13 held −12.6 dBFS (225 packets, peak −12.5); released nothing. The app reports `HotkeysGlobal` ("Keybinds work in other apps too, through Input Monitoring"). **By hand (user):** Opencord launched on its own asked for Input Monitoring when a keybind was added; after Allow it is listed and, after a relaunch, binds F12 globally and the Keybinds page says they work in other apps. The first build crashed there (below); at the M5 visit the user confirmed "Open settings" opens the Input Monitoring pane and a grant works without a relaunch. **§0 checks:** fmt clean; `clippy --workspace --all-targets -D warnings` clean, no exceptions; `cargo test --workspace --lib --bins --tests` 658 passed, 0 failed, 8 ignored over 25 binaries; `flutter analyze` no issues; `flutter test` +735 ~1 −16 (the 16 goldens; the D-Bus tests skip since L1). |
| M3 | `M3: H.264 through VideoToolbox; scaling with pic-scale and yuv` (a0f2ff6, + 0ef70a4 `shared:`) | **Codec:** `video/codec/macos/` — VideoToolbox encoder and decoder, pic-scale + yuv scaler, raw decoder; `supported()` true. **Encoder opened:** `Backend::VideoToolbox` is the hardware encoder (Quick Sync on this i7-9750H: `UsingHardwareAcceleratedVideoEncoder` true) with low-latency rate control; software encoder as the fallback. **Encode time per picture** (release, wall clock, pictures made beforehand; hardware work runs in VideoToolbox's own process, so not this process's CPU): 720p30 hardware 8.87 ms mean (5.17 best), software 9.53 (7.93); 360p30 hardware 5.70 (3.05), software 2.74 (2.25); 180p15 hardware 3.86 (1.68), software 1.66 (1.37); 1080p30 hardware 10.83 (6.89), software 20.29 (16.26). **Bitrate over 10 s of the test pattern:** 720p at 1.5 Mbit/s: hardware 1.53 Mbit/s (+2 %), software 1.19 (−20 %); 360p at 500 kbit/s: hardware 289 k (−42 %), software 515 k; 180p at 150 k: hardware 106 k, software 155 k (ruling below). Without low latency the hardware encoder overshoots: 729 k at 360p, 2.12 M at 720p. **First hardware open in a process:** 667–674 ms (291 ms without low latency); later opens 25–29 ms. **Decoding:** round trip worst PSNR 47.3 dB (software and hardware decoders, 640×360, 30 pictures, each out of its own call); a size change mid-stream decodes; nothing before a keyframe; garbage and damage are errors. **Scaler:** 1080p NV12 → 640×360 RGBA 1.23 ms in release (budget 2 ms; 2.1 ms in a debug test build); BT.601 colour patches (full red, green, blue, grey) within 3 levels. **Tests:** 21 in `codec::macos` (port of the Linux ones plus AVCC, I420, colour patches, the 2 ms budget), all pass; the shared sender/receiver/codec tests (84 in `video::`) now run here: all pass alone, two sender tests are load-sensitive (below). Voicebot: `a_voicebots_encoded_camera_is_decoded_by_another_voicebot` and `screen_audio.rs` pass on macOS. **Accept, `app/tool/macos/check_v4.sh`:** passes. Shared flow: 299 pictures, 299 intact, none of the top layer, 498 kbit/s. Real H.264 (`--camera`, VideoToolbox both ends): 195 pictures in 10 s, all 195 decoded, none of the top layer, 216 kbit/s with the debug voicebot (it draws the 720p scene itself); with a release voicebot 298 pictures, 298 decoded, 366 kbit/s. **Release build:** `flutter build macos --release` builds the universal app (x86_64 + arm64; `rustup target add aarch64-apple-darwin`); `opencord_core.framework` links VideoToolbox, CoreMedia, CoreVideo, Carbon, CoreGraphics. **§0 checks:** fmt clean; clippy `--all-targets -D warnings` clean; `cargo test --workspace --lib --bins --tests` 678 passed, 1 failed (`a_reshaped_screen_goes_out_as_its_new_track_and_layers`, "30 at 15 fps"; alone 3 of 3 pass), 8 ignored over 25 binaries; `flutter analyze` no issues; `flutter test` +735 ~1 −16 (the goldens). |
| M4 | `M4: others' camera and screen stream in the app on the Mac, checked` (1da14f7, + 77ff3dd `fix(shared):`) | **Found:** irondash 0.5 on macOS does not convert RGBA: it makes its IOSurfaces BGRA and copies our bytes in unchanged (red and blue swapped), with rows of exactly width × 4 bytes. Metal on this Mac (Intel UHD 630 and AMD Radeon Pro 555X: `minimumLinearTextureAlignmentForPixelFormat:BGRA8Unorm` = 256 bytes) aborted the app on Flutter's raster thread (`_mtlValidateStrideTextureParameters`, two crash reports) at the first tile, and at irondash's 2×2 blank before it. Fixed in the core's `FlutterSink` (ruling below). **Shared `check_v5.sh` with `-d macos`** (`run_check.sh v5`): passes after the fix (crashed twice before it). **`app/tool/macos/check_receive_video.sh`:** passes. A voicebot's camera (real H.264) drawn: 72 pictures presented, 73 drawn; another voicebot's screen stream (`--screen 720p30`), watched with the tile's Watch button: opens large (focus mode), 29 presented, 30 drawn; 2 wants while both show; after switching to the text channel the wants are empty and in 3 s camera +0, screen +0 pictures arrive. **Colours:** red/blue order is right by construction (scaler colour patches, the swap's unit test, Flutter reading the surface as BGRA: `populateTextureFromRGBAPixelBuffer` in the crash trace), but not yet seen: Flutter's layer snapshots leave external textures out (the tile came back near black, R 18 B 20), so it is checked by eye at the next visit (M5). **§0 checks:** fmt clean; clippy `--all-targets -D warnings` clean; `cargo test --workspace --lib --bins --tests` 679 passed, 1 failed (`a_reshaped_screen_goes_out_as_its_new_track_and_layers`, "30 at 15 fps"; alone 3 of 3 pass), 8 ignored over 25 binaries; `flutter analyze` no issues; `flutter test` +735 ~1 −16 (the goldens). |
| M5 | `M5: the camera through AVFoundation` (c3f55c2) | **`camera/avfoundation.rs`:** discovery (built-in, External, Continuity on 14+; ExternalUnknown on 13), access asked once (refused → `Denied`), the shared mode rule over the device's formats, output NV12 video range scaled to fit 1280×720, delegate (`define_class!`) on a serial queue with `try_send`, disconnection and session runtime errors close the queue; Objective-C exceptions become `CameraError::Failed`. Cameras listed here: "FaceTime HD Camera (Built-in)" and "Mohammad Camera" (not identified here). **Real-camera Rust test** (`--ignored`, iTerm allowed): FaceTime HD at 1280×720, 30 fps; session started in 0.37–0.54 s, first frame 1.03–1.06 s after asking (5 runs), 61 frames in 2 s, stopped in 62–74 ms, no frames after stopping. **Accept, `app/tool/macos/check_v5.sh`:** passes. In the app: camera on (button → preview drawn) **1104 ms**, off **15 ms**, preview mirrored; three voicebots watching at 180/360/720 decoded every picture: 87 of 87 each, layers 180p, 360p, and 27 × 360p then 60 × 720p. The debug app reached only about 9 fps and used 163–194 % of a core (debug per-pixel loops). **CPU in release** (the sending side alone, `the_cameras_sending_side_costs`, `--release --ignored`): camera 1280×720 30 fps, three layers at 15.0/30.0/30.0 fps, a 640×360 RGBA preview, **12 % of one core** (twice); VideoToolbox's hardware encoding runs in its own process, not counted. **By hand (user, Opencord launched on its own):** macOS asked for the camera for Opencord; the preview is mirrored; a voicebot's test scene shows its left half orange-yellow and its right half blue, as BT.601 says (left ≈ RGB 167/102/51, right ≈ 78/126/164), so M4's red/blue order is right. **M2 by hand:** after removing Opencord from Input Monitoring and allowing it again, push-to-talk works; "Open settings" opens the Input Monitoring pane, and the grant takes effect when Opencord's window is focused again, without a relaunch (user). **Not done:** Continuity Camera (not tried); camera-on under 1 s (below). **§0 checks:** fmt clean; clippy `--all-targets -D warnings` clean; `cargo test --workspace --lib --bins --tests` 679 passed, 1 failed (`a_reshaped_screen_goes_out_as_its_new_track_and_layers`; alone 3 of 3 pass), 10 ignored over 25 binaries; `flutter analyze` no issues; `flutter test` +735 ~1 −16 (the goldens). `flutter build macos --release` builds the universal app. |
| M6 | `M6: screen sharing through ScreenCaptureKit` (87cec4e) and fixes (adff8e6, + `shared:` StartRequest.max_pixels) | **`video/screen/macos.rs`:** supported on 14+, the system's `SCContentSharingPicker` (one display or one window, Opencord excluded), or `OPENCORD_SCREEN_DISPLAY=main`; an `SCStream` at the content's pixels within the share's budget (never under 1080p's), NV12, cursor, queue depth 5; only complete pictures; a resized window updates the stream's size; the stream stopping closes the queue. **Rust (Screen Recording for iTerm):** the main display (Retina, 3360×2100) captured at 30 fps (91 frames in 3 s), first picture 0.97 s after asking, queue closed on stop. **CPU at 1080p30** (release, capture + 2 layers + hardware encode): captured at 3360×2100 and scaled by the sender, 23.0/22.6 fps main layer at **82–83 %** of a core; captured at 1080p's budget (1820×1138, ScreenCaptureKit scaling), **29.4/29.5 fps at 24–25 %**. **Accept, `app/tool/macos/check_v6.sh`:** passes. The app goes live in 643–833 ms; a Watcher decodes every picture (197 of 197; 195 of 195) at the main layer; stopping ends the stream for it. The debug app reaches only ~11 fps at 157–221 % CPU (debug loops); the test server caps shares at 720p, so the 1080p numbers are the release test's. **By hand (user):** Opencord launched on its own: Go live → the system picker → a window; macOS asked for Screen Recording (and, with sound on, audio recording); the share started. Window share through the picker: also in M7's window check (Safari). **Not done:** HDR on an XDR display (no XDR here); macOS 13's "needs macOS 14" path (read only). **§0 checks** (with M7): fmt clean; clippy `--all-targets -D warnings` clean; `cargo test --workspace --lib --bins --tests` 682 passed, 3 failed (two CPU-speed audio tests, 3 of 3 alone each; `a_reshaped_screen_goes_out_as_its_new_track_and_layers`, "30 at 15 fps", with the hand server's camera voicebot and a hand-run Opencord using ~3.5 cores; it passed alone 3 of 3 on an idle Mac earlier), 16 ignored over 26 binaries; `flutter analyze` no issues; `flutter test` +735 ~1 −16 (the goldens). |
| M7 | `M7: screen share sound through Core Audio process taps` (26367f0) and fixes (adff8e6, 2d92b66) | **`share_audio/coreaudio.rs`:** 14.2+; a stereo tap (everything but Opencord's processes, or one app's: bundle ID or prefix, executable name, the app macOS holds responsible; or a process and its children), private and unmuted, in a private aggregate device on the default output, read by an IOProc block (the tap's stream is the last buffer: the main device's input streams come first; reading two buffers as left/right had given −80 dBFS), 48 kHz stereo (rubato otherwise); the process list is followed; teardown in the plan's order. **The sandbox:** taps work in the sandboxed app (decision E holds). **Taps (`check_share_audio.sh`, all on BlackHole, iTerm allowed):** 4 of 4 pass: the screen hears another app's 700 Hz at **−12.0 dBFS**, its own child's 1100 Hz at −81.7; one app's tap: 600 Hz −12.1 / the other's 900 Hz −82.2, switched: 900 Hz −12.0 / 600 Hz −77.9; an app that starts later is tapped (−12.0); apps listed (a command-line player shows as its terminal). **Accept, `check_v7.sh`** (no double audio): passes. The stream has the music (1 kHz) at **−12.0 dBFS** left and right and Talker (440 Hz) at **−117.8** (Talker's own voice −12.2). **Mutation** (`--mutation`, Opencord kept in): Talker at −40.4 dBFS in the stream, so the check fails as it must. **Window** (`--window`, the user picked Safari's window in the picker): the app knew the window's app ("sound from Safari", no "Sound from" question; macOS 15.2's `includedWindows`); the stream has Safari's 600 Hz at **−16.2 dBFS**, afplay's 1 kHz at −110.1, Talker at −123.8; Safari's sound comes from its WebContent processes, found through the responsible app. **By hand (user):** Opencord asked to record audio from other apps when sharing with sound. **Echo (step 4):** not checked with the speakers (silent-night rule); nothing to build. **§0 checks:** as M6's. |
| M8 | `M8: headsets coming and going, a LAN server for the hand checks, Release entitlements` (this commit); **the hardware checks with the user are still to do** | **Headsets without AirPods, `app/tool/macos/check_devices.sh`** (`crates/opencord-media/tests/devices_macos.rs`: the headset is a public aggregate device over BlackHole 2ch, the engine holds push-to-talk, an `afplay` tone is looped back as the microphone): 3 of 3 pass, in each of two runs. **A chosen headset pulled out:** the microphone falls back to the default (BlackHole 2ch) 642–674 ms later and the speaker 934–947 ms later, one `DeviceFellBack` each, then 55–56 frames in the next second. **The default headset pulled out** (both on Default): silent 600–700 ms, then the new default, with no event (nothing to tell the user). **The headset's rate changed during the call** (16 kHz, as AirPods' headset profile, then 44.1 kHz): reopened with no fallback, silent 1.1–1.2 s, and the device is back at 48 kHz afterwards (ruling below). **Sleep (unplanned, not the checklist's):** the hand-run Opencord stayed in General through the Mac's sleep from 02:24 to 10:17 (lid closed); 35 minutes after waking it was still in the call, sending about 190 kbit/s and receiving about 110 kbit/s from the server on the same Mac. **Release:** `flutter build macos --release` builds the universal app (x86_64 and arm64, for both the app and `opencord_core`), `minos 13.0`, ad hoc, `codesign --verify --deep --strict` passes. Debug and Release share one Info.plist, so their usage strings match; the entitlements are now exactly `Release.entitlements` (ruling below). Launched with a throwaway profile, it was still running after 10 s, used 92 MB, and left no crash report. **For the hand checks:** `app/tool/macos/serve_lan.sh [--talker] [--camera]`, tried: it prints the invite, and Talker joins General. **Not done (needs the user):** two monitors with mixed DPI; sleep and wake during a call; AirPods in and out during a call; Continuity Camera; the call with the Linux app over the LAN (later with the Windows PC). XDR: there's no XDR display here. **§0 checks:** fmt clean; clippy `--all-targets -D warnings` clean; `cargo test --workspace --lib --bins --tests --no-fail-fast` 680 passed, 5 failed, 19 ignored over 27 binaries. The failures are four sender wall-clock tests and `opencord-voice` `tests/video.rs` `an_xor_transform_end_to_end_does_not_stop_the_node`, run with the CPU held to 22 % of its speed (ruling below) and the trial LAN server running. `video::sender` alone, three times: `a_reshaped_screen_goes_out_as_its_new_track_and_layers` failed each time, `the_planners_cuts_shrink_and_slow_a_screens_main_layer` once. `flutter analyze` no issues; `flutter test` +735 ~1 −16 (the goldens). |

Rulings (`Mn: Ruling: … — cost if wrong: …`):
- M0: Ruling: decision A as written: the objc2 0.3 framework crates (cpal's
  line), delegates through `define_class!`, HIToolbox declared by hand,
  shims only where objc2 can't, `objc2::available!` for OS versions — cost
  if wrong: a binding gap means a Swift/Objective-C shim per gap.
- M0: Ruling: decision B as written: VideoToolbox directly, no FFmpeg on
  macOS — cost if wrong: 600–900 lines of glue FFmpeg would have given.
- M0: Ruling: decision C as written, but **the test Mac is an Intel
  MacBook Pro** (i7-9750H, 6 cores, 2019), not Apple silicon. The 2 ms
  budget for 1080p NV12 → 640×360 RGBA is checked on this Intel CPU, which
  is the harder bar; nobody measures Apple silicon until a Mac with it
  turns up — cost if wrong: an Apple-silicon-only regression goes unseen
  until then.
- M0: Ruling: decision D as written. This Mac runs 15.8.1, so the 13.x and
  14.0–14.1 "needs a newer macOS" paths are only checked by reading the
  code, never run — cost if wrong: a wrong message or crash on an old
  macOS that nobody here can see.
- M0: Ruling: decision E as written: the sandbox stays on; M7 checks
  whether process taps work in it — cost if wrong: DebugProfile drops the
  sandbox for taps.
- M0: Ruling: decision F as written (BlackHole, `SwitchAudioSource`, `sox`,
  `afplay`, `OPENCORD_SCREEN_DISPLAY=main`) — cost if wrong: different
  stand-ins, the same tests.
- M0: Ruling: the checks use the Flutter CI pins, 3.44.0, cloned to
  `~/flutter-3.44.0` and put first on `PATH` by the session. The Mac's own
  Flutter (3.47.5 at `~/dev/flutter/flutter`) is left as it is — cost if
  wrong: none; 3.47.5 is one `PATH` away.
- M0: Ruling: Debug signing. The project-level Debug
  `CODE_SIGN_IDENTITY = "-"` in `project.pbxproj` won over the xcconfig, so
  it moved into `Runner/Configs/Debug.xcconfig`, which then does
  `#include? "Signing.local.xcconfig"` (gitignored in `app/macos/.gitignore`).
  This Mac's file names `Apple Development` and its team. Profile and
  Release stay ad hoc — cost if wrong: none; without the local file Debug
  is ad hoc as before.
- M0: Ruling: the CocoaPods integration that `flutter build macos` writes
  (`app/macos/Podfile`, `Podfile.lock`, the Pods phases in
  `project.pbxproj`, the `#include?` lines in `Flutter-*.xcconfig`, the
  workspace entry) is committed, as Flutter's template expects; otherwise
  every Mac build leaves the tree changed. The iOS files `pub get` touches
  are not the Mac's and stay as they are — cost if wrong: CI's `pod
  install` rewrites the lock, as it did before.
- M0: Ruling: `app/tool/macos/run_check.sh <name>` runs a shared
  `app/tool/check_<name>.sh` that needs nothing from Linux but the device
  (`m5`, `m6`, `v0`, `v2`, `v3`, `v8`) with `-d macos`, from a temporary
  copy, so the shared scripts stay untouched. Scripts that use PipeWire
  (`user_audio`, `v7`) get their own ports — cost if wrong: a shared
  script whose wording changes defeats the `sed`; the runner says so.
- M0: Ruling: until L1, `cargo clippy --all-targets -D warnings` can't pass
  on macOS, and that is left alone: the three `opencord-media` examples
  (`codec_costs`, `camera_load`, `screen_load`) import Linux-only modules,
  and the shared files' off-Linux leftovers fail `dead_code`, `unused` and
  `clippy::unnecessary_literal_unwrap` (`opencord-voicebot/src/lib.rs:777`,
  `let decoded: Option<u64> = None`). The check run instead: `cargo clippy
  --workspace --lib --bins --tests -- -D warnings -A dead_code -A unused -A
  clippy::unnecessary_literal_unwrap` — clean. For L1: the examples need a
  plan too — cost if wrong: none; L1 removes the gates.
- M0: Ruling: step 5's "run the app, sign in, open a text channel and
  send a message" was checked with the shared M6 check against the Mac app
  (`run_check.sh m6`: sign in, join, send in #general, get the owner's
  answer live) instead of by hand with `flutter run -d macos` — cost if
  wrong: none seen; M1's hand-run launch (the microphone prompt) also
  signed in and joined.
- M1: Ruling: §0's checks before each commit were not all re-run for M1's
  first four commits (only `cargo fmt` and `flutter analyze`; M1 changed no
  Rust and nothing under `app/lib` or `app/test`). Run in full after M1:
  fmt clean; clippy (as M0's ruling) clean; `cargo test` 625 passed, 0
  failed, 4 ignored; `flutter analyze` no issues; `flutter test` +731 ~1
  −19, the same 16 goldens and 3 D-Bus tests as M0 — cost if wrong: none.
- M1: Ruling: MacPorts in place of Homebrew (the user's Homebrew no longer
  works on this Intel Mac): `sudo port install sox switchaudio-osx
  BlackHole`. The scripts look in `/opt/local/bin`, then Homebrew's folders
  — cost if wrong: none; both are searched.
- M1: Ruling: **one BlackHole device, not two** (decision F said 2ch and
  16ch). MacPorts' BlackHole port builds one device; its channel variants
  conflict. Building a second from BlackHole's source and signing it with
  the user's identity was not done (it needs the user's say-so). BlackHole
  2ch is the app's speaker and microphone in `check_user_audio.sh` and its
  microphone and speaker in `check_microphone.sh`; both pass. The scripts
  read its UID from `SwitchAudioSource -f json` (`BlackHole2ch_UID`; cpal's
  id is `coreaudio:BlackHole2ch_UID`) — cost if wrong: a check that needs
  two separate devices gets the second BlackHole then.
- M1: Ruling: the app sandbox refuses to start `sox` (`ProcessException:
  Operation not permitted`), and the sandbox stays (decision E). So
  `check_user_audio.sh` records BlackHole with sox for the whole run, the
  app prints each window to measure (`WINDOW: <name> <start ms> <end ms>`),
  and the script measures them with the shared test's Goertzel blocks and
  limits. Each window drops 300 ms at both ends (sox starts 100–200 ms
  after its timestamp); BlackHole is set to 48 kHz first, because the app
  changing its rate mid-recording made the recording lose time (the first
  run's −10 dB at 50 % and −56 dBFS muted); the script fails if the
  recording is shorter than its wall-clock time — cost if wrong: a
  mis-timed window, which the per-block dump on failure shows.
- M1: Ruling: test data. The sandboxed app keeps its data in its container,
  so `e2e_server.sh`'s `XDG_DATA_HOME` doesn't isolate it, and a failed run
  left its server in the profile. `app/tool/macos/profile.sh` deletes the
  check's own `e2e*` profile folder before each run (it refuses any other
  name); `run_check.sh` uses it too — cost if wrong: none.
- M1: Ruling: under `flutter test -d macos` the app is the terminal's
  child, so macOS asks (and records) microphone access for the terminal
  app, not Opencord. Opencord's own prompt was checked by hand, launched
  with `open --env OPENCORD_PROFILE=…`. The same will hold for camera,
  screen and input permissions in the automated checks of M2–M7 — cost if
  wrong: none; a manual run per permission covers the app.
- M1: Ruling: the microphone check turns noise suppression off (a steady
  tone is what it removes) and drives push-to-talk through
  `Repository.setPushToTalk`, which the key handler calls; real keys are
  M2's. The echo check holds push-to-talk so the app sends throughout and
  adds a phase with echo cancellation and noise suppression off, which
  shows the echo was there (−19.1 dBFS) — cost if wrong: none.
- M2: Ruling: HIToolbox reads its input sources only on the main thread
  in an app: the first build read the layout on the hotkey thread and the
  app stopped with `_dispatch_assert_queue_fail` in
  `TSMGetInputSourceProperty` as soon as a keybind was added (a plain
  process, like `cargo test`, isn't checked). The layout is now read on
  the main queue (`dispatch2`), at bind time and after a layout change;
  `bind` waits 2 s for it, then uses a US layout (a process whose main
  thread doesn't run its queue, i.e. a test) — cost if wrong: in such a
  process printable keys follow US positions.
- M2: Ruling: with no keybinds, `bind` neither taps nor asks; it reports
  `Global` if Input Monitoring is allowed and `FocusedOnly` with the pane
  otherwise. The app binds an empty list at every start, and the prompt
  would otherwise greet everyone at first launch — cost if wrong: the
  prompt comes with the first keybind instead.
- M2: Ruling: the reason starts lower-case ("allow Opencord under System
  Settings → Privacy & Security → Input Monitoring"), as it follows "work
  only while Opencord is focused:" on the Keybinds page; `method` is
  "Input Monitoring" — cost if wrong: wording.
- M2: Ruling: keys without a Mac key stay unbound (Print, Scroll_Lock,
  Pause, Menu, F21–F24); Insert is Help and Num_Lock is Clear, where Mac
  keyboards put them; Caps Lock alone follows its lock flag (on = down) —
  cost if wrong: a keybind made on Linux with those keys does nothing on
  the Mac.
- M2: Ruling: a modifier's side comes from the device flags; an event
  without any (posted by a program) falls back to the modifier's own flag
  — cost if wrong: none seen; the posted-key tests cover both.
- M2: Ruling: posted keys go to the focused app, which is iTerm running
  this session, so the live tests use F13–F15 and Control, and run with
  TextEdit focused (`check_hotkeys.sh` opens it and quits it only if it
  started it) — cost if wrong: none.
- M2: Known limit: while any app has Secure Event Input on (a password
  field is focused), no event tap gets keys, so keybinds don't work then.
- M2: Ruling: `cargo test` and `flutter test` run one after the other:
  run together, `audio::processing::tests::high_stays_when_it_keeps_up`
  (a CPU-speed test) failed once (Standard instead of High); alone it
  passed 3 of 3 (1.01 s, 0.75 s, 0.74 s) and in the full run — cost if
  wrong: none.
- M3: Ruling: VideoToolbox, Core Media and Core Video are declared by
  hand in `video/codec/macos/ffi.rs` (about 30 functions and keys), with
  objc2-core-foundation for the CF types, instead of the objc2 0.3
  `objc2-video-toolbox`/`-core-media`/`-core-video` crates (decision A):
  those are plain C APIs whose generated free functions are deprecated
  aliases (which `-D warnings` refuses) of methods spread over many
  features — cost if wrong: a wrong declaration is caught only by the
  tests, which exercise every call.
- M3: Ruling: decision C chosen by measurement on this Mac: pic-scale
  (planes, bilinear) + yuv (NV12 → RGBA, BT.601 video range) took 1.33 ms
  for 1080p NV12 → 640×360 RGBA against 9.93 ms for a hand-written
  separable filter. Both crates are BSD-3-Clause OR Apache-2.0, in the
  macOS section only — cost if wrong: two dependencies.
- M3: Ruling: the encoder tries low-latency rate control first, then
  without it, for each backend. Low latency never overshoots; on the test
  pattern it undershoots small layers (360p: 289 k of 500 k), which only
  means the pattern needs fewer bits there; without it the hardware
  encoder overshot by 40–46 %. Step 7's ±25 % test runs at 720p, where
  both encoders hold (+2 %, −20 %) — cost if wrong: a small camera layer
  looks a little softer than it could.
- M3: Ruling: encoder errors are reported as
  `CodecError::Unavailable("VideoToolbox's encoder")` (and the decoder's
  likewise), with the OSStatus in the debug log, so the shared
  `CodecError` keeps its variants — cost if wrong: the status is only in
  the log.
- M3: Ruling: a process's first hardware encoder takes 0.67 s to open, so
  the first call to `supported()` (the app's `video_support()` at launch)
  opens and drops a 320×180 hardware encoder on a thread of its own; this
  crate's own tests wait for it, so the sender tests time the sender, not
  VideoToolbox loading — cost if wrong: one small encoder at launch.
- M3: Ruling: VideoToolbox conceals a damaged access unit without an
  error, so an access unit with slices that gives no picture is an error
  (the receiver then asks for a keyframe, as with FFmpeg) — cost if wrong:
  a needless keyframe request.
- M3: Ruling: `check_v4.sh`'s `--test-pattern` camera is synthetic
  (checksummed NAL units), so it never reaches the codec;
  `app/tool/macos/check_v4.sh` runs it and then a second part with
  `--camera` (real H.264 through the app's encoders), requiring every
  picture decoded — cost if wrong: none.
- M3: Ruling: `dev` profile `opt-level = 3` for pic-scale and yuv
  (0ef70a4, `shared:`): profile settings only exist at the workspace root,
  and without them the debug app scales each picture far slower than its
  frame time — cost if wrong: longer debug builds of two crates.
- M3: Note: VideoToolbox prints "AVCaptureDeviceTypeExternal is
  deprecated for Continuity Cameras" once per process; it comes from the
  system, not from Opencord's code.
- M4: Ruling: irondash 0.5's macOS surfaces are fixed where pictures
  reach them, in `crates/opencord-core/src/video/flutter.rs` (77ff3dd,
  `fix(shared):`, macOS only): each picture is stretched to the next width
  that is a multiple of 64 pixels (256-byte rows), nearest column, with red
  and blue swapped, and the blank before the first picture is 64 pixels
  wide. Flutter scales a texture to its tile, so the stretch doesn't show;
  it costs one pass over the tile. Should have been a seam (each system's
  texture format); a patched irondash (rows aligned with
  `IOSurfaceAlignProperty`, BGRA written in its own upload) would save the
  pass. irondash 0.6.0-dev.0 says BGRA on darwin but still makes unaligned
  rows — cost if wrong: up to 63 columns of stretch in a tile's texture.
- M4: Ruling (checked at M5: right): the colour check of step 1 is by eye, not in the test:
  Flutter's `OffsetLayer.toImage` leaves external textures out, and a
  screenshot needs Screen Recording for the terminal (M6) — cost if wrong:
  none; the user looks at a voicebot's camera at the M5 visit (the test
  scene's left half is red-magenta, its right half blue).
- M5: Ruling: §8's "camera on under 1 s" is not met on this Mac: from
  the button to the preview drawn takes 1.10 s, and the FaceTime HD camera
  alone gives its first frame about 0.65 s after `startRunning` returns
  (without setting the frame rate, or without the output scaling, the
  same). Opening the camera earlier would light it before the user asked.
  The tests allow 1.5 s and the numbers are logged — cost if wrong: the
  camera takes a tenth of a second longer to show than the plan wants.
- M5: Ruling: `AVCaptureSessionPresetInputPriority` is iOS-only (macOS
  throws "not a supported preset"); on macOS the device's `activeFormat`,
  set while locked and kept locked until the session runs, holds without a
  preset. Frame durations are 1/30 s only inside a format's range, else the
  range's own `minFrameDuration`, since AVFoundation throws on anything
  else — cost if wrong: none seen.
- M5: Ruling: the frames are copied plane by plane from the pixel buffer
  (`codec::macos::nv12_picture`, the decoder's copy) rather than through
  `camera::pack`, which expects the chroma plane right after the luma
  plane; a CVPixelBuffer's planes have their own addresses — cost if wrong:
  none.
- M5: Ruling: the CPU number is the sending side measured in a release
  test (`the_cameras_sending_side_costs`): `flutter test` can't build in
  profile mode, and `flutter drive --profile`
  (`integration_test/macos/driver.dart`) stopped at the shared onboarding
  helper's backup step, which behaves differently outside debug builds —
  cost if wrong: the UI's own share of CPU is not in the number.
- M6: Ruling: the capture size is the content's pixels (`contentRect` ×
  `pointPixelScale`) at most 3840×2160 (the largest preset, `Source`):
  `start` gets only the frame rate, and the sender scales each layer to
  the share's own preset, as on Linux — cost if wrong: more pixels copied
  than a lower preset needs.
- M6: Ruling: `Picked.sound` for a window is `Source::App(bundle ID)` with
  the app's name, from `SCContentFilter.includedWindows` (macOS 15.2+); on
  14.0–15.1 the app asks "Sound from" as on Wayland — cost if wrong: an
  extra question on older systems.
- M6: Ruling: the share dialog's line "Your desktop asks which screen or
  window to share once you go live" fits macOS well enough; "Your Mac
  asks…" would read better there (the dialog is the Windows session's
  file) — cost if wrong: wording.
- M7: Ruling: iTerm's Info.plist has no `NSAudioCaptureUsageDescription`,
  so macOS can't ask on its behalf: a tap made from a process iTerm is
  responsible for (`cargo test`, `flutter test`) is created and its
  aggregate device starts, but the IOProc is never called — no frames at
  all, not silence. The user adds iTerm by hand under Screen & System
  Audio Recording → System Audio Recording Only; Opencord itself has the
  usage string and asks — cost if wrong: none; the checks wait for it.
- M7: Ruling: an app's processes, besides bundle ID and prefix (the plan's
  rule), are found through the process macOS holds responsible
  (`responsibility_get_pid_responsible_for_pid`, looked up with `dlsym`):
  Safari's sound plays from `com.apple.WebKit.WebContent`, whose bundle ID
  doesn't start with Safari's. `Source::App` also matches an executable's
  name, for command-line players. `apps_with_audio()` groups by the
  responsible app, so a command-line player shows as its terminal — cost
  if wrong: a private symbol; without it, only bundle IDs match.
- M7: Ruling: `OPENCORD_SHARE_AUDIO_INCLUDE_SELF=1` (read only in
  `coreaudio.rs`) keeps Opencord in a screen's tap, for the V7 check's
  mutation (`check_v7.sh --mutation` must see Talker in the stream) —
  cost if wrong: a variable nobody else sets.
- M7: Ruling: with one BlackHole device, the V7 check's app plays Talker
  into BlackHole and records from it too, so its microphone is muted in the
  check; a process tap takes processes' output whatever the device, so the
  music and Talker share BlackHole without mixing in the tap — cost if
  wrong: none.
- M8: Ruling: CI's macOS step `rustup target add aarch64-apple-darwin
  x86_64-apple-darwin` before the build: the release is universal, and
  this Mac's build needed the Apple-silicon target added by hand. Proposed
  for the Linux session (platforms.md §4): a `cargo test` job on
  `macos-latest` for the tests that need no hardware (everything not
  `#[ignore]`d; it would also show the load-sensitive sender tests) —
  cost if wrong: a few seconds of CI.
- M8: Ruling: **voice never connected in a release build** ("Voice
  disconnected"; found by the user at the AirPods check). Release had only
  `network.client`; the sandbox stopped the voice connection before its
  UDP socket was made, with no denial in the system log. Every check before
  this ran a Debug build, whose entitlements have `network.server` for
  Flutter's debugger, and voicebots aren't sandboxed, so none saw it. A
  copy of the release app re-signed with `network.server` connected at
  once (the user), so `Release.entitlements` now has it. Release's
  entitlements now differ from Debug's only by `cs.allow-jit`. Xcode also
  added `com.apple.security.get-task-allow` to the ad hoc release, which
  lets a debugger attach and which notarization refuses;
  `CODE_SIGN_INJECT_BASE_ENTITLEMENTS = NO` in `Release.xcconfig` (Release
  and Profile) leaves exactly `Release.entitlements`. For the release
  notes (the `release` job is shared): "The macOS build isn't notarized
  yet. The first time, allow it under System Settings → Privacy &
  Security → Open Anyway, or run `xattr -dr com.apple.quarantine
  Opencord.app`. Each update has a new signature, so macOS asks again for
  the microphone, camera, screen recording and Input Monitoring; if a
  switch is on but the permission doesn't work, remove Opencord from that
  list and allow it again." On macOS 15, right-click → Open no longer
  gets past Gatekeeper — cost if wrong: `network.server` lets the
  sandboxed app accept connections, which it never opens a listener for.
  The checks by hand from now on use the release build.
- M8: Ruling: AirPods are stood in for by a headset made in the test: a
  public aggregate device over BlackHole 2ch (every app lists it while it
  exists), pulled out by destroying it, with its rate set through Core
  Audio (`tests/devices_macos.rs`, run by `check_devices.sh`, which moves
  the defaults to BlackHole and back). The microphone is counted in
  frames, so it hears a looped-back tone: on BlackHole's silence Opus
  leaves frames out (2–8 a second), which hides a stopped microphone. The
  gap is the longest run of 100 ms windows with at most one frame; up to
  1.5 s passes — cost if wrong: a real Bluetooth device could report its
  loss differently (cpal listens for `DeviceIsAlive` either way); the
  check with AirPods covers it.
- M8: Ruling (068ec6b, `fix(shared):`, `audio/device.rs`): **AirPods gave
  no sound** once their microphone was open (found by the user).
  `choose()` asked for 48 kHz, and cpal on macOS then sets the device's
  nominal rate, which is the whole system's. Opening the AirPods
  microphone moves them to their headset profile (16 kHz): the speaker
  stream was invalidated, reopening it at 48 kHz failed ("Sample rate
  update timed out", macOS can't change it while the headset microphone
  runs), the default (the same AirPods) failed the same way, and the
  engine never tried again. On macOS, `choose()` now takes the device's
  own rate, and the processor resamples it. Checked on AirPods Pro with
  `a_real_headset_keeps_playing_when_its_microphone_opens`
  (`OPENCORD_TEST_HEADSET=74-65-0C-EE-4E-66`): before, the speaker was lost
  and reopening failed; after, it reopened at 16 kHz mono and plays. The
  user hears Talker in the AirPods. This also ends Opencord resetting
  other apps' rate to 48 kHz; on the virtual headset, a rate change is
  still about 1 s silent (much of that is the looped-back tone pausing
  while BlackHole changes rate) — cost if wrong: resampling on every Mac
  device that isn't at 48 kHz. Still open for the Linux session: after a
  `DeviceFailed`, the engine never tries the device again, so any device
  that fails once stays silent until the call is rejoined.
- M8: Finding for the Linux session (the shared root `Cargo.toml`, not
  edited): the debug app's audio thread spent 79 % of its time in
  microphone processing (`sample`: 650 of 827 samples, 363 in `aec3`'s
  echo canceller), with the CPU throttled: in a call with AirPods, Talker
  cut out while the user talked. `aec3` isn't in `[profile.dev.package]`
  with tract and the FFT crates. The release app in the same call used
  7–12 % of a core — cost if wrong: debug-only stutter.
- M8: Ruling: with no camera chosen (the app has no camera menu), the
  camera is macOS's `systemPreferredCamera` on 14 and later: the one last
  picked in Control Center, or an iPhone mounted as a Continuity Camera.
  Otherwise, as before, it's the first one discovered (built-in first).
  Until now an iPhone was listed ("Mohammad Camera" in M5) but could never
  be used. `the_camera_used_by_default_is_a_listed_one` (ignored) passes:
  with no iPhone near, the FaceTime HD camera. A bare test process gets no
  preferred camera, since it lacks the app's
  `NSCameraUseContinuityCameraDeviceType` — cost if wrong: someone who
  picked another camera in Control Center gets that one.
- M8: Ruling: `app/tool/macos/serve_lan.sh` is the server for the
  checks across machines: it binds 0.0.0.0, advertises en0's address, uses
  port 47700 and a temporary folder, and runs optional Talker and camera
  voicebots. This Mac's application firewall is off, so nothing asks —
  cost if wrong: none.
- M8: Ruling: the timing failures in M8's §0 run are logged, not chased.
  The CPU was held to 22 % of its speed (`pmset -g thermlog`:
  `CPU_Speed_Limit = 22`; `machdep.xcpm.cpu_thermal_level` 100, then
  49–50 with the Mac idle), the thermal throttling this 2019 Intel model
  is known for. Nothing in the sender or the layer planning has changed
  since M3, when they passed alone 3 of 3 — cost if wrong: a real sender
  regression hides behind the heat. Rerun when `CPU_Speed_Limit` is 100.
- M8: Known limits on macOS (for `docs/decisions.md`):
  - No keybinds while an app has Secure Event Input on (M2).
  - Minimum versions: the app needs 13.0; the camera works on 13 and
    lists Continuity Camera from 14; screen sharing needs 14.0 (the
    system's picker; on 13 it says it needs 14); a share's sound needs
    14.2 (process taps); naming a shared window's app without asking
    "Sound from" needs 15.2.
  - Screens are captured in SDR only; HDR was not checked (no XDR display
    here).
  - The release is signed ad hoc, so permissions are asked again after
    each update.
  - Turning the camera on takes 1.1 s, most of it waiting for the
    camera's first frame (M5). A process's first hardware encoder takes
    0.67 s; it is warmed up at launch (M3).
  - The debug app draws video at 9–11 fps; release numbers are the
    measured ones (M5, M6).
  - Video tiles' textures are stretched to a multiple of 64 pixels wide
    (irondash, M4).
  - A device whose rate changes is silent for about 1.1 s (above).

`shared:` commits (file, why):
- M0 f6aff9a `crates/opencord-voicebot/tests/voice.rs`:
  `a_voicebots_encoded_camera_is_decoded_by_another_voicebot` gated
  `cfg(target_os = "linux")` like `publish_encoded_camera`, which it calls;
  the file did not compile on macOS, so `cargo test --workspace` failed. It
  should have been a seam: L1's run-time skip (platforms.md §2.6) replaces
  the gate.
- M3 0ef70a4 `Cargo.toml` (workspace root): `[profile.dev.package]`
  `opt-level = 3` for pic-scale and yuv, the macOS scaler's crates, as for
  tract and the FFT crates. Not a seam: profiles exist only at the root.
- M4 77ff3dd `crates/opencord-core/src/video/flutter.rs` (`fix(shared):`):
  on macOS, `FlutterSink::present` hands irondash BGRA rows a multiple of
  256 bytes long, and its blank is 64 pixels wide; without it the app
  crashed at the first video tile (Metal) and showed red as blue. A seam
  for each system's texture format would have been the place.
- M8 068ec6b `crates/opencord-media/src/audio/device.rs`
  (`fix(shared):`): on macOS, `choose()` opens a device at its own rate
  instead of asking for 48 kHz; with 48 kHz, AirPods' speaker could not be
  reopened once their microphone was open, and calls went silent. Should
  have been a seam (each system's rule for a device's rate).

Shared tests that fail on macOS for a platform reason (not edited):
- M0: `app/test/unit/desktop/linux_notifications_test.dart` (2 tests) and
  `linux_tray_test.dart` (1): `package:dbus` says "Authentication not
  supported on macos" (they need a Linux session bus), then time out after
  30 s. They should skip off Linux.
- M0: the golden tests, 16, rendered on Linux, as expected (§0).
- M0: `opencord-voice` `tests/video.rs`
  `a_capped_uplink_stops_encoding_the_cameras_top_layer_until_lifted`
  failed once in a full `cargo test --workspace` ("the receiver got no lower
  layer" within 5 s), then passed 3 of 3 alone (10.1 s, 16.1 s, 12.2 s) and
  in the next full run. Timing on a loaded 6-core Intel Mac; not a macOS
  difference.
- M3: with the codec on, `opencord-media`'s video tests run here, and the
  sender's wall-clock tests are load-sensitive on this 6-core Intel Mac in
  debug builds: `video::sender::tests::a_reshaped_screen_goes_out_as_its_new_track_and_layers`
  failed in the full runs ("30 at 15 fps"; a frame on the old track) and
  alone once in 5 ("19 at 15 fps"), then passed alone 3 of 3;
  `the_planners_cuts_shrink_and_slow_a_screens_main_layer` failed once in
  a full run ("11 pictures in 1 s") and passed alone 2 of 2 after the
  warm-up. Under the same load `audio::processing::tests::high_stays_when_it_keeps_up`
  and `high_suppression_silences_every_kind_of_noise` failed once each in
  a full `opencord-media` run, and passed in the full workspace run.
