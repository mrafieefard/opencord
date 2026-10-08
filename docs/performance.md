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

Not measured yet:

- **End-to-end voice latency on a LAN, with devices** (≤ 150 ms). It needs two machines. The software path's 40 ms comes on top of the device buffers: 10 ms each way at 48 kHz, plus the engine's 30 ms speaker queue.
- **Video, screen share, memory and the voice node's load.** These arrive with V4–V10.
