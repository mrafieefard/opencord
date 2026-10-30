# Session logs

Every time the desktop app starts, it writes one log file for that session. When something goes wrong, the user exports it from **Settings → About → Logs** and sends it. From that one file you can tell what the machine was, what the app did, in what order, and where it failed. D82 records the decisions.

## Where they are

A `logs` folder in the app's support folder:

| System | Folder |
|---|---|
| Windows | `%APPDATA%\dev.opencord\Opencord\logs` |
| macOS | `~/Library/Containers/dev.opencord.opencord/Data/Library/Application Support/dev.opencord.opencord/logs` (the app is sandboxed) |
| Linux | `~/.local/share/dev.opencord.opencord/logs` |

- Each session is `opencord-2026-10-30T14-32-05.log`, named by its local start time. A second start in the same second adds `-2`.
- The newest 10 are kept; starting an eleventh deletes the oldest.
- Profiles share the folder.

## What the user sees

**Settings → About → Logs**, on desktop:

- **Export this session's log:** a save dialog suggests `opencord-log-2026-10-30-1432.txt`, and a toast says "Log saved".
- **Export the last session's log:** shown only when the last session ended unexpectedly (it crashed, or was killed), with the subtitle "It ended unexpectedly". After a crash, the restart is a new session, so this row is how the crashed one gets sent.
- **Open logs folder.**

The footer reads: "Logs never contain passwords, keys, messages or addresses. The last 10 sessions are kept."

## Reading one

The file opens with a header:

```
=== Opencord session log ===
started: 2026-10-30 14:32:05
app version: 0.0.2-canary.3
track: canary
started by: the launcher
profile: default
display 1: 2560x1440, scale 1.25, 144 Hz
launcher log:
    [1761831125.120] launcher 0.0.2-canary.3, track canary
    …
core version: 0.1.0 (protocol v1)
os: Windows 11 Pro 24H2 (26100.2314), edition Professional
cpu: AMD Ryzen 7 5800X 8-Core Processor, 16 threads
memory: 31.9 GB
===
```

Then one event per line:

```
14:32:07.412 +2.118s INFO  media::video::screen::windows  share starting kind="window" fps=30 capture_api=true app=Google Chrome
```

Each line holds, in order:
- the local time, then the time since the session started;
- the level and where the event came from: a Rust module with `opencord_` cut, or `app::<area>` for the Flutter side;
- the message, then its fields.

Lines after a message's first (stack traces, backtraces) are indented.

### Lines to look for

- **`core::machine`**, just after the header: graphics cards with their driver versions, audio devices with the defaults marked, and whether screen sharing, the capture API (Windows) and sound sharing are available. They're written from a thread after start, so they may come a few lines down.
- **Connecting:**
  - `server-A is port 443, a certificate an authority vouches for, protocol v1`: once per server;
  - `connected` or `could not connect`, with `after_ms`;
  - `tcp connected` with the address reached (`public-ip-1` or `lan-ip-1`).
- **Voice:**
  - `voice joining` / `voice joined` / `voice join refused`, with `channel-N`;
  - `voice media connecting`, with the remote and local address and where the address came from;
  - `voice media connected`, `disconnected` or `failed`.
- **A screen share, in order:**
  - `app::share starting a screen share`;
  - `share starting`, with the kind, the frame rate, whether the capture API is used, and for a window its app, never its title;
  - `the capture API refused; using GDI` (with the HRESULT), `the capture API sent no frame; using GDI`, or `no usable GPU for capture; using WARP`;
  - `encoder chosen`, with the backend, hardware or not, size, fps and bitrate; `an encoder did not open` lists the ones that failed first;
  - `first frame captured` / `first frame sent`, with `after_ms`;
  - every 5 seconds, `capturing` (frames grabbed, the queue full, nothing new) and `sending:` (`captured=… sent=… dropped=… (reason n) keyframes=… keyframe_requests=… encode_ms=…`);
  - `capture stopped`, with `cause`: `stopped`, `source closed`, `window closed` or `error`;
  - `a video track ended`, with the last counts.
- **Watching a share:** `watching a video track`, `decoder opened` (hardware wanted and got), and every 5 seconds `receiving: received=… decoded=… decode_errors=… keyframe_requests=…`.
- **Failures from the app:** `app::core  a core call failed`, with the error and the app's calling frames; `app::flutter` and `app::dart` for uncaught errors.
- **Crashes:**
  - `ERROR panic  <message> thread=… location=…`, then a backtrace;
  - `FATAL native crash  code=0xC0000005 module=… offset=…` on Windows, or `signal=11 (SIGSEGV)` elsewhere.
  - Windows' fail-fast crashes (heap corruption, `0xC0000374`) skip every handler in the process. A session that ends without `session ended` and without a crash line is most likely one of those.
- **The end:** `session ended`, the last line of every clean exit.

## What's never in it

The core filters every line before it reaches the disk (`crates/opencord-core/src/log/scrub.rs`). That covers third-party crates' lines too.

| Never written | Written instead |
|---|---|
| A server's host | `server-A`, `server-B`…, the same all session |
| An IP address | `public-ip-1`, `lan-ip-2` (private ranges, Tailscale's 100.64/10, link-local), `loopback`, `ipv6-public-1`, `ipv6-lan-1`; ports stay |
| Another domain or URL | `https://host-1:8443`: scheme and port only, no path or query |
| An e-mail address | `<email>` |
| A user or channel id the core logs | `user-1`, `you` for the signed-in user, `channel-1` |
| The home folder | `~` |
| Tokens, passwords, keys | `<secret>` after `token=`, `password=`, `Authorization:`, `Bearer`; and any long random-looking run (24+ characters with digits, or 32+ hex) |

- **Message text, file names and window titles** are never logged in the first place.
- **The alias table** lives only in memory, so not even the logs folder holds the real values.
- **Device names** (microphones, cameras, graphics cards) are kept: they've mattered before (D72).

## Writing to it

- **Rust:** `tracing`, as before. Opencord's crates go in at `INFO`. Its media and connection modules go in at `DEBUG`: `opencord_media::video`, `opencord_media::share_audio`, `opencord_core::connection`, `::voice`, `::media` and `::video`. Other crates go in at `WARN`.
  - Name a server with `crate::log::alias::server(key)`, an address with `alias::address(ip)` or `alias::host(host)`, a user with `alias::user(id)`, a channel with `alias::channel(id)`.
  - Never log message text, file names or window titles.
- **Flutter:** `appLog(message, name: area, error:, stackTrace:, level:)` from `core/logs/app_log.dart`. It also goes to the debugger.
  - The Flutter side can't alias ids, so it doesn't log them. Server keys are fine, because the filter replaces registered servers.
- **Limits:**
  - **Size:** a session stops taking `INFO` and `DEBUG` lines at 20 MB, and says so once.
  - **Repeats:** an identical line within a second is folded into `(repeated N times)`.
  - **Overflow:** if the writer falls behind by 8192 lines, new lines are dropped and counted.
