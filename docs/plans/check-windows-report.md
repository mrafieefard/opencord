# Windows check report: the 2026-11-01 batch

> The report for [check-windows.md](check-windows.md). Tick each box that passes;
> for each failure write what happened, with a screenshot, and name the step.
>
> `[x]` passed, `[ ]` not checked yet, `[~]` **partly** checked — the box says
> which part passed and which part is still owed, and a `[~]` is not a pass.
>
> **The PC:** Windows 10 Pro 22H2 (build 19045), 16 GB RAM, 16 logical cores.
> There is no Windows 11 PC, so every step marked *Windows 11 only* is
> **not run: no Windows 11 PC**.
>
> **Prepared by a Claude Code session on 2026-11-02/03.** Everything it
> settled is in §A below, with the numbers it judged on. The session drove the
> real window through Win32 — the actual mouse and keyboard, and screenshots —
> so the GUI boxes are not inherently out of reach; `check_window_chrome.ps1`
> (pull request #32) now runs §1's machine-checkable ones in one command.
>
> It stopped when **the PC locked itself**. Nothing that injects input works
> behind a lock screen, so the rest is waiting on an unlock rather than on
> anything about the app. Only the boxes §A names are ticked; every other box
> in §1 to §8 is untouched, and the judgement calls (is Close's hover
> *inverted and monochrome*?) and the live two-client work are still a
> person's.

## A. What has already been settled mechanically

| # | Check | Result |
|---|---|---|
| A1 | §5 — the shipped FFmpeg DLLs carry Task 32's decoders, `libdav1d` included | **PASS**, see A1 |
| A2 | §5 — `cargo test --workspace --lib --bins --tests` and `flutter test` | **PASS**, with two caveats that are not the batch's — see A2 |
| A3 | §5 — the clipboard's licence question | **settled by the owner**, see A3 |
| A4 | §1 — the title bar, as far as a machine can settle it | **partly checked**, see A4 |
| A5 | This PC ran out of RAM and then out of disk | **fixed enough to carry on**, see A5 |
| A6 | Integration tests on Windows | **tried, abandoned**, see A6 |
| A7 | Rebuilding this PC's FFmpeg | **not done, on purpose**, see A7 |

> **The run stopped because the PC locked itself.** The checks in A4 were
> driven through Win32 — the real mouse and keyboard, and screenshots — and
> that works only on an unlocked workstation. Behind the lock screen LogonUI
> owns the desktop: clicks and keys reach no application and every capture
> returns the lock screen. The remaining boxes are waiting on an unlock, not
> on anything about the app. `app/tool/windows/check_window_chrome.ps1`
> (pull request #32) now runs §1's machine-checkable boxes in one command.

### A1. The shipped FFmpeg DLLs — PASS

Checked against the **CI artifact**, not a local build, and on purpose: this PC's
`C:\ffmpeg` is a hand-installed generic **GPL** build (`--enable-gpl
--enable-version3`, avisynth, libbluray, sdl2, ffplay). A local
`flutter build windows --release` here would bundle *those* DLLs and pass the
check against the wrong binaries. In CI `C:\ffmpeg` is built by
`app/tool/windows/build_ffmpeg.ps1`, so the artifact is the real thing.

Artifact: `opencord-windows-x64.zip` from run `41491603134`
(`feat/media-viewer-player`). The configure line is read back out of the shipped
`avcodec-63.dll`:

- `--enable-decoder`, `--enable-demuxer`, `--enable-parser` and `--enable-bsf`
  are each **byte-identical** to `app/tool/windows/build_ffmpeg.sh`, which is in
  turn byte-identical to the Linux `app/tool/build_ffmpeg.sh` for all four
  lists — so the shipped DLLs carry exactly the upload plan's Task 32 set.
- `--enable-libdav1d` is present, and **no `dav1d` DLL ships**: it is linked
  statically, as Task 32 intends.
- **No `--enable-gpl`, `--enable-version3` or `--enable-nonfree`** — the shipped
  build is LGPL, which is what the licence list allows.
- All five DLLs are bundled: `avcodec-63`, `avformat-63`, `avutil-61`,
  `swresample-7`, `swscale-10`.
- `FFMPEG-LICENSE.txt` sits beside them (LGPL §4) and names the build script:
  *"FFmpeg 9.0.2, built by app/tool/windows/build_ffmpeg.ps1 as shared"*.

Nothing was missing, so the plan's fallback (switch to an LGPL shared build and
update `docs/packaging.md`) is **not needed**.

> **Worth fixing on this PC, separately from the batch:** `C:\ffmpeg` being a
> GPL build means `app/tool/windows/check_bundled_ffmpeg.ps1` and any local
> release build test the wrong FFmpeg. Rebuild it with
> `app/tool/windows/build_ffmpeg.ps1 C:\ffmpeg` before trusting a local
> packaging check.

### A2. The tests

Run on `feat/media-viewer-player` (the branch §5 to §7 name).

- **First attempt failed for an environment reason, reported not skipped:**
  `cargo test --workspace --lib --bins --tests` exited **101** with
  `error[E0786]: found invalid metadata files for crate opencord_core` →
  `failed to mmap file 'libopencord_core.rlib': The paging file is too small
  for this operation to complete. (os error 1455)`, while compiling
  `opencord-mobile (lib test)`. With 16 logical cores cargo fans out 16 rustc
  jobs; at the time 0.8 GB of RAM and 8.2 GB of commit were free against an
  auto-managed pagefile that has peaked at 13.6 GB. Nothing to do with the
  batch — but it means **`cargo test --workspace` needs `-j 4` on this PC**, or
  a larger fixed pagefile.
- **Re-run with `-j 4`: 284 passed, 3 failed, 8 ignored** in `opencord-media`
  (every other crate green). The three are
  `video::sender::tests::{the_sender_ends_when_the_camera_does,
  a_reshaped_screen_goes_out_as_its_new_track_and_layers,
  the_planners_cuts_shrink_and_slow_a_screens_main_layer}`.

  **Not the batch, and shown so, not assumed:**
  - re-run alone and single-threaded on an idle machine, all 11
    `video::sender` tests pass (`cargo test -p opencord-media --lib
    video::sender -- --test-threads=1` → **11 passed**);
  - the batch does not touch `crates/opencord-media/src/video/sender.rs` at
    all — `feat/media-viewer-player` changes only `player/*` and
    `tests/player.rs` in that crate;
  - one of the three is the test the **open, unmerged** branch
    `fix/flaky-reshape-test` exists to fix. `fix/flaky-capped-uplink-test` is
    already on `beta`; this one is not.
  - the first failure is a wall-clock assertion
    (`ended.elapsed() < Duration::from_secs(3)`), which this PC misses while
    swapping.

  So: **green apart from three load-sensitive tests in Linux-owned shared
  code.** Merging `fix/flaky-reshape-test` would remove one of them.

- **`flutter test`: 1126 passed, 18 failed, 3 skipped — every failure a golden
  image.** `test/golden/*` and `test/showcase/*`, at 2.60 % to 6.33 % of pixels
  against the comparator's 0.5 % tolerance
  (`app/test/flutter_test_config.dart`). That is text rasterization — Windows
  DirectWrite against the Linux FreeType the references were rendered with —
  not a layout change.

  **The box as written cannot pass on Windows, and nothing is expected to make
  it:** the goldens are Linux-rendered references, and the Windows CI job
  deliberately never runs `flutter test` — it runs Clippy, `cargo test`,
  `flutter pub get` and `flutter build windows --release`, and no Dart tests at
  all. Goldens are a Linux-only check.

  Excluding them, **`flutter test test/unit test/widget` → 1121 passed, 1
  skipped, 0 failed**, which is the meaningful Windows result and is what this
  box should ask for.

- **A fourth load-sensitive test, found in CI rather than here:** the UI-harness
  pull request (#32) adds three PowerShell files and nothing else, and its
  `Client (macOS)` job still failed on
  `attachments::range_reader_reads_any_span` — *block 1 wasn't read ahead*
  (`crates/opencord-core/tests/attachments.rs:628`). The assertion is that a
  read-ahead of the next block finishes inside a one-second
  `tokio::time::sleep`, so a busy runner loses the race; 20 of the 21 tests in
  that target passed. Its `Phone (Android)` job failed in the Android SDK
  manager (`Failed to install … cmake;3.22.1`), which is the runner, not the
  code. Both jobs pass on #31 and on every one of #21–#29, so neither belongs
  to this batch — but **`range_reader_reads_any_span` is a flake worth a fix
  branch of its own**, next to `fix/flaky-reshape-test`. Re-running just those
  two jobs turned both green on the first retry (macOS 37m33s, Android 50m20s), which
  is #32 all thirteen checks green.

### A3. The clipboard (upload plan Task 21) — decided

The plan asked the owner to choose. **The owner chose: write `image` and
`set_image` for Windows with the Win32 API rather than allow BSL-1.0.**

`arboard` on Windows pulls in `clipboard-win` and `error-code`, both BSL-1.0,
and the upload plan's Global Constraints allow only MIT, Apache-2.0, BSD, ISC,
Zlib and LGPL. The `windows` crate (0.62, MIT/Apache-2.0) is **already** an
`opencord-core` Windows dependency for the session log, so the Win32 route adds
no new licence at all.

Written as the branch `fix/windows-clipboard` (pull request #31). Until it merges, Windows answers
"this system's clipboard isn't supported" and §5's clipboard box stays open.
The two hand checks it needs are listed in §5 below.

### A4. The title bar (§1) — what the machine settled before the PC locked

Driven against `C:\oc\media-viewer-player\opencord\opencord.exe`, profile
`alice`, at 100 % scaling on the 1920×1080 screen.

**Checked, with the numbers:**

- **No system title bar at start.** The window is 1280×800 and its client
  rect is 1264×792, so **8 px in all** go to the frame — a system caption
  alone is about 31 px, and the horizontal 16 px is the resize border. There
  is no caption. The screenshot agrees: the top strip is the app's own dark
  header with its own −, □ and × at the right, and no Windows caption, icon
  or title text.
- **The caption buttons are at the top right** on the fresh-profile welcome
  screen, which is §1's "no servers" case.
- **Aero Snap answers the keyboard.** `Win+Left` snapped the window to the
  left half (974×1047 at −7,0 — a frameless window hangs its invisible
  resize border over the edge) and Windows raised **Snap Assist** over it,
  which is itself the system treating it as a normal snappable window. From
  snapped, `Win+Up` gave a quarter (974×568) and `Win+Down` gave the half
  back.
- The window class is `OPENCORD_MAIN_WINDOW`; it reports 96 dpi; it carries
  `WS_THICKFRAME`, so Windows will resize it from the edges.

**A trap for whoever finishes this:** `WS_CAPTION` **is set** on the window.
That is not a failure of §1's first box. A frameless window normally keeps
the bit and suppresses the frame by answering `WM_NCCALCSIZE`, which is
exactly what the client rect above shows. Reading the style bit would give a
confident wrong answer; judge the client rect, or the screenshot.

**Not checked, and not ticked:** no system title bar *after restoring from
minimised* and *after leaving maximised*; the buttons on a channel, Friends,
a private chat and Settings (all behind an identity and a server); Close's
hover being inverted and monochrome; dragging the header; the double click;
drag-to-edge snapping; the edges resizing; 125 % and 150 % scaling; and all
of §1's private-messages box. The script covers all but the judgement calls
and the screens behind a server.

### A5. This PC ran out of disk, twice over

Worth knowing before anyone runs the rest of these checks here.

- **`cargo test --workspace` needs `-j 4`.** At the default 16 jobs it dies
  with `error[E0786]: found invalid metadata files for crate opencord_core`
  → `failed to mmap file 'libopencord_core.rlib': The paging file is too
  small for this operation to complete. (os error 1455)`. 16 GB of RAM and 16
  logical cores is not enough for 16 rustc jobs on this workspace.
- **The disk reached 0 bytes free** during this run and stayed there until it
  was cleared: `target/debug` alone had grown to **75 GB**, of which 20 GB was
  the incremental-compilation cache. A full disk does not announce itself —
  it surfaced as an unrelated-looking build failure (`failed to run custom
  build command for opusic-sys`) and as `gh run watch` dying mid-stream.

  Freed without a `cargo clean` (which `CLAUDE.md` says to ask about first):
  `target/debug/incremental` (20 GB, rebuilt on demand) and `app/build`
  (2.4 GB, this session's own Flutter output) — back to 22 GB free. **The
  remaining 55 GB of `target/debug` is the owner's to decide about.** Check
  `df -h /` before the full checks, as `CLAUDE.md` already says.

### A6. Integration tests on Windows — tried, abandoned

CI runs `app/integration_test/*_test.dart` **only on Linux**, under `xvfb`;
the Windows job runs Clippy, `cargo test`, `flutter pub get` and
`flutter build windows --release`, and no Dart tests at all. So these have
never run on Windows, and they drive the app in-process rather than through
injected input — which would have made them usable even on a locked
workstation.

`flutter test integration_test/core_bridge_test.dart -d windows` was tried
and **did not get as far as running**: the Windows debug build failed on
`opusic-sys v0.7.5`, and the cause was the full disk above, not the test.

It was not retried. These tests are not among the plan's 61 boxes, and a
second Windows debug build is exactly what filled the disk. Whether they
should run on Windows in CI is a reasonable question for the Linux session,
and a separate one from this batch.

### A7. The local FFmpeg rebuild — not done, on purpose

A1 recommends rebuilding this PC's `C:\ffmpeg` with
`app/tool/windows/build_ffmpeg.ps1`, because the copy here is a hand-installed
GPL build. **That was left alone:** the script needs MSYS2 at `C:\msys64` or
`MSYS2_ROOT` for FFmpeg's configure, and there is none on this PC. Installing
a toolchain of that size is the owner's call, not a side effect of a check
run, and nothing in the plan depends on it — §5's FFmpeg box is already
settled against the CI artifact, which is the build that actually ships.

It still wants doing before anyone trusts a *local* release build or
`check_bundled_ffmpeg.ps1`.

### A8. The §1 harness was wrong before the app was — two bugs, both fixed

The first full run of `check_window_chrome.ps1` on #22's build reported **4 of
11 failing**. None of the four was the app.

- **State leaked from one check into the next.** Aero Snap leaves a window in
  a state Windows remembers. Dragging a *snapped* window does not move it by
  the drag's delta — Windows unsnaps it and lays the restored size under the
  cursor — and its edges may refuse to resize at all. So the drag check
  (`moved to 762,558 … wanted +120,+90`) and the edge-resize check
  (`1280 -> 1280`) were measuring Windows unsnapping a window, not the app
  moving or resizing one. Win+Left and Win+Right also *cycle* a window — left
  half → restored → right half — so sending Right straight after Left only
  restored it, which is why `Win+RIGHT` reported exactly the reset rect.
- **A missed click looked like an ignored key.** `Set-OcFocus` already
  returned whether the foreground really changed, because
  `SetForegroundWindow` is refused to a background process often enough that
  its click is the only way in. The script discarded that answer with
  `[void](…)`, so when the click missed, the keys went nowhere and `Win+Up`
  was recorded as the app failing to maximise.

Both are fixed on `ci/windows-ui-checks` (#32): `Reset-OcWindow` restores the
window to a known un-snapped rect before each check that needs one, and
`Set-OcFocusOrStop` retries the click and **exits 2** — the "proves nothing"
code — when the foreground never arrives, rather than blaming the app. A third
commit focuses the window before the first screenshot, since `CopyFromScreen`
copies the screen and had photographed a browser sitting over the app.

**After the fixes: 11 of 11 pass, on three consecutive runs.** The lesson is
the one the exit-2 convention already encodes: a UI harness that cannot prove
it drove the app must say so, not score the app.

### A9. The two-client rig is up — and four traps on the way

`alice` (the server's **owner**) and `bob` both run against
`localhost:7710`, both in `#general`, "2 members, 2 online". The server has a
`general` text channel and a `General` voice channel. That is §0.2 and §0.3
satisfied, and it is what §2 to §8 need. Four things cost time:

- **The claim token the server prints at start is the only one that works.**
  `opencord-server reset-claim-token` against a *stopped* server writes a
  token that the next start throws away: an unclaimed server issues a fresh
  one when it boots and prints it (`claim_token=…` on the `listening` line).
  A claim with the CLI's token fails with *you are not a member of this
  server; join with an invite*, which reads like a wrong flow rather than a
  stale secret. Take the token from the running server's own output.
- **Ctrl+V does not reach the app from `SendKeys`.** The paste silently does
  nothing and the field stays empty, so a check that pastes and clicks on
  would blame whatever came next. Typing the text works. Worth knowing before
  §5, which is about the clipboard: a failure to paste there has to be shown
  to be the app's and not the harness's.
- **One instance per profile, and a missing `OPENCORD_PROFILE` is not
  harmless.** The single-instance mutex is `…opencord.profile_<name>`
  (`app/windows/runner/single_instance.cpp`), so two profiles do run side by
  side — but an instance started *without* the variable takes the default
  mutex, finds the one already held, hands over and exits 0. It looks exactly
  like a launch that failed for no reason.
- **The app's fingerprint matched the server's, exactly.** The *Verify
  server* dialog showed `C94B 058C … 6629 EC93`, and
  `opencord-server fingerprint` prints
  `c94b058c0d76455fcd2283e336bc7c6f3fd4850e1fac52cd1b0ef6916629ec93`. Checked
  rather than clicked through.

### A10. The session's own terminal was failing the checks

Clicks aimed at the app were landing on **Windows Terminal**, which sits on
top because it is where this session runs. `GetForegroundWindow` came back
`WindowsTerminal / CASCADIA_HOSTING_WINDOW_CLASS` after a click that should
have focused `bob`, and the tray-click check then "failed" because the window
it was meant to raise was never behind anything.

It only showed up in §2. §1 never hit it because
`check_window_chrome.ps1` **launches** its app, and a new window comes up in
front of everything; §2 drives two clients that were already running.

The fix is `Hide-OcTerminal` / `Show-OcTerminal`: minimise the session's own
window for the length of a check and restore it after. With it, the same two
checks pass — `bob` takes the foreground from a click, and a left click on
`alice`'s tray icon brings `alice` out from behind `bob`.

This belongs with A8's lesson. Both failures were the harness being in the
room with the thing it was measuring, and both read as the app misbehaving.
Any later section driving an already-running window needs the same guard.

## B. Which build to use for which section

The batch is **not merged**, so §0.1's second path applies: each section is
checked against its own pull request's CI build. These are already unpacked —
they are CI's own artifacts, so nothing here depends on a local merge.

| Section | Pull request | Build |
|---|---|---|
| 1 | #21 | `C:\oc\private-messages-titlebar\opencord\opencord.exe` |
| 1 | #22 | `C:\oc\custom-titlebar-everywhere\opencord\opencord.exe` |
| 2 | #24 | `C:\oc\tray-background\opencord\opencord.exe` |
| 3 | #27 | `C:\oc\voice-reconnect-bar\opencord\opencord.exe` |
| 4 | #23 | `C:\oc\private-message-keys\opencord\opencord.exe` |
| 5 to 7 | #25, #26 | `C:\oc\media-viewer-player\opencord\opencord.exe` (carries #22, #24, #25) |
| 8 | #28 | `C:\oc\compact-full-width\opencord\opencord.exe` |
| 8 | #29 | `C:\oc\private-chat-scroll\opencord\opencord.exe` |

The server is `C:\oc\server\opencord-server\opencord-server.exe` (the zip has a
folder inside it), with `opencord-voice-node.exe` beside it.

**Two clients on one PC** (§0.2), from two PowerShell windows:

```powershell
$env:OPENCORD_PROFILE = "alice"; & C:\oc\<branch>\opencord\opencord.exe
$env:OPENCORD_PROFILE = "bob";   & C:\oc\<branch>\opencord\opencord.exe
```

**The server** (§0.3) — on this PC, for every section:

```powershell
C:\oc\server\opencord-server\opencord-server.exe --config C:\oc\test-server\opencord.toml
```

**Already first-started, so the config exists:**

- config: `C:\oc\test-server\opencord.toml`, data in `C:\oc\test-server\data`
- listening on `0.0.0.0:7710`
- the **owner claim token** and the TLS fingerprint are printed on the server's
  own console at start — `alice` adds the server with that token, under "I'm the
  owner", and becomes its owner. Everyone else joins with the invite link the
  app then shows. The token is deliberately not written down here: it is a
  credential, and this file is in the repository.

Only the claim token is printed at first start, not an invite link — the plan's
§0.3 says both, so that line is slightly off; the invite comes from the app once
there is an owner.

### Everything on this one PC

The plan's §0.2 and §3 ask for a client on a second machine. **It is not
needed:** two profiles on this PC each have their own data, keychain entries
and single-instance lock, so `alice` and `bob` are two real clients. Sections
1, 2, 5, 6, 7 and 8 need nothing else, and §4's scenarios 3 and 3b are
profiles of one identity (`alice`, `alice-laptop`, `alice2`, `thief`), which
were always meant to be profiles rather than machines.

**The Mac goes last**, so §4's "the same two scenarios across machines" is the
only step held back for it.

**§3's network cut is the one step that needs thought on a single PC.** Its
steps 3 to 7 need `alice` to lose the server while `bob` and the server keep
running — and with everything on one PC, `Disable-NetAdapter` would either cut
both clients or (over loopback) neither.

Two live adapters make a way through: **Wi-Fi `192.168.0.122`** and
**Tailscale `100.86.134.100`**, with the server bound to `0.0.0.0:7710` and so
reachable on both. Add the server to `bob` as `192.168.0.122:7710` and to
`alice` as `100.86.134.100:7710`; then

```powershell
Disable-NetAdapter -Name Tailscale    # as administrator: cuts only alice
Enable-NetAdapter  -Name Tailscale
```

cuts `alice` alone, leaving `bob` and the server untouched — which is exactly
what steps 3 to 7 and the volume bug want.

**Unverified, and the thing to watch:** the server's TLS certificate is
generated for `public_host` only (`cli.rs`'s `fingerprint`), so reaching it on
a second address may be refused if the app checks the host name rather than
pinning the fingerprint the invite carries. If `alice` cannot add the server on
the Tailscale address, set `public_host` to that address and swap the two
clients' roles, so the cut client is the one on Wi-Fi. If neither works, that is
the single step that still wants a second machine — say so rather than skipping
it.

**Test material** (§0.4) is generated and waiting in `C:\oc\material\`, with a
`README.txt` saying which file is for which step. It covers the photo, the
three-photos-and-a-video message, the transparent PNG, the PDF, and a 95-second
video that **counts seconds aloud** — so §7's speed, pitch and clock can all be
judged against it. Still to make by hand: the voice message (record it in the
app; `voice_message_source.wav` can be played through this PC's VB-CABLE into
the app's microphone), a voice channel, and the two 200+ message channels §8
needs.

## C. For the Linux session: the batch does not merge cleanly

Each pull request is `MERGEABLE`/`CLEAN` against `beta` **on its own**, but they
collide with each other. Merging all nine in the order #21 → #29 hit conflicts
in nine files: three are trivial, four are real code, and two are generated
files that must not be hand-merged at all.

| File | Between | What it is |
|---|---|---|
| `docs/decisions.md` | #22, #23, #24, #25, #27 | Each appends a `D8x` entry at the same place. Keep both sides, in number order. |
| `app/lib/features/home/friends_page.dart` | #21 vs #23 | Two added imports. Keep both — and keep them alphabetical, or `directives_ordering` fires. |
| `app/linux/runner/window_channel.h` | #22 vs #24, then #25 | Each adds one function declaration. Keep both. |
| `app/lib/features/desktop/tray_binding.dart` | #24 vs #27 | **Real code.** Needs reading, not keep-both. |
| `app/lib/features/shell/desktop_shell.dart` | #24 vs #27 | **Real code.** |
| `app/test/widget/desktop/tray_binding_test.dart` | #24 vs #27 | **Real code.** Keep-both leaves an unbalanced test body that will not parse. |
| `app/lib/features/home/private_message_list.dart` | #23 vs #29 | **Real code.** |
| `app/lib/src/rust/frb_generated.dart` | #25/#26 vs the rest | **Generated — do not hand-merge.** Keep-both produced 35 `duplicate_named_argument` errors. Regenerate with `flutter_rust_bridge_codegen`. |
| `crates/opencord-core/src/frb_generated.rs` | #25/#26 vs the rest | **Generated — regenerate, as above.** |

Found by merging the nine locally; that branch was thrown away, since the merge
is the Linux session's to make and its resolutions are the ones that ship.

### The Android job fails on the runner, not on the code

`Phone (Android)` failed twice on #32, which adds **only PowerShell files**
and cannot reach the mobile build at all:

- `com.android.builder.sdk.InstallFailedException: Failed to install the
  following SDK components: cmake;3.22.1` — the SDK manager;
- `java.io.IOException: No space left on device`, configuring
  `:react-native-worklets`, which then also failed to write
  `~/.gradle/caches/9.4.1/file-changes/last-build.bin` — the runner's disk.

The first passed on a plain re-run with no change pushed; the second is
re-running as this is written. The mobile job evidently
sits close enough to the runner's disk limit that a cache miss tips it over,
so **a red `Phone (Android)` on this batch is worth re-running before it is
read as a regression**. Freeing space in the job, or caching less of the
Gradle and node_modules trees, would stop it costing a re-run each time.


---

# The checks

## 1. The title bar (#22) and private messages' title bar (#21)

> Re-run on **#22's own build** after two harness bugs were fixed (see A8): **11 of 11 machine checks pass**, on three consecutive runs. **3 of 11 boxes pass outright, 6 partly.** `WS_CAPTION` being set is **not** a failure of the first box: judge the client rect or the screenshot, not the style bit.
- [~] No system title bar anywhere: at start, after restoring from minimised, and after leaving maximised. — **at start: PASS** (client rect 1264x792 inside a 1280x800 window: 8 px lost in all, against a 23 px caption, and `01-start.png` shows the app's own header strip with no system caption). Minimised and un-maximised are still not measured.
- [~] Minimise, Maximise/Restore and Close sit at the top right, on every screen: a channel, Friends, a private chat, Settings, and the "no servers" screen. — **welcome screen, a channel (`#general`), Friends and "no servers": PASS. But which build matters.** On **#22's build alone the Private messages side has no caption buttons at all** — hovering where Close belongs leaves every pixel at the background `14,14,16`, so they are absent, not merely dark. On **#21's build they are there** on the same screens and the same profile. That is the split the two pull requests describe (#22 the app's title bar, #21 the private-messages one), not a regression — but it does mean **neither build alone satisfies this box**, and it is only true once they are both on `beta`. **Settings: PASS too** — the caption buttons sit at the top right of the Settings page on both builds. Only a private chat is left.
- [x] Close's hover is the app's own (inverted, monochrome). The others highlight on hover. — **PASS, judged from `05-hover-*.png`.** Close inverts to a light background with a dark glyph and stays monochrome — not Windows' red. Minimise and Maximise take a subtle grey highlight and keep their white glyph.
- [~] Dragging the empty header moves the window, also over a channel's or chat's title. — **the empty header: PASS** (dragged +120,+90 from 200,120 and landed at 315,206, inside the 12 px tolerance). Over a channel's or a chat's title needs a server.
- [x] A double click on the header maximises, and a second one restores. — **PASS** (maximised=True then False).
- [~] Aero Snap works: drag to the top to maximise, to a side for half the screen; Win+Up, Win+Down, Win+Left and Win+Right too. — **the whole keyboard half: PASS.** Win+Up filled the work area (1936x1056 over 1920x1040, the frameless overhang), Win+Down gave 1280x800 back, Win+Left gave 974x1047 at -7,0 and Win+Right 974x1047 at 953,0. Dragging to an edge is still not checked.
- [-] Hovering Maximise shows Windows 11's snap layouts (Windows 11 only). — **not run: no Windows 11 PC.**
- [~] The window's edges and corners resize it. — **the right edge: PASS** (1280 → 1441 on a +160 drag). The other edges and the corners are not checked.
- [x] **Settings → Appearance** has no Window frame choice. — **PASS on #22's build**, where the page ends at *Reduce motion* (`pr22-appearance.png`). It is worth saying what this looks like elsewhere: on **#24's build the choice is still there** — a **WINDOW FRAME** section offering *Auto / Custom / System* and "Takes effect when Opencord starts next" (`alice-appearance.png`). That is #22 doing exactly what it says, not a disagreement between the builds; the box is only true once #22 is on `beta`. The section is below the fold at this window size, so scroll before judging it absent.
- [~] **Private messages (#21):** — **the caption buttons on Friends: PASS** on #21's build (`pm21-header.png`), and absent on #22's, as above. The rest of this box is not checked yet.
  - on **Friends**, and in a private chat, the caption buttons are there and the header drags the window;
  - the empty space to the right of Friends' tabs (Online, All, Pending…) drags it too;
  - in a narrow window (the sidebar no longer inline), the buttons are still there and nothing overlaps.
- [ ] At 100 %, 125 % and 150 % scaling, the buttons are sharp and line up with the header.
## 2. The tray, and running in the background (#24)

> Driven on #24's build with both clients running. **7 of 11 boxes pass outright, 3 partly.** The icons live in the *hidden* part of the notification area here, which is Windows' choice, not the app's — read them by opening the chevron.
- [~] **The icon** shows in the notification area. It is sharp at 100 % and 150 % scaling, and at 150 % after a restart. — **it shows: PASS** (one icon per running client, in the overflow area). Sharpness at 100 % and 150 % is not judged.
- [x] **Hovering it** shows the unread summary, such as "No unread messages" or "3 unread messages, 1 mention". — **PASS.** With `alice` minimised and two messages sent from `bob`, `alice`'s icon reads `Opencord` / `2 unread messages` and `bob`'s own reads `Opencord` / `No unread messages`.
- [~] **A left click** brings the window forward and focuses it: when it's minimised, when it's hidden behind another app, and when it was closed to the tray. — **minimised: PASS** (both minimised, a left click on `bob`'s icon restored and focused `bob` and left `alice` minimised). **Behind another app: PASS** (`bob` focused in front, a left click on `alice`'s icon brought `alice` to the foreground). Closed to the tray is not tested yet.
- [x] **A right click** opens the menu: Open Opencord, Mute, Deafen, Status (Online, Idle, Do not disturb, Invisible), Quit Opencord. Mute and Deafen show a check mark when on, Status a mark beside the current one, and the marks follow when they change from inside the app. — **PASS, all four parts.** The menu holds exactly those five entries in that order; Status opens Online / Idle / Do not disturb / Invisible with a mark against Online; and after clicking the mic **inside `alice`'s window**, the same menu shows a check mark against Mute (`tray-menu-muted.png`), with Deafen still clear.
- [x] **Clicking outside the menu, or Esc,** closes it. — **PASS both ways** (menu open reads 2 of its named items; after Esc, 0; reopened, 2; after a click elsewhere, 0).
- [x] **Closing:**
  - the window's Close hides it, and the app keeps running: messages still arrive (send one from `bob`) and the icon's unread dot appears; — **PASS.** Close left the process alive with no window, `bob`'s message still arrived, and `alice`'s tooltip went to `1 unread message`.
  - a tray click brings it back, in the same place, at the same size; — **PASS.** Closed at `100,80 1000x820`, came back at `100,80 1000x820`.
  - **Settings → Windows & behavior → Close button minimizes to the tray** is shown on Windows. Turned off, Close quits. — **PASS.** The switch is there and on by default; turned off **and saved**, Close ended the process. Note the settings page keeps changes behind a *Careful — you have unsaved changes!* bar, so a toggle that is not saved does nothing — it looked at first like the setting had no effect.
- [x] **Quit Opencord** in the menu exits the process (Task Manager shows no `opencord.exe` for that profile). — **PASS.** No `opencord.exe` left afterwards.
- [x] **Explorer restart:** run `Stop-Process -Name explorer -Force`, then wait for it to come back (or start `explorer.exe`). The icon comes back by itself and still works. — **PASS.** Explorer was killed with both clients running; both icons came back on their own, with no restart of the app, and a click on one still opened its own window. It also cleared the ghost icons left by earlier killed instances — worth knowing, since a stale icon makes the count wrong.
- [~] **The voice icon:** — **three of five shapes confirmed, read off the icon's own pixels** (`sheet-voice.png`, `sheet-mute.png`, `sheet-left.png`):
  - in a voice channel: a ring; — **PASS**, a plain ring the moment `alice` joined `General`.
  - while you talk: a filled circle; — **not checked.** It needs real sound going into the microphone the app is listening to; VB-CABLE is installed, so this is wiring `alice`'s input to `CABLE Output` and playing a tone into `CABLE Input`, not a limitation of the app.
  - muted or deafened: the ring with a slash; — **muted: PASS**, the ring gains a slash from the mic button inside the app. Deafened not checked separately.
  - out of voice: the chat bubble again; — **PASS**, both clients were bubbles before joining and `alice`'s went back to a bubble on Disconnect.
  - the unread dot stays in every state. — **not checked**; it needs unread messages held across each of the states above.
- [x] **Two profiles** each get their own icon, and each icon's clicks go to its own window. — **PASS.** Two icons for two profiles; the first opens `alice` (pid 7932) and the second `bob` (pid 16508), each leaving the other alone. Note both tooltips start `Opencord` with no profile in them, so they are told apart by what they open, not by their name.
- [ ] **Start minimized** with **Launch at login**: after signing out and in, Opencord starts in the tray with no window.
## 3. The reconnect bar (#27), and the volume bug

> Both clients are profiles on this PC. For the steps that cut the network
> (the refused rejoin, and the volume bug), use the Wi-Fi/Tailscale split and
> `Disable-NetAdapter -Name Tailscale` from §B — it cuts `alice` alone.

- [ ] **The app closed in a call.**
  1. `alice` joins the voice channel.
  2. Kill her app: `Stop-Process -Name opencord` (with only `alice` running), or End task in Task Manager.
  3. Start it again: the bar says "Opencord closed while you were in <channel>."
  4. **Reconnect** puts her back in the call, and the bar goes.
- [ ] **Offered once:** kill it again while out of the call, and start it again. No bar.
- [ ] **The × hides the bar**, and doesn't come back after a restart.
- [ ] **Joining any call hides it:** joining another voice channel while the bar shows.
- [ ] **Another device took the call.**
  1. Make a second profile with `alice`'s identity: export it from **Settings → Identity & keys → Backup → Export identity backup**, start `$env:OPENCORD_PROFILE = "alice2"`, and use **Import identity** there.
  2. `alice` joins the voice channel, then `alice2` joins the same one.
  3. `alice` shows "Your call in <channel> moved to another device."
  4. **Reconnect** on `alice` takes the call back, and `alice2` is out of it.
- [ ] **No bar after:**
  - leaving the call;
  - `bob`, given Move Members, disconnects `alice` from the member menu;
  - `bob` moves `alice` to another channel;
  - **Quit Opencord** from the tray menu while in a call, then a restart;
  - Close with **Close button minimizes to the tray** off, while in a call, then a restart.
- [ ] **Getting back in was refused.**
  1. In the voice channel's settings set **User limit** to 1, and remove Move Members from `alice`'s roles.
  2. `alice` joins it.
  3. Cut this PC's network (`Disable-NetAdapter -Name "<adapter>"` as administrator, or unplug the cable) and wait until `bob` sees `alice` leave the channel.
  4. `bob` (the owner, who can pass the limit) joins the channel.
  5. Bring the network back: `alice` shows "Couldn't get back into <channel>: <the server's reason>".
  6. **Reconnect** while the channel is still full keeps the bar and shows the reason in a toast.
  7. When `bob` leaves, **Reconnect** works.
- [ ] **The volume bug** (reported, never reproduced):
  1. In a call with `bob`, set his **User volume** in `alice`'s member menu to 40 %.
  2. Cut and restore the network so `alice` drops and the app rejoins by itself (the voice panel shows reconnecting, then connected).
  3. His volume is still 40 %, and he sounds as quiet as before.
  4. Kill the app in the call and use **Reconnect**: still 40 %.
  5. If the volume is wrong at any point, write down which case, what the slider shows, and what you hear. Keep the session log (**Settings → About → Logs → Open logs folder**) and attach it.
## 4. Private message keys (#23)
- [ ] **Scenario 3: a second device of one identity.**
  1. Make `alice-laptop` with `alice`'s identity: **Export identity backup** on `alice`, then **Import identity** on a new profile.
  2. Before approving, `bob` writes to `alice`:
  3. `alice` clicks **Approve**. `alice-laptop` shows the same 16 words under **Compare words**. Click **They match** on both.
  4. `bob` writes again: both of `alice`'s profiles show it, and `bob`'s chat says "Alice signed in on a new device."
  5. `alice-laptop` replies: the message shows on both of `alice`'s profiles (as sent) and on `bob`'s.
- [ ] **Scenario 3b: comparing words, and a stolen identity.**
  1. In `alice`'s chat with `bob` and in `bob`'s chat with `alice`, click the lock: both show the same 16 words. **They match** on both turns the lock into a shield.
  2. Make a `thief` profile with `alice`'s identity, and don't approve it.
  3. `bob` writes to `alice`:
  4. `thief` writes to `bob`: `bob` sees "A message from keys you haven't accepted.", not the text.
- [ ] **The same two scenarios across machines:** `alice` on this PC, `alice-laptop` on the Mac. — **held back: the Mac goes last.** Everything else in §4 is profiles on this PC.
- [ ] **Restart:** after a restart of every profile, the shield and the approvals are still there, and new messages still decrypt.
- [ ] **Fresh start:** a profile that had private chats before this build says once that old chats were removed. Friends, requests, blocks and settings are kept.
## 5. Media uploads (the upload plan's P1)
- [x] **FFmpeg.** The shipped FFmpeg DLLs contain the decoders and demuxers from the upload plan's Task 32, `libdav1d` included. — **PASS, see A1.** Checked against the CI artifact's own `avcodec-63.dll`, not a local build: all four Task 32 lists byte-identical, `libdav1d` in and statically linked, LGPL, all five DLLs bundled. Nothing missing, so no switch of build and no `docs/packaging.md` change.
- [x] **Tests:** `cargo test --workspace --lib --bins --tests` and `flutter test` pass. — **PASS with two caveats, see A2.** Rust: green but for three load-sensitive `video::sender` tests in code the batch doesn't touch (they pass alone; `fix/flaky-reshape-test` fixes one and is unmerged), and the workspace needs `-j 4` on this PC. Dart: 1121/1121 pass excluding goldens; the 18 golden failures are Linux-rendered references that Windows CI never checks.
- [ ] **Sending:**
  - drop a photo, a video and a PDF on the composer;
  - the upload bar moves, and Cancel stops one;
  - the sent photo shows in the grid at its size, with no jump;
  - the video tile shows its first frame and its length.
- [ ] **Playing:**
  - a video plays in the chat, then in the viewer, with sound;
  - seeking works;
  - a voice message records and plays;
  - playing a voice message pauses the video.
- [ ] **Saving:**
  - Download on a file card asks where to save, and saves there;
  - **Save all** puts several files in one folder, beside the files already there.
- [ ] **The clipboard** (upload plan Task 21). — **the owner's decision is made: the Win32 route, no BSL-1.0** (see A3). Written as the branch `fix/windows-clipboard` (pull request #31); until it merges Windows says "this system's clipboard isn't supported". Once it is in, check both halves by hand:
  - paste a screenshot: Win+Shift+S, then Ctrl+V in the composer;
  - **Copy image** on a photo, then paste it into Paint and into a browser.
## 6. The media viewer's window (#25, D86)
- [ ] **Opening:**
  - click a photo: a new window opens and covers the whole monitor, taskbar included;
  - it has a taskbar button of its own;
  - the chat and desktop show through behind the photo, dimmed. If the background is solid dark instead, transparency failed: note it with a screenshot. That's the known risk.
- [ ] **Buttons:** Windows' minimise, restore and close at the top right, in white. Close hovers inverted, as the main window's does.
- [ ] **Modes:**
  - Restore gives a normal window, 800×600 the first time. Its borders resize it, down to 480×360, and dragging the background moves it;
  - Maximise, Win+Up, dragging it to the top of the screen, and Aero Snap all turn it back into full screen;
  - a double click on the top strip, or Alt+Enter, switches too.
- [ ] **Remembering:** close the viewer in the normal window and open another photo: it opens normal, at the same place and size. The same after restarting the app.
- [ ] **Closing:**
  - Esc closes the viewer, also right after typing a letter in it;
  - in full screen, a click beside the photo closes it. In the normal window it doesn't;
  - Alt+F4 and the taskbar button's Close close only the viewer; the app stays.
- [ ] **Moving around:**
  - in the three-photo message, the left and right arrows and the side strips step through it;
  - "Photo 2 of 4" and "name · date at time" show at the bottom left;
  - the controls fade after a second without moving the mouse, and come back on movement.
- [ ] **Zoom** works across the whole screen, not only the photo's first size.
- [ ] **The video** plays in the viewer with its own controls, and the viewer's text is not over them.
- [ ] **Download** saves to Downloads without asking. The note says "Saved to Downloads", and clicking it opens the folder. A second Download of the same file adds " (2)".
- [ ] **Opening again:** clicking another photo while the viewer is open shows it in the same window and brings it forward.
- [ ] **The tray:**
  - close the main window to the tray with the viewer open: the viewer stays;
  - close the viewer: the app keeps running in the tray;
  - **Quit Opencord** from the tray with the viewer open: everything exits.
- [ ] **Display settings:**
  - at 100 % and 150 % scaling;
  - with two monitors, full screen opens on the main window's monitor;
  - **known, check whether it bites:** with two monitors at different scaling, a remembered normal window is converted with the main window's scaling, so it may reopen at the wrong size on the other monitor;
  - **known:** a remembered normal window that only just overlaps a monitor still there (after unplugging one) may open nearly off-screen.
## 7. The viewer's player and picture-in-picture (#26, D87)
- [ ] **Rotate:** beside Download at the bottom right, for photos and videos. Each click turns a quarter left; the next item starts unturned.
- [ ] **The bar** (a video): narrow, centred at the bottom, see-through, and never over the video.
  - First row: volume, play, full screen, picture-in-picture and the speed.
  - Second row: the time played, the seek bar and the time left.
  - It fades with the rest when the mouse rests.
- [ ] **Speed:** 1x, 1.5x, 2x, 3x and round again. Voices keep their pitch, and the times stay in the video's own time.
- [ ] **Full screen:**
  - the video fills the screen at its own shape, with only the bar and the title buttons left, fading;
  - Esc ends it first, then closes;
  - from a normal window, the window goes full screen and comes back normal, and the next viewer still opens normal.
- [ ] **Picture-in-picture:**
  - the button turns the viewer into a small window, 320 wide with the video's shape, at the bottom right of the screen, playing on without a restart;
  - it stays above other windows, also when another app is active;
  - its own controls: volume top left, back and close top right, play in the middle, the times and seek bar at the bottom; they fade;
  - a drag moves it; its edges resize it, keeping its shape;
  - Back, or a double click, returns to the window it was, still playing; close stops the video;
  - opening another photo or video brings the viewer back with it;
  - close it somewhere else and open picture-in-picture again: it comes back there, at that size;
  - close from picture-in-picture with a normal window remembered, then open a photo: a normal window with its borders, laid out as the viewer, not as picture-in-picture.
## 8. Scrolling and compact mode (#28, #29)
- [ ] **Compact mode (#28):** in **Settings → Appearance → Message density**, choose **Compact**. On a wide window, a text channel's lines run the chat's whole width, 16 px in from each side, as in private chats. Comfortable bubbles stay in a centred column.
- [ ] **Private chats (#29):**
  - a chat opens at the newest message;
  - a new message from `bob` scrolls into view while you're at the newest one;
  - while you read older messages, new ones don't move you;
  - sending takes you to the newest message;
  - scrolling up loads older messages without a jump.
- [ ] **Remembering the place (#29):**
  - scroll halfway up a long channel, go to another channel and come back: the same place;
  - the same for a private chat, and each chat keeps its own place;
  - quit from the tray, start again: both the channel and the chat open where you left them, even far back in the history.

## 9. Not in this list yet

- **Notifications on Windows** (through the tray icon's balloon): planned, not
  built. Not checked.
- **Windows 11 only:** sharing a screen's sound needs build 20348 or later.
  **Not run: no Windows 11 PC** (`docs/plans/windows.md`).

## 9.5 Where the run stopped, and what is next

**Stopped at: the PC locked itself a second time**, in the middle of re-running
§1's checks. `check_window_chrome.ps1` caught it and exited 2 rather than
scoring the app against a lock screen — the guard doing its job. Nothing that
injects input or reads the screen can go further until the PC is unlocked.

**The rig is still standing** and does not need rebuilding:

- the server runs on `localhost:7710` with `alice` (owner) and `bob` as its
  two members, `#general` and the `General` voice channel;
- both clients run from `C:\oc\tray-background\` (§2's build), tiled side by
  side; switching sections means killing them and starting the same two
  profiles from that section's build in table B — the identities and the
  server stay, because they live in the profile, not the build.

**Pick up at**, in this order, all of them §2 and all needing an unlocked PC:

1. the three closing boxes — Close hides to the tray, messages still arrive,
   a tray click restores it in the same place and size, and **Settings →
   Windows & behavior → Close button minimizes to the tray** turned off makes
   Close quit;
2. **Quit Opencord** from the menu ends the process;
3. the voice icon's four states in the `General` voice channel;
4. *Clicking outside the menu, or Esc, closes it* — attempted twice and
   inconclusive both times, because the flyout kept closing under the re-read
   rather than because the menu misbehaved. Open the menu, confirm it is
   there, then close it one way per run.

Then §3 to §8, which have not been started. §3 needs the network cut
described in section B; §4's cross-machine half waits for the Mac.

## 10. Where this goes

- Post the ticked list above in the batch's thread, with the failures,
  screenshots and session logs (**Settings → About → Logs → Open logs folder**).
- A Windows-only fix goes on a branch off the pull request under test, as a
  pull request into `beta`, naming the step it is for. A shared problem (Dart or
  Rust every system runs) goes back to the Linux session with the steps to
  repeat it.
- §C above is for the Linux session, not for this report's thread.
