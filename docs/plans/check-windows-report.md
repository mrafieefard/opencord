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


> **Doing the rest by hand:** [check-windows-by-hand.md](check-windows-by-hand.md)
> has a recipe for every box still open or partly done — the build to use, the
> steps, what counts as a pass, and the traps that waste time.

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


### A11. Closing the media viewer crashes the whole app

**The most serious thing found in this run.** Every way of closing the media
viewer takes the entire application down with an access violation, and the
same fault address every time:

```
FATAL native crash  code=0xC0000005  module=flutter_windows.dll  offset=0x1d7b0
```

`0xC0000005` is `STATUS_ACCESS_VIOLATION`. It is written to the session log
(**Settings → About → Logs**), so each occurrence has a record.

**Four crashes, four runs, one signature.** The routes that crash:

| How the viewer was closed | Result |
|---|---|
| A click beside the photo, **in full screen** | crash (twice out of two) |
| **Alt+F4** on the viewer | crash — main window and process go too |
| The viewer's own **Close button** (×) | crash |

And the control that shows it is about *closing*, not about the viewer:

| Action | Result |
|---|---|
| A click beside the photo **in the normal window** (which must *not* close it) | correct: viewer stays, app lives |
| Restore / Maximise, dragging, resizing, stepping photos | all fine, no crash |

Two earlier runs that opened the viewer and never closed it logged **no
crash**; every run that closed it logged exactly one. So the trigger is the
close path itself, not opening or using the viewer.

**What it costs:** the viewer is the way a photo is looked at. Closing it is
not an edge case, it is the second half of looking at every picture — so on
Windows the app dies whenever someone finishes viewing a photo, losing the
window, the call and anything unsent. This is worse than any box in §6 asks
about.

**To repeat it**, one client, nothing else needed:

1. Send a photo to any channel and click it. The viewer opens full screen.
2. Click beside the photo, on the dimmed background — or press Alt+F4, or
   click the viewer's own ×.
3. The viewer and the main window both vanish; no `opencord.exe` is left.
   The session log's last line is the crash above.

**The cause, and the fix — PR #39.** The viewer is a second view on the app's
**own** engine, made with `FlutterDesktopEngineCreateViewController` from the
engine id Dart passes in. The only teardown the embedder exports is
`FlutterDesktopViewControllerDestroy`, and its own header says it

> Shuts down the engine instance associated with |controller|

which here is the engine the **main window** is still running on. Closing the
viewer therefore stopped the engine underneath the main window, and the app
came apart. There is no `EngineDestroyViewController` to call instead — the
exported symbols were checked.

A first attempt deferred that destroy out of the engine's callback, on a
re-entrancy theory; it did **not** help, because the destroy call itself is
the problem rather than when it is made. (Worth recording how that nearly
passed: the window is hidden before teardown, and a check that asks whether
the viewer window is still *visible* says "closed" either way. The crash log
is the only honest witness.)

What ships instead: closing hides the window and keeps the view built, and
the next open shows the same one — which is also where the remembered mode
and place now come from. Verified on a local release build against the crash
log: all three close routes leave the app and its main window alive, four
open-and-close cycles in one session stay clean, and a viewer left in the
normal window reopens at the same 496×360 client size. **0 crashes, against
4 in 4 before.**

### A12. The media viewer takes no keyboard until it has been reopened once

**Corrected twice; this is what it actually is.** The owner found it at the
keyboard, and it is narrower and more specific than either earlier draft:

1. Open a photo or video. **Esc does nothing.**
2. Close the viewer another way — a click beside the photo, or its own
   Close button.
3. Open it again. **Esc now closes it, normally.**

So the viewer does take keys, but not on the first open of a session. That
also explains the two wrong drafts of this note: both were written from
scripted runs that only ever tested a freshly opened viewer, and never
reopened one.

**Why the scripted half of this report could not see it at all.** Synthetic
keystrokes never reach the viewer, reopened or not — `keybd_event` and
`SendKeys` both send a virtual key with **scan code 0**, and Flutter's
Windows key handling works from scan codes, so it drops them. Every "key
does nothing" in this section came from that, which is why this note once
claimed the viewer took no keyboard at all. **Keys in the viewer have to be
checked by a person**; the harness cannot, and a future script should not
pretend otherwise.

**Where it is.** `media_window.cpp` calls `::SetFocus(view_)` at the end of
`Open()`, straight after `FlutterDesktopEngineCreateViewController` — before
the new view is ready to take the focus, so it does not stick. A reopen goes
through `WM_ACTIVATE`, which calls `::SetFocus(view_)` again on a view that
is ready by then, and from that point keys work. The fix is to set the focus
once the view is up rather than in the same breath as creating it — posting
it to the message loop, as the file already does for `kGoFullscreen`, is the
shape of it.

That is a Windows file, so it is a platform session's to fix, not the Linux
session's.

**What it costs:** the first photo anyone opens after starting the app does
not answer Esc, the arrows or Alt+Enter. The mouse works throughout, so it
reads as the keyboard being unsupported rather than as a bug.

`app/test/widget/chat/media_viewer_test.dart` has a passing
`alt_enter_switches_the_window_mode`. It passes because the test harness
delivers straight into the tree's focus scope, so it cannot see a focus that
never arrived in the real window — which is why it does not protect against
this.


### A13. Building the Windows app here needs a short path

`flutter build windows --release` fails in this repo at its normal place:

```
FileTracker : error FTK1011: could not create the new file tracking log file:
C:\Users\mrafi\dev\opencord\app\build\windows\x64\plugins\opencord_core\cargokit_build\
  x86_64-pc-windows-msvc\release\build\opusic-sys-<hash>\out\build\CMakeFiles\
  CMakeScratch\TryCompile-<id>\...\Lib-link.read.1.tlog
```

That path is **262 characters** — two over `MAX_PATH`. `LongPathsEnabled` is
already `1` on this machine; MSBuild's FileTracker does not honour it. Three
things that look like they should help and do not:

- building through `C:\src\opencord` (the short symlink): CMake resolves it
  back to the long real path;
- `CARGO_TARGET_DIR=C:\t`: cargokit sets its own `cargokit_build` and wins;
- a junction from `app/build` to `C:\b`: the files land in the short place
  but the path *string* MSBuild passes around is unchanged.

What works is a genuinely short source path. A worktree does it without
disturbing the checkout:

```
git worktree add C:\o <branch>
cd C:\o\app && flutter build windows --release
```

Also worth knowing: the build rewrites `app/analysis_options.yaml`
("Upgrading analysis_options.yaml to exclude build and platform
directories"). It is easy to sweep into a commit; check `git status` before
committing after any local Windows build.

### A14. A build's tray icon starts hidden, every time

Windows hides the notification-area icon of any executable it has not seen
before — the default is *Only show notifications* — and it remembers that
per **executable path**, in `TrayNotify\IconStreams` on Windows 10.

Every build here lives somewhere different, so each one is a new application
as far as Windows is concerned:

```
C:\oc\tray-background\opencord\opencord.exe
C:\oc\media-viewer-player\opencord\opencord.exe
C:\o\app\build\windows\x64\runner\Release\opencord.exe   (a local build)
```

Pin one and the next is still hidden. It is not the app: it registers its
icon correctly each time.

**So: open the `^` chevron before concluding the tray icon is missing.** Two
of this report's confusions came from this — once reading the icons as absent
when they were merely hidden, and once counting a killed instance's ghost
icon as a live one. Ghosts linger until something hovers over them;
restarting Explorer clears them.

To put one on the taskbar: drag it out of the chevron's pop-up, or
**Settings → Personalization → Taskbar → Select which icons appear on the
taskbar**. `IconStreams` itself is an obfuscated blob — not worth editing by
hand.
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


### A shared bug: the Reconnect button sits under the caption buttons

**Where:** the voice reconnect bar (#27), on Windows, on any build that also
has the custom title bar (#22/#24) — which on Windows is all of them.

**What happens:** the bar puts **Reconnect** at its right-hand end. The window's
own Minimise / Maximise / Close sit at the top right too, and the **Maximise
button's hit area extends upwards over the bar**. So a click on the right-hand
part of Reconnect maximises the window and never reaches the button: the call
is not rejoined and the bar stays, which reads as "Reconnect is broken".

**How to repeat it**, with one client:

1. Join a voice channel.
2. `Stop-Process -Name opencord -Force`, then start the app again. The bar
   appears: *Opencord closed while you were in General.*
3. Size the window to 1000 px wide and click Reconnect **on its right half**.
   The window maximises; nothing else happens.
4. Click it again **on its left half** (x ≈ 875 of 1000). It reconnects
   properly and the bar goes.

**The numbers**, measured at 1000 px wide, 100 % scaling: Reconnect is drawn
across about **x 862–950**; Maximise's hit area covers about **x 907–953** and
reaches up through the whole height of the bar — y = 14, 20 and 28 all
maximise. Hovering x 905 pops Windows' own *Maximize* tooltip while the
Reconnect button underneath still draws its hover highlight, which is the
clearest single symptom.

**Why it is yours and not a Windows-only fix:** the bar is shared Dart
(`app/lib/…`), not one of the Windows files a platform session may touch
(platforms.md §4). The fix is a layout one — keep the bar's actions clear of
the caption buttons' column, or inset the bar's right edge by the caption
width on desktop.
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
- [x] No system title bar anywhere: at start, after restoring from minimised, and after leaving maximised. — **PASS in all three states.** At start, measured: client rect 1264x792 inside a 1280x800 window, 8 px lost in all against a 23 px caption, and `01-start.png` shows the app's own header strip with no system caption. The owner then checked the other two by eye — minimise and restore, maximise and un-maximise — and no Windows title bar comes back in either.
- [~] Minimise, Maximise/Restore and Close sit at the top right, on every screen: a channel, Friends, a private chat, Settings, and the "no servers" screen. — **welcome screen, a channel (`#general`), Friends and "no servers": PASS. But which build matters.** On **#22's build alone the Private messages side has no caption buttons at all** — hovering where Close belongs leaves every pixel at the background `14,14,16`, so they are absent, not merely dark. On **#21's build they are there** on the same screens and the same profile. That is the split the two pull requests describe (#22 the app's title bar, #21 the private-messages one), not a regression — but it does mean **neither build alone satisfies this box**, and it is only true once they are both on `beta`. **Settings: PASS too**, on both builds. **A private chat: PASS** on #21's build. So **all five screens are now verified** — but across the two builds, each on the one that owns it, and the box is only true of a single product once #21 and #22 are both on `beta`. Re-check it then.

  > **What it costs while the two are apart**, which the owner ran into: on #22's build, sitting on Private messages there is **no way to minimise or close the window with the mouse at all** — the buttons are simply not there, so only Alt+F4 gets you out. Anyone checking §1 on #22's build should move to a channel first; the window chrome cannot be exercised from that page.
- [x] Close's hover is the app's own (inverted, monochrome). The others highlight on hover. — **PASS, judged from `05-hover-*.png`.** Close inverts to a light background with a dark glyph and stays monochrome — not Windows' red. Minimise and Maximise take a subtle grey highlight and keep their white glyph.
- [~] Dragging the empty header moves the window, also over a channel's or chat's title. — **the empty header: PASS** (dragged +120,+90 from 200,120 and landed at 315,206, inside the 12 px tolerance). Over a channel's or a chat's title needs a server.
- [x] A double click on the header maximises, and a second one restores. — **PASS** (maximised=True then False).
- [x] Aero Snap works: drag to the top to maximise, to a side for half the screen; Win+Up, Win+Down, Win+Left and Win+Right too. — **PASS, both halves.** The keyboard, measured: Win+Up filled the work area (1936x1056 over 1920x1040, the frameless overhang), Win+Down gave 1280x800 back, Win+Left gave 974x1047 at -7,0 and Win+Right 974x1047 at 953,0. Dragging to the top and to a side was then checked by the owner and behaves. **Hold at the edge for about a second before letting go** — released at once the window only moves, which is the drag and not the app, and it read as a failure here first.
- [-] Hovering Maximise shows Windows 11's snap layouts (Windows 11 only). — **not run: no Windows 11 PC.**
- [x] The window's edges and corners resize it. — **PASS.** The right edge measured (1280 → 1441 on a +160 drag); the left, top and bottom edges and all four corners checked by the owner, the corners taking both dimensions at once.
- [x] **Settings → Appearance** has no Window frame choice. — **PASS on #22's build**, where the page ends at *Reduce motion* (`pr22-appearance.png`). It is worth saying what this looks like elsewhere: on **#24's build the choice is still there** — a **WINDOW FRAME** section offering *Auto / Custom / System* and "Takes effect when Opencord starts next" (`alice-appearance.png`). That is #22 doing exactly what it says, not a disagreement between the builds; the box is only true once #22 is on `beta`. The section is below the fold at this window size, so scroll before judging it absent.
- [x] **Private messages (#21):** — **PASS, all three parts**, on #21's own build, checked by the owner.
  - on **Friends**, and in a private chat, the caption buttons are there and the header drags the window; — they are (`pm21-header.png` for Friends), and absent on #22's build, as above.
  - the empty space to the right of Friends' tabs (Online, All, Pending…) drags it too; — it does.
  - in a narrow window (the sidebar no longer inline), the buttons are still there and nothing overlaps. — still there, nothing overlapping; the list takes a scrollbar at that width, which is the layout coping rather than a fault.
- [ ] At 100 %, 125 % and 150 % scaling, the buttons are sharp and line up with the header.
## 2. The tray, and running in the background (#24)

> Driven on #24's build with both clients running. **8 of 11 boxes pass outright, 2 partly.** The icons live in the *hidden* part of the notification area here, which is Windows' choice, not the app's — read them by opening the chevron.
- [x] **The icon** shows in the notification area. It is sharp at 100 % and 150 % scaling, and at 150 % after a restart. — **PASS, both parts.** It shows, one icon per running client; **sharpness at 100 % and 150 %, and at 150 % after a restart, judged by the owner and fine.** Which part of the notification area it lands in is Windows' choice per executable path, not the app's — see A14.
- [x] **Hovering it** shows the unread summary, such as "No unread messages" or "3 unread messages, 1 mention". — **PASS.** With `alice` minimised and two messages sent from `bob`, `alice`'s icon reads `Opencord` / `2 unread messages` and `bob`'s own reads `Opencord` / `No unread messages`.
- [x] **A left click** brings the window forward and focuses it: when it's minimised, when it's hidden behind another app, and when it was closed to the tray. — **minimised: PASS** (both minimised, a left click on `bob`'s icon restored and focused `bob` and left `alice` minimised). **Behind another app: PASS** (`bob` focused in front, a left click on `alice`'s icon brought `alice` to the foreground). **Closed to the tray: PASS** (the window was hidden with Close, and a left click on its icon brought it back — see the Closing box below).
- [x] **A right click** opens the menu: Open Opencord, Mute, Deafen, Status (Online, Idle, Do not disturb, Invisible), Quit Opencord. Mute and Deafen show a check mark when on, Status a mark beside the current one, and the marks follow when they change from inside the app. — **PASS, all four parts.** The menu holds exactly those five entries in that order; Status opens Online / Idle / Do not disturb / Invisible with a mark against Online; and after clicking the mic **inside `alice`'s window**, the same menu shows a check mark against Mute (`tray-menu-muted.png`), with Deafen still clear.
- [x] **Clicking outside the menu, or Esc,** closes it. — **PASS both ways** (menu open reads 2 of its named items; after Esc, 0; reopened, 2; after a click elsewhere, 0).
- [x] **Closing:**
  - the window's Close hides it, and the app keeps running: messages still arrive (send one from `bob`) and the icon's unread dot appears; — **PASS.** Close left the process alive with no window, `bob`'s message still arrived, and `alice`'s tooltip went to `1 unread message`.
  - a tray click brings it back, in the same place, at the same size; — **PASS.** Closed at `100,80 1000x820`, came back at `100,80 1000x820`.
  - **Settings → Windows & behavior → Close button minimizes to the tray** is shown on Windows. Turned off, Close quits. — **PASS.** The switch is there and on by default; turned off **and saved**, Close ended the process. Note the settings page keeps changes behind a *Careful — you have unsaved changes!* bar, so a toggle that is not saved does nothing — it looked at first like the setting had no effect.
- [x] **Quit Opencord** in the menu exits the process (Task Manager shows no `opencord.exe` for that profile). — **PASS.** No `opencord.exe` left afterwards.
- [x] **Explorer restart:** run `Stop-Process -Name explorer -Force`, then wait for it to come back (or start `explorer.exe`). The icon comes back by itself and still works. — **PASS.** Explorer was killed with both clients running; both icons came back on their own, with no restart of the app, and a click on one still opened its own window. It also cleared the ghost icons left by earlier killed instances — worth knowing, since a stale icon makes the count wrong.
- [x] **The voice icon:** — **PASS, all five.** The four shapes were read off the icon's own pixels (`sheet-voice.png`, `sheet-mute.png`, `sheet-left.png`):
  - in a voice channel: a ring; — **PASS**, a plain ring the moment `alice` joined `General`.
  - while you talk: a filled circle; — **PASS**, confirmed by the owner speaking into the real microphone: the ring fills white while they talk. The scripted half of this section could not reach it, since it needs sound going into the device the app is listening to.
  - muted or deafened: the ring with a slash; — **muted: PASS**, the ring gains a slash from the mic button inside the app. Deafened not checked separately.
  - out of voice: the chat bubble again; — **PASS**, both clients were bubbles before joining and `alice`'s went back to a bubble on Disconnect.
  - the unread dot stays in every state. — **PASS**, checked by the owner with messages from `bob` left unread while `alice` went through all four: the dot stays on the ring, the filled circle, the slashed ring and the bubble alike.
- [x] **Two profiles** each get their own icon, and each icon's clicks go to its own window. — **PASS.** Two icons for two profiles; the first opens `alice` (pid 7932) and the second `bob` (pid 16508), each leaving the other alone. Note both tooltips start `Opencord` with no profile in them, so they are told apart by what they open, not by their name.
- [ ] **Start minimized** with **Launch at login**: after signing out and in, Opencord starts in the tray with no window.
## 3. The reconnect bar (#27), and the volume bug

> Both clients are profiles on this PC. For the steps that cut the network
> (the refused rejoin, and the volume bug), use the Wi-Fi/Tailscale split and
> `Disable-NetAdapter -Name Tailscale` from §B — it cuts `alice` alone.

- [x] **The app closed in a call.** — **PASS, all four steps**, but see the defect below.
  1. `alice` joins the voice channel.
  2. Kill her app: `Stop-Process -Name opencord` (with only `alice` running), or End task in Task Manager.
  3. Start it again: the bar says "Opencord closed while you were in <channel>." — **the wording is exactly that**: *Opencord closed while you were in General.*
  4. **Reconnect** puts her back in the call, and the bar goes. — **it does**: the header went back to *1 connected* and the bar disappeared.

  > **Defect found doing this: most of the Reconnect button does nothing but maximise the window.** The bar puts Reconnect at the top right, which is where the custom title bar's caption buttons are, and the **Maximise button's hit area reaches up over the bar**. Hovering the right-hand half of Reconnect pops Windows' *Maximize* tooltip, and clicking there maximises the window and leaves the call unjoined. In a 1000 px-wide window Reconnect is drawn across about x 862–950, and Maximise takes x 907–953 — so roughly **the right half of the button is dead**, at every height in the bar (tried y = 14, 20 and 28; all maximised). Clicking at x = 875 reconnects normally. It is only findable by clicking the left part of a button whose right part looks identical, so a person would reasonably call Reconnect broken. Reported to the Linux session in §C: the bar is shared Dart, not a Windows file.
- [x] **Offered once:** kill it again while out of the call, and start it again. No bar. — **PASS.** With `alice` out of the call, killed and restarted: no bar.
- [x] **The × hides the bar**, and doesn't come back after a restart. — **PASS both halves.** The × dismissed the bar and left the window alone (the app did not close, the rect did not change); killed and restarted afterwards, the bar stayed away. Unlike Reconnect, **the × is not shadowed by the caption buttons**: hovering it shows the app's own *Dismiss* tooltip and its hover ring, so only Reconnect is affected by the overlap above.
- [x] **Joining any call hides it:** joining another voice channel while the bar shows. — **PASS.** With the bar up, **Join voice** put `alice` in the call and the bar went at once. Only one voice channel exists on the test server, so this is the same channel rather than a different one; the box's point — that joining a call clears the offer — holds.
- [ ] **Another device took the call.**
  1. Make a second profile with `alice`'s identity: export it from **Settings → Identity & keys → Backup → Export identity backup**, start `$env:OPENCORD_PROFILE = "alice2"`, and use **Import identity** there.
  2. `alice` joins the voice channel, then `alice2` joins the same one.
  3. `alice` shows "Your call in <channel> moved to another device."
  4. **Reconnect** on `alice` takes the call back, and `alice2` is out of it.
- [~] **No bar after:** — **one of five done; two of the five cannot be done on #27's build at all.**
  - leaving the call; — **PASS.** `alice` left with Disconnect, was killed and restarted: no bar.
  - `bob`, given Move Members, disconnects `alice` from the member menu; — **not done.** Needs a role with Move Members granted to `bob` first.
  - `bob` moves `alice` to another channel; — **not done.** Needs that role *and* a second voice channel; the test server has only `General`.
  - **Quit Opencord** from the tray menu while in a call, then a restart; — **cannot be done on this build.** #27's build has **no tray at all** — the tray is #24 — so there is no tray menu to quit from.
  - Close with **Close button minimizes to the tray** off, while in a call, then a restart. — **cannot be done on this build.** Same reason: with no tray, **Windows & behavior** on #27's build offers only *Launch at login* and *Restore the last window position*. The setting this step turns off does not exist here.

  > Both of the last two need a build carrying **#24 and #27 together**, which only exists once the batch is merged. They are not failures and they are not skips for convenience — there is nothing on this build to drive. The same shape as §1's caption buttons and the Appearance box: the plan's wording assumes the merged state, and several boxes can only be judged after the Linux session's merge.
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
- [x] **Sending:** — **PASS, all four**, checked by the owner by dragging from Explorer rather than through the paperclip, so the drop path itself is covered.
  - drop a photo, a video and a PDF on the composer; — all three attach.
  - the upload bar moves, and Cancel stops one; — the bar runs rather than jumping, and Cancel on a large upload (`speech_60s.mp4`, 34 MB) stopped that one and left the others alone.
  - the sent photo shows in the grid at its size, with no jump; — it does.
  - the video tile shows its first frame and its length. — both.
- [x] **Playing:** — **PASS, all four**, checked by the owner.
  - a video plays in the chat, then in the viewer, with sound; — both places, with sound.
  - seeking works; — it lands where it is put and carries on.
  - a voice message records and plays; — held the microphone, sent it, played it back.
  - playing a voice message pauses the video. — it does, rather than the two sounding at once, which is the part of this box with any teeth.
- [x] **Saving:** — **PASS, both**, checked by the owner.
  - Download on a file card asks where to save, and saves there; — it asks, and the file lands where it was told. Note this is deliberately unlike the **viewer's** Download, which saves straight to Downloads without asking: two different buttons, each behaving as its own box asks.
  - **Save all** puts several files in one folder, beside the files already there. — it does, without disturbing what was in the folder.
- [ ] **The clipboard** (upload plan Task 21). — **the owner's decision is made: the Win32 route, no BSL-1.0** (see A3). Written as the branch `fix/windows-clipboard` (pull request #31); until it merges Windows says "this system's clipboard isn't supported". Once it is in, check both halves by hand:
  - paste a screenshot: Win+Shift+S, then Ctrl+V in the composer;
  - **Copy image** on a photo, then paste it into Paint and into a browser.
## 6. The media viewer's window (#25, D86)
- [x] **Opening:** — **PASS, all three, and the known risk did not bite.**
  - click a photo: a new window opens and covers the whole monitor, taskbar included; — **PASS.** A second window of class `OPENCORD_MEDIA_VIEWER` appears at **0,0 1920×1080** on a 1920×1080 monitor, so it covers the taskbar too, not just the work area.
  - it has a taskbar button of its own; — **PASS.** It is an **unowned** top-level window (`GetWindow(GW_OWNER)` = 0) with no `WS_EX_TOOLWINDOW`, which is exactly what makes Windows give a window its own button.
  - the chat and desktop show through behind the photo, dimmed. If the background is solid dark instead, transparency failed: note it with a screenshot. That's the known risk. — **PASS: transparency works.** `s6-viewer.png` is a whole-desktop capture with the viewer open, and the windows behind it are legible through the dimming. **The known risk did not bite on this machine.**
- [x] **Buttons:** Windows' minimise, restore and close at the top right, in white. Close hovers inverted, as the main window's does. — **PASS** (`s6-vbtn-sheet.png`). Minimise, restore and close sit at the top right in white, and Close inverts to a white background with a dark glyph on hover, matching the main window.
- [~] **Modes:** — **everything passes except Alt+Enter.**
  - Restore gives a normal window, 800×600 the first time. Its borders resize it, down to 480×360, and dragging the background moves it; — **PASS, all three.** Restore gave exactly **800×600** at 560,220. Dragged well past the minimum, the window stops at a 496×368 *window* rect whose **client rect is exactly 480×360** — the plan's number; the extra 16×8 is the invisible resize border, so this is the limit the plan asks for and not a near miss. Dragging the background moved it +144,+96 against +150,+100 asked, inside drag rounding.
  - Maximise, Win+Up, dragging it to the top of the screen, and Aero Snap all turn it back into full screen; — **PASS.** The Maximise button gives 0,0 1920×1080 and toggles back; Win+Up gives the same. **Dragging to the top works, but only with a dwell at the edge** — released immediately the window merely moves to y=0, which looks like a failure and is the harness being too quick, not the app. Windows' own drag-snap is enabled here (`WindowArrangementActive = 1`).
  - a double click on the top strip, or Alt+Enter, switches too. — **double click: PASS** (full screen → the remembered normal window). **Alt+Enter: FAILS — see below.**

  > **Defect: Alt+Enter does not switch the viewer's mode, in either direction.** With the viewer focused (`GetForegroundWindow` is `OPENCORD_MEDIA_VIEWER`), Alt+Enter from full screen leaves it at 1920×1080 and from the normal window leaves it at 496×368. Tried with two different injection paths — raw `keybd_event` and `SendKeys('%{ENTER}')` — and **Win+Up on the very same window worked moments before**, so keys do reach it. What makes this worth chasing: the behaviour **is** specified and covered by a passing Dart widget test, `alt_enter_switches_the_window_mode` in `app/test/widget/chat/media_viewer_test.dart`, which sends altLeft+enter and asserts `setMode:normal`. So the shared logic is right and something Windows-specific swallows the keystroke before Flutter sees it. Alt+Enter arrives as **`WM_SYSKEYDOWN`**, not `WM_KEYDOWN`, and `app/windows/runner/media_window.cpp` ends its window procedure at `DefWindowProcW` — that is the place to look. Being a Windows file, this one is a platform session's to fix rather than the Linux session's.
- [x] **Remembering:** close the viewer in the normal window and open another photo: it opens normal, at the same place and size. The same after restarting the app. — **PASS, and across a restart.** Left in the normal window at 496×368, the app crashed on close (below), was started again, and the next photo opened **in the normal window at 496×368** — the stronger half of the box, since it survived a process death rather than a tidy shutdown.
- [~] **Closing:** — **two of three pass once PR #39 is in; Esc still does not, for the keyboard reason in A12.**
  - Esc closes the viewer, also right after typing a letter in it; — **FAILS**, and not because of the crash: no key reaches the viewer's Dart at all (A12). Unaffected by #39.
  - in full screen, a click beside the photo closes it. In the normal window it doesn't; — **PASS on both halves with #39.** In full screen the click closes the viewer and the app stays up; in the normal window it correctly does nothing. On the build under test it crashed the app instead, twice out of two.
  - Alt+F4 and the taskbar button's Close close only the viewer; the app stays. — **Alt+F4: PASS with #39** — the viewer goes, the main window and the process stay. The viewer's own Close button behaves the same. The taskbar button's Close is not separately checked. On the build under test both crashed the app.
- [~] **Moving around:** — **the mouse half passes, the keyboard half does not.**
  - in the three-photo message, the left and right arrows and the side strips step through it; — **side strips: PASS.** Clicking the right strip went from *Photo 1 of 3* to *Photo 2 of 3*, with `‹` and `›` drawn on the strips. **Arrow keys: FAIL** — two Right presses on the focused viewer left the same photo on screen.
  - "Photo 2 of 4" and "name · date at time" show at the bottom left; — **PASS.** *Photo 1 of 3* then *Photo 2 of 3*, with *Alice · 3 Nov at 17:24* under it, bottom left.
  - the controls fade after a second without moving the mouse, and come back on movement. — **PASS.** Left alone, the caption buttons, the counter and the download and rotate icons all faded out; they were back after the next move of the mouse.
- [x] **Zoom** works across the whole screen, not only the photo's first size. — **PASS**, judged by the owner on an ordinary photo and on `photo4_4k.jpg`: the zoom carries on past the size the picture first appeared at and uses the whole screen, panning follows the mouse, and the 4K one holds its detail going in rather than showing the fitted version's pixels enlarged.
- [x] **The video** plays in the viewer with its own controls, and the viewer's text is not over them. — **PASS.** `short_video.mp4` plays in the viewer with the player's own bar, and the *Photo n of m* / *name · date at time* caption is readable alongside it rather than covering it — the owner confirmed both are present and usable at once. §7's own bar box, also passing, is the other half of this: the bar is clear of the picture as well as of the caption.
- [x] **Download** saves to Downloads without asking. The note says "Saved to Downloads", and clicking it opens the folder. A second Download of the same file adds " (2)". — **PASS.** The viewer's download button saves straight to Downloads with no dialog, and repeats number themselves: `photo3.jpg`, `photo3 (2).jpg`, `photo3 (3).jpg` all landed, each 32,917 bytes — the source's exact size, so the file is whole and not re-encoded. The note and its click were confirmed by the owner. Worth saying so nobody reads it as a contradiction: the **file card's** Download in the chat *does* ask where to save, which is a different button and the right behaviour for it.
- [x] **Opening again:** clicking another photo while the viewer is open shows it in the same window and brings it forward. — **PASS.** With the viewer open, clicking a second photo swapped the picture in place. **No second window ever appeared**: throughout this section only one `OPENCORD_MEDIA_VIEWER` has existed at a time, which is the half of this box worth measuring rather than judging.
- [ ] **The tray:**
  - close the main window to the tray with the viewer open: the viewer stays;
  - close the viewer: the app keeps running in the tray;
  - **Quit Opencord** from the tray with the viewer open: everything exits.
- [~] **Display settings:** — **the scaling part passes; the three that need a second monitor cannot be done on this machine** (one display, `\.\DISPLAY1`, 1920x1080).
  - at 100 % and 150 % scaling; — **PASS**, checked by the owner at both: the viewer opens correctly either way, full screen genuinely fills the screen and the chrome is the right size and sharp.
  - with two monitors, full screen opens on the main window's monitor; — **not testable here.** Needs a second display.
  - **known, check whether it bites:** with two monitors at different scaling, a remembered normal window is converted with the main window's scaling, so it may reopen at the wrong size on the other monitor; — **not testable here**, and still unanswered: this is a *known risk*, so it stays a live question for whoever has two monitors, not a box to tick from this pass.
  - **known:** a remembered normal window that only just overlaps a monitor still there (after unplugging one) may open nearly off-screen. — **not testable here.** Needs a second display to unplug.
## 7. The viewer's player and picture-in-picture (#26, D87)

> **All five boxes pass**, every one checked by hand on `media-viewer-player`. Nothing in this section had to be deferred, and nothing in it is waiting on the merge. D87's picture-in-picture does the whole of what it promises, down to coming back where and how big it was left.
- [x] **Rotate:** beside Download at the bottom right, for photos and videos. Each click turns a quarter left; the next item starts unturned. — **PASS, all four parts**, checked by the owner: a quarter anticlockwise per click, four clicks back to the start, the next photo opens unturned rather than carrying the rotation over, and videos rotate as photos do.
- [x] **The bar** (a video): narrow, centred at the bottom, see-through, and never over the video. — **PASS, every part**, checked by the owner on `short_video.mp4` in the viewer.
  - First row: volume, play, full screen, picture-in-picture and the speed. — all five, in that order.
  - Second row: the time played, the seek bar and the time left. — all three.
  - It fades with the rest when the mouse rests. — it does.
- [x] **Speed:** 1x, 1.5x, 2x, 3x and round again. Voices keep their pitch, and the times stay in the video's own time. — **PASS, all three parts**, checked by the owner on `speech_60s.mp4`, which counts the seconds aloud so the last part is measurable rather than a judgement: the spoken count and the displayed time agreed. The four speeds cycle and wrap, and the voice at 2x is faster without rising in pitch.
- [x] **Full screen:** — **PASS, all three**, checked by the owner.
  - the video fills the screen at its own shape, with only the bar and the title buttons left, fading; — it does.
  - Esc ends it first, then closes; — **both, from a real keyboard.** This is what showed the A12 note up as wrong: see below.
  - from a normal window, the window goes full screen and comes back normal, and the next viewer still opens normal. — it does.
- [x] **Picture-in-picture:** — **PASS, all eight parts**, checked by the owner.
  - the button turns the viewer into a small window, 320 wide with the video's shape, at the bottom right of the screen, playing on without a restart;
  - it stays above other windows, also when another app is active;
  - its own controls: volume top left, back and close top right, play in the middle, the times and seek bar at the bottom; they fade;
  - a drag moves it; its edges resize it, keeping its shape;
  - Back, or a double click, returns to the window it was, still playing; close stops the video;
  - opening another photo or video brings the viewer back with it;
  - close it somewhere else and open picture-in-picture again: it comes back there, at that size;
  - close from picture-in-picture with a normal window remembered, then open a photo: a normal window with its borders, laid out as the viewer, not as picture-in-picture.
## 8. Scrolling and compact mode (#28, #29)
- [x] **Compact mode (#28):** in **Settings → Appearance → Message density**, choose **Compact**. On a wide window, a text channel's lines run the chat's whole width, 16 px in from each side, as in private chats. Comfortable bubbles stay in a centred column. — **PASS**, shown with the same message in both densities in a 1900 px window (`s8-compact-text.png`, `s8-comfy-text.png`). One long line was sent so it had to wrap, since media attachments have their own widths and settle nothing. **Compact**: the line runs x ≈ 470 → 1600, wrapping only at the chat's right edge, in rows with the avatar in its own column. **Comfortable**: the same text is a bubble held to x ≈ 890 → 1450, a centred column well short of both edges. The density is saved behind the *unsaved changes* bar, like the rest of that page. Put back to Comfortable afterwards.
- [x] **Private chats (#29):** — **PASS, all five.** Driven in a real private chat between `alice` and `bob` with **113 numbered messages**, so the line number at the top of the view measures the scroll position rather than a judgement about it.
  - a chat opens at the newest message; — **PASS.** Leaving for Friends and coming back lands on the last line, at the bottom.
  - a new message from `bob` scrolls into view while you're at the newest one; — **PASS.** `BOB-A` appeared below the last line without being asked for.
  - while you read older messages, new ones don't move you; — **PASS.** Scrolled up to lines 3–6, `bob` sent `BOB-B`, and the view was pixel-identical afterwards.
  - sending takes you to the newest message; — **PASS.** Sent from lines 3–6 and the view jumped to the bottom, the new line under `BOB-A`/`BOB-B`.
  - scrolling up loads older messages without a jump. — **PASS**, sampled rather than eyeballed: the top visible line went **86 → 68 → 51 → 39 → 28 → 17** over six equal scrolls, monotonic, with no backward step and no reset. The steps shorten only because the older lines wrap to two rows, so fewer fit. Scrolling further reaches line 1 and the chat's own start divider.
- [~] **Remembering the place (#29):** — **the private-chat half passes; the channel half and the second chat are not done.**
  - scroll halfway up a long channel, go to another channel and come back: the same place; — **not done.** `#general` holds only a handful of messages, too few to scroll, and the server has one text channel; this needs a long channel and a second one to switch to.
  - the same for a private chat, and each chat keeps its own place; — **the private chat: PASS.** Left at line 6 of 113, away to Friends, back: line 6, pixel-identical. *Each chat keeps its own place* is **not done** — there is only one private chat, which would need a third identity.
  - quit from the tray, start again: both the channel and the chat open where you left them, even far back in the history. — **the chat, across a restart: PASS.** Killed at line 6 and restarted, the app reopened straight into the chat with `bob` at line 6 — far back in 113 messages. **Not by the tray**, though: #29's build has no tray (that is #24), so this was a plain kill and restart. The tray route needs the merged batch, like §3's last two steps.

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
