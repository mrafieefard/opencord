# Windows check scripts

The Windows half of `app/tool/`: the acceptance checks that can only run on
Windows, and the pieces they share. See
[docs/plans/windows.md](../../../docs/plans/windows.md) for the plan they
belong to, and [platforms.md](../../../docs/plans/platforms.md) §3 for who
owns what.

They are **PowerShell 7** (`pwsh`), not Windows PowerShell 5.1 and not bash:
Git Bash's `python3` and `setsid` are unreliable here, so the Linux scripts'
shell is not an option (windows.md §3, decision F).

```powershell
pwsh -File app/tool/windows/check_m5.ps1
```

| File | What it does |
|---|---|
| `e2e_server.ps1` | The twin of `../e2e_server.sh`: a server of its own on a free port, in a temporary folder, with the app on a profile of its own. Dot-source it and run the check's body through `Invoke-E2ECheck`, which stops everything it started even when the body throws. |
| `audio_devices.ps1` | Turns a device's name into the `wasapi:` id the app knows it by, from the `MMDevices` registry. |
| `measure.py` | The level of a tone in raw audio, in dBFS: the same Goertzel as the voicebot's `tone_dbfs`, for the checks that measure what ffmpeg captured rather than what a voicebot heard. `py -3`. |
| `check_m5.ps1` | Phase 1 M5: a fresh install creates an identity, claims a local server and is still connected after a restart. |
| `check_m6.ps1` | Phase 1 M6: two identities chat live, and a new restriction reaches the member's UI without a reconnect. |
| `check_user_audio.ps1` | W1 step 4: the per-user voice menu, measured at the speaker through the cable. |
| `check_microphone.ps1` | W1 step 5: the microphone path, in voice activity and push-to-talk, held and released. |
| `check_echo.ps1` | W1 step 6: the echo test, on real speakers. **Exits 2 when the run cannot discriminate** — see below. |
| `hold_key.ps1` | Holds one key down for a while, wherever the focus is, and says which app had it. How the checks press a global keybind with nobody at the PC. |
| `check_keybinds.ps1` | W2 step 7: push-to-talk on a global keybind, held from outside the app while Notepad has the focus. |
| `check_key_recorder.ps1` | W2 step 6: what the app's key recorder makes of Print Screen, the Windows key and the right Alt. **Exits 2 for a key it cannot record at all** — see below. |
| `check_v4.ps1` | Phase 2 V4 (W3 step 7): a voicebot's three-layer camera reaching a watching voicebot in a 360-pixel tile, every picture intact. The twin of `../check_v4.sh`. |
| `check_bundled_ffmpeg.ps1` | W3 step 5: takes FFmpeg away and checks the release build still starts and still encodes, on the DLLs it ships. |
| `sample_video.ps1` | The mean colour of a rectangle of the app's window, copied off the screen. How W4 checks that red looks red, which Flutter cannot answer about a `Texture`. |
| `check_v5.ps1` | W5: this PC's camera in a real call — on and off timings, the mirrored preview, and three voicebots watching at three sizes to see which layers go out. **Exits 2 when only the top layer is missing.** |
| `check_v6.ps1` | W6: screen and window sharing, picked in Opencord's own dialog, with an ffplay test pattern to share. **Exits 2 when the app shared but nothing reached the watcher.** |
| `check_screen_cost.ps1` | W6 step 7: what a shared screen costs the CPU, which decides whether the convert and scale move to the GPU. |
| `check_v7.ps1` | W7: a screen shared *with its sound*, and the call's own voice kept out of it. **Needs Windows 11 (build 20348); exits 2 below that.** |
| `check_receive_video.ps1` | W4: a voicebot's camera and screen shown in the app, drawn, the right colours, focus mode, and hidden tiles that stop receiving. **Exits 2 when only the screen half did not come** — see below. |

A check exits 0 when it passed and 1 when it failed. Two of them also exit
**2**, for a result that proves nothing either way:

- `check_echo.ps1`, when the control run was already quiet. Its acceptance is
  "the echo comes back under −50 dBFS", so it first runs with echo cancellation
  off, and if that control is already under −47 dBFS there was nothing to
  cancel and a silent speaker would score the same as a working canceller.
  Treat 2 as "run it again with louder speakers", not as a pass.
- `check_v6.ps1`, when the app shared correctly but no pictures reached the
  watching voicebot — the screen-share media gap in windows.md §7.
- `check_v7.ps1`, on a Windows older than build 20348, which has no process
  loopback at all. Sharing sound genuinely cannot work there, and the app
  disables the switch with the same words.
- `check_v5.ps1`, when the camera worked but the top layer never went out. That
  is not the camera: this PC never reaches the top layer at all (windows.md
  §7, W5), and a shared test has said so since W1.
- `check_receive_video.ps1`, when the camera half passed and only the screen
  share failed to arrive. It does arrive on this PC, but only about half the
  time and always at a trickle (windows.md §7, W4); W6 is where screen sharing
  is built and settled. 1 is kept for the camera half.
- `check_key_recorder.ps1`, when the recorder could not record a key at all.
  That is a limit in shared Dart, which a platform session does not fix
  (platforms.md §4), and the key may still work as a keybind once set; 1 is
  kept for a key recorded as the *wrong* key.

`check_v4.ps1` and `check_bundled_ffmpeg.ps1` need `FFMPEG_DIR` and
`LIBCLANG_PATH` set, like any build here from W3 on, and
`check_bundled_ffmpeg.ps1` renames `FFMPEG_DIR` out of the way while it runs —
so don't start anything else that needs `ffmpeg.exe` at the same time. It puts
the folder back whatever happens.

## Test stand-ins

Where the Linux checks use PipeWire null sinks, `parec` and `pw-play`, these
use VB-CABLE, Hi-Fi CABLE and FFmpeg (windows.md §3, decision F). `ffmpeg.exe`
and `ffplay.exe` come from `C:\ffmpeg\bin`, which `FFMPEG_DIR` also points at.
