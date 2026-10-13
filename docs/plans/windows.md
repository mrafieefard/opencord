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

## 0. Working rules

- **Branch:** `phase2-windows`, made from `phase2-remaining`.
  - Start each milestone with `git fetch && git merge origin/phase2-remaining`.
  - Push at the end of each milestone.
  - Never merge `phase2-macos`, and never commit to `main` or `pre`.
- **Your files:** change only the files [platforms.md](platforms.md) §3 gives
  to Windows. Anything else follows §4 there: a minimal `shared: …` commit
  with the reason in the log.
- **Wait for L1:** W0 and W1 can start now. W2 onwards need the Linux
  session's seams (platforms.md §2). Check with
  `git log origin/phase2-remaining --oneline --grep 'platform seams (L1)'`.
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
  - `cargo clippy --workspace --all-targets -- -D warnings`. Before L1, the
    dead-code warnings in shared files are expected (platforms.md §4); don't
    fix them.
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
  `[target.'cfg(windows)'.dependencies]` section (platforms.md §2.7).
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
   - Delete `windows_pending.rs` and its block.
   - Set `SUPPORTED`.

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
5. **`Picked`:**
   - `kind`.
   - `audio = Some(Source::Process(pid))` for a window, using decision E for
     UWP apps; `None` for a screen.
6. **`support()`:** supported when `GraphicsCaptureSession::IsSupported()`;
   `own_picker: true`.
7. **GPU work only if needed:** start with the CPU path. Measure CPU at 1080p30
   and 1440p60. If 1080p30 takes more than 15 % of a core, convert and scale
   on the GPU with the D3D11 video processor (to NV12, at the layer size)
   before mapping. Record the numbers either way.
8. **App**, in your Dart files (platforms.md §3):
   - `screen_source_picker.dart`: Screens and Windows tabs; thumbnails that
     refresh every 2 s while open; a selected state.
   - `screenshare_picker.dart` shows it when `ownPicker` is set. Go live stays
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
| W2 | | |
| W3 | | |
| W4 | | |
| W5 | | |
| W6 | | |
| W7 | | |
| W8 | | |

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

For the Linux session, and for the user:
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

Shared code that fails on Windows (platforms.md §4: listed, not edited here):
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

Warnings in shared files, expected before L1 and not fixed (§0, platforms.md §4):
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
