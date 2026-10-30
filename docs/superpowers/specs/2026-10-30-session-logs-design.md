# Session logs in the desktop app: design

Date: 2026-10-30. Branch `feat/session-logs`, from `canary` at 4cec01c; the pull request goes into `canary` and later moves to `beta`.

## Why

When a user hits a problem, there is no record of what happened. The Rust core already writes about 150 `tracing` events (voice, media, connection), but nothing in the app installs a subscriber, so all of them are lost. The Flutter side's 25 `developer.log` calls only reach a debugger. The owner's current case is screen sharing, which still fails on some Windows 10 and Windows 11 machines (D73 and D74 fixed some of the causes, not all), and the Windows capture code writes only 9 lines.

The owner wants this: a user who hits a problem exports their log and sends it to the owner, who hands it to Claude. **Claude must be able to tell from that one file what the machine was, what the app did, in what order, and where and why it failed**, without asking the user anything.

## Goals

1. Every launch of the desktop app writes one log file, a session. **The last 10 sessions are kept**; the oldest goes when an eleventh starts.
2. **Settings → About** gains a **Logs** section with **Export this session's log** and **Open logs folder** (owner's request).
3. One timeline holds the Rust core's events, the Flutter side's errors and the user's key actions, each with the time.
4. The file opens with a header that describes the machine and the build.
5. Screen sharing, on both ends and on every system, writes enough to find a failure; Windows gets the most detail because that's where it fails now.
6. **Nothing private is ever written to disk** (owner's rule, below).

## Non-goals

- Uploading logs anywhere. The user exports a file and sends it themselves.
- Logs on the server, or in the phone apps. (The phone apps can reuse the Rust part later.)
- A log viewer inside the app.
- Crash dumps (minidumps). A native crash records what it can in the log (below), nothing more.

## Anti-goals: it fails, even if it works, when

- a log contains a password, token, key, invite code, IP address, domain, URL, message text or file name; or
- the session that crashed can't be exported, because the restart started a new session; or
- the log is so full of noise that the failure is hard to find, or it fills the disk; or
- logging slows down voice or screen sharing. Nothing on an audio or video thread waits on a disk write.

## Privacy (owner's rule)

Never written: passwords, login and session tokens, private keys and anything from the key store, invite codes, message and attachment text, file names, IP addresses, domain names and URLs, the user's home folder path.

Replaced with aliases instead. Each alias is fixed for the whole file, so the same server is `server-A` on every line:

| What | Alias | Kept beside it |
|---|---|---|
| A server | `server-A`, `server-B`… | scheme, port, whether it's behind Cloudflare or a proxy, the server's version |
| An IP address | `public-ip-1`, `lan-ip-2`, `loopback`, `ipv6-public-1`, `ipv6-lan-1` | the port, and which kind of address it is |
| Any other domain or URL | `host-1`, `host-2`… (path and query dropped) | the scheme and the port |
| A user | `user-1`, `user-2`…; the signed-in user is `you` | nothing else |
| A server's or channel's name or id | `channel-1`… | the kind (text, voice) |
| The home folder | `~` | the rest of the path |

Kept as they are, because they help and identify nobody: device names (microphones, speakers, cameras, GPUs, monitors, for example "VoiceMeeter Input"), OS build numbers, error codes, codec and encoder names, ports.

The table that maps real values to aliases lives only in memory and is never written. Even the logs folder holds nothing to leak.

Two layers do the work:

1. **At the source.** Code that logs an address, a host, a user or a channel passes it through the alias table (`server-A` for the connection the app knows to be a server; `user-3` for an id) instead of writing it raw.
2. **A filter on every line before it reaches the disk,** for whatever the first layer missed, including third-party crates (reqwest, rustls, tungstenite, sqlx). It replaces:
   - IPv4 and IPv6 addresses;
   - URLs;
   - anything shaped like a domain name, except file names with extensions code uses (`.rs`, `.dart`, `.dll`, `.exe`, `.so`, `.dylib`, `.json`, `.db` and so on);
   - the home folder;
   - long runs of base64 or hex (tokens and keys) of 24 characters or more, except values the code marks as safe: event ids, hashes it writes on purpose.

   A test feeds it a corpus of real lines and known secrets and checks nothing leaks.

## The log file

- **Where:** a `logs` folder in the app's support folder, beside its data (`getApplicationSupportDirectory()/logs`).
- **Name:** `opencord-2026-10-30T14-32-05.log`, the start time in local time, so the newest sorts last.
- **Format:** plain UTF-8 text, one event per line, readable by a person and by Claude:
  ```
  14:32:07.412 +2.118s INFO  media::screen::windows  capture api opened  source=screen-1 size=2560x1440 api=wgc
  ```
  - The local wall-clock time, then the time since the session started.
  - The level and where the event came from: the Rust module, or `app::<area>` for Flutter.
  - The message, then its fields as `key=value`.
- **Levels kept:**
  - `INFO` and above everywhere;
  - `DEBUG` for Opencord's own screen share, video, voice and connection code;
  - `WARN` and above from third-party crates.
- **Size:** at most 20 MB per session. Past that, only `WARN` and above are kept, and the file says so once. So 10 sessions take at most about 200 MB.
- **Writing:**
  - Lines go through a channel to one writer thread, so no audio or video thread waits on the disk.
  - The writer flushes after every `WARN` or `ERROR`, and at least once a second otherwise.
  - On a clean quit it writes `session ended` and flushes.
- **Repeats:** an identical line repeated within a second is folded into `(repeated 37 times)`, so a failing loop doesn't bury the rest.

## The header

Written first, before anything can fail:

- the app version, the core version, the release track, and whether it was started by the launcher (D71);
- the OS and its exact build, for example "Windows 11 24H2 (26100.2314)" or "Windows 10 22H2 (19045.5011)", and the edition, since N editions lack media parts (D73);
- the CPU, the memory, and every GPU with its driver version;
- the monitors: count, resolution, scale and refresh rate, and which one is primary;
- the audio input and output devices, and the default of each;
- whether Windows.Graphics.Capture, borderless capture (D74) and process loopback (D72) are available;
- the number of servers, as aliases with their versions once they connect;
- whether the previous session ended cleanly.

## What gets logged

- **Rust core:** everything it already writes, which is now kept.
- **Flutter:**
  - uncaught errors with their stack traces (`FlutterError.onError`, `PlatformDispatcher.instance.onError`);
  - the existing `developer.log` calls, routed into the log;
  - one line per key user action: connecting to or leaving a server, joining or leaving voice, opening the share dialog, picking a source, starting and stopping a share, starting to watch someone's share, changing a device, changing a video or voice setting.
- **Panics in Rust:** a panic hook writes the message, the thread and a backtrace before the panic carries on.
- **Native crashes on Windows:** an unhandled-exception filter writes the exception code, the faulting module's name and the offset into it, then flushes. D72's crash (heap corruption) would have shown up as `0xC0000374 in ntdll.dll`.
  - macOS and Linux get the same through a signal handler that writes a fixed, preformatted line.
- **A session that didn't end cleanly:** the next launch notices that its last line isn't `session ended` and marks it.

### Screen sharing in detail

**A share starting:**

- the source kind (screen or window), its size and scale, its monitor, and for a window its process name (e.g. `chrome.exe`; no title, since titles can hold private text);
- each capture method tried, in order (Windows.Graphics.Capture, then GDI), and why each was given up, with the HRESULT;
- whether the border request was asked and its answer, with how long it took;
- the encoder chosen (hardware or software, its name and the GPU it runs on), its settings, and every encoder that was tried and refused, with the reason;
- the time to the first captured frame and to the first sent frame.

**While sharing, one line every 5 seconds:**

- frames captured, encoded, sent and dropped, with the reason for each drop;
- resolution, frame rate and bitrate;
- the encoder's time per frame;
- keyframes sent and keyframe requests received.

**Watching a share:**

- the stream picked up;
- the decoder chosen and any fallback;
- one line every 5 seconds with frames received, decoded and shown, decode errors, keyframe requests and packet loss.

**Every stop,** with its cause: the user stopped, the source closed, the window was minimised, an error, the session ended.

Voice and connecting get the same treatment, lighter: each step with its outcome and time, and the reason for every reconnect.

## In the app: Settings → About → Logs

- **Export this session's log:** opens a save dialog (`file_selector`) with the file name `opencord-log-2026-10-30-1432.txt`, then writes the current file there, flushed up to that moment. A toast confirms.
- **Export the last session's log:** shown only when the last session ended unexpectedly, with the subtitle "It ended unexpectedly". After a crash, the restart starts a new session, so this row is the way to send the one that crashed.
- **Open logs folder:** opens the folder in Explorer, Finder or the file manager.
- **Footer:** "Logs never contain passwords, keys, messages or addresses. The last 10 sessions are kept."

## Constraints

- Desktop only: Windows, macOS, Linux.
- The Rust side lives in `opencord-core`, in a `log` module, using `tracing-subscriber`, which the workspace already has. Its API to Flutter goes through flutter_rust_bridge like the rest (`api/system.rs`). It starts before `init` opens the data folder, because Flutter calls it first with the logs folder.
- No new crates beyond what the workspace has, except a small one for system information if one does the job (see below).
- The launcher's own `launcher.log` (D71) stays where it is; its last run is copied into the session's header area when the launcher started this session, home folder scrubbed.
- `.github/check.sh` passes; Windows-only code type-checks from Linux the way docs say (windows-lint-from-linux).
- The pull request records a decision, **D82** (D75–D80 are the phone apps on feat/mobile, D81 is reserved for their iOS review).

## Left to the builder

- How the header gets GPU, driver, monitor and edition details on each system: a small crate, or the system's own calls through the `windows` crate that's already there.
- The exact list of key user actions in Flutter, within the list above.
- How the alias table is shared between the source layer and the filter.
- The exact domain-name pattern and its list of allowed file extensions, as long as the leak test passes.

## Calls made by Claude, not the owner

- Logs stay on the user's machine; nothing uploads.
- 20 MB per session; `INFO`, with `DEBUG` for Opencord's own media and connection code; `WARN` from other crates.
- Window titles are left out (they can hold private text); the process name stays.
- Device names stay (they've mattered before: D72).
- The "last session" export row exists because a crash's restart is a new session.
- A native crash writes one line, not a crash dump.
- The file is plain text, not JSON: easier to read for both the owner and Claude.
