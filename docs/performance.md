# Performance

Measurements against the Phase 2 targets (plan §16.1), with the test that produces each one. Areas without a number yet belong to later milestones.

The machine is the development laptop: an Intel Core i7-13620H (10 cores, 16 threads) on Arch Linux. Times are per 10 ms audio tick on one thread, from release builds. "Of a core" is that time divided by 10 ms. The plan's reference machine is a mid-range 4–8 core laptop, so expect somewhat higher shares there.

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
- **Screen share, decoding a 1080p60 stream, and the voice node's load.** These arrive with V6–V10.
