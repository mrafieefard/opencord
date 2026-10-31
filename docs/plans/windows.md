# Windows plan (Phase 2, platform work)

> For the Claude Code session on the Windows PC, and for the person setting the
> PC up. It covers the Windows half of Phase 2 V3, V5, V6 and V7: everything
> the Linux machine built but couldn't build or test for Windows. Phase 2's
> plan ([phase-2.md](phase-2.md)) stays the spec; this file says what to build
> on Windows, in what order, how to test it, and what was decided. Section
> numbers like §9.3 refer to phase-2.md.
>
> **A Mac session works at the same time** ([macos.md](macos.md)). Read
> [platforms.md](platforms.md) first: it says which files are yours, which are
> shared, and how the work gets merged without the two sessions colliding.

## Next phase: W2 to W8 (after the Mac's M8)

**Done:** W0 and W1 (merged), and the Linux session's seams, L1
(platforms.md §2). **The Mac goes first** (M2 to M8); start when the user
says so. Begin with `git fetch && git merge origin/phase2-remaining`, which
by then holds the Mac's work too, and check that
`git log --oneline --grep 'platform seams (L1)'` prints a commit.

**This phase is everything left on Windows: W2 to W8 in §4, in order.**

| Milestone | What you build | Your files (L1 stubs, the FFmpeg files from W3, your folders) |
|---|---|---|
| W2 | Global keybinds: a low-level keyboard hook | `hotkeys/windows.rs` |
| W3 | FFmpeg codecs on Windows (NVENC, AMF, Quick Sync, Media Foundation), D3D11VA decoding, bundled DLLs, CI | the FFmpeg files in `video/codec/`, `video/codec/d3d11va.rs`; delete `windows_pending.rs` |
| W4 | Others' video shown in the app (irondash textures) | none new: check them |
| W5 | Camera through Media Foundation | `video/camera/media_foundation.rs` |
| W6 | Screen share: Windows.Graphics.Capture and Opencord's own picker | `video/screen/windows.rs` (+ files beside it); `screenshare_picker.dart`, a new `screen_source_picker.dart`, the source-passing part of `voice_controls.dart` |
| W7 | Screen share sound through WASAPI process loopback | `share_audio/wasapi.rs` |
| W8 | Hardware checklist, CI steps, packaging notes, the log | `app/windows/**`, your CI steps, §7 |

Paths are under `crates/opencord-media/src/` and `app/lib/features/voice/`.
Checks and integration tests go in `app/tool/windows/` and
`app/integration_test/windows/`.

**L1 already did, so don't build these:**
- Every Rust↔Dart API change this plan needs:
  - `video_support()` (with `own_screen_picker`);
  - `screen_sources()` with thumbnails;
  - `screen_share_start(request, source_id)` and the repository's
    `startScreenShare(…, sourceId)`;
  - `ScreenShareStarted.audio_app`, so a window share that knows its app
    doesn't ask "Sound from".
- The mock repository's `debugScreenSources`, `lastScreenSourceId` and
  `debugShareSound`, for your picker's widget tests.
- Shared code that calls your files.
- Run-time skips for tests that need a codec.

**Adjusted to this PC** (from W0 and W1):
- **Windows 10 22H2 (build 19045).** Screen share sound needs 20348.
  - In W7, write the backend and unit-test what runs here; its `support()`
    says "Sharing sound needs Windows 11".
  - The §10.7 live test needs a Windows 11 PC. Whether to upgrade is the
    user's call: log W7's live test as not done unless one is there.
  - W2 to W6 are unaffected.
- **LLVM must be installed before W3,** from an administrator prompt (the
  user does this): `winget install --id LLVM.LLVM -e`, then set
  `LIBCLANG_PATH=C:\Program Files\LLVM\bin`.
- **One VB-CABLE pair,** no Hi-Fi CABLE. Wherever §4 says Hi-Fi CABLE, use the
  W1 arrangement: `check_microphone.ps1` makes CABLE Input the default output
  for a run, and the app's microphone is CABLE Output.
  - In W7 the app's speaker can be CABLE Input too: process loopback
    captures by process, not by device.
- **Builds run through the `C:\src\opencord` junction,** as in W0.
- **The echo test** (W1 step 6) is met but not demonstrated. It needs external
  speakers: optional, the user's call.
- **`opencord-voice`'s `tests/video.rs`** misses its timing on this PC. It's a
  Linux-owned file: rerun it alone, log it, and don't let it block a
  milestone.

**When the user has to be at the PC:**

| Milestone | What for |
|---|---|
| W3 | Installing LLVM (administrator) |
| W5 | The webcam, or OBS Virtual Camera |
| W7 | A Windows 11 PC for the live test, if there is one; a video in Edge (optional) |
| W8 | Monitors, an HDR monitor, a headset, sleep and wake, and the Mac and the Linux machine for a call |

Batch these, and say exactly what to do.

## 0. Working rules

- **Branch:** `phase2-windows`, made from `phase2-remaining`.
  - Start each milestone with `git fetch && git merge origin/phase2-remaining`.
  - Push at the end of each milestone.
  - Never merge `phase2-macos`, and never commit to `main` or `pre`.
- **Your files:** change only the files [platforms.md](platforms.md) §3 gives
  to Windows. Anything else follows §4 there: a minimal `shared: …` commit
  with the reason in the log.
- **L1 has landed** (commit `refactor: platform seams (L1)`, platforms.md §2):
  the stub files you fill in from W2 on exist, the shared code already
  calls them, and every Rust↔Dart API change this plan needs is made.
- **Don't start V9.** Don't use or mention the Remotion connector.
- **No questions mid-work.** Make the call, record it as a ruling in §7, and
  go on. Some steps need the user at the PC: speakers on, a real webcam,
  monitors to plug in. Batch those and say exactly what to do.
- **Say where you are:** every progress message names the plan position, for
  example "Windows plan W3, step 3.2".
- **Style:** match the code around you. Read the Linux module you're mirroring
  first; copy its doc comments, naming and plan references. Keep the public
  API the stub file defines, so callers compile unchanged.
- **Checks before each commit:**
  - `cargo fmt --all --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`. The off-Linux
    dead-code warnings L1 removed must not come back; a warning in a shared
    file goes in the log.
  - `cargo test --workspace --lib --bins --tests`
  - In `app/`: `flutter analyze` and `flutter test`
  - Golden tests (`test/golden/`) were rendered on Linux, so they may fail on
    Windows. That's expected; never regenerate them there.
  - A shared test that fails on Windows for a platform reason goes in the log.
    Don't edit it.

## 1. Install on the PC

Windows 11 is best: sharing screen sound needs build 20348 or later. In an
administrator PowerShell:

```powershell
# Developer Mode: Flutter needs symlinks for plugins.
start ms-settings:developers        # turn on "Developer Mode"
# Long paths: cargokit's build folders get deep.
New-ItemProperty -Path HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem `
  -Name LongPathsEnabled -Value 1 -PropertyType DWORD -Force

winget install --id Git.Git -e
winget install --id GitHub.cli -e
winget install --id Microsoft.PowerShell -e   # pwsh 7, for the check scripts
winget install --id Python.Python.3.12 -e     # the dB measurements (py -3)
winget install --id LLVM.LLVM -e              # libclang, for FFmpeg's bindings
winget install --id Rustlang.Rustup -e
# Visual Studio 2022 with "Desktop development with C++"
# (MSVC, the Windows SDK, CMake):
winget install --id Microsoft.VisualStudio.2022.Community -e `
  --override "--add Microsoft.VisualStudio.Workload.NativeDesktop --includeRecommended --passive"
```

In a normal PowerShell:

```powershell
git config --global core.longpaths true
rustup default stable-x86_64-pc-windows-msvc

# Flutter 3.44.0, the version CI pins.
git clone https://github.com/flutter/flutter.git -b 3.44.0 C:\src\flutter
# Add C:\src\flutter\bin to the user PATH, open a new terminal, then:
flutter doctor

# The repository: a short path, against MAX_PATH trouble.
gh auth login
git clone https://github.com/mrafieefard/opencord.git C:\src\opencord
cd C:\src\opencord
git switch phase2-remaining
git switch -c phase2-windows
```

**FFmpeg (decision B in §3).** It's needed from W3, but install it now:
1. Download an LGPL shared build of FFmpeg **9.0**, the major the Linux
   bundle ships (`app/tool/build_ffmpeg.sh`, D32), from
   https://github.com/BtbN/FFmpeg-Builds/releases. Pick the asset named like
   `ffmpeg-n9.0-latest-win64-lgpl-shared-9.0.zip`. Never a `gpl` one. If no
   9.0 build is there, use the closest 9.x and record it in §7.
2. Unzip it to `C:\ffmpeg`, so that `C:\ffmpeg\bin\avcodec-*.dll`,
   `C:\ffmpeg\include` and `C:\ffmpeg\lib\*.lib` exist.
3. Set the user environment:
   - `FFMPEG_DIR=C:\ffmpeg`
   - `LIBCLANG_PATH=C:\Program Files\LLVM\bin`
   - add `C:\ffmpeg\bin` to `PATH`, which also gives you `ffmpeg.exe` and
     `ffplay.exe` for the tests.
4. If the zip has no `lib\*.lib`, use vcpkg's `ffmpeg:x64-windows` (LGPL
   features only) instead, and record that in §7.

**Test audio devices** (stand-ins for PipeWire null sinks):
- Install **VB-CABLE** and **Hi-Fi CABLE** from https://vb-audio.com (both
  free). They are drivers: run the installers as administrator, then reboot.
  - VB-CABLE carries the app's output to a recorder.
  - Hi-Fi CABLE is the app's fake microphone.
- To switch the default device from scripts:
  `Install-Module AudioDeviceCmdlets -Scope CurrentUser`.

**Optional:**
- OBS Studio, for its virtual camera (`winget install OBSProject.OBSStudio`).
- A second monitor, an HDR monitor and a headset, for W8.
- Excluding `C:\src` from Microsoft Defender's real-time scan makes builds much
  faster. That's the user's call.

`flutter_rust_bridge_codegen` isn't needed: this session doesn't change the
Rust↔Dart API (platforms.md §4).

## 2. What you will be asked to allow

Windows doesn't prompt desktop apps for most of this, so a missing permission
looks like a failure. Under Settings → Privacy & security:

| Setting | Needed for | When |
|---|---|---|
| Microphone → "Microphone access" and "Let desktop apps access your microphone" | voice | W1 |
| Camera → "Camera access" and "Let desktop apps access your camera" | camera | W5 |
| Windows Defender Firewall prompt, the first time the server or app listens on the LAN: allow on Private networks | calls between machines | W8 |

Global keys and screen capture need no permission. Windows 11 draws a yellow
border around what's being captured; that's expected.

The user also has to be at the PC to:
- turn the speakers on for the echo test (W1);
- use the real webcam (W5);
- optionally, play a video in a browser for the window-sound test (W7);
- for the hardware checklist (W8): plug monitors and a headset in and out,
  put the PC to sleep, and start the other machines for a call between them.

## 3. Where Windows stands, and the decisions

### What the code does on Windows today
- **Works, but never checked on Windows:**
  - the voice transport, audio engine (cpal's WASAPI host), processing chain,
    soundboard and all the UI;
  - the Windows runner (window chrome, single instance, `opencord://` links,
    tray).

  CI builds the Windows app for `main`/`pre` and PRs, never this branch. On
  2026-10-13 the Linux session type-checked the not-Linux code paths with a
  FreeBSD target. They compile, with dead-code warnings in shared files.
- **Linux-only, gated behind `cfg(target_os = "linux")`:**
  - cameras;
  - the H.264 codec (FFmpeg, OpenH264);
  - the video sender, receiver and screen capture;
  - screen share sound;
  - Flutter textures.

  L1 turns each into a Windows stub file that reports "not supported"
  (platforms.md §2). Global hotkeys report `FocusedOnly`.
- **Bundling:** the Rust library is a DLL (`opencord_core.dll`), so it links
  its own Windows libraries; there's no framework list as on macOS. Any extra
  DLL it needs at run time (FFmpeg's) must be added to
  `opencord_core_bundled_libraries` in
  `app/rust_builder/windows/CMakeLists.txt`. Flutter copies that list next to
  the exe.

### Decisions (copy each into §7 as a ruling when you start)

**A. Bindings: the `windows` crate at version 0.62, the one cpal already
uses.**
- Add only the features each milestone needs, in opencord-media's
  `[target.'cfg(windows)'.dependencies]` section (platforms.md §2, item 7).
- WinRT (Windows.Graphics.Capture) also comes through the `windows` crate.
- COM: initialize as multithreaded apartment (`CoInitializeEx`, MTA) on our
  own threads, never on Flutter's UI thread.
- COM callbacks (the audio activation handler, capture events) use
  `#[windows::core::implement]`.

**B. Video codec: FFmpeg, reusing the Linux codec layer (§7.9's choice).**
- Windows uses the same `encoder.rs`, `decoder.rs`, `raw.rs`, `scale.rs` and
  `ffmpeg.rs` as Linux. This reuses about 1,200 tested lines, including
  NVENC's settings, MJPEG decoding for webcams and swscale.
- Encoders, in order:

  | Encoder | Notes |
  |---|---|
  | NVENC | |
  | AMF (AMD) | |
  | Quick Sync (Intel) | |
  | Media Foundation (`h264_mf`) | a GPU's MFT, else Microsoft's software one |
  | OpenH264 | only when Cisco's DLL is present |

- Decoding: D3D11VA for large layers, software otherwise.
- Packaging: the LGPL shared DLLs ship next to the exe.
- Cost if wrong: about 25–40 MB of DLLs in the download (measure and record),
  against writing Media Foundation transforms by hand.
- macOS chose VideoToolbox without FFmpeg (its decision B), so the two never
  share codec files.

**C. Minimum Windows versions.**

| Feature | Needs |
|---|---|
| Voice, keybinds, camera, video, screen share | Windows 10 21H2 or later (Windows.Graphics.Capture needs 1903) |
| Screen share sound | build 20348 or later (Windows 11 in practice) |

On Windows 10 the sound switch is disabled with "Sharing sound needs
Windows 11".

**D. Opencord's own picker (§9.1).**
- Rust lists screens and windows with thumbnails (`screen_sources`, an API L1
  adds).
- The share dialog shows Screens and Windows tabs; the user chooses there.
- No system dialog follows on Windows, so the integration tests can drive the
  real picker.

**E. A window's sound is known.**
- The window's process comes from `GetWindowThreadProcessId`. For UWP apps it
  comes from the hosted `CoreWindow` child of `ApplicationFrameHost.exe`.
- A window share shares that process tree. It doesn't ask "Sound from…", but
  that menu still works for choosing another app.

**F. Test stand-ins, like the Linux ones.**

| Linux | Windows |
|---|---|
| PipeWire null sinks | VB-CABLE (app → recorder) and Hi-Fi CABLE (fake microphone) |
| `parec` | `ffmpeg -f dshow -i audio="CABLE Output (VB-Audio Virtual Cable)"` |
| `pw-play` / other apps playing | `ffplay -nodisp -f lavfi "sine=frequency=1000"` (a separate process) |
| the virtual PipeWire screen | `ffplay -f lavfi testsrc2=size=1280x720:rate=30`: moving content, and a window to share; with `-f lavfi "testsrc2…;sine=…"` it is also one app with sound |
| XTest key presses | `SendInput` |
| the virtual PipeWire camera | the real webcam with the user present, or OBS Virtual Camera |

- **Scripts** are PowerShell 7 (`app/tool/windows/*.ps1`), because Git Bash's
  `python3` and `setsid` are unreliable. Their server start mirrors
  `e2e_server.sh` in `app/tool/windows/e2e_server.ps1`.
- **dB measurement:** copy the Goertzel code from the Linux scripts into
  `app/tool/windows/measure.py`, run with `py -3`.
- **The voicebots** measure what's received exactly as on Linux. That part
  doesn't depend on the platform.

## 4. Milestones

Order: W0 → W1 → (L1) → W2 → W3 → W4 → W5 → W6 → W7 → W8. Each one ends with
a commit, a push, and a line in §7.

### W0: Setup and baseline
1. Install (§1). Record in §7:
   - the Windows edition and build (`winver`);
   - the CPU and GPUs (`Get-CimInstance Win32_VideoController | select Name, DriverVersion`);
   - the Visual Studio, Rust and Flutter versions.
2. Rust:
   - `cargo build --workspace`
   - `cargo test --workspace --lib --bins --tests`
   - `cargo clippy --workspace --all-targets`

   Expect some shared tests to assume Unix (paths, signals). List each failure
   in §7; don't fix it (platforms.md §4).
3. In `app/`: `flutter pub get`, `flutter analyze`, `flutter test`.
4. Run the app: `flutter run -d windows`, against a local server started with
   `cargo run -p opencord-server`. Sign in, open a text channel, send a
   message. Check that `opencord://` links and the tray work.
5. Write `app/tool/windows/e2e_server.ps1`, the PowerShell twin of
   `e2e_server.sh`. It sets up:
   - a free port;
   - a temporary folder that is removed on exit;
   - the server's claim token;
   - separate app data folders through `OPENCORD_PROFILE`.

   It has to stop everything it started, even on failure (`try`/`finally`).
6. **Accept:**
   - everything builds;
   - the tests pass, apart from the listed Unix-only ones and the goldens;
   - the app runs and chats;
   - `e2e_server.ps1` starts and stops a server.

### W1: Voice works on Windows, measured (§7.6, V2, V3)
1. Turn on microphone access for desktop apps (§2).
2. **Find out what happens when access is off:** the stream fails to open, or
   records silence. Record which. If it's silence, check that the app's "No
   microphone could be opened." / level meter makes it noticeable. If it
   isn't noticeable, note that in the log as a shared UI follow-up.
3. **Join voice from the app.** Check:
   - a voicebot hears the app, and the app hears a voicebot;
   - the level meter moves;
   - the device lists name the PC's devices.

   Record the device ids the app gets from `audio_devices()`, for example
   `wasapi:…`; the test below needs them.
4. **Per-user audio check:**
   - Write `app/integration_test/windows/user_audio_test.dart` (a copy of the
     shared one, using Windows device ids) and
     `app/tool/windows/check_user_audio.ps1`.
   - The app's output device is CABLE Input, chosen in the app's own
     settings, as the Linux test chooses its sink.
   - Record with ffmpeg from CABLE Output.
   - **Accept:** the same numbers as Linux, within 0.5 dB:

     | Setting | Expected |
     |---|---|
     | 50 % | −6 dB |
     | 200 % | +6 dB |
     | Mute | silence (< −90 dBFS) |
     | Mute soundboard | the sound silent, the voice unchanged |

5. **Microphone path:**
   - The app's input is Hi-Fi CABLE Output.
   - Play a tone into Hi-Fi CABLE Input: make it the default output for a
     moment with AudioDeviceCmdlets, play with `ffplay`, then restore.
   - A listening voicebot measures the tone.
   - Repeat in push-to-talk and in voice activity mode.
6. **Echo test** (V3's acceptance; needs the user, with the speakers on):
   - The app uses the PC's speakers and microphone.
   - A Talker voicebot plays 440 Hz; a Listener voicebot measures 440 Hz in
     what the app sends.
   - **Accept:** under −50 dBFS while the app plays the tone at about
     −12 dBFS.
7. Check that the audio thread got real-time priority: audio_thread_priority
   uses MMCSS on Windows; look in the app's log.

### W2: Global keybinds (§7.13; needs L1)
1. **`crates/opencord-media/src/hotkeys/windows.rs`**, the L1 stub, becomes a
   low-level keyboard hook:
   - A thread of its own calls `SetWindowsHookExW(WH_KEYBOARD_LL, …)` and runs
     a `GetMessageW` loop.
   - The hook procedure only:
     - maps the key;
     - updates the shared `Held` counts;
     - sends `HotkeyEvent`s down the channel;
     - always calls `CallNextHookEx`.

     It never swallows a key and never blocks: Windows drops a hook that
     exceeds `LowLevelHooksTimeout`, without telling it.
   - Stopping: `PostThreadMessageW(WM_QUIT)`, then `UnhookWindowsHookEx`.
   - The plan suggests `RegisterHotKey` for toggles. Don't use it: it takes
     the key away from other apps. The hook handles toggles too, and that is
     recorded as a ruling.
2. **Keys.** Accelerators use X keysym names (`CTRL+SHIFT+m`, `Control_R`,
   `F13`, `KP_5`). Map them to virtual-key codes:
   - **A fixed table** for function keys, navigation, arrows, the keypad and
     modifiers:
     - keypad digits are `VK_NUMPAD0`–`9`;
     - `KP_Enter` is `VK_RETURN` with `LLKHF_EXTENDED`;
     - the left/right modifiers are `VK_LCONTROL`/`VK_RCONTROL`,
       `VK_LSHIFT`/`VK_RSHIFT`, `VK_LMENU`/`VK_RMENU` and `VK_LWIN`/`VK_RWIN`,
       which the hook reports directly.
   - **Printable keys:** use `VkKeyScanExW`, with the keyboard layout of the
     foreground window's thread. Rebuild the table when the bindings change
     and when that layout changes.
   - **AltGr** arrives as a left Ctrl plus a right Alt. Don't let it trigger a
     `CTRL+ALT+…` binding by accident; follow what `x11.rs` does with
     modifiers.
   - Keep modifier state from the hook's own events, not
     `GetAsyncKeyState`.
   - Pressing and releasing must behave exactly as in `x11.rs`: read its tests
     and mirror them.
3. **Support:** `Global { method: "a Windows keyboard hook" }`.
4. **Known limit, for the log:** keys typed into an app running as
   administrator don't reach a hook in a normal process (UIPI). Push-to-talk
   in such a game needs Opencord run as administrator.
5. **Tests:**
   - Unit tests: the keysym ↔ virtual-key table, extended keys, AltGr, and
     left/right modifiers.
   - An `#[ignore]`d live test that presses real keys with `SendInput` (the
     hook sees injected keys), mirroring the Xvfb test:
     - press and release reach the action;
     - two push-to-talk keys hold until both are up;
     - Right Ctrl alone works;
     - a toggle fires once per press.
6. **The app's key recorder** (shared Dart) on Windows: check that AltGr, the
   Windows key and Print Screen record sensibly. Put any problems in the log.
7. **Accept:**
   - Push-to-talk works while Notepad has focus. Measured: a listening
     voicebot hears the Hi-Fi CABLE tone only while the injected key is held.
   - Settings say the keybinds are global.

### W3: FFmpeg codecs on Windows (§7.9, V5)
1. **Turn the codec layer on for Windows.** In `video/codec/mod.rs`:
   - The FFmpeg block's `cfg(target_os = "linux")` becomes
     `cfg(any(target_os = "linux", windows))` for `decoder`, `encoder`,
     `ffmpeg`, `openh264`, `raw` and `scale`.
   - `vaapi` stays Linux-only.
   - Delete `windows_pending.rs` and its block, and add Windows to the
     FFmpeg block's `supported()`.

   `ffmpeg-next` and `openh264` move into the Windows target section of
   `opencord-media/Cargo.toml` as well. Linux behaviour must not change: the
   Linux session runs its full checks when merging W3.
2. **Encoders.**
   - `Backend` gets `#[cfg(windows)]` variants `Amf`, `Qsv` and
     `MediaFoundation`.
   - `Backend::ALL` is per OS:
     - Windows: `[Nvenc, Amf, Qsv, MediaFoundation, OpenH264]`;
     - Linux: unchanged.
   - FFmpeg's encoders: `h264_nvenc`, `h264_amf`, `h264_qsv`, `h264_mf`.
   - Low-latency options (check the names with `ffmpeg -h encoder=<name>`):

     | Encoder | Options |
     |---|---|
     | NVENC | the Linux settings |
     | AMF | `usage=ultralowlatency`, `rc=cbr`, `quality=speed`, `bf=0`, headers on every IDR |
     | Quick Sync | `preset=veryfast`, `look_ahead=0`, `async_depth=1`, `bf=0` |
     | Media Foundation | `rate_control=cbr`, a low-latency `scenario`, `hw_encoding=1` (then without it, for Microsoft's software encoder) |

   - **Each encoder must pass the existing tests:**
     - every picture comes out of its encode call;
     - a keyframe arrives on request, led by the SPS and PPS;
     - the bitrate stays within target.

     An encoder that holds pictures back is dropped from `ALL`; record that
     as a ruling.
   - Frames go to the encoders in memory, as NV12. D3D11 frames without
     copying are V10's.
3. **Decoder.**
   - New file `video/codec/d3d11va.rs`, beside `vaapi.rs`: a D3D11VA device
     (`AV_HWDEVICE_TYPE_D3D11VA`); `get_format` prefers `AV_PIX_FMT_D3D11`;
     download NV12 with `av_hwframe_transfer_data`.
   - In `decoder.rs`, the hardware device comes from `vaapi` on Linux and
     `d3d11va` on Windows (`cfg`). Software otherwise.
4. **OpenH264.**
   - Cisco's Windows build is named like `openh264-2.6.0-win64.dll`. Give
     `openh264.rs`'s file-name check and its system names a Windows branch.
   - Nothing calls `set_library_dir` yet, and downloading Cisco's binary on
     first use isn't built on any OS (out of scope). Media Foundation's
     software encoder is Windows' practical fallback.
5. **Bundle FFmpeg.**
   - Add the DLLs `opencord_core.dll` depends on to
     `opencord_core_bundled_libraries`, from `$ENV{FFMPEG_DIR}/bin`. Check the
     list with `dumpbin /dependents`; it's likely avcodec, avformat, avutil,
     swscale and swresample.
   - **Check the release build:**
     1. `flutter build windows --release`.
     2. Rename `C:\ffmpeg` for a moment.
     3. Run `build\windows\x64\runner\Release\opencord.exe`; it must start
        and encode.
     4. Restore the name.
6. **CI**, in the `desktop` job's Windows steps only:
   - Before Build: download the same BtbN zip, pinning the exact asset URL and
     checking its SHA-256, as `build_ffmpeg.sh` pins its source. Set
     `FFMPEG_DIR` and `LIBCLANG_PATH` (LLVM is on `windows-latest`).
   - BtbN's build carries every LGPL codec. A minimal build like Linux's
     (D32: only the codecs the engine uses) is a W8 follow-up; record the
     DLL sizes so the user can decide.
   - Since 2026-10-31 CI builds that minimal FFmpeg itself
     (`app/tool/windows/build_ffmpeg.ps1`, cached by its hash): BtbN deletes
     dated builds after about two weeks, so a pinned asset stops existing.
     BtbN's full build is still fine on a developer's PC, where its
     `ffplay.exe` serves the hand checks.
   - In "Package Windows": add FFmpeg's license file next to the DLLs.
   - Record the zip's size before and after.
7. **Voicebot:**
   - After L1 its video code compiles on Windows; check that it runs.
   - Write `app/tool/windows/check_v4.ps1`, the `check_v4.sh` flow: a voicebot
     camera → a watching voicebot at 360p, every picture intact.
8. **Accept:**
   - The codec tests pass with this PC's hardware encoder.
   - `check_v4.ps1` passes.
   - The release build runs without FFmpeg installed.
   - Encode cost per layer (720p30, 360p30, 180p15), hardware and software, is
     in the log.

### W4: Seeing video in the app (§7.11)
1. After L1 the textures (irondash) compile on Windows. Check that they work:
   - `video_init` gets the engine handle;
   - a voicebot's test-pattern camera shows up;
   - its colours are right (red looks red);
   - `video_texture_stats` shows pictures both presented and drawn.
2. Write `app/integration_test/windows/receive_video_test.dart`, after the
   `v5` flow, receive half.
3. **Accept:**
   - The app on Windows shows a voicebot's camera and a voicebot's screen
     stream.
   - Watching opens focus mode.
   - Hidden tiles stop receiving.

### W5: Camera (§8, V5)
1. **`video/camera/media_foundation.rs`**, the L1 stub, becomes the real
   camera code:
   - **Listing:**
     - `MFStartup` once.
     - `MFEnumDeviceSources` for video capture devices.
     - `id` = the symbolic link (stable while plugged in); `name` = the
       friendly name.
   - **Starting:**
     - `MFCreateDeviceSource`, then `MFCreateSourceReaderFromMediaSource`.
       Leave the reader's video processing off; we convert ourselves.
     - Pick the native type: up to 1280×720 at ≥ 30 fps, preferring NV12, then
       YUY2, then MJPG. Set it with `SetCurrentMediaType` and report it as the
       `Mode`.
   - **Capture thread:**
     - A synchronous `ReadSample` loop.
     - Lock the buffer with `IMF2DBuffer::Lock2D` for its stride.
     - Pack it with `camera::pack` into a `RawFrame`, then `try_send` it.
     - MJPEG goes to the shared `RawDecoder`, which is FFmpeg's after W3.
   - **Errors:**

     | Case | Result |
     |---|---|
     | `E_ACCESSDENIED` | `CameraError::Denied`, with "Allow camera access for desktop apps in Settings → Privacy & security → Camera" |
     | Another app has the camera | `Failed`, with that said. Find and record the HRESULT. |
     | Unplugged (`ReadSample`'s error flag, or a failure) | close the frame queue |

2. **Tests:**
   - An `#[ignore]`d real-camera test, run with the user present (or OBS
     Virtual Camera): the camera is listed, the first frame arrives within
     1 s, and frames stop after stopping.
   - `app/tool/windows/check_v5.ps1`: the app turns the camera on, and a
     watching voicebot decodes every picture.
3. **Accept** (§8 and V5):
   - Camera on and off each take under 1 s (measured).
   - Three layers sent.
   - The preview is mirrored; what's sent isn't.
   - CPU in the log.

### W6: Screen share (§9, V6)
1. **Listing sources:** `video/screen/windows.rs` (the L1 stub), plus files
   beside it.
   - `sources()` returns, newest first, at most 30 windows:
     - **Screens:**
       - `EnumDisplayMonitors`.
       - Names like "Screen 1 · 2560×1440".
       - id `screen:<device name>` (`\\.\DISPLAY1`).
       - A thumbnail of at most 320×180, from `BitBlt` of the monitor from
         the desktop DC.
     - **Windows:**
       - `EnumWindows`, skipping these: invisible windows, cloaked ones
         (`DWMWA_CLOAKED`), `WS_EX_TOOLWINDOW` windows, untitled ones and
         Opencord's own.
       - The app name is the exe's `FileDescription`.
       - id `window:<hwnd>`.
       - The thumbnail comes from `PrintWindow(…, PW_RENDERFULLCONTENT)`.
2. **Capture**, with `start()`:
   - `IGraphicsCaptureItemInterop::CreateForMonitor` / `CreateForWindow`.
   - A D3D11 device with BGRA support, wrapped as an `IDirect3DDevice`.
   - `Direct3D11CaptureFramePool::CreateFreeThreaded(B8G8R8A8, 2, size)`.
   - A session with the cursor captured.
   - Set `MinUpdateInterval` from the fps where the system has it; otherwise
     drop frames above the fps.
3. **Each frame:**
   - Copy it to a staging texture and `Map` it.
   - Pack the BGRx rows into a `RawFrame`.
   - When the content size changes, `Recreate` the pool at the new size.
4. **Endings and edge cases:**
   - `item.Closed` (the window closed, the monitor gone) closes the queue, and
     the core shows "The shared window was closed".
   - Minimized windows send nothing. Check that the sender keeps sending the
     last picture at 1 fps (§9.4).
5. **`Picked`** (the contract is in `screen/mod.rs`):
   - `kind`.
   - `sound = Some(WindowSound { source: Source::Process(pid), app })` for a
     window, using decision E for UWP apps; `None` for a screen. The app
     then skips "Sound from".
6. **`support()`:** supported when `GraphicsCaptureSession::IsSupported()`;
   `own_picker: true`.
7. **GPU work only if needed:** start with the CPU path. Measure CPU at 1080p30
   and 1440p60. If 1080p30 takes more than 15 % of a core, convert and scale
   on the GPU with the D3D11 video processor (to NV12, at the layer size)
   before mapping. Record the numbers either way.
8. **App**, in your Dart files (platforms.md §3):
   - `screen_source_picker.dart`: Screens and Windows tabs; thumbnails that
     refresh every 2 s while open; a selected state.
   - `screenshare_picker.dart` shows it when the repository's
     `capabilities.ownScreenPicker` is set. Go live stays
     disabled until something is chosen.
   - "Change source" while live opens it again.
   - The chosen id goes through `startScreenShare(…, sourceId)`, which L1
     adds.
   - Widget tests use the mock repository's fake sources.
9. **Tests** (`app/tool/windows/check_v6.ps1` and
   `app/integration_test/windows/v6_test.dart`):
   - `ffplay` shows `testsrc2`. The test picks "Screen 1" in the real dialog
     and goes live at 720p30. A watching voicebot decodes every picture, at
     the right size and about 30 fps.
   - The same with the `ffplay` window.
   - Closing that window ends the share with its message.
10. **Accept:**
    - V6's acceptance on Windows.
    - CPU in the log.
    - Optional, with an HDR monitor: the colours aren't washed out.

### W7: Screen share sound (§10.1, §10.2, §10.7, V7)
1. **`share_audio/wasapi.rs`** (the L1 stub): WASAPI process loopback, as in
   Microsoft's ApplicationLoopback sample.
   - **`support()`:** the build is 20348 or later (`RtlGetVersion`).
     Otherwise "Sharing sound needs Windows 11".
   - **What to capture:**

     | Source | Mode | Target process |
     |---|---|---|
     | `Everything` | `PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE` | Opencord's pid |
     | `Process(pid)`, or `App` resolved to its pid | `INCLUDE_TARGET_PROCESS_TREE` | that pid |
     | `Nothing` | no capture | — |

   - **Activating:**
     - Call `ActivateAudioInterfaceAsync(VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
       IAudioClient, …)` with `AUDIOCLIENT_ACTIVATION_PARAMS` in a `VT_BLOB`
       PROPVARIANT.
     - The completion handler is an `#[implement]`ed
       `IActivateAudioInterfaceCompletionHandler`. It must also be agile.
     - Wait for it to complete.
   - **Initializing:**
     - Shared mode, `LOOPBACK | EVENTCALLBACK`.
     - Try 48 kHz stereo float first. If refused, use 16-bit PCM at 48 kHz, as
       the sample does. Record which works.
     - Follow the sample's buffer handling: some `IAudioClient` calls aren't
       supported on this virtual device.
   - **Capture thread:**
     - Event-driven, with `GetBuffer`/`ReleaseBuffer`.
     - Send `Captured { samples, position, captured }` exactly as
       `pipewire.rs` does (read what `position` means there).
   - **`share(source)` while live:** start a new client for the new target.
     A process loopback client can't change its target.
   - **`apps_with_audio()`:** for "Sound from…".
     - `IAudioSessionManager2` on the default output.
     - The active sessions, by process, without Opencord.
     - The exe's `FileDescription` as the name.
2. **Echo:** the processor already feeds the shared sound to the echo
   canceller (§10.6). Check that it works on Windows; nothing to build.
3. **Tests** (`crates/opencord-media/tests/share_audio_windows.rs`,
   `#[ignore]`d):
   - An exclude capture hears an `ffplay` tone but not a tone the test process
     plays itself.
   - An include capture of one `ffplay` hears only its tone while a second
     `ffplay` plays another.
   - After L1, the voicebot's screen-sound test runs on Windows by itself.
4. **Accept** (§10.7, the "no double audio" test), with
   `app/tool/windows/check_v7.ps1` and every device on the cables, so nothing
   is audible:
   - The app shares the screen with sound.
   - A Talker voicebot talks in the call, which the app plays to CABLE Input.
   - `ffplay` plays 1 kHz.
   - A Watcher voicebot measures 1 kHz in the stream on both channels, and
     Talker below −55 dBFS.
   - **Mutation check:** exclude another pid instead of Opencord's. Talker is
     then in the stream, and the check must fail.
   - **Window test:** share an `ffplay` window playing 1 kHz while another
     `ffplay` plays 2.4 kHz. Only 1 kHz may be in the stream.
   - **With the user, optional:** the same with a video in Edge.

### W8: Platform checklist, CI, packaging, notes
1. **The §15 checklist on Windows,** with the user. Record each result:
   - every GPU at hand (NVIDIA, AMD and Intel; a hybrid laptop counts as
     two);
   - two monitors with mixed scaling (100 % and 150 %);
   - an HDR monitor;
   - sleep and wake during a call;
   - unplugging a headset during a call (the fallback toast);
   - a Bluetooth headset, which switches profile when the microphone opens.
     Note what happens.
   - a call between this PC, the Linux machine and the Mac through one server
     on the LAN, talking both ways and watching each other's screens.
2. **CI:** only your steps. In the log, propose a `cargo test` job on
   `windows-latest` for the tests that need no hardware (platforms.md §4).
3. **Packaging:**
   - The zip carries the Visual C++ runtime (it already does) and the FFmpeg
     DLLs with their license.
   - Code signing (SmartScreen warns about unsigned downloads) and an
     installer are the user's decisions; list them in the log.
4. **Log:** every measurement and ruling in §7. The Linux session moves them
   into `docs/decisions.md` and `docs/performance.md`.

## 5. Out of scope on Windows
- "Use the operating system's processing" (Windows communications effects,
  §7.4); it's off by default.
- "Lower other apps' volume" (§12): not built on any OS yet. It needs shared
  UI.
- "Hide Opencord from my screen share" (`WDA_EXCLUDEFROMCAPTURE`, §9.1): it
  needs a shared setting and API, which the Linux session adds when Windows
  and macOS both get to it.
- A capture without the yellow border, the DXGI Desktop Duplication fallback,
  and D3D11 textures without copying (V10).
- Downloading Cisco's OpenH264 on first use; not built on any OS.
- V9.

## 6. Hand-offs
See [platforms.md](platforms.md) §1 and §5: the Linux session merges each
pushed milestone, and the Mac's work reaches this branch only through
`phase2-remaining`.

## 7. Log (the Windows session fills this in)

| Milestone | Commit | Result, measurements, what's not done |
|---|---|---|
| W0 | (this commit) | **The PC.** Windows 10 Pro 22H2, build 19045.7417 — *not* Windows 11, so screen share sound (decision C) cannot be measured here; see the W7 note below. i7-13620H; RTX 4070 Laptop (32.0.16.1078) and Intel UHD (31.0.101.4255); 15.7 GB. Visual Studio Community 2026 18.8.2 (NativeDesktop), rustc/cargo 1.99.0, Flutter 3.44.0 (Dart 3.12.0), FFmpeg n9.0.2 LGPL shared, pwsh 7.6.6, Python 3.14.7. **Rust.** `cargo build --workspace` 6 m 30 s, 8 warnings, every one off-Linux dead code in a shared file. `cargo fmt --all --check` clean. `cargo test --workspace --lib --bins --tests --no-fail-fast`: **608 passed, 5 failed, 4 ignored**, plus one target that does not compile. `cargo clippy --workspace --all-targets` **fails** (five `opencord-media` examples do not compile off Linux). **Dart.** `flutter analyze` clean, 49.1 s. `flutter test`: **730 passed, 20 failed, 1 skipped**. Every failure of either is listed under "Shared code that fails on Windows" below; none was fixed here (platforms.md §4). **The app.** `check_m5.ps1` and `check_m6.ps1` both pass: the app builds (225.8 s cold, 23–29 s warm), creates an identity, claims a server, is still connected after a restart, and two identities chat live while a new restriction reaches the member's UI without a reconnect. `e2e_server.ps1` starts and stops a server. **Not done:** the tray and `opencord://` links still need a person at the PC (W0 step 4); LLVM is not installed, which W3 needs. |
| W1 | (three commits) | **Steps 1 to 5 and 7 pass. Step 6's acceptance is met but is not demonstrative on this machine's own speakers; it needs external ones.** **Step 1** needed nothing — microphone access and "Let desktop apps access your microphone" were both already `Allow`. **Step 2 answered: a denied microphone fails to open, it does not record silence.** `Microphone::open` returns `could not open the microphone: Failed to initialize audio client: Access is denied. (os error -2147024891)` — `0x80070005`, `E_ACCESSDENIED`, from `IAudioClient::Initialize`. Measured by setting both `ConsentStore\microphone` values to `Deny` and restoring them. So the app's noticeable path is the right one and no shared UI follow-up is needed: `engine.rs:551` turns that `Err` into `EngineEvent::DeviceFailed { output: false }` → `MediaEvent::DeviceFailed` → Dart `AudioDeviceFailed` → the notice "No microphone could be opened." (traced through the code, not yet watched end to end in the app). **Step 3, the device lists:** ids are `wasapi:{0.0.1.00000000}.{guid}` for microphones and `wasapi:{0.0.0.00000000}.{guid}` for speakers. 3 microphones — Intel Smart Sound mic array (the default), WO Mic Device, Razer Barracuda X 2.4 — and 2 speakers — Realtek(R) Audio, Razer Barracuda X 2.4 (the default). A control measurement with access allowed: the default microphone opened at 48000 Hz on 2 channels and delivered 144000 samples in 1.5 s (exactly 48 kHz × 2), peak −42.7 dBFS, 0 glitches, not lost. `crates/opencord-media/tests/devices_windows.rs` holds both, and is the instrument for step 2. **The rest of step 3** came out of steps 4 and 5: a voicebot hears the app (the Listener took 595 packets carrying the app's tone) and the app hears a voicebot (it played Talker's 440 Hz at −9.2 dBFS). The level meter itself was not measured separately; it is a shared Dart binding of `InputLevelChanged` and only fires while a meter is open. **Step 4, the per-user audio check, passes, every number inside the plan's 0.5 dB:** Talker's 440 Hz at 100 % −9.2 dBFS; at 50 % −15.3 dBFS (**−6.0 dB**, wanted −6); at 200 % −3.2 dBFS (**+6.0 dB**, wanted +6); muted **−106.2 dBFS** (wanted < −90); unmuted −9.7 dBFS. Player's 880 Hz sound −11.7 dBFS, with the soundboard muted **−123.0 dBFS** while Talker stayed at −9.2 dBFS, and −11.7 dBFS again when unmuted. **Step 5, the microphone path, passes in both processing modes.** 440 Hz as a Listener voicebot heard it: voice activity −21.1 dBFS, push-to-talk released **nothing at all — no packets** (recorded as −200), push-to-talk held −21.1 dBFS. Repeated with `standard` (RNNoise) suppression: −21.0 / none / −21.1 dBFS, so the chain passes a pure tone through unchanged, which is what W2's acceptance will rest on. **Step 7: the audio thread does get real-time priority.** The core installs no tracing subscriber, so there is no app log to read as the step suggests; measured instead on the threads, where MMCSS shows up as a boost into the 16–26 band: while the app held voice, exactly one of its 128 threads sat at base priority **21**, against 98 at the ordinary 8, in a process whose class is Normal. **Step 6, the echo test: the plan's acceptance is met in every run, but it does not demonstrate echo cancellation on this machine.** The app plays Talker's 440 Hz at −9.2 dBFS (W1 step 4; the plan says about −12) and what comes back is −129 dBFS or quieter, against an acceptance of under −50 — met with about 80 dB to spare. What it does not show is the canceller doing that, because the echo does not survive to be cancelled. Over 5 runs, 4 had the un-cancelled control at −129.5, −130.4, −136.4 and −147.7 dBFS, already far under the threshold, with the cancelled figure the same to within a few dB; a dead speaker would score identically. The one run that did discriminate — control −42.1 dBFS, cancelled −132.5 dBFS, 90.4 dB removed — had a sent peak of −10.9 dBFS against −78 to −88 dBFS in all the others, so what held the gate open was room noise, not the speakers, and it did not repeat. The cause is measured, not guessed: a 440 Hz tone played out of `Speakers (Realtek(R) Audio)` arrives at the Intel microphone array at **−56.8 dBFS** from a −18.2 dBFS source, with 1 kHz unmoved at −111 dBFS (room floor −112.3) — about **38 dB of acoustic loss**. At that level noise suppression removes the echo as the non-speech it is before the canceller is reached. `check_echo.ps1` therefore exits 2, "inconclusive", rather than reporting a pass it has not earned. **To demonstrate the canceller, the machine needs external speakers, turned up, close to the microphone** — see the note below. |
| W2 | (this commit) | **All seven steps done. Step 7's acceptance is measured and passing; step 6 found one limit, in shared Dart.** **The merge first,** because it settles what W0 logged: after `git merge origin/phase2-remaining` (L1 at `eb40bc8`, a fast-forward to `2de3e0c`), `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` **both exit 0 on Windows**. The 8 shared dead-code warnings, the 5 non-compiling examples and `opencord-voicebot/tests/voice.rs`'s compile error are all gone, so `-D warnings` is usable here from now on. **Steps 1 to 4, the hook.** `hotkeys/windows.rs` is a `WH_KEYBOARD_LL` hook on a thread of its own, installed from that thread and stopped with `PostThreadMessageW(WM_QUIT)`; the procedure maps the key, updates the shared `Held`, sends the events and always calls `CallNextHookEx`, taking no lock that could block (`try_borrow_mut`, and the only other borrower is a `WM_TIMER` on the same thread). `RegisterHotKey` is not used at all. Keysyms map to virtual keys through a fixed table plus `VkKeyScanExW` against the **foreground window's** layout, rebuilt on a 500 ms `WM_TIMER`. Support is `Global { method: "a Windows keyboard hook" }`. **Step 5, tests: 16 unit tests pass, 0 fail**, covering the keysym↔virtual-key table (F1/F24, KP_0/KP_9, Control_R, Super_L, Escape, KP_Divide, and `m`/`7`/`space` through the layout), extended keys (`KP_Enter` and `Return` share `VK_RETURN` and are told apart; a keypad key with Num Lock off does **not** match the arrow it reports), AltGr, left/right modifiers, and the five press/release behaviours mirrored from `x11.rs`. **5 `#[ignore]`d live tests pass** with real keys injected by `SendInput`: press and release reach the action, two push-to-talk keys hold until both are up, Right Ctrl alone works, a toggle fires once per press, and Print Screen reaches the hook. **Step 6, the key recorder: 2 of 3 keys record as the core names them, Print Screen records nothing.** Measured with keys really pressed into the real app window (`check_key_recorder.ps1`): the right Alt → `Alt_R`, shown "Right Alt"; the left Windows key → `Super_L`, shown "Left Super" (it also opens the Start menu, as it must — nothing may swallow a key); **Print Screen → `""`**, the recorder still saying "Press keys…". The cause is not the hook: `print_screen_reaches_the_hook` shows the hook gets both the down and the up, so `Print` works as a keybind once set — Windows just doesn't send a focused window the key-down, and the recorder commits a key on its key-down. A note for the Linux session, below; `check_key_recorder.ps1` exits 2 for it rather than failing. AltGr itself could not be recorded on this PC: a US layout has no AltGr key, so the right Alt is only the right Alt here; the phantom-Ctrl path is covered by the unit test instead. **Step 7's acceptance passes, measured.** `check_keybinds.ps1`, with F13 bound to push-to-talk, the app's microphone on CABLE Output and a 440 Hz tone down the cable, the key injected from outside the app **while Notepad had the focus** (`foreground=notepad` in the held run's own report): held, the Listener voicebot took **604 packets** and heard 440 Hz at **−21.1 dBFS**; never pressed, it heard **nothing at all — no packets** (recorded as −200). A difference of 178.9 dB against a 20 dB requirement, and −21.1 dBFS is the same figure W1 step 5 measured for a held push-to-talk, so the hook costs the chain nothing. Both runs reported `support=global (a Windows keyboard hook)`, which is the second half of step 7: the settings page can say the keybinds are global. **Checks:** `cargo test --workspace --lib --bins --tests` 686 passed, 4 failed, 10 ignored — every failure in `opencord-voice/tests/video.rs`, the Linux-owned file this PC has never got through (W1's note); every other W0 Rust failure is gone. `flutter analyze` clean; `flutter test` **735 passed, 1 skipped, 16 failed**, all 16 goldens and not regenerated (W0 had 20 failures; L1's shared fixes removed the other 4). `dart format` clean on own files. **Zero `shared:` commits:** `hotkeys/mod.rs` needed no change. **Known limit, for the log:** keys typed into a window of a process running as administrator never reach a hook in a normal one (UIPI), so push-to-talk inside such a game needs Opencord started as administrator; this is in the file's doc comment. |
| W3 | (this commit) | **All eight steps done; the acceptance is met. Windows encodes and decodes H.264.** **Step 1:** the FFmpeg block in `video/codec/mod.rs` is now `cfg(any(target_os = "linux", windows))` for `decoder`, `encoder`, `ffmpeg`, `openh264`, `raw` and `scale`; `vaapi` stays Linux-only and `d3d11va` is its Windows counterpart; `windows_pending.rs` is deleted; `ffmpeg-next`, `openh264` and `openh264-sys2` are in opencord-media's Windows section. LLVM 23.1.3 was installed by the user and `LIBCLANG_PATH` was already set machine-wide; `ffmpeg-sys-next` 9.0.1's bindgen builds in **39.9 s**. **Step 2, the encoders. What this PC has, from FFmpeg itself:** `h264_nvenc`, `h264_amf`, `h264_qsv` and `h264_mf` all present, **no `libx264` and no `h264` software encoder** — the LGPL build carries none, so OpenH264 is the only software path and Cisco's DLL is not fetched on any OS. `Backend` gained `Amf`, `Qsv` and `MediaFoundation` behind `cfg(windows)`, `VaApi` is now Linux-only, and `Backend::ALL` on Windows is `[Nvenc, Amf, Qsv, OpenH264]`. **Media Foundation is deliberately not in `ALL`: measured, it holds 16 pictures before letting the first out and only 14 of 30 encode calls produce a packet** (~533 ms at 30 fps), which W3 step 2 says is grounds for dropping it; NVENC and Quick Sync both answer on call 0, 30 of 30. It stays openable by name, and `media_foundation_holds_pictures_back` keeps that measurement as a test. Option names were read off `ffmpeg -h encoder=…` on this machine, not guessed. **One real bug found and fixed:** the shared `profile=high` broke `h264_mf`, which has no `profile` option at all, so FFmpeg tried the generic enum and refused to open the encoder ("Invalid argument"); `profile` is now set per encoder. **Step 3:** `video/codec/d3d11va.rs`, a D3D11VA device on the default adapter — Windows picks the adapter, so there is nothing like `vaapi.rs`'s render-node hunt; `decoder.rs` takes `vaapi` on Linux and `d3d11va` on Windows by `cfg`, and `prefer_vaapi` is now `prefer_hardware` against a per-system `SURFACE`. Hardware decoding works: `hardware_decoding_gives_nv12_pictures_where_there_is_a_device` passes here. **Step 4:** `openh264.rs` has Windows names (`openh264-2.6.0-win64.dll`, and `openh264-` as Cisco's prefix without the `lib`). Nothing fetches Cisco's library on any OS, and with Media Foundation out of `ALL` **Windows currently has no software encoder at all** — see the note below. **Step 5, FFmpeg bundled, and the release build runs without it installed.** `dumpbin /dependents` gives five DLLs transitively: avcodec-63 (91.1 MB), avformat-63 (22.8 MB), avutil-61 (3.0 MB), swresample-7 (0.7 MB), swscale-10 (2.3 MB) — **120.0 MB**, against the plan's guess of 25–40 MB. The release folder is **195.8 MB**. `check_bundled_ffmpeg.ps1` renames `C:\ffmpeg` away and measures both halves: the exe is still running after 12 s, so every DLL resolved, and the V4 check through only the bundled DLLs gives **146 pictures, 146 intact, 146 kbit/s**. **One real bug found and fixed:** `FFMPEG_DIR` is `C:\ffmpeg`, so the globbed paths reached the generated `cmake_install.cmake` as `C:\ffmpeg/bin/…` and CMake read `\f` as a bad escape — `file(TO_CMAKE_PATH …)` first. **Step 6, CI:** a Windows-only "Use FFmpeg" step downloads a pinned BtbN asset (`autobuild-2026-10-13-13-05`, `ffmpeg-n9.0.2-27-g7e93014c5b-win64-lgpl-shared-9.0.zip`, 77,230,840 bytes, SHA-256 `9df4db960f3f0cd3ed76be0a2be96265d00db4c3be2526a6d39b03c2b547100d` — downloaded and hashed here, not copied from a page) and sets `FFMPEG_DIR`, `LIBCLANG_PATH` and PATH; "Package Windows" now also ships `FFMPEG-LICENSE.txt`. **Step 7:** `check_v4.ps1` passes — **154 pictures, 154 intact, 151 decoded, 3 keyframes, 163 kbit/s, by layer 140/14/0**. **Step 8, what an encode costs on this PC** (wall time per `encode` call, 480 rounds after 10; `tests/codec_costs_windows.rs`, ignored): 720p30 NVENC **1.99 ms** / 1497 kbit/s, Quick Sync **3.50 ms** / 1079 kbit/s, Media Foundation 1.48 ms / 1380 kbit/s; 360p30 NVENC **0.60 ms** / 509 kbit/s, Quick Sync 1.89 ms / 482 kbit/s, MF 0.31 ms / 447 kbit/s; 180p15 NVENC **0.31 ms** / 149 kbit/s, Quick Sync 1.56 ms / 148 kbit/s, MF 0.18 ms / 124 kbit/s. All three layers on NVENC come to **2.90 ms** of a 33 ms frame. AMF does not open (no AMD GPU here) and OpenH264 does not (no Cisco DLL), so no software column could be measured. Quick Sync undershoots its target at 720p (1079 of 1500 kbit/s) and is the slowest of the three. **Checks:** `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` both **0**. `cargo test --workspace --lib --bins --tests -- --test-threads=1`: **676 passed, 4 failed, 11 ignored** — `opencord-media` is **243/243**, and all four failures are in the two Linux-owned files listed below. `flutter analyze` clean. **Turning the codec on woke up every `needs_codec!()` test**, and one of them found a real Windows bug, fixed in `decoder.rs`: FFmpeg here *conceals* a damaged picture and returns `Ok`, so `receiver.rs` never asked the sender for a keyframe; `err_recognition = AV_EF_EXPLODE \| AV_EF_BITSTREAM` on Windows makes it report the damage, and `damage_asks_for_a_keyframe` passes. |
| W4 | (this commit) | **The textures work. Four of the five acceptance points are measured and passing; the fifth — a voicebot's screen stream — arrives, but only about half the time, and that is carried to W6.** No Rust was needed: L1's irondash work compiles and runs here as it stands, so W4 is `app/integration_test/windows/receive_video_test.dart`, `check_receive_video.ps1` and `sample_video.ps1`. **Step 1, all four parts.** `video_init` gets the engine handle — the feed has a texture id, which is what proves it. A voicebot's test-pattern camera shows in its tile, and `video_texture_stats` reports pictures **presented and drawn**: 30–32 presented and 31–32 drawn across five runs. **The colours are right**, measured off the screen rather than assumed: the camera's chroma bands run U=96,V=160 to U=152,V=104, so the picture should be reddish on the left and bluish on the right, and it is — left `R=152.1 G=102.8 B=69.8`, right `R=104.8 G=136.3 B=161.3`, a redness (R−B) of **+82.3 on the left against −56.5 on the right**, repeatable to about a decibel across runs. A BGRA-for-RGBA swap would have reversed the sign. The capture was also looked at: the diagonal luma ramp, the bright square and the warm-to-cool bands, in the app's own rounded tile labelled "Camera". **Step 2:** the test is written, in two phases (`camera`, `screen`), each with its own server and profile. **Step 3, the acceptance.** *Shows a voicebot's camera* — yes, above. *Watching opens focus mode* — yes: focusing the tile fills the pane, which is also what makes the colour sample big enough to read. *Hidden tiles stop receiving* — yes, measured twice over: moving to a text channel empties `videoWants`, and `presented` then does not move (**132 and 132 two seconds later**, 119/119 and 128/128 in other runs). *Shows a voicebot's screen stream* — **demonstrated, but not reliably**: 2 of 4 runs, and both times at exactly 10 pictures presented and 11 drawn, which is a trickle rather than 30 fps; the other two timed out after 90 s. §9.5 is honoured, incidentally: `BEFORE WATCH: feeds=1 presented=0`, so a feed object exists but nothing has been received until Watch is clicked. **Checks:** `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings` and `flutter analyze` all 0; `cargo test … --test-threads=1` has `opencord-media` at **243/243** and no new failures — the only ones are the same two Linux-owned files (6 of 10 in `opencord-voice/tests/video.rs` this run, and the voicebot's encoded camera); `flutter test` fails only on the 16 goldens; `dart format` clean. |
| W5 | (this commit) | **The camera works, first try, and every part of the acceptance passes except "three layers sent" — and that one is not the camera's doing.** No user was needed: this PC has a built-in **HD Webcam**, and camera access for desktop apps was already `Allow`. **Step 1, `video/camera/media_foundation.rs`.** `MFEnumDeviceSources` for video capture devices; the id is the symbolic link (`mf:\\?\usb#vid_5986&pid_211b&…`) and the name the friendly name. `MFCreateDeviceSource` → `MFCreateSourceReaderFromMediaSource` with `MF_READWRITE_DISABLE_CONVERTERS`, so the reader inserts no decoder or colour converter of its own. The native types are read and `mode::choose` picks one; **this camera gives NV12 1280×720 at 30 fps**, which is the best case — a raw format, so no MJPEG decode at all. A synchronous `ReadSample` loop on its own thread, `IMF2DBuffer::Lock2D` for the real stride, then `camera::pack` and `try_send`. COM is multithreaded and started per thread, never on Flutter's UI thread (decision A). **Step 2, measured on the real camera** (`crates/opencord-media/tests/camera_windows.rs`, ignored): listed, **opened in 79 ms**, first frame **620 ms** after asking, **93 frames in 3.0 s — 30.8 a second**, stopped in **491 ms**, and nothing arrives after stopping. A frame is 1,382,400 bytes, exactly 1280×720×1.5, so the NV12 packing is right. **The "another app has the camera" HRESULT could not be found, because on this Windows it does not happen:** opening the same camera twice from one process succeeded, and holding it with ffmpeg's DirectShow first changed nothing. Windows' Frame Server has shared cameras between applications since Windows 10 1607, so an MF client is not refused. Recorded rather than invented. **Step 3, the acceptance.** *Camera on and off each under 1 s* — **on 348–378 ms, off 2 ms**, measured in the app from the tap to the preview feed appearing and going away. *The preview is mirrored, what is sent is not* — `mirrored=true` on the own feed, which is the only feed that carries it. *Three layers sent* — **2 of 3**, see below. The preview itself draws: 370 presented, 370 drawn. **CPU, for the log:** the whole check — app build excluded, server, three voicebots, the app, the camera and three encoded layers — takes **37.0 s of wall clock**; W3's own measurements are the per-layer cost (720p30 1.99 ms, 360p30 0.60 ms, 180p15 0.31 ms on NVENC, 2.90 ms for all three of a 33 ms frame). **The layer shortfall is this PC, not the camera.** Three voicebots watched at 180, 360 and 720 pixels at once — one watcher can only ever receive one layer, so three sizes is what makes "three layers" visible. The 180 watcher got layer 0 (186 pictures, 15 kbit/s), the 360 watcher layer 1 (330 of 354, 47 kbit/s), and **the 720 watcher also got layer 1** (363, 50 kbit/s), never layer 2: `210/693/0` over all three. A single watcher at 720 alone gave the same `0/371/0`, so it is not the three watchers crowding the uplink. This is the **fourth** sighting of one thing — see the note below — and the shared test `a_capped_uplink_stops_encoding_the_cameras_top_layer_until_lifted` has been failing here with exactly that complaint since W1. `check_v5.ps1` therefore exits 2, not 1. **Checks:** `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings` and `flutter analyze` all 0; `cargo test … --test-threads=1` adds `camera_windows.rs` (1 passing, 2 ignored) with no new failures; `flutter test` fails only on the 16 goldens; `dart format` clean. |
| W6 | (this commit) | **Screen sharing is built and works from the app: the picker, the capture, the sound source and the ending are all measured. What no longer works on this PC is screen-share *media* reaching anyone, in either direction — the same gap W4 found from the other side.** **Steps 1–6, the Rust.** `video/screen/windows.rs` plus `windows_gdi.rs` beside it. `support()` asks `GraphicsCaptureSession::IsSupported()` rather than reading a build number, because a machine can have the API and still refuse it; `own_picker` is true. `sources()` lists monitors from `EnumDisplayMonitors` (`screen:\\.\DISPLAY1`, "Screen 1 · 1920×1080") then at most 30 windows from `EnumWindows` in Z-order, skipping invisible, cloaked (`DWMWA_CLOAKED`), `WS_EX_TOOLWINDOW`, untitled, zero-sized and Opencord's own; the app name is the exe's `FileDescription`. Thumbnails are GDI — `StretchBlt` with `HALFTONE` from the desktop for a screen, `PrintWindow(PW_RENDERFULLCONTENT)` for a window — deliberately not a capture session each, which would light up every window's capture border while the picker is open. Capture is `IGraphicsCaptureItemInterop` → `Direct3D11CaptureFramePool::CreateFreeThreaded` → staging texture → `Map` → BGRx rows. **Measured on this PC:** a screen share starts in **184 ms** at 1920×1080 and runs at **30.3 frames a second** against the 30 asked for, and stops in 27 ms; a window share runs at **30.3** at 1440×759 and reports its sound as `Process(9800)` named "Notepad", which is decision E working. **One real bug, found by the test and fixed:** the `GraphicsCaptureItem.Closed` event never fires, because the capture thread pumps no messages — a closed Notepad left the share running for the full ten seconds the test waited. The loop now also looks at the handle itself every 250 ms, and the share ends **0.3 s** after the window closes. **Step 7, the CPU, and the decision it settles:** 1080p30 on the CPU path costs **14.8 % of one core** (median of 7 samples; worst 15.3; 0.9 % of all 16 cores), just inside the 15 % the plan allows, so the CPU path stays and no D3D11 video-processor work is needed. That is a **debug** build, so a release one is cheaper still — it is a ceiling, not a close call. 1440p60 could not be measured: this PC's only monitor is 1920×1080. `check_screen_cost.ps1` keeps the measurement. **Step 8, the app.** `screen_source_picker.dart`: Screens and Windows tabs with counts, thumbnails refreshed every 2 s (a refresh that fails leaves the list alone rather than blanking it mid-choice), and a chosen state. The share dialog shows it where `ownScreenPicker` is set, and **Go live stays disabled until something is chosen**; "Change source" while live opens the picker again, as its own dialog with only the picker in it. The chosen id goes through `startScreenShare(…, sourceId)`. **Five widget tests on the mock's fake sources, all passing**, and the 47 voice tests that were there still pass: `RepoCapabilities.everything` leaves `ownScreenPicker` false, so nothing else changed shape. **Step 9, live.** `check_v6.ps1` runs an ffplay test pattern (silent: `-an` and a video-only source) and drives the real dialog. All three phases work from the app's side: the screen shares as `StreamSource.screen`; the window shares as `StreamSource.window` with **`audioFrom=ffplay`**, so the app already knows whose sound to send and will not ask; and closing the shared window **ends the share by itself**. **What does not work: no pictures reach the watching voicebot in any of the three phases** — see the note below. The check exits 2 for that rather than 1, since every app-visible part passed. **Two of my own bugs on the way, both mine not the app's:** `Start-Process` splits an argument on its spaces, so a quoted ffplay window title arrived as three arguments and ffplay refused to start; and the watcher first asked for a 720-pixel tile, which this PC never serves (W5), so it would have measured that instead of the share. **Not done:** the optional HDR-monitor colour check (step 10) needs an HDR monitor and a person. |
| W7 | (this commit) | **Written and unit-tested; the live half has never run, because this PC cannot run it.** Process loopback needs Windows build **20348** and this is **19045**, so `support()` reports "Sharing sound needs Windows 11" and every capture path refuses before it starts. That is the decision C behaviour the plan asks for, and it is now measured rather than assumed. **Step 1, `share_audio/wasapi.rs`.** `support()` reads the build with `RtlGetVersion` — not `GetVersionExW`, which lies about anything past 8.1 without a compatibility manifest. Capture activates the virtual process-loopback device with `ActivateAudioInterfaceAsync`, an `AUDIOCLIENT_ACTIVATION_PARAMS` in a `VT_BLOB` PROPVARIANT and an `#[implement]`ed `IActivateAudioInterfaceCompletionHandler` that only wakes the caller, the result being read off the operation. `Everything` excludes Opencord's own process tree, `Process(pid)` and a resolved `App` include theirs, `Nothing` captures nothing without failing. Initialize is shared mode with `LOOPBACK | EVENTCALLBACK`, 48 kHz stereo float first and 16-bit PCM if that is refused, as the sample does — `IsFormatSupported` is not reliable on this virtual device, so it simply tries. The capture thread is event-driven with `GetBuffer`/`ReleaseBuffer`, honours `AUDCLNT_BUFFERFLAGS_SILENT`, and counts `position` in frames on its own clock as `pipewire.rs` does. `share()` tears the client down and opens another, because a process-loopback client cannot be retargeted. **`apps_with_audio()` does work here**, because it is the ordinary session API rather than loopback: `IAudioSessionManager2` on the default output, active sessions only, one row per app. **A real bug it found:** listing by pid alone offered **`opencord`** itself (a second instance left over from an earlier check) and **`audiodg`**, Windows' audio engine. Both are now excluded by name as well as by this process's id — §10.1's "never Opencord" means any Opencord, not just this one. **Step 2, echo:** not checkable here. The processor feeds the shared sound to the canceller already (§10.6) and there is nothing to build, but with no capture there is nothing to watch it do. **Step 3, tests:** `crates/opencord-media/tests/share_audio_windows.rs` holds the four capture tests — a screen hearing another app, a window hearing only its own, switching source while live, and nothing chosen capturing nothing — and all four **skip** here with "Sharing sound needs Windows 11" rather than pretending. **8 unit tests pass**, covering the build gate, the exclude/include targets, the mutation hook, the 48 kHz stereo format and the app list. **Step 4, `check_v7.ps1`:** written in full — the §10.7 run with a Talker in the call and ffplay playing 1 kHz, the **mutation check**, and the window test — and it stops at its first line here with the same words the disabled switch uses. The mutation is a real hook, not a manual edit: `OPENCORD_TEST_EXCLUDE_PID`, compiled in only under `debug_assertions`, makes the backend exclude some other process so the call's voice *must* reappear in the stream. A check that cannot be made to fail proves nothing, and now it can be. **Not done, and it needs a Windows 11 PC:** every live part of W7. The code has never captured a single sample. |
| W8 | (this commit) | **Step 1 is three items done and measured, two skipped at the user's request, two still open; steps 2 to 4 are done.** **GPUs.** This is a hybrid laptop, so it counts as two of the three: an RTX 4070 Laptop (32.0.16.1078) and an Intel UHD (31.0.101.4255); there is no AMD GPU here, and `h264_amf` accordingly never opens. Both GPUs encode every layer (`codec_costs_windows`, 480 encodes each): NVENC **2.33 / 0.70 / 0.43 ms** at 720p30 / 360p30 / 180p15 for 1497 / 509 / 149 kbit/s, Quick Sync **3.20 / 1.73 / 1.44 ms** for 1079 / 482 / 148 kbit/s, Media Foundation 2.04 / 0.58 / 0.32 ms. NVENC is what `ALL` picks here; Quick Sync opening and encoding is the Intel half of the checklist, not what the app uses while the NVIDIA GPU is there. **The headset** (`check_hardware.ps1 -Phase headset`, a Razer Barracuda X 2.4): with the headset chosen the cable measures **-135.8 dBFS** — digital silence, so the call really is in the headset; unplugged, the call keeps playing on the device it fell back to at **-9.2 dBFS** and the toast reads *"Microphone unavailable. Using Microphone Array (Intel® Smart Sound Technology for Digital Microphones)."*; replugged, the cable is **-135.8 dBFS** again, which is `switch_back` doing its job. Both of the headset's endpoints leave and return in the same 2 s device poll, and both halves fall back — but `audioNoticeProvider.show` keeps only the newest notice, so the microphone's toast is the one left on screen and the speaker's is never seen. **Sleep and wake** (`-Phase sleep`): this PC has **Modern Standby (S0 Low Power Idle, Network Connected)** and no S3 — `powercfg /a` says the firmware disables it — so both variants were run. Plain sleep, 30 s: the call was **still `connected`** at the wake, played at **-9.2 dBFS** against -9.2 before, and no toast appeared; nothing was torn down, so nothing had to recover. Hibernate (`-Hibernate`), power off for **77 s**: the same, `connected` 2 ms after the resume and -9.2 dBFS. No device was lost in either, so the audio streams either survived or were reopened on the same device without a fallback. Caveat: the app, the server and the voicebot all slept together, so no connection had to be re-negotiated with a peer that stayed awake — that case belongs to the three-machine call. **Skipped at the user's request:** two monitors with mixed scaling, and the HDR monitor. The manifest already asks for `PerMonitorV2`. **Not done:** a Bluetooth headset's profile switch (the headset here is a 2.4 GHz dongle, not Bluetooth; the same check takes `-Headset` and would cover it), and **the call between this PC, the Linux machine and the Mac** — both machines were not available. `host_lan_server.ps1` is written and waiting for it. **Step 2, CI:** no new job added; the `windows-latest` `cargo test` job is proposed below, because a new job is shared ground (platforms.md §4). **Step 3, packaging:** the zip's contents are checked below; code signing and an installer are the user's decisions, listed there. **Housekeeping:** `target/` had grown to **71.7 GB** with 6.8 GB free, which failed a link with `LNK1180`; `cargo clean` took it back (76.25 GB free) at the cost of one cold rebuild. A stale `opencord.exe` from the W7 run was holding the linker's output file, which is what `LNK1104: cannot open opencord.exe` means here. `flutter pub get` rewrote `app/analysis_options.yaml` and bumped five packages in `app/pubspec.lock` again, as in W0 and W7; both are Linux-owned and were reverted before committing. **Checks:** `cargo fmt --all --check` 0; `cargo clippy --workspace --all-targets -- -D warnings` 0 in 2 m 37 s; `cargo test --workspace --lib --bins --tests -- --test-threads=1` fails in the same two Linux-owned files as W7 and nowhere else (`opencord-voice/tests/video.rs` 4 of 11, `opencord-voicebot/tests/voice.rs` 1 of 10 — the top-layer fault in the W5 note); `flutter analyze` clean; `flutter test` **740 passed, 1 skipped, 16 failed**, every failure a golden rendered on Linux. |

Rulings (`Wn: Ruling: … — cost if wrong: …`):

The six decisions §3 settled, taken as they stand:
- W0: Ruling: bindings come from the `windows` crate 0.62, the one cpal already
  uses, with features added per milestone in opencord-media's
  `[target.'cfg(windows)'.dependencies]`; COM is initialized multithreaded on
  our own threads and never on Flutter's UI thread; COM callbacks use
  `#[windows::core::implement]` (decision A) — cost if wrong: a second copy of
  the `windows` crate in the tree, or apartment errors at run time.
- W0: Ruling: the video codec is FFmpeg reusing the Linux codec layer, with
  encoders tried NVENC → AMF → Quick Sync → Media Foundation → OpenH264,
  D3D11VA for large layers, and the LGPL shared DLLs shipped beside the exe
  (decision B) — cost if wrong: 25–40 MB of DLLs in the download, against
  writing Media Foundation transforms by hand.
- W0: Ruling: Windows 10 21H2 is the floor for voice, keybinds, camera, video
  and screen share; build 20348 for screen share sound (decision C) — cost if
  wrong: the sound switch is disabled on a machine that could have managed it.
- W0: Ruling: Opencord uses its own picker, with Screens and Windows tabs and
  no system dialog after it (decision D) — cost if wrong: the integration tests
  cannot drive a system dialog, so this is also what makes W6 testable.
- W0: Ruling: a window's sound is found from `GetWindowThreadProcessId`, and
  for UWP apps from the hosted `CoreWindow` child of `ApplicationFrameHost.exe`
  (decision E) — cost if wrong: a window share captures the wrong process's
  sound, or none.
- W0: Ruling: the test stand-ins are VB-CABLE and Hi-Fi CABLE for devices,
  `ffmpeg`/`ffplay` for recording and playing, and `SendInput` for keys, with
  every script in PowerShell 7 (decision F) — cost if wrong: Git Bash's
  `python3` and `setsid` are unreliable here, which is why the shell changed.

Decided while doing W0:
- W0: Ruling: Flutter 3.44.0 is cloned to `C:\src\flutter` and put first on
  PATH, instead of the 3.47.6 already on the PC. 3.47.6's `pub get` rewrites
  `app/analysis_options.yaml` (adding platform excludes) and `app/pubspec.lock`
  (5 packages), both shared files this session does not own, and CI pins
  3.44.0 — cost if wrong: analyzer and test results here would not match CI's
  or the Linux session's, and the churn would reach a commit.
- W0: Ruling: the repository is reached at `C:\src\opencord`, a directory
  junction to `C:\Users\mrafi\dev\opencord`, and every build runs through it.
  MSBuild's `FileTracker` ignores `LongPathsEnabled`, and cargokit's deepest
  path — opusic-sys's CMake `TryCompile` tlog — is **260** characters from the
  real path against a 259 limit, but **248** through the junction. The tree
  could not simply be moved to the short path §1 asks for, because the session
  holds it open. A junction is what makes §1's intent reachable without moving
  it — cost if wrong: builds fail with `FTK1011: could not create the new file
  tracking log file`, which is how this was found; and cargo rebuilds `target/`
  once, because its fingerprints record absolute paths.
- W0: Ruling: FFmpeg is BtbN's `ffmpeg-n9.0-latest-win64-lgpl-shared-9.0`
  (it reports `n9.0.2-27-g7e93014c5b`), unzipped to `C:\ffmpeg` so that `bin`,
  `include` and `lib\*.lib` all exist, with `FFMPEG_DIR=C:\ffmpeg` and
  `C:\ffmpeg\bin` first on PATH. §1's vcpkg fallback is not needed, and the
  build has `dshow`, `lavfi`, `testsrc2` and `sine`, which decision F's stand-ins
  all rely on — cost if wrong: a `gpl` build would make the shipped DLLs
  undistributable, and a build without `lib\*.lib` would block W3.
- W0: Ruling: PowerShell 7.6.6 was installed from the Microsoft Store, which is
  per-user, rather than §1's MSI, because this session cannot elevate — cost if
  wrong: none found; `pwsh -File` runs the check scripts.
- W0: Ruling: Visual Studio Community **2026** (18.8.2) with the NativeDesktop
  workload stands in for §1's 2022, being what the PC has. The Rust DLL and the
  Flutter runner both compile and link against it — cost if wrong: an MSVC or
  Windows SDK incompatibility would have shown as a link failure, and did not.
- W0: Ruling: `e2e_server.ps1` isolates the app's data with `OPENCORD_PROFILE`
  alone, and empties that profile's folder before and after a check. Windows
  has no `XDG_DATA_HOME`, and `getApplicationSupportDirectory` is
  `%APPDATA%\dev.opencord\Opencord`, so a profile is a folder inside it
  (`app/lib/app_start.dart`); the keychain is left to the credential manager,
  which the M5 test clears itself — cost if wrong: a check inherits the
  previous run's identity.
- W0: Ruling: a hard kill runs no `finally`, so `e2e_server.ps1` also clears
  stale `opencord-e2e-*` folders when it starts, waits for a killed process to
  actually exit before removing its work folder, and retries that removal —
  cost if wrong: temporary folders and a live server survive an interrupted
  check, which happened once before this was added.
- W0: Ruling: W0 step 4 is checked by `check_m5.ps1` and `check_m6.ps1`, the
  twins of the Linux scripts, rather than by hand, so "the app runs and chats"
  is a measured result rather than an impression — cost if wrong: none; the
  tray and `opencord://` links still need a person, and are listed as not done.
- W0: Ruling: `app/tool/windows/README.md` is written now, though
  platforms.md §2.8 has L1 create that folder's README. The folder was needed
  for W0 step 5 — cost if wrong: a small conflict when L1 merges, in a file
  platforms.md §3 gives to Windows, so this side wins.

Decided while doing W1:
- W1: Ruling: step 2's denied-microphone case was measured by setting
  `HKCU\…\CapabilityAccessManager\ConsentStore\microphone` and its
  `NonPackaged` child to `Deny`, running the live test, and putting both back
  in a `finally`, rather than waiting for someone to use the Settings UI. Both
  values are per-user and are the same store the UI writes — cost if wrong: the
  UI may also notify listeners that a registry write does not, so an app
  already holding a stream open might not react the same way; what was measured
  is a *fresh* stream, which is the case the plan asks about.
- W1: Ruling: the Windows device test lives in
  `crates/opencord-media/tests/devices_windows.rs`, which platforms.md §3 gives
  to Windows, and asserts only what holds when the machine has devices, so a
  runner with none does not fail it. Its live microphone test is `#[ignore]`d,
  as W2 step 5 does for injected keys — cost if wrong: nothing; the enumeration
  half still runs everywhere.
- W1: Ruling: **one VB-CABLE pair stands in for both of the plan's cables, so
  Hi-Fi CABLE is not needed.** Only the base VB-CABLE was available (Hi-Fi
  Cable is a separate download), giving one pair: `CABLE Input` → `CABLE
  Output`. `CABLE In 16ch` is a second render endpoint of the *same* cable, not
  an independent pair. It is enough because the two steps need the cable at
  different ends: step 4 plays into CABLE Input and records CABLE Output, while
  step 5 does the reverse and needs no cable on the app's speaker. What step 4
  also needs is Linux's *silent* sink, and `Microphone (WO Mic Device)` is
  digitally silent (−inf dBFS over 1.5 s) whenever its phone client is not
  connected — measured, along with CABLE Output at −inf, the Razer headset mic
  at −70.3 and the Intel array at −23.7 — cost if wrong: a second cable pair
  would have to be installed, which is an administrator install and a reboot.
- W1: Ruling: the recording path was proved before anything was built on it:
  with nothing playing, `ffmpeg -f dshow -i audio="CABLE Output …"` through
  `measure.py` reads −155.2 dBFS at 440 Hz; with a 440 Hz sine playing into
  CABLE Input it reads −18.2 dBFS at 440 and −85.9 at 880, steady across 29
  blocks (median −18.1, loudest −18.1); and −154.6 dBFS again afterwards — cost
  if wrong: nothing, and it is why the step 4 twin worked first time.
- W1: Ruling: the Windows `_record` gives ffmpeg `-t` and waits for it, rather
  than killing it as the Linux original kills `parec`. A killed ffmpeg can lose
  whatever it had not flushed down the pipe, and a 2-second recording is mostly
  unflushed. The first 600 ms of every recording is dropped, because dshow
  takes a moment to start delivering and a silent lead-in would drag a median
  down — cost if wrong: short recordings would read quieter than the truth.
- W1: Ruling: device names are turned into the app's `wasapi:` ids from the
  `MMDevices` registry (`app/tool/windows/audio_devices.ps1`), matching the
  friendly name, the device name, or cpal's "<friendly> (<device>)" spelling of
  the two, and only where `DeviceState` is 1. The ids it builds were checked
  against what the app itself reports: identical for CABLE Input, CABLE Output
  and WO Mic Device — cost if wrong: a check would configure the app with an id
  no device has, which the app reports as a device that is not connected.
- W1: Ruling: step 5's acceptance is the **comparison between phases**, not an
  absolute level: a pure sine is not speech, and how much the processing chain
  colours one was the unknown. A held push-to-talk must be at least 20 dB above
  a released one, and so must voice activity. It then turned out that `off` and
  `standard` agree to 0.1 dB, so the worry was unfounded — cost if wrong: an
  absolute threshold would have made the check fail on a processing change that
  is not a regression.
- W1: Ruling: push-to-talk is driven in the test through
  `repository.setPushToTalk(…)`, which is what the hotkey path itself calls
  (`providers.dart` `_act`), so step 5 does not wait for W2's global keybinds —
  cost if wrong: it exercises everything below the hotkey, but not the hotkey;
  W2 covers that end with injected keys.
- W1: Ruling: the echo test carries a **control run with echo cancellation
  off**, and `check_echo.ps1` exits 2 unless that control is above −47 dBFS.
  The acceptance is "under −50 dBFS", so the run only proves anything when the
  un-cancelled echo would itself have failed; otherwise a silent speaker scores
  the same as a working canceller. This is not in the plan, and adding it is
  what stopped step 6 being reported as a pass it had not earned — cost if
  wrong: a machine with a genuinely quiet acoustic path reports inconclusive
  instead of passing, which is the intended outcome.
- W1: Ruling: the echo test drops the voice gate to −90 dBFS
  (`automaticSensitivity: false`, `sensitivityDbfs`), because with the default
  −45 dBFS gate, or with automatic sensitivity, the app sends nothing at all
  and neither the echo nor its absence can be measured. The measurement is
  frequency-selective, so the room noise this lets through does not affect the
  440 Hz figure — cost if wrong: it is not the gate a user would have, so the
  run says nothing about whether voice activity *would* have opened on an echo;
  what it measures is how much echo reaches the stream once it is open.
- W1: Ruling: how loud the app plays is taken from step 4's −9.2 dBFS rather
  than measured again inside `check_echo.ps1`. Measuring it needs a third app
  session with the output pointed at the cable, for a number the per-user audio
  check already establishes under the same Talker and the same chain; a first
  attempt also lost it, because the recording ffmpeg was killed with the
  check's other processes before it flushed — cost if wrong: the figure is from
  a different run, though the same configuration.
- W1: Ruling: step 7 was measured on the app's threads, not from a log. The
  core installs no tracing subscriber, so the warning the step tells us to look
  for ("the audio thread runs at normal priority") has nowhere to go and its
  absence would prove nothing — cost if wrong: base priority is a proxy for
  MMCSS rather than the call's own return value; one thread at 21 against 98 at
  8, in a Normal-class process, is not something else in the app asking for it.
- W1: Ruling: `cargo clippy` cannot be run with `-D warnings` over the
  workspace before L1: the 8 shared dead-code warnings in `hotkeys/mod.rs` and
  `video/camera/mod.rs` become errors. Own files are checked by confirming
  clippy names none of them — cost if wrong: a warning in an own file could
  hide among the shared ones, so each one is grepped for by name.

Decided while doing W2:
- W2: Ruling: `RegisterHotKey` is not used at all, not even for the toggles the
  plan suggests it for; the one `WH_KEYBOARD_LL` hook handles every action —
  cost if wrong: none measured, since a hook does everything `RegisterHotKey`
  does; what it buys is that Opencord never takes a key away from the game or
  app the user is actually in.
- W2: Ruling: a keysym that names a key the keyboard has twice is matched on
  the extended-key flag as well as the virtual-key code. `Left` matches only
  the arrow cluster, `Return` only the main keyboard's, `KP_Enter` only the
  keypad's — so a keypad key pressed with Num Lock off matches **nothing**
  rather than the arrow someone bound. X11 gives those keys keysyms of their
  own (`KP_Left`), which no accelerator can name, so matching nothing is what
  mirrors Linux — cost if wrong: someone who bound `Left` and presses keypad 4
  with Num Lock off gets no push-to-talk, exactly as on Linux.
- W2: Ruling: AltGr's phantom left Ctrl is recognised by its extended-key flag
  — a real left Ctrl never carries it — and dropped before the matcher sees
  it, rather than by timing it against the right Alt — cost if wrong: on a
  driver that doesn't set the flag, AltGr+key could trigger a `CTRL+ALT+…`
  binding. The timing approach was rejected because it would have the hook
  procedure look at the message queue, which it must not do.
- W2: Ruling: the printable keys' table is rebuilt from the **foreground
  window's** keyboard layout, on a 500 ms `WM_TIMER` in the hook thread's own
  message loop and never in the hook procedure — cost if wrong: for up to half
  a second after the user switches layout, a printable keybind still watches
  the old layout's key. Doing it in the procedure would risk
  `LowLevelHooksTimeout`, which drops the hook silently.
- W2: Ruling: a key the layout cannot type at all (`watch` returning `None`)
  matches nothing, instead of falling back to some other key — cost if wrong:
  a keybind recorded on one layout quietly stops working on another, which is
  better than quietly firing on the wrong key.
- W2: Ruling: `hotkeys/windows.rs` needed no change to `hotkeys/mod.rs`. A
  child module reaches its ancestors' private items, and L1 made `Held`,
  `press` and `release` `pub(crate)` anyway — cost if wrong: nothing; it is
  what makes W2 a zero-`shared:` milestone.
- W2: Ruling: the Rust live tests inject keys with `SendInput`, as decision F
  says, and the app-level checks use `keybd_event` from
  `app/tool/windows/hold_key.ps1`, because a check cannot inject from inside
  the app under test. Both put the key into the same stream a real keyboard
  uses — cost if wrong: `keybd_event` is deprecated but not removed, and if it
  ever goes, the script is four lines of P/Invoke to change.
- W2: Ruling: the live tests press F13, F14, F15 and Right Ctrl, and nothing
  else. They have virtual-key codes, no app binds them, and none of them types
  a character anywhere — cost if wrong: a test run would type into whatever
  window happened to have the focus.
- W2: Ruling: `check_key_recorder.ps1` exits **2** for a key the recorder
  cannot record at all, and 1 only for a key recorded as the *wrong* key, the
  same convention as `check_echo.ps1` — cost if wrong: a milestone could
  otherwise be blocked by a shared-Dart limit this session may not fix
  (platforms.md §4).
- W2: Ruling: `check_keybinds.ps1` builds the app with
  `flutter build windows --debug` before any phase starts. A voicebot's run is
  a fixed number of seconds from when it starts, so a cold cargokit build
  inside a phase had the Listener gone before the app ever joined — the first
  attempt failed in exactly that way, with the Listener listed offline in the
  app's own sidebar — cost if wrong: none; it only moves work earlier.
- W2: Ruling: AltGr was not measured through the app's key recorder. This PC's
  layout is US, which has no AltGr key at all, so the right Alt is only the
  right Alt here; adding a German layout would change the user's system
  settings for one measurement. The hook's phantom-Ctrl handling is unit-tested
  instead (`altgr_does_not_press_a_ctrl_alt_hotkey`) — cost if wrong: the
  recorder's own AltGr behaviour is untested on a layout that has one; it is
  shared Dart, so the Mac and Linux sessions are better placed for it anyway.

Decided while doing W3:
- W3: Ruling: `Backend::MediaFoundation` exists but is **not** in
  `Backend::ALL`. Measured: it holds 16 pictures before the first one comes
  out and only 14 of 30 encode calls produce a packet, about 533 ms at 30 fps;
  W3 step 2 says an encoder that holds pictures back is dropped. FFmpeg's own
  capability line for it says `delay hybrid`, so this is by design, not a
  setting — cost if wrong: Windows has **no software encoder** until Cisco's
  OpenH264 is fetched (see the note below), so a PC with no usable GPU encoder
  cannot send video at all. That is the lesser evil: half a second of latency
  would make a call unusable rather than merely limited.
- W3: Ruling: `profile=high` is set per encoder instead of once for all of
  them. `h264_mf` has no `profile` option, so FFmpeg fell back to the generic
  enum, could not parse "high", and refused to open the encoder — the
  "Invalid argument" this milestone first saw. NVENC, AMF and Quick Sync all
  keep it — cost if wrong: Media Foundation encodes at whatever profile it
  picks, which for a conferencing scenario is baseline or main, not high.
- W3: Ruling: `err_recognition = AV_EF_EXPLODE | AV_EF_BITSTREAM` is set on
  the decoder **on Windows only**. Measured: with every NAL unit of one
  picture truncated, FFmpeg here returned `Ok` with a concealed picture, so
  `receiver.rs` never called its `lost` hook and never asked the sender for a
  keyframe; with the flags it returns "Invalid data found when processing
  input" and the shared `damage_asks_for_a_keyframe` passes. Linux is left
  alone because it already reports this damage and W3 must not change how
  Linux behaves — cost if wrong: Windows may now treat a stream Linux would
  have ridden out as damaged and ask for a keyframe it did not need, which
  costs one keyframe. Worth the Linux session's opinion on setting it
  everywhere.
- W3: Ruling: the hardware device is chosen by `cfg` in `decoder.rs`
  (`vaapi` or `d3d11va` behind one name) rather than by a runtime list, and
  `d3d11va.rs` asks `av_hwdevice_ctx_create` for the **default adapter**
  instead of walking adapters as `vaapi.rs` walks `/dev/dri` — cost if wrong:
  on a hybrid laptop the decoder uses whichever GPU the display is on rather
  than the fastest one. That is the right default: the decoded picture goes
  to the screen.
- W3: Ruling: only the five FFmpeg DLLs `opencord_core.dll` needs
  transitively are bundled — avcodec, avformat, avutil, swresample, swscale —
  matched by prefix because the names carry a soname (`avcodec-63.dll`).
  avfilter, avdevice and postproc are left out; nothing calls them — cost if
  wrong: a future dependency on one of them fails at load time rather than at
  build time, which the `FATAL_ERROR` on a missing DLL at least makes loud for
  the five that are listed.
- W3: Ruling: `app/rust_builder/windows/CMakeLists.txt` **fails the build**
  when `FFMPEG_DIR` is unset or missing a DLL, instead of quietly bundling
  nothing — cost if wrong: a developer without FFmpeg cannot build at all,
  which is better than shipping a zip that dies on someone else's PC.
- W3: Ruling: CI pins an exact BtbN asset by URL **and** SHA-256, downloaded
  and hashed here rather than taken from a web page, because BtbN's `latest`
  tag moves and an LGPL **shared** build is required — cost if wrong: the
  asset is eventually deleted and the Windows job fails loudly, which is the
  intended failure. The hash check also means a replaced asset stops the build
  instead of silently changing what ships.
- W3: Ruling: step 5's "rename `C:\ffmpeg` for a moment" is a script,
  `check_bundled_ffmpeg.ps1`, not four manual steps, and it checks *both*
  halves: that the exe stays up (every DLL resolved) and that the bundled DLLs
  really encode, by running V4 with the release folder on PATH. The folder is
  restored in a `finally` — cost if wrong: a crash between the two renames
  leaves `C:\ffmpeg.hidden`, which the script refuses to start on top of and
  names in its error.
- W3: Ruling: on Windows the §0 test check is run with `--test-threads=1`.
  Measured, and reproducible: in parallel, `cargo test -p opencord-media
  --lib` fails 3 of 243 and the log shows `[h264_nvenc] No capable devices
  found` — a GeForce card caps simultaneous NVENC sessions, and when they run
  out `Encoder::open` silently falls through to Quick Sync, which is 3.5 ms
  against NVENC's 2.0 ms at 720p, so the sender's timing assertions miss.
  Serially: **243 passed, 0 failed**. A real user opens three layers, well
  inside the cap; only a test suite opens dozens at once — cost if wrong: the
  suite takes 61 s instead of 12 s for that crate, and a genuine race between
  tests would no longer show up here. Flagged for the Linux session below.
- W3: Ruling: encode cost is measured by a Windows-owned ignored test,
  `crates/opencord-media/tests/codec_costs_windows.rs`, rather than by
  extending `examples/codec_costs`, which is Linux's. It reports wall time per
  `encode` call where the Linux example also reports CPU time from `/proc`:
  there is no equivalent here without adding a dependency for one
  measurement, and wall time is what the real-time sender actually lives with
  — cost if wrong: the two systems' numbers are not directly comparable on
  CPU, only on latency.

Decided while doing W4:
- W4: Ruling: "the colours are right" is measured by copying the app's own
  window off the screen (`sample_video.ps1`), not from inside Flutter. A
  `Texture` is composited by Windows and never enters Flutter's layer tree,
  so a widget-level pixel read sees an empty box where the video is. The
  rectangle comes from `tester.getRect` times the device pixel ratio, and the
  measurement is the redness of the left third against the right, which the
  camera's own chroma bands make predictable — cost if wrong: the check needs
  the window visible and unobstructed, so it cannot run on a locked PC or a
  headless CI runner. It is a local check, like the echo test.
- W4: Ruling: the sampler **verifies** the app's window really came to the
  front before it copies anything, and fails outright if it did not. Windows
  refuses `SetForegroundWindow` from a process that is not already in front,
  so the first version silently captured whatever *was* in front — a terminal
  — and reported its colours as the video's. It now attaches to the
  foreground thread's input queue to make the call stick, then checks
  `GetForegroundWindow` — cost if wrong: on a machine where the window cannot
  be raised the check fails instead of running, which is the right way round:
  a colour check that can measure the wrong window is worse than none.
- W4: Ruling: the sampler finds the window by **process id**, which the test
  passes as its own, not by title. Under `flutter test -d windows` the window
  is not called "Opencord", so `FindWindow` by name found nothing; the test
  runs inside the app's own process, so its `pid` is exact — cost if wrong:
  nothing; the title lookup is kept as a fallback for a normally-launched app.
- W4: Ruling: the two phases get a server and a profile each, as W1's checks
  do. Sharing one profile left the identity from the first phase in place, so
  the second never reached the screen it was waiting for — cost if wrong: the
  check takes about twice as long.
- W4: Ruling: the test waits for `presented` **and** `drawn` together, rather
  than waiting on one and asserting the other. They are independent counters
  and `drawn` runs ahead here, because Flutter repaints a texture more often
  than the core hands it a new picture; the shared `v5_test.dart` waits for
  `drawn >= 30` and then asserts `presented >= 30`, which failed on the first
  run with 30 against 29 — cost if wrong: none here; noted below for the
  shared test.
- W4: Ruling: `check_receive_video.ps1` exits **2** when only the screen half
  did not arrive, keeping 1 for the camera half, the same convention as
  `check_echo.ps1` and `check_key_recorder.ps1`. The screen does arrive here,
  just not dependably, and W6 is where screen sharing is actually built —
  cost if wrong: a real screen-share regression would read as the known
  flakiness. W6 has to tighten this back to 1.
- W4: Ruling: the claim that nothing of someone else's screen arrives before
  Watch was turned from an assertion into a measurement. A feed object does
  exist beforehand, so asserting `screens` is empty failed; what matters is
  that nothing has been *received*, and `presented=0` says so — cost if
  wrong: a regression where video flows before Watch would be reported rather
  than failing the check.

Decided while doing W5:
- W5: Ruling: the camera id is the device's **symbolic link**, not its
  friendly name or an index. Windows keeps it the same while the camera stays
  in the same port, which is what the contract asks for; a name is not unique
  and an index moves when something else is plugged in — cost if wrong: a
  chosen camera is forgotten when it moves to another USB port, which is also
  true of Linux's `/dev/v4l/by-id` path.
- W5: Ruling: the source reader is created with
  `MF_READWRITE_DISABLE_CONVERTERS`. Otherwise it quietly inserts a decoder or
  a colour converter of its own, and the engine already does that, into the
  picture it actually wants — cost if wrong: on a camera that offers only a
  format the engine cannot use, the open fails instead of Media Foundation
  converting it. `mode::choose` accepts MJPEG, YUY2, NV12 and I420, which is
  every format a webcam realistically offers.
- W5: Ruling: `IMF2DBuffer::Lock2D` is used where the buffer offers it, and a
  plain `Lock` only as a fallback. A camera's rows are usually padded and only
  `Lock2D` gives the stride; `pack` needs it to unpad them — cost if wrong: a
  padded frame read as packed would come out sheared, which is exactly the
  bug this avoids.
- W5: Ruling: the "another app has the camera" case is recorded as **not
  reproducible on this Windows** rather than guessed at. Two readers of the
  same camera in one process both succeeded, and holding it with ffmpeg's
  DirectShow first changed nothing: the Frame Server shares cameras between
  applications from Windows 10 1607 on. The code still maps `E_ACCESSDENIED`
  to `Denied` with the settings path in the message, which is the case that
  does happen — cost if wrong: a camera whose driver really does take
  exclusive use reports `Failed` with its HRESULT instead of something
  friendlier, which is what the plan asks for anyway.
- W5: Ruling: "three layers sent" is measured with **three voicebots watching
  at three tile sizes**, not one. Only the layers somebody wants are encoded
  (§7.11), so a single watcher can never see more than one layer, and the
  first version of this check failed for that reason rather than for anything
  about the camera — cost if wrong: three more bots per run, about ten
  seconds.
- W5: Ruling: `check_v5.ps1` exits **2** when everything but the top layer
  passed. The camera, the preview, the timings and the mirroring are all met;
  what is missing is a shared-code behaviour this PC never reaches, already
  represented by a failing shared test — cost if wrong: a real regression in
  layer selection would read as the known gap. W8 should check this again if
  the top layer ever starts arriving.

Decided while doing W6:
- W6: Ruling: the picker's thumbnails are drawn with GDI — `StretchBlt` from
  the desktop for a screen, `PrintWindow(PW_RENDERFULLCONTENT)` for a window
  — and not with a capture session each. A session per source would put the
  yellow capture border around every window on the desktop for as long as the
  picker is open — cost if wrong: a window that refuses to draw itself shows
  a plain block instead of a picture, which the picker handles and which a
  capture session would have got right.
- W6: Ruling: the source id is the monitor's **device name**
  (`screen:\\.\DISPLAY1`) and the window's **handle** (`window:329088`), and
  a screen is found again by device name when the share starts. A list a few
  seconds old then still works after the monitors have been rearranged — cost
  if wrong: a window handle can be reused by Windows after the window closes,
  so a stale id could in principle name a different window; `still_a_window`
  is checked first, and the picker's list is never more than two seconds old.
- W6: Ruling: capture **polls** the free-threaded frame pool on its own
  thread instead of handling `FrameArrived`. The rate is then ours to set,
  which is what `request.fps` asks for, and on Windows 11
  `SetMinUpdateInterval` does it for us — cost if wrong: a poll that misses
  wastes up to 8 ms; an event-driven version would need a message pump on
  that thread, which is the very thing that broke `Closed` below.
- W6: Ruling: the end of a share is noticed by **looking at the handle**
  every 250 ms, not by the `Closed` event alone. Measured: the event never
  arrives, because the capture thread pumps no messages, and a closed Notepad
  left the share running for the whole ten seconds the test allowed; with the
  poll it ends in 0.3 s. The event is kept as well, in case a future caller
  does pump — cost if wrong: up to a quarter-second of sharing a window that
  has gone, which is what the viewer would see anyway.
- W6: Ruling: the CPU path stays, on the measurement rather than on a guess:
  1080p30 costs 14.8 % of a core against the plan's 15 % ceiling, in a
  **debug** build, so release is lower. W6 step 7's GPU conversion is not
  built — cost if wrong: a 1440p or 4K monitor would cost more than measured
  here, and this PC has no such screen to check with. W8 should measure it
  again if a bigger monitor is ever plugged in.
- W6: Ruling: "Go live" is disabled until a source is chosen, and "Change
  source" opens a dialog containing only the picker. The quality and the
  sound switch are not asked about again, because neither is changing — cost
  if wrong: someone wanting a new source *and* a new quality does two
  dialogs, which is what "Change quality" is beside it for.
- W6: Ruling: the picker's widget tests live in
  `app/test/widget/voice/screen_source_picker_test.dart`, a **new** file in a
  shared directory. W6 step 8 asks for widget tests, they test only
  Windows-owned widgets, and no existing file was touched — cost if wrong:
  the Linux session may prefer it elsewhere; moving a new file is cheap.
- W6: Ruling: `check_v6.ps1` exits **2** when the app shared correctly but no
  pictures reached the watcher, keeping 1 for a break in the app's own side —
  the same convention as W4 and W5 — cost if wrong: a regression in the
  sending path would read as the known media gap. W8 has to re-check this.

Decided while doing W7:
- W7: Ruling: the build is read with `RtlGetVersion`, not `GetVersionExW` or
  the registry's `CurrentBuildNumber`. `GetVersionExW` reports 6.2 for
  anything past 8.1 unless the exe carries a compatibility manifest, which
  Flutter's runner does not — cost if wrong: on a Windows 11 PC the app would
  disable the sound switch it should be offering. `check_v7.ps1` reads the
  registry instead, because it is a script and has no process to lie about.
- W7: Ruling: the capture asks for 48 kHz stereo float and falls back to
  16-bit PCM, by **trying `Initialize` twice** rather than asking
  `IsFormatSupported` first. Microsoft's own sample does the same: that call
  is not reliable on this virtual device — cost if wrong: one extra failed
  activation on a device that wants PCM, which costs a few milliseconds once.
- W7: Ruling: `apps_with_audio()` leaves out **`opencord` and `audiodg` by
  name**, on top of leaving out this process's own id. Measured: with only
  the id checked, a second Opencord left over from an earlier check appeared
  in the list, and so did Windows' audio engine. §10.1's "never Opencord"
  means any Opencord — cost if wrong: an app genuinely called "opencord" by
  someone else could not be shared, which is a trade worth making.
- W7: Ruling: the mutation check W7 step 4 asks for is a **hook**, not a
  manual code edit: `OPENCORD_TEST_EXCLUDE_PID`, read only under
  `debug_assertions` so it cannot exist in a shipped build. The check can
  then prove it is able to fail, every run, without anyone editing Rust —
  cost if wrong: a debug build carries one environment-variable read that
  nothing sets. A unit test covers the hook both on and off.
- W7: Ruling: the four capture tests **skip** rather than fail on a Windows
  without process loopback, printing the same sentence the app shows. A
  failing test here would say "Windows is broken" when the truth is "this
  Windows is too old" — cost if wrong: a Windows 11 machine that silently
  lost the capability would skip instead of failing; `support()`'s own unit
  test is what catches that, since it asserts against the build number.

- W8: Ruling: the hardware items are one check script with **no prompts**.
  Its output goes to the session driving it, not to the screen in front of
  whoever is at the PC, so every step waits for the hardware event itself —
  a device leaving the list, a device returning, `SetSuspendState` returning
  — for up to four minutes each, and the instructions are given by the
  session instead — cost if wrong: a run that proves nothing because the
  person was slow, which is exactly what the first headset run did (exit 2
  after four minutes).
- W8: Ruling: the device the app falls back to is **CABLE Input, made the
  system default for the length of the run** and put back afterwards. The
  fallback is then silent and, through CABLE Output, measurable — cost if
  wrong: the call plays out of the laptop's speakers at night and "it fell
  back" is an impression rather than a number.
- W8: Ruling: a sleep is witnessed by `SetSuspendState` returning **in a
  process of its own**, which happens only once the system is back, and not
  by a clock jump inside the app. Under Modern Standby a desktop app is
  throttled rather than stopped, so its own clock may show no gap at all —
  cost if wrong: a real sleep is reported as "the PC never slept".
- W8: Ruling: the sleep item is run **twice**, plain and `-Hibernate`, and
  both are logged. On a PC with no S3, a plain sleep tears nothing down, so
  the item would pass because nothing happened; hibernation cuts the power,
  which is the case the checklist is after — cost if wrong: an item that
  passes for the wrong reason.
- W8: Ruling: `check_hardware.ps1` **exits 2 when the hardware never moved**,
  joining the other checks' convention (0 passed, 1 failed, 2 proved
  nothing) — cost if wrong: a session that reads "the person did not get to
  it" as "the app is broken".
- W8: Ruling: the Windows `cargo test` CI job is **proposed in this log, not
  added**. A new job is shared ground, and Windows' own steps are the only
  CI this session owns (platforms.md §4) — cost if wrong: two sessions
  editing the same workflow in the same week.
- W8: Ruling: `host_lan_server.ps1` adds its firewall rules for **every
  profile**, not just Private. This laptop's Wi-Fi is categorised Public, and
  a rule on the wrong profile fails in precisely the way the rule exists to
  prevent: the other machines join the call and hear nothing — cost if
  wrong: a rule wider than the test needs, on a laptop, for two ports.

For the Linux session, and for the user:
- W8: **The three-machine call is the one acceptance left on Windows.** It
  needs the Mac and the Linux machine at the same time, and neither was
  available; nothing else about it is missing.
  **Neither plan says which machine hosts the server** — W8 step 1 and
  platforms.md §5 both say only "one server on the LAN". Any of the three
  can, and the **Linux machine is the obvious one**: it owns the server, it
  is the machine that is always on, and in that case Windows needs nothing
  new at all, since outbound connections need no firewall rule and the
  acceptance is about the *app* as a client, not about the server's Windows
  build. `app/tool/windows/host_lan_server.ps1` is only for the other case,
  where this PC is the one that has to host: it finds the
  interface with the default route (192.168.0.122 here), runs a **release**
  server on 0.0.0.0:7710 with UDP 7711 and `public_host` set to that address
  so the invite link is not `localhost`, keeps its database across runs in
  `%LOCALAPPDATA%\Opencord-LAN` so the three machines stay joined, and adds
  the two firewall rules — or, unelevated, prints the exact commands. Expect
  the **screen-share media gap** (the W6 note) to be the first thing it runs
  into: the app shares correctly here and nothing arrives at the other end.
- W8: **CI has no Windows `cargo test` and no Windows clippy.** The
  `desktop` job builds the app, so Windows-only code that fails to *compile*
  is caught — but nothing runs a test or a lint there, which is why W0 found
  eight off-Linux dead-code warnings that Linux CI could not see. Proposed,
  for the Linux session to land, since a new job is shared ground:

  ```yaml
    rust-windows:
      name: Rust (Windows)
      runs-on: windows-latest
      steps:
        - uses: actions/checkout@v7
        - uses: dtolnay/rust-toolchain@stable
          with:
            components: clippy
        - uses: Swatinem/rust-cache@v2
        # The same pinned asset as the desktop job's "Use FFmpeg" step.
        - name: Use FFmpeg
          shell: pwsh
          run: |   # copied from the desktop job, or factored into one
                   # composite action used by both
        - name: Clippy
          run: cargo clippy --workspace --all-targets -- -D warnings
        - name: Test
          run: cargo test --workspace
  ```

  What it buys: every `#[cfg(windows)]` file in this plan gets linted and its
  unit tests run. What it will *not* cover: a GitHub runner has no GPU, so
  `h264_nvenc`, `h264_amf` and `h264_qsv` never open and the encoder tests
  fall through to their run-time skips (L1) — only Media Foundation's
  software path can actually encode there. The hardware checks
  (`app/tool/windows/*.ps1`) need devices and a person and are not CI work.
  `--test-threads=1` is **not** needed on a runner: the cap that forced it
  here (W3's ruling) is NVENC's session limit, which a GPU-less machine
  cannot hit.
- W8: **Packaging is in order; two decisions are the user's.** The zip is the
  `Release` folder plus `msvcp140.dll`, `vcruntime140.dll` and
  `vcruntime140_1.dll` from System32 (all three present on this PC) and
  FFmpeg's `LICENSE.txt` as `FFMPEG-LICENSE.txt`, which LGPL §4 asks for;
  the five FFmpeg DLLs are already beside the exe because `rust_builder`'s
  CMakeLists puts them there, which `check_bundled_ffmpeg.ps1` proved on a
  real release build in W3. The staging step itself only runs in CI and has
  not been executed here. Still open, both the user's call: **code signing**
  — unsigned, SmartScreen warns on every download, and the fix is a paid
  certificate (an OV/EV code-signing certificate, or Azure Trusted Signing)
  plus a signing step in the `desktop` job; and **an installer** — MSIX or
  WiX/Inno Setup, which would also give updates. Neither blocks the zip.
  Worth doing first, and cheaper than both: the **minimal FFmpeg build**
  already recommended in the W3 note — `avcodec-63.dll` alone is 87 MB of
  the 149 MB in `C:fmpegin`.
- W8: **Mixed scaling and HDR were skipped at the user's request,** so three
  things in §15 are unverified on Windows: two monitors at 100 % and 150 %
  together, a capture surviving a monitor being unplugged (§8's "display
  changes: restart capture on the same display, or stop if it's gone"), and
  HDR colour. The manifest already declares `PerMonitorV2`, which is Flutter's
  default and not evidence. The W6 check takes a monitor index, so the first
  two are a short job whenever two monitors are to hand.
- W7: **Nothing in W7 has ever captured a sample.** It is written, it
  compiles, its unit tests pass and its live tests skip politely, but this PC
  is build 19045 and process loopback needs 20348. The app behaves correctly
  here — the sound switch is disabled and reads "Sharing sound needs Windows
  11", which is decision C — and that part *is* measured. Everything else in
  W7 is unverified code. The first Windows 11 machine to run
  `cargo test -p opencord-media --test share_audio_windows -- --ignored` and
  `app/tool/windows/check_v7.ps1` is what settles it, and both are written to
  run unchanged there. Whether to get such a machine is the user's call;
  nothing else in this plan is blocked by it.
- W6: **No screen-share media moves on this PC, in either direction, while
  camera media does.** W4 saw it one way: the app watching a voicebot's
  720p30 screen share drew anything in only 2 runs of 4, and never more than
  a trickle. W6 sees it the other way: the app shares a screen or a window
  correctly — the picker, the source kind, `audioFrom`, the ending are all
  right — and a voicebot watching it receives **nothing at all**, in all
  three phases, at a 360-pixel tile as well as a 720 one. Cameras are fine in
  both directions: V4 bot-to-bot gives 154 pictures, W5 app-to-bot gives 363.
  So this is narrower than "the top layer is never reached" (W5's note): it is
  screen shares specifically. Both notes are probably one bug. It is shared
  code and is not touched here, and it is what W8's two-machine test will run
  into first.
- W5: **This PC never reaches the top layer, and it is one fault, not four.**
  Four independent measurements now say the same thing: (1) W3's voicebot
  encoded-camera test is stuck on `layers [30, 0, 0]`; (2) V4 gives
  `by layer 140/14/0`, the top layer always zero; (3) W4's screen share
  trickles, 2 runs in 4 and only ~10 pictures; (4) W5's camera serves the
  **middle** layer to a voicebot explicitly asking for a 720-pixel tile —
  `0/371/0` with one watcher, `210/693/0` with three. And the shared test
  `a_capped_uplink_stops_encoding_the_cameras_top_layer_until_lifted` has
  failed here 5 times out of 5 since W1 with "never reached the top layer".
  It is not the encoder: NVENC encodes 720p in 1.99 ms here (W3). It is not
  W3's `err_recognition` change: turning that off changed nothing. It looks
  like the uplink estimate never climbing far enough on this machine. This is
  shared code, so it is not touched here — but it is the one thing most worth
  the Linux session's time, because W6 and W8 both build on layers arriving.
- W4: **The screen-share receive path is unreliable on this PC, and it looks
  like the same slow layer climb seen twice already.** With a voicebot
  sharing `720p30` and the app watching, 2 of 4 runs drew anything at all,
  and both of those reached exactly 10 pictures in 90 s — a trickle, not
  30 fps. It is **not** caused by W3's `err_recognition` change: with the
  flags turned off it still timed out. It sits with the other two
  observations of the same shape — the voicebot's encoded camera stuck on
  `layers [30, 0, 0]`, and V4's `by layer 140/14/0` where layer 1 only
  appears after several seconds — so the common cause is likely how fast the
  bandwidth estimate climbs on this machine, not anything about screens.
  Worth the Linux session's eyes before W6 builds on it.
- W4: **`v5_test.dart` waits for `drawn` and then asserts `presented`.**
  They are separate counters, and on Windows `drawn` runs ahead: the first
  run of the Windows twin saw 30 drawn against 29 presented. The shared test
  would be flaky here for the same reason. Waiting for both is a one-line
  change in shared Dart, so it is noted rather than made.
- W3: **Windows has no software H.264 encoder.** The LGPL FFmpeg build carries
  no `libx264` and no `h264` software encoder (checked: only `h264_nvenc`,
  `h264_amf`, `h264_qsv`, `h264_mf`), Cisco's OpenH264 is not fetched on any
  OS, and Media Foundation — which does have a software path — is out of
  `ALL` because it holds pictures back. So a Windows PC with no usable GPU
  encoder cannot send video. Nothing in this plan fixes that; downloading
  Cisco's library on first use is out of scope on every OS (W3 step 4). It
  deserves a decision before release: either fetch Cisco's DLL, or accept
  Media Foundation's latency as a last resort, or say the feature needs a GPU
  from this decade.
- W3: **The FFmpeg DLLs are 120.0 MB** — avcodec alone is 91.1 MB — and the
  release folder is 195.8 MB. The plan guessed 25–40 MB. BtbN's build carries
  every LGPL codec; the engine uses H.264 and MJPEG. A minimal build like
  Linux's (D32) is listed as a W8 follow-up and on these numbers it is worth
  doing: it should take the download back under 100 MB.
- W3: **`cargo test` needs `--test-threads=1` on Windows**, and the reason is
  the GPU, not a flaky test: NVENC's session cap is reached when many encoder
  tests run at once, and the fallback to Quick Sync is slow enough to miss the
  sender's timing assertions (the ruling above has the numbers). If the shared
  suite is ever meant to run in parallel on Windows CI, the encoder tests need
  a lock of their own — that is a shared-code change, so it is noted here
  rather than made.
- W3: **`err_recognition` is set on the decoder on Windows only.** Linux
  already reports the damage `damage_asks_for_a_keyframe` relies on; Windows
  concealed it. Setting the flags everywhere would make the two behave alike
  and is probably right, but it changes Linux behaviour, which W3 must not —
  the Linux session's call.
- W2: **Print Screen cannot be recorded by the app's key recorder on Windows**,
  though it works perfectly well as a keybind once set. Measured: pressing it
  into the real app window leaves the recorder at "Press keys…" and saves `""`,
  while the hook below it sees both the key-down and the key-up
  (`print_screen_reaches_the_hook`). Windows does not send a focused window the
  key-down for Print Screen, and `keybind_recorder.dart` commits a non-modifier
  key on `KeyDownEvent`. The fix is in shared Dart, so it is not made here:
  committing a non-modifier key on its `KeyUpEvent` too, when no down was seen,
  would cover it. Right Alt (`Alt_R`) and the left Windows key (`Super_L`) both
  record correctly.
- W2: Recording the Windows key alone also opens the Start menu, which takes
  the focus. That is correct behaviour — nothing may swallow a key — but it
  does make the Windows key a poor keybind, and the Keybinds page says nothing
  about it. Worth a word in the UI, in shared Dart.
- W0: **W7 cannot be measured on this PC.** Decision C puts screen share sound
  at build 20348 and this is 19045. W2–W6 are unaffected. Unless the PC moves
  to Windows 11, W7 can only be written and unit-tested here, shipped behind
  the "Sharing sound needs Windows 11" switch the plan already specifies, with
  the live capture never measured. The user has been told; the call is theirs
  and is not needed before W7.
- W0: **LLVM is not installed**, because there is no user-scope installer and
  this session cannot elevate. W3's `ffmpeg-sys-next` bindgen needs
  `LIBCLANG_PATH` — cost if wrong: W3 cannot start.
- W0: `app/test/golden/failures/` is written by any golden run off Linux and no
  `.gitignore` covers it. One line in a shared `.gitignore` would save every
  platform session from almost committing it.
- W1: **Step 6 needs external speakers to prove anything.** The acceptance is
  met as written, but this machine's built-in speakers reach its own microphone
  about 38 dB down, so the echo is removed by noise suppression before the
  canceller sees it and the run cannot tell a working canceller from a dead
  speaker (the W1 row has the five runs). Speakers that can be driven louder,
  placed near the microphone, would settle it; `check_echo.ps1` is written and
  needs no changes, and will report the cancellation depth once the control
  clears −47 dBFS. Until then step 6 stands as met-but-not-demonstrated.
- W1: The VB-CABLE installer made `CABLE Input` the **default playback
  device**, which leaves the machine with no audible sound. It was put back to
  `Speakers (Realtek(R) Audio)`. `check_microphone.ps1` takes the default over
  for the length of a run and restores whatever it found, so the echo test
  should be run with a real speaker as the default.
- W1: `Speakers (Razer Barracuda X 2.4)` is the default this PC normally uses,
  and it was powered off throughout (`DeviceState` "not present"). A headset is
  the wrong device for the echo test anyway — cancelling an echo that barely
  exists is no test — so the built-in speakers were used instead.

Shared code that fails on Windows (platforms.md §4: listed, not edited here).
**Re-measured after merging L1 at W2: everything below is fixed except
`opencord-voice/tests/video.rs` and the goldens.** The voicebot test compiles,
the five examples compile, the server's config tests pass, the D-Bus tests are
`@TestOn('linux')`, `hardcoded_colors_test` accepts backslashes, and
`app/test/golden/failures/` is gitignored. The entries are kept as the record
of what W0 found. **W3 re-measured them again once the codec was on, which
woke up every `needs_codec!()` test; the two that still fail are below the W0
list, under "found in W3".**
- `crates/opencord-voicebot/tests/voice.rs` **does not compile**:
  `a_voicebots_encoded_camera_is_decoded_by_another_voicebot` calls
  `publish_encoded_camera`, which `src/lib.rs:570` gates behind
  `cfg(target_os = "linux")`. That takes the whole target with it, so none of
  that file's tests can run here. platforms.md §2.6 has L1 make these skip at
  run time instead.
- `crates/opencord-media/examples/{camera_load,camera_probe,codec_costs,screen_load,virtual_camera}.rs`
  **do not compile** off Linux: they import `camera::pipewire`, `camera::v4l2`,
  `camera::virtual_camera`, `video::sender`, `video::screen` and
  `codec::{encoder,raw,scale}` ungated. This is the whole reason
  `cargo clippy --workspace --all-targets` fails here while
  `cargo build --workspace` and `cargo test --lib --bins --tests` both pass —
  only `--all-targets` builds examples. L1's "everything compiles on every
  desktop OS" should be read to include examples.
- `opencord-server` `config::tests::reads_an_existing_file` and
  `config::tests::environment_overrides_the_file`: Unix absolute paths.
  `/srv/opencord` loads as `C:/srv/opencord` and `/certs/full.pem` as
  `C:/certs/full.pem`, which is correct on Windows — the expectations are what
  is Unix-only.
- `opencord-voice tests/video.rs` is **unstable** here, not cleanly failing.
  Over 5 runs, 5–7 of the 10 runnable tests passed each time (63–77 s).
  Failing in **5 of 5**: `a_capped_uplink_stops_encoding_the_cameras_top_layer_until_lifted`
  ("never reached the top layer") and `a_screens_main_layer_reaches_its_viewer`
  ("the main layer never came"). Intermittent:
  `more_than_twenty_wants_a_second_close_the_gateway` 3/5,
  `a_published_camera_reaches_whoever_wants_it_intact` 2/5, and
  `unused_layers_stop_being_encoded`,
  `an_xor_transform_end_to_end_does_not_stop_the_node` and
  `a_track_far_over_its_ceiling_is_stopped` 1/5 each. The single worst run was
  the one run alone with `--test-threads=1` (5 failed), so this is not CPU
  contention from other work. The suite is pacing-sensitive by design — it
  already carries
  `a_500_kbps_receiver_gets_a_lower_layer_within_2_s_and_recovers` as
  `ignored, needs a release build's timing`. The file is the Linux session's.
- `app/test/golden/**`: 16 failures, expected — the goldens were rendered on
  Linux (§0). Not regenerated here.
- `app/test/unit/desktop/linux_notifications_test.dart` (2 tests) and
  `linux_tray_test.dart` (1): `TimeoutException after 0:00:30` each. They need
  a D-Bus session.
- `app/test/unit/hardcoded_colors_test.dart`: it excludes the theme with
  `file.path.startsWith('lib/ui/theme/')`, but `Directory.listSync` returns
  `lib\ui\theme\…` on Windows, so every token in `oc_colors.dart` and
  `oc_theme.dart` is reported as an offender.

Found in W3, once the codec was on and the skipped tests ran (platforms.md §4:
listed, not edited here). Both are Linux-owned files, and both were measured
with `--test-threads=1`, so this is not test contention:
- **`opencord-voicebot` `tests/voice.rs`
  `a_voicebots_encoded_camera_is_decoded_by_another_voicebot` fails, and
  deterministically: 4 runs out of 4, always
  `Seen { frames: 30, intact: 0, decoded: 30, layers: [30, 0, 0] }`, 34,074
  bytes in three of them.** `intact: 0` is what the test wants ("real H.264,
  not the test pattern"); the assertion that misses is `seen.layers[1] > 0`.
  Everything arrives on layer 0 and nothing on layer 1 within the 30 frames
  the test waits for. The cause looks like how long this PC takes to climb:
  V4 over 14 s gives `by layer 140/14/0`, so layer 1 only appears several
  seconds in, and one of the two bundled-FFmpeg V4 runs got `146/0/0` — no
  layer 1 at all. If that reading is right the test needs to wait for a layer
  rather than for 30 frames. It is a run-time skip now, so it no longer blocks
  compilation (thank you); it does fail.
- **`opencord-voice` `tests/video.rs` still misses its timing**, as W1 found:
  7 passed, 3 failed in the final serial run
  (`a_capped_uplink_stops_encoding_the_cameras_top_layer_until_lifted`,
  `a_screens_main_layer_reaches_its_viewer`, `unused_layers_stop_being_encoded`).
  The first two also failed 5 of 5 in W1, so they are the stable pair. Logged
  and not treated as blocking, as the user asked.

Warnings in shared files, expected before L1 and not fixed (§0, platforms.md §4).
**All 15 are gone after merging L1:** `cargo clippy --workspace --all-targets
-- -D warnings` exits 0 on Windows as of W2. Kept as the record of what W0
found:
- `opencord-media` lib, 8: `hotkeys/mod.rs` `shortcut_id`,
  `shortcut_description`, `action_of`, `shortcut_ids`, `Held::press` and
  `Held::release`; `video/camera/mod.rs` `pack`.
- `opencord_core` lib, 5: `client.rs` unused import `ScreenProblem` and field
  `screen_restore_token`; `video.rs` fields `runtime`, `screen`, `texture` and
  unused variable `stream_delays`.
- `opencord-voicebot`, 2: an unneeded `mut` in `src/lib.rs:374` and an unused
  import `StreamSourceKind` in `src/main.rs:12`.

`shared:` commits (file, why):
- (none yet)
