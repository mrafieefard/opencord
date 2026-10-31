# Session Logs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every desktop launch writes a private, aliased session log. The last 10 are kept. The user exports one from Settings → About, and Claude can diagnose a failure (screen sharing on Windows first) from that file alone.

**Architecture:**
- A `log` module in `opencord-core`:
  - installs a `tracing` subscriber;
  - formats each event as one line;
  - passes the line through an alias filter;
  - hands it to a writer thread that owns the file, the rotation and the size cap.
- Flutter starts it through flutter_rust_bridge before `core.init`, and feeds its own errors and key actions into it.
- Crash hooks (panic, Windows unhandled exception, Unix signals) write straight to the open file.
- `opencord-media` gains a `system` module for the header and more screen-share checkpoints.

**Tech Stack:** Rust (`tracing`, `tracing-subscriber`, `regex`, `windows` 0.62, `libc`), flutter_rust_bridge 2, Flutter (Riverpod, `file_selector`, `url_launcher`).

**Spec:** `docs/superpowers/specs/2026-10-30-session-logs-design.md`

## Global Constraints

- **Desktop only:** Windows, macOS, Linux. Under `flutter test` and the mock, logging is a no-op unless a test starts it.
- **Location and names:**
  - folder: `getApplicationSupportDirectory()/logs`, the base folder even when a profile is used;
  - session file: `opencord-YYYY-MM-DDTHH-MM-SS.log` (local time);
  - exported file: `opencord-log-YYYY-MM-DD-HHMM.txt`.
- **Retention:** keep 10 sessions, counting the new one.
- **Size:** 20 MB per session. Past it, only `WARN` and above are kept, with one line saying so.
- **Line format:** `HH:MM:SS.mmm +S.SSSs LEVEL target  message  key=value…`, with the level padded to 5.
- **Levels:** `INFO` and above by default; `DEBUG` for `opencord_media::video`, `opencord_media::share_audio`, `opencord_core::connection`, `opencord_core::voice`, `opencord_core::media`, `opencord_core::video`, `app`; `WARN` and above for crates outside `opencord`.
- **Flushing:** after every `WARN`/`ERROR`, and at least once a second otherwise.
- **Endings and repeats:**
  - a clean quit writes `session ended`;
  - an identical line within 1 s is folded into `(repeated N times)`.
- **Never on disk:** passwords, tokens, keys, invite codes, message or attachment text, file names, IPs, domains, URLs, the home folder, window titles.
- **About copy:**
  - section title: `Logs`;
  - rows: `Export this session's log`, `Export the last session's log` (subtitle `It ended unexpectedly`), `Open logs folder`;
  - footer: `Logs never contain passwords, keys, messages or addresses. The last 10 sessions are kept.`
- **Records and checks:**
  - the decision is **D82**;
  - every cargo/flutter command starts with `source .superpowers/sdd/media/env.sh &&`;
  - `.github/check.sh` passes before pushing.

## Review Focus

1. **A secret in a third-party crate's message:** a reqwest error printing `https://opencord2.example.com/api/x?token=…`. Expected: `https://host-1` or `server-A`, no token, no path. Task 2: `filter_scrubs_third_party_url_with_token`.
2. **A crash during a share:** a heap-corruption exception. Expected: the file ends with the exception line, and on the next launch "Export the last session's log" appears. Task 3: `crash_line_is_written_without_allocating`. Task 1: `unclean_previous_session_is_reported`.
3. **Disk full or folder not writable:** the app still starts, and logging becomes a no-op. Task 1: `start_in_unwritable_folder_returns_error_and_app_continues`; Task 5 catches the error.
4. **A failing loop writes thousands of lines a second:** repeats fold, and the cap holds at 20 MB. Task 1: `repeats_fold` and `cap_switches_to_warnings`.
5. **Hot restart or a second `start` call:** no second subscriber and no second file. Task 1: `second_start_returns_same_session`.

---

### Task 1: The session writer and the subscriber

**Files:**
- Create: `crates/opencord-core/src/log/mod.rs`, `crates/opencord-core/src/log/writer.rs`, `crates/opencord-core/src/log/format.rs`
- Modify: `crates/opencord-core/src/lib.rs` (add `pub mod log;`), `crates/opencord-core/Cargo.toml` (`tracing-subscriber.workspace = true`, `regex = "1"` with the version in Cargo.lock)
- Test: `#[cfg(test)]` in each file

**Interfaces:**
- Produces:
  - `pub struct Session { pub current: PathBuf, pub previous_crashed: Option<PathBuf> }`
  - `pub fn start(dir: &Path, header: &[(String, String)]) -> io::Result<Session>`, idempotent: a second call returns the first `Session`.
  - `pub fn write_app(level: Level, area: &str, message: &str)`, for Flutter; its target is `app::<area>`.
  - `pub fn flush()`
  - `pub fn end()`, which writes `session ended` and flushes.
  - `pub(crate) fn raw_handle() -> Option<RawFile>`, for Task 3.
  - `writer::Writer::new(file, cap_bytes)`, with `write_line(level, line)`, `flush()`, `bytes_written()`.

**Steps:**
- [ ] **Test the writer** (writer.rs, against tempfiles):
  - `rotation_keeps_ten`: 12 old files exist, then `start`; exactly 10 remain, the newest included.
  - `unclean_previous_session_is_reported`: the newest old file doesn't end with `session ended`, so `previous_crashed` is `Some` (and `None` when it does).
  - `cap_switches_to_warnings`: with `cap_bytes = 1000`, INFO lines stop after the cap, one line contains `size limit reached`, and WARN lines still arrive.
  - `repeats_fold`: the same line 5 times within 1 s writes 1 line plus `(repeated 4 times)`.
  - `start_in_unwritable_folder_returns_error_and_app_continues`: the path is a file, not a folder, and the call returns `Err`.
  - `second_start_returns_same_session`.
- [ ] **Run** `cargo test -p opencord_core log::`. Expect FAIL (no module).
- [ ] **Implement:**
  - `start` creates the folder, deletes the oldest `opencord-*.log` beyond 9, reads the previous file's last line, and opens the new file.
  - It writes the header block: `=== Opencord session log ===`, `key: value` lines, then `===`.
  - It spawns the writer thread, which receives lines over an `std::sync::mpsc` channel and flushes on WARN+ or after 1 s.
  - It installs a `tracing_subscriber::registry()` with a `Layer` whose `on_event` formats the line (format.rs) and sends it. Filter: `EnvFilter` built from the Global Constraints levels.
  - Format: local time from `time`, or from `libc::localtime_r` / `GetLocalTime` (whatever the `time` crate's `local-offset` gives without new crates). `+S.SSSs` comes from an `Instant` at start; fields are appended as ` key=value` through a `Visit` implementation.
- [ ] **Run** the tests. Expect PASS. Run `cargo clippy -p opencord_core --all-targets -- -D warnings`.
- [ ] **Commit:** `feat(core): session log files with rotation, a size cap and folded repeats`.

### Task 2: Aliases and the privacy filter

**Files:**
- Create: `crates/opencord-core/src/log/alias.rs`, `crates/opencord-core/src/log/scrub.rs`
- Modify: `log/mod.rs` (each line runs through `scrub::line` before it's sent to the writer); `connection.rs`, `client.rs` and the sites that log `server_key`, `address` or user ids (from `grep -rn 'server_key = \|%address\|user_id' src`): log `alias::server(..)`, `alias::address(..)`, `alias::user(..)`.
- Test: `crates/opencord-core/src/log/scrub.rs` tests, with the corpus `crates/opencord-core/src/log/testdata/leaks.txt`

**Interfaces:**
- Produces, all returning the alias and registering the real value for exact replacement:
  - `alias::server(host: &str) -> String`: `server-A`…`server-Z`, then `server-AA`.
  - `alias::server_facts(host: &str, facts: &str)`: records `(https, port 443, behind Cloudflare, version …)` once, as an INFO line.
  - `alias::address(ip: IpAddr) -> String`: `public-ip-N`, `lan-ip-N` (RFC 1918, CGNAT 100.64/10, link-local), `loopback`, `ipv6-public-N`, `ipv6-lan-N` (ULA, link-local).
  - `alias::user(id: i64) -> String`: `user-N`.
  - `alias::set_self(id: i64)`: that id becomes `you`.
  - `alias::channel(id: i64) -> String`: `channel-N`.
  - `scrub::line(text: &str) -> Cow<str>`.

**Steps:**
- [ ] **Test:**
  - `same_host_same_alias`;
  - `addresses_keep_kind_and_port`: `192.168.1.5:7712` gives `lan-ip-1:7712`, `8.8.8.8` gives `public-ip-1`, `[fd00::1]:443` gives `[ipv6-lan-1]:443`;
  - `filter_scrubs_third_party_url_with_token`: the line `error sending request for url (https://chat.example.org:8443/api/v1/x?token=abcd)` contains neither `example`, `token` nor `abcd`;
  - `registered_server_replaced_everywhere`: after `alias::server("opencord2.mamad.xyz")`, a line containing it reads `server-A`;
  - `home_folder_becomes_tilde`;
  - `file_names_survive`: `windows.rs`, `ntdll.dll`, `opencord.exe` and `0xC0000374` are unchanged;
  - `long_tokens_removed`: 24+ base64 or hex characters become `<secret>`, except after `event=` or `hash=`;
  - `corpus_has_no_leaks`: every line of `leaks.txt` is scrubbed, then checked for the secret list at its top.
- [ ] **Run.** Expect FAIL.
- [ ] **Implement:**
  - The alias table is a `Mutex<Aliases>` in a `OnceLock`.
  - `scrub::line` does exact replacements first, longest first. Then regexes in this order: URL (`[a-z][a-z0-9+.-]*://[^\s"'<>)]+`, with the host aliased, path and query dropped), IPv6, IPv4, then domain (`(?i)\b(?:[a-z0-9-]+\.)+[a-z]{2,}\b`, unless the last label is an allowed extension: `rs dart dll exe so dylib json db log txt toml yaml png jpg wav mp3 ogg`). The home folder (`dirs` via `std::env::home_dir()`) becomes `~`. Last, long secrets.
  - Wrap the call sites listed above.
- [ ] **Run.** Expect PASS. Run clippy.
- [ ] **Commit:** `feat(core): aliases and a privacy filter for session logs`.

### Task 3: Panics and native crashes

**Files:**
- Create: `crates/opencord-core/src/log/crash.rs`, with `#[cfg(windows)]` and `#[cfg(unix)]` parts
- Modify: `crates/opencord-core/Cargo.toml` (`libc.workspace = true` for unix if it's not there; the `windows` 0.62 features `Win32_System_Diagnostics_Debug`, `Win32_System_LibraryLoader`, `Win32_Storage_FileSystem`, `Win32_Foundation`, `Win32_System_Kernel` in the core's windows section)
- Test: `crash.rs` tests

**Interfaces:**
- Consumes: `log::raw_handle()`.
- Produces: `crash::install()`, called by `log::start` once.
  - The panic hook calls the previous hook after writing: `ERROR panic  thread=<name> location=<file:line> message=<…>` plus `Backtrace::force_capture()` lines, sent through the writer and then flushed.
  - Native crashes write one preformatted line with `write`/`WriteFile` on the raw handle, without allocating:
    - Windows: `FATAL native crash code=0xC0000374 module=ntdll.dll offset=0x1234`;
    - Unix: `FATAL native crash signal=11`.
  - Each then chains to the previous filter or handler.
- `crash::format_native(buf: &mut [u8; 256], code: u32, module: &[u8], offset: usize) -> usize` is pure, for the test.

**Steps:**
- [ ] **Test:**
  - `crash_line_is_written_without_allocating`: `format_native` gives the exact expected bytes for `0xC0000374`, `ntdll.dll`, `0x1234`;
  - `panic_is_logged`: start a session, `catch_unwind(|| panic!("boom"))`, flush; the file contains `panic` and `boom`.
- [ ] **Run.** Expect FAIL.
- [ ] **Implement:**
  - Windows: `SetUnhandledExceptionFilter`. Get the module with `GetModuleHandleExW(FROM_ADDRESS | UNCHANGED_REFCOUNT)`, then `GetModuleFileNameW`, keeping the base name as ASCII in a stack buffer.
  - Unix: `sigaction` for `SIGSEGV`, `SIGBUS`, `SIGILL`, `SIGFPE`, `SIGABRT` with `SA_SIGINFO | SA_ONSTACK`, keeping the old actions and re-raising through them.
- [ ] **Run.** Expect PASS. Type-check the Windows part from Linux as docs describe (the windows-lint scratch crate), then run clippy.
- [ ] **Commit:** `feat(core): panics and native crashes end up in the session log`.

### Task 4: The header

**Files:**
- Create: `crates/opencord-media/src/system/mod.rs`, `system/windows.rs`, `system/linux.rs`, `system/macos.rs`
- Modify: `crates/opencord-media/src/lib.rs` (`pub mod system;`); media's windows features (`Win32_System_Registry`, `Win32_System_SystemInformation`, `Win32_Graphics_Dxgi` if missing)
- Test: `system/mod.rs` tests

**Interfaces:**
- Produces `opencord_media::system::facts() -> Vec<(String, String)>`, with these keys:
  - `os` (for example `Windows 11 Pro 24H2 (26100.2314)`), `cpu`, `memory` (`15.8 GB`);
  - `gpu 1`, `gpu 2`… (for example `NVIDIA GeForce RTX 3060, driver 32.0.15.6094`);
  - `capture api` (`yes` / `no`), `borderless capture` (`yes` / `no` / `unknown`), `process loopback` (`yes` / `no`). These three are Windows only and come from the existing checks in `video/screen/windows.rs` and `share_audio/wasapi.rs`; expose them `pub(crate)` if they're private.
  - `audio inputs`, `audio outputs`: names joined with `; `, the default marked `(default)`, from `audio::device::devices()`.
- How each system gets these:
  - **Windows:** `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion` (ProductName, EditionID, DisplayVersion, CurrentBuild, UBR). Windows 11 is build ≥ 22000 even where ProductName says 10. GPUs from `IDXGIFactory1::EnumAdapters1` with `CheckInterfaceSupport` for the driver version, skipping the Basic Render Driver. CPU and memory from the registry and `GlobalMemoryStatusEx`.
  - **Linux:** `/etc/os-release` `PRETTY_NAME` and `uname -r` from `libc::uname`; `/proc/cpuinfo` model name and `/proc/meminfo`; GPUs from `/sys/class/drm/card*/device/{vendor,device}` and the driver name from `uevent`.
  - **macOS:** `sysctl` `kern.osproductversion`, `kern.osversion`, `machdep.cpu.brand_string`, `hw.memsize`. GPU from `MTLCopyAllDevices` is too heavy, so it's `gpu 1: Apple (integrated)` on arm64 and is otherwise omitted.

**Steps:**
- [ ] **Test** `facts_have_os_cpu_memory`: the keys `os`, `cpu` and `memory` exist and aren't empty on the host.
- [ ] **Run.** Expect FAIL.
- [ ] **Implement.**
- [ ] **Run.** Expect PASS. Type-check Windows and run clippy.
- [ ] **Commit:** `feat(media): describe the machine for the session log's header`.

### Task 5: Bridge API and Flutter wiring

**Files:**
- Create: `crates/opencord-core/src/api/logs.rs`, `app/lib/core/logs/app_log.dart`, `app/lib/core/logs/session_log.dart`
- Modify:
  - `crates/opencord-core/src/api/mod.rs`;
  - the generated `app/lib/src/rust/**` and `crates/opencord-core/src/frb_generated.rs` (regenerated);
  - `app/lib/app_start.dart`, `app/lib/main.dart`;
  - every `developer.log(` in `app/lib`, which becomes `appLog(...)`;
  - the Flutter call sites for key actions, found through the repository methods: `addServer`, `removeServer`, `joinVoice`, `leaveVoice`, `startScreenShare`/`stopScreenShare`, `watchStream`/`unwatchStream`, device changes, voice and video setting changes. Use `grep -n "Future<void> \(joinVoice\|leaveVoice\|startShare\|stopShare\)" app/lib/core/rust/rust_repository.dart` to find the exact names.
- Test:
  - `app/test/core/logs/app_log_test.dart`;
  - Rust `api::logs` tests.

**Interfaces:**
- Rust API, through frb:
  - `#[frb(sync)] pub fn session_log_start(dir: String, facts: Vec<LogFact>) -> Result<SessionLogFiles, CoreError>`, which adds `opencord_media::system::facts()` itself. `LogFact { key: String, value: String }`; `SessionLogFiles { current: String, previous_crashed: Option<String> }`.
  - `#[frb(sync)] pub fn session_log_write(level: LogLevel, area: String, message: String)`, where `LogLevel { Debug, Info, Warn, Error }`.
  - `pub fn session_log_export(source: String, destination: String) -> Result<(), CoreError>`: it flushes first when `source` is the current file.
  - `#[frb(sync)] pub fn session_log_end()`.
- Dart:
  - `void appLog(String message, {required String name, Object? error, StackTrace? stackTrace, LogLevel level = LogLevel.info})`. It also calls `developer.log`, and writes error and stack as extra lines.
  - `SessionLog`, a provider value with `current`, `previousCrashed` and `logsDir` (all `String?`), and `sessionLogProvider` (default: all null).

**Steps:**
- [ ] **Test:**
  - Rust `export_copies_flushed_file`.
  - Dart `appLog_forwards_to_sink`: `appLogSink` is a replaceable `void Function(LogLevel, String, String)?` so tests can capture without Rust. The test checks that the area, the message and the error with the first stack frame arrive.
- [ ] **Run.** Expect FAIL.
- [ ] **Implement:**
  - Run `flutter_rust_bridge_codegen generate` from `app/`.
  - In `startApp`, after `RustLib.init()`, and on desktop only when not under test, call `session_log_start(<support dir>/logs, facts)`:
    - the facts are `app version`, `track`, `launched by launcher`, `profile` (`yes`/`no`), and `display N` from `PlatformDispatcher.instance.displays` (size, devicePixelRatio, refreshRate);
    - when `installation?.launcher` exists, also `launcher log`, the last 40 lines of the launcher's log file, whose path comes from the launcher layout (read `Installation` for it);
    - on error, `developer.log` it and carry on.
  - Set `FlutterError.onError` and `PlatformDispatcher.instance.onError` to `appLog` at error level, chaining the previous handlers.
  - Route the existing `developer.log` calls.
  - Add one `appLog` line per key action, with ids passed as-is (the core aliases ids it knows; the filter catches the rest).
  - On quit (the window's quit path), call `session_log_end`.
  - Give the alias table the ids:
    - `alias::set_self` when the session signs in (in `client.rs`, where the self user is known);
    - `alias::server` when a server is added or connected (in `connection.rs`).
- [ ] **Run** the Dart tests, the Rust tests and `flutter analyze`. Expect PASS.
- [ ] **Commit:** `feat(app): start the session log, and send Flutter errors and key actions to it`.

### Task 6: About → Logs

**Files:**
- Modify: `app/lib/features/settings/user/app_pages.dart` (a `_LogsSection` in `AboutPage`, shown on desktop when `sessionLogProvider.current != null`)
- Test: `app/test/features/settings/about_logs_test.dart`

**Interfaces:**
- Consumes: `sessionLogProvider` and `session_log_export` (behind a `SessionLogActions` class with `export(source, destination)` and `openFolder(dir)`, provided by `sessionLogActionsProvider` so tests can fake it).

**Steps:**
- [ ] **Test:**
  - `shows_export_and_open_rows`;
  - `last_session_row_only_when_crashed`;
  - `export_uses_suggested_name`: the fake save location gets `opencord-log-2026-10-30-1432.txt` for a session started at 14:32 on 2026-10-30, and a toast `Log saved` appears;
  - `open_folder_calls_actions`;
  - `footer_copy`.
- [ ] **Run.** Expect FAIL.
- [ ] **Implement:**
  - Export: `getSaveLocation(suggestedName: …)` from `file_selector`, then `export`.
  - Open folder: `launchUrl(Uri.file(dir))`.
  - Use the existing `SettingsSection` and `SettingsRow` with an `OcButton` trailing, as the Updates section does.
  - On failure, a toast with the error's message.
- [ ] **Run.** Expect PASS, and `flutter analyze` clean.
- [ ] **Commit:** `feat(app): export the session log and open the logs folder from About`.

### Task 7: Screen sharing and voice checkpoints

**Files:**
- Modify:
  - `crates/opencord-media/src/video/screen/windows.rs`, `windows_gdi_capture.rs`, `capture.rs`;
  - `video/sender.rs`, `video/receiver.rs`, `video/codec/encoder.rs`, `video/codec/decoder.rs`;
  - `crates/opencord-core/src/voice.rs` and `connection.rs` for step lines.
- Test: `video/sender.rs` tests (stats), `video/screen/capture.rs` tests (stop cause)

**Interfaces:**
- Produces:
  - `struct ShareStats { captured, encoded, sent, dropped_by_reason: [(&'static str, u32)], keyframes, keyframe_requests, encode_ms_avg }`;
  - `ShareStats::line(&self, elapsed) -> String`, logged at INFO every 5 s from the sender's loop;
  - `ReceiveStats`, the same for the receiver (received, decoded, shown, decode_errors, keyframe_requests, lost).

**Lines to add (target, level, message, fields):**
- **Share start:**
  - `INFO share starting kind=screen|window size=WxH scale= monitor=N process=<exe>`. The process name comes from `GetWindowThreadProcessId` + `QueryFullProcessImageNameW`, base name only, never a title.
- **Each capture method:**
  - `INFO capture method tried method=wgc|gdi result=ok|refused hresult=0x… after_ms=`;
  - the existing "using GDI" lines gain `hresult` and the reason (`not present`, `refused`, `no frame in 3 s`);
  - border: `INFO capture border asked allowed= after_ms=`.
- **Encoders:**
  - `Encoder::open` logs each backend tried: `DEBUG encoder tried backend= result=ok|<error>`;
  - then `INFO encoder chosen backend= hardware= gpu= size= fps= bitrate=`.
- **First frames:** `INFO first frame captured after_ms=`, `INFO first frame sent after_ms=`.
- **Receiving:** `INFO watching stream=<alias user> decoder= hardware=`, and decoder fallback at WARN.
- **Every stop:** `INFO share stopped cause=user|source closed|minimised|error:<…>|session ended duration_s=`.
- **Voice and connection:**
  - `INFO connecting server=<alias> step=dns|tls|websocket|auth result= after_ms=`;
  - `INFO voice joining channel=<alias>` …;
  - `INFO voice connected transport=udp|tcp remote=<alias address>`;
  - `WARN reconnecting reason=`.

**Steps:**
- [ ] **Test:**
  - `stats_line_counts_and_drops`: feed `ShareStats` 10 captured, 8 sent, 2 dropped as `encoder busy`; the line contains `captured=10 sent=8 dropped=2 (encoder busy 2)`.
  - `stop_cause_is_reported`: the existing capture test harness ends a share by source loss, and the captured log (through a test subscriber, `tracing::subscriber::with_default`) contains `cause=source closed`.
- [ ] **Run.** Expect FAIL.
- [ ] **Implement** the lines above.
- [ ] **Run.** Expect PASS. Type-check Windows and macOS from Linux and run clippy for the media crate.
- [ ] **Commit:** `feat(media): screen share, voice and connection checkpoints in the session log`.

### Task 8: Docs, checks, decision

**Files:**
- Modify: `docs/decisions.md` (D82), `docs/plans/desktop-ui.md` §8.1 About (the Logs section), `GEMINI.md` (a rule: log through `tracing`/`appLog`, alias addresses and ids, never log secrets or text)
- Create: `docs/logs.md` (what's in a log, where it lives, how to read one)

**Steps:**
- [ ] Write D82: why, what was decided, the privacy rules, the calls made without the owner (from the spec), the checks, and what's owed by hand (a share on Windows 10 and 11 with the log read back).
- [ ] **Run** `df -h /`, then `source .superpowers/sdd/media/env.sh && .github/check.sh`. Expect all checks to pass.
- [ ] **By hand on Linux:**
  - run the app, share a screen, quit;
  - read the log for its header, the share lines, `session ended`, and no IPs or domains (`grep -E '[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+|\.(com|org|xyz|net)'` is empty);
  - kill the app with `kill -SEGV` and check the crash line and the "last session" row.
- [ ] **Commit:** `docs: D82 session logs, docs/logs.md, the About page's Logs section`.
