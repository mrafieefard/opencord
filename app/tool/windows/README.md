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
| `check_m5.ps1` | Phase 1 M5: a fresh install creates an identity, claims a local server and is still connected after a restart. |
| `check_m6.ps1` | Phase 1 M6: two identities chat live, and a new restriction reaches the member's UI without a reconnect. |

## Test stand-ins

Where the Linux checks use PipeWire null sinks, `parec` and `pw-play`, these
use VB-CABLE, Hi-Fi CABLE and FFmpeg (windows.md §3, decision F). `ffmpeg.exe`
and `ffplay.exe` come from `C:\ffmpeg\bin`, which `FFMPEG_DIR` also points at.
