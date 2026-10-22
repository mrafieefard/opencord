# Performance

Measurements against the Phase 2 targets (plan §16.1), with the test that produces each one. The summary below is where each target stands; the sections after it have the detail by milestone and system.

The machine is the development laptop: an Intel Core i7-13620H (10 cores, 16 threads) on Arch Linux. Times are per 10 ms audio tick on one thread, from release builds. "Of a core" is that time divided by 10 ms. The plan's reference machine is a mid-range 4–8 core laptop, so expect somewhat higher shares there.

## Every target (V10, 2026-10-15)

| §16.1 target | Measured on the development laptop | Where |
|---|---|---|
| Voice end-to-end latency on a LAN, devices included, ≤ 150 ms | A round trip through a playback and a capture device, the app's processing and the node twice: **92 ms** median, 100 ms at the 95th percentile. One way is less | `app/tool/check_voice_latency.sh` |
| 5 % loss + 40 ms jitter: no gaps | No gaps | `speech_survives_5_percent_loss_and_40_ms_of_jitter` |
| Silent participant upload < 3 kbps | About 620 bit/s | `a_silent_participant_sends_almost_nothing` |
| Noise suppression ≤ 2 % / ≤ 10 % of a core | 1.8 % / 6.1 % | `processing_time_per_tick` |
| Camera on, 720p30 with layers, ≤ 6 % CPU | 2.0 % of the machine (32.7 % of a core) | Camera (V5) below |
| Screen share 720p30 / 1080p60 ≤ 8 % / ≤ 15 % CPU | 1.1 % / 2.0 % of the machine | Screen share (V6) below |
| Screen share capture to display ≤ 250 ms | Capture to the viewer's decoded picture: **72 ms** median, 88 ms at the 95th percentile, at 1080 px; drawing adds a frame | `app/tool/check_screen_latency.sh` |
| Watching 1080p60, hardware decoding, ≤ 8 % CPU | **5.5 %** of the machine (87 % of a core); on a 4-core, 8-thread laptop that would be about 11 %. With zero-copy textures (`OPENCORD_ZERO_COPY=1`, opt-in for now): **2.4–3.3 %** (38–52 % of a core), within the target on 8 threads too | `app/tool/check_v10_load.sh watch` |
| Client memory, 10 people, 4 cameras and a stream, ≤ 350 MB | **344 MB** steady (305–355 over 30 s), with NVIDIA's EGL libraries loaded besides Intel's on this hybrid laptop; **298 MB** with only the integrated GPU's | `app/tool/check_v10_load.sh call` |
| Voice node on 4 vCPUs, 10 channels × 10 people, 2 cameras + 1 stream each, < 50 % CPU | **15.6 %** with everyone in 360 px tiles (127 Mbit/s); **20.2 %** with everyone watching everything at 1080 px (593 Mbit/s) | `opencord-voice-load` |

### V10 measurements

- **The node.** `opencord-voice-load` claims a release server pinned to CPUs 0–3 (`taskset`) and fills it from the other twelve: 10 channels of 10 bots, two talking in each, two with cameras (the test pattern at each layer's full bitrate: 150 kbit/s, 500 kbit/s, 1.5 Mbit/s) and one sharing a 1080p30 screen (its layers at full bitrate), everyone watching every video. Over 30 s once everyone was in: 0.62 cores (15.6 % of the 4) at 127 Mbit/s with 360 px tiles, 0.81 cores (20.2 %) at 593 Mbit/s with 1080 px tiles; 125 MB and 242 MB of memory. The bots used 2.6 of their 12 cores, so they were not what limited it.
- **The client in a call of ten.** A profile build (`flutter drive --profile`) in a 1920×1080 window joins a call with three bots sending real H.264 cameras, one sharing a 1080p30 screen, four sitting and one talking; it watches the stream large and turns its own camera on (a virtual camera). Memory at each step: 225 MB launched, 260 in voice, 260 watching, 325–336 with the camera, then a steady 344 (sampled every second for 30 s: 336–355). With Mesa's EGL only (`__EGL_VENDOR_LIBRARY_FILENAMES`), as a laptop with only an integrated GPU loads it: 189 launched, 298 steady (286–310). CPU 108–120 % of a core (7 % of the machine). Every camera tile kept 15 fps, the stream 30.
  - Before V10's changes it sat at 371–380 MB. Three things brought it down: large allocations go straight back to the system (a fixed glibc mmap threshold; decoded and converted pictures had stayed resident in heap arenas), about 30 MB; DeepFilterNet gives back what loading its networks left (37 MB to 23); and a layer nobody has wanted for 10 s closes its encoder (a 720p VA-API encoder holds about 20 MB). Fewer malloc arenas and smaller VA-API surface pools changed nothing (the surfaces live in GPU memory).
- **Watching 1080p60.** One bot shares 1080p60; the app watches it large: 87 % of a core (5.5 % of the machine), 59.8 pictures a second presented, 55.9 drawn. The video thread takes 39 % of a core: decoding on the GPU, copying each picture back, converting and scaling it on the CPU.
- **Zero-copy textures** (`OPENCORD_ZERO_COPY=1`, D37). The same stream, the picture never leaving the GPU: 38.5 % of a core with NVIDIA's EGL libraries loaded, 52.4 % with Mesa's only; the video thread 1.9–2.2 %, the GL worker 1.4–1.6 %; memory 298 and 258 MB instead of 331; 59.7–59.8 pictures a second presented. A headless Mesa context draws a decoded 1280×720 picture within 1.16 levels of 255, on average, of the CPU conversion of the same picture (`a_picture_drawn_from_the_gpu_matches_the_one_converted_in_memory`).
- **Screen share latency.** The virtual screen draws the wall-clock time into every frame (`video/stamp.rs`); the app shares it at 720p30 from a 1920×1080 screen, and a bot watching in a 1080 px tile decodes each picture and reads the time back: 72 ms median, 88 ms at the 95th percentile, 144 ms at most, over 634 pictures. That covers PipeWire capture, the app's conversion and encoding, the node and decoding; the app was a debug build.
- **Voice latency.** The app plays into a PipeWire null sink and records its monitor as the microphone, echo cancellation, noise suppression and gain control off; a bot beeps every second and times each beep coming back: 92 ms median, 100 ms at the 95th percentile, over 28 beeps (the first, before the jitter buffer settled, took 857 ms). The round trip holds both devices and two trips through the node, so one way, mouth to ear, is less.

## Voice

| Area | Target | Measured | Where |
|---|---|---|---|
| Voice processing, Standard noise suppression | ≤ 2 % of a core | 179 µs per tick, 1.8 % | `processing_time_per_tick` (opencord-media, `--release --ignored`) |
| Voice processing, High noise suppression | ≤ 10 % of a core | 614 µs per tick, 6.1 % | same |
| The whole audio tick, Standard | — | 319 µs, 3.2 % | same: processing plus Opus both ways and one voice heard |
| The whole audio tick, High | — | 777 µs, 7.8 % | same |
| High's first-run benchmark, every stage on | High by default under 20 % | about 4.6 % | `the_benchmark_measures_a_share_of_each_tick` |
| High's added delay | 20–40 ms (plan §7.3) | 30 ms | `the_voice_comes_out_30_ms_later` |
| Software path, microphone tick to speaker tick | part of ≤ 150 ms end to end | about 40 ms | `the_software_path_adds_little_latency` |
| 5 % packet loss with 40 ms jitter | intelligible, no gaps | no gaps | `speech_survives_5_percent_loss_and_40_ms_of_jitter` |
| A silent participant's upload | < 3 kbps | about 620 bit/s | `a_silent_participant_sends_almost_nothing` (opencord-voice) |
| Back after a network change | < 3 s (plan §7.14) | about 100 ms | `a_new_network_keeps_the_call_going` (opencord-voice) |

The voice processing chain is the high-pass filter, echo cancellation, noise suppression and automatic gain control, at their defaults. The audio is a voice from the far end with its echo, a voice at the microphone, and a fan underneath. Echo cancellation takes about 103 µs of Standard's 179, RNNoise 54, gain control 25 and the high-pass filter 2.

## Camera (V5)

Release builds, with a virtual PipeWire camera at 1280×720 and 30 fps in place of a webcam. "Of a core" is CPU time over wall time. "Of the machine" divides that by this machine's 16 logical cores. A 4–8 core reference laptop has 8–16, so expect up to twice those shares there.

| Area | Target | Measured | Where |
|---|---|---|---|
| Camera on, 720p30 with three layers, hardware encoding | ≤ 6 % CPU | YUYV: 32.7 % of a core, 2.0 % of the machine. MJPEG: 40.5 %, 2.5 % | `camera_load` example (opencord-media, `--features testing`) |
| Camera on/off | < 1 s (V5) | on 26–62 ms to the preview's first picture; off 18–46 ms to the preview released | `app/tool/check_v5_load.sh` |
| A call of nine, every camera on | smooth (V5) | every other tile at 15.0 fps (their 180p layer, the tallest that fits a 230-pixel tile), never under 14 in any second; own preview at 30 fps | same |
| The app's CPU in that call | — | 51–56 % of a core, 3.2–3.5 % of the machine | same |
| The app's memory in that call | ≤ 350 MB for 10 people with 4 cameras and a stream | 404–411 MB with 9 cameras | same |

Where a camera picture's time goes, per 720p frame (`codec_costs` example):

| Step | Back to back | Paced at 30 fps |
|---|---|---|
| YUYV to NV12 | 0.38 ms | 2.8 ms |
| MJPEG to NV12 (JPEG decode 1.2 ms, conversion 0.2 ms) | 1.38 ms | 7.9 ms |
| Halving 720p to 360p (and 360p to 180p) | 0.2 ms | 1.4 ms |
| VA-API encoding, 720p / 360p / 180p | 1.3 / 0.8 / 0.5 ms of CPU | 4.1 / 2.2 / 1.8 ms |
| NVENC encoding, 720p / 360p / 180p | 1.1 / 0.3 / 0.1 ms of CPU | 1.6 / 0.6 / 0.4 ms |

At 30 fps, the powersave governor runs this work at low clocks, so each step takes 5–7 times its back-to-back time. Shares of a core therefore overstate the work done. VA-API costs the CPU much the same per frame whatever the size, which is why the small layers cost nearly as much as the large one. Uploading the camera picture once and scaling on the GPU belongs to V10's zero-copy work, as does decoding JPEG on the GPU.

In the call of nine, the app's time splits like this:

- video threads (its camera's encoding plus eight decoders and tile conversions): 17–20 % of a core;
- audio engine: 10–15 %;
- Flutter's raster thread: 8–10 %;
- the main thread: 7–8 %.

Memory is about 16 % over its target. That call had more cameras than the target's scenario, but the gap is still V10's to close, along with a measurement of the exact scenario: 10 people, 4 cameras and a stream.

The call's eight others are voicebots on the same machine: release builds encoding the moving test scene through the same code, against a release server. A debug server's processing delays distort the node's bandwidth estimates (D28).

Not measured yet:

- **End-to-end voice latency on a LAN, with devices** (≤ 150 ms). It needs two machines. The software path's 40 ms comes on top of the device buffers: 10 ms each way at 48 kHz, plus the engine's 30 ms speaker queue.
- **A real webcam.** The camera numbers come from the virtual camera; this machine's webcam (MJPEG 1280×720 at 30) wasn't switched on.

## Screen share (V6)

Same setup as the camera: release builds, a virtual PipeWire screen at 1920×1080 in place of a real capture.

| Area | Target | Measured | Where |
|---|---|---|---|
| Screen share on, 720p30 with two layers, hardware encoding | ≤ 8 % CPU | 17.6 % of a core, 1.10 % of the machine | `screen_load` example (opencord-media, `--features testing`) |
| Screen share on, 1080p60 with two layers, hardware encoding | ≤ 15 % CPU | 32.5 % of a core, 2.03 % | same |

The screen share numbers are well within their targets. A screen's source is read from PipeWire (zero CPU) and converted to NV12 in Rust. The low layer (640×360 at 15 fps) adds little to the cost of the main layer.

| Step | 1080p60, back to back |
|---|---|
| BGRx to NV12 | 0.6 ms |
| NVENC encoding, 1080p60 main | 2.1 ms |
| NVENC encoding, 360p15 low | 0.3 ms |

Not measured yet:

- **A real screen.** The numbers come from the virtual screen at a fixed 1920×1080.
- **Watching** is measured in V10 above.

## Media uploads (A8)

Targets from the media uploads plan (Task 36), measured by `app/tool/check_transfer.sh`: a release server on loopback, and the core's `transfer_load` example sending a 1 GiB file to #general and saving it back (the copy is compared with the original). Upload time runs until the message is sent, so it includes the server storing the file. The server's memory is sampled every 0.2 s from `/proc`.

| Target | Measured, three runs (2026-10-20) |
|---|---|
| Upload ≥ 80 MB/s on loopback | 77.8, 94.9 and 58.3 MB/s |
| Download ≥ 80 MB/s on loopback | 353.5, 91.2 and 77.0 MB/s |
| Server memory grows < 100 MB during the transfer | 60, 72 and 51 MiB |

- **Memory** stays within the target: pieces are 8 MiB, downloads are read 16 segments (1 MiB) at a time, and nothing holds a whole file.
- **Speed** met the target in one run of three for uploads and two for downloads. The laptop was busy during all three (load average about 10, with VS Code, a browser and Spotify running), and the runs spread from 58 to 95 MB/s, so they say more about the machine than the code. A run on an idle machine is still owed.
- **What sets the upload speed:** pieces go one at a time, and the server encrypts each, flushes it to disk (`sync_data`, so an upload that continues after a crash starts from bytes that are really there) and records it in the database before answering. About 90–145 ms per 8 MiB piece in these runs.
- **Scrolling a channel with 200 image messages at 60 fps** (profile mode) needs a display and is not measured yet.

## macOS (D34)

An Intel MacBook Pro 2019 (i7-9750H, 6 cores, 12 threads) on macOS 15.8, release builds unless stated. VideoToolbox's hardware encoding runs in a process of its own, so it is not in the app's CPU. The machine throttles hard when hot (its CPU was held at 22 % of its speed in one run), so its timing tests are logged, not chased.

| Area | Target | Measured | Where |
|---|---|---|---|
| Per-user volume (50 % / 200 %), local mute | −6 / +6 dB, silent | −6.0 / +6.0 dB, −106 dBFS; soundboard muted −200 dBFS | `app/tool/macos/check_user_audio.sh` |
| Echo with the laptop's speakers and microphone | not heard | −110.1 dBFS with processing, −19.1 without | `check_echo.sh` |
| Camera on, 720p30 with three layers and a preview | ≤ 6 % CPU | 12 % of a core (sending side, encoding not counted) | `the_cameras_sending_side_costs` |
| Camera on / off | < 1 s | 1104 ms / 15 ms (the camera's first frame takes 0.65 s) | `check_v5.sh` |
| Screen share 1080p30, two layers | ≤ 15 % CPU | 24–25 % of a core at the share's size; 82–83 % capturing the full Retina screen | release test, M6 |
| Going live | — | 643–833 ms | `check_v6.sh` |
| Encoding per picture, VideoToolbox hardware | — | 720p 8.9 ms, 360p 5.7 ms, 180p 3.9 ms, 1080p 10.8 ms (wall time) | M3 |
| Scaling 1080p NV12 to a 640×360 RGBA tile | ≤ 2 ms | 1.23 ms | `codec::macos` tests |
| "No double audio" | the call's voices not in the stream | music −12.0 dBFS, Talker −117.8 dBFS | `check_v7.sh` |
| A headset pulled out | back on the default | microphone after 642–674 ms, speaker after 934–947 ms | `check_devices.sh` |

The app's memory after launch is 92 MB. The debug app draws video at only 9–11 fps (per-pixel loops in debug code); every number above is from release code.

## Windows (D35)

An i7-13620H laptop (the development laptop's CPU) with an RTX 4070 Laptop and Intel UHD, on Windows 10 22H2. Wall time per call unless stated.

| Area | Target | Measured | Where |
|---|---|---|---|
| Per-user volume (50 % / 200 %), local mute | −6 / +6 dB, silent | −6.0 / +6.0 dB, −106.2 dBFS; soundboard muted −123.0 dBFS | `app/tool/windows/check_user_audio.ps1` |
| Push-to-talk on a global keybind, another app focused | heard only while held | held −21.1 dBFS (604 packets), released nothing | `check_keybinds.ps1` |
| Camera on / off | < 1 s | 348–378 ms / 2 ms | `check_v5.ps1` |
| The built-in camera | — | opened in 79 ms, first frame 620 ms, 30.8 fps NV12 1280×720 | `tests/camera_windows.rs` |
| Encoding, 720p30 / 360p30 / 180p15, NVENC | — | 1.99 / 0.60 / 0.31 ms (all three 2.90 ms of a 33 ms frame) | `tests/codec_costs_windows.rs` |
| Encoding, the same, Quick Sync | — | 3.50 / 1.89 / 1.56 ms (720p undershoots: 1079 of 1500 kbit/s) | same |
| Screen share 1080p30, CPU conversion | ≤ 15 % CPU | 14.8 % of a core in a **debug** build | `check_screen_cost.ps1` |
| Screen share start / stop | — | 184 ms / 27 ms; 30.3 fps; a closed window ends it in 0.3 s | `tests/screen_windows.rs` |
| Sleep and hibernate during a call | the call continues | still connected, same level, after 30 s sleep and 77 s hibernation | `check_hardware.ps1` |

The release folder is 195.8 MB, 120 MB of it FFmpeg's DLLs (D35). Not measured on Windows: screen share sound (needs build 20348), the echo canceller (the laptop's speakers are too quiet at its microphone), 1440p shares (no such monitor).
