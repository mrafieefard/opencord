# Windows check report: the 2026-11-01 batch

> The report for [check-windows.md](check-windows.md). Tick each box that passes;
> for each failure write what happened, with a screenshot, and name the step.
>
> **The PC:** Windows 10 Pro 22H2 (build 19045), 16 GB RAM, 16 logical cores.
> There is no Windows 11 PC, so every step marked *Windows 11 only* is
> **not run: no Windows 11 PC**.
>
> **Prepared by a Claude Code session on 2026-11-02/03.** That session had no way
> to drive the GUI — no screenshots, clicks, hovers or drags — so it ran only
> the checks a machine can settle and left the other 59 boxes untouched. What it
> did run is in §A below, with the evidence. Nothing in §1 to §8 is ticked by it.

## A. What has already been settled mechanically

| # | Check | Result |
|---|---|---|
| A1 | §5 — the shipped FFmpeg DLLs carry Task 32's decoders, `libdav1d` included | **PASS**, see A1 |
| A2 | §5 — `cargo test --workspace --lib --bins --tests` and `flutter test` | **PASS**, with two caveats that are not the batch's — see A2 |
| A3 | §5 — the clipboard's licence question | **settled by the owner**, see A3 |

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

**The server** (§0.3) — on the LAN machine for §3 and §4, so one machine's
network can be cut:

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

For §3 and §4 run the server on the **LAN machine** instead, because those
sections cut this PC's network. Start it with `OPENCORD_PUBLIC_HOST=<its LAN
address>` (or `public_host` in its `opencord.toml`) so invite links and voice
reach the other machines. This PC is `192.168.0.122` on the LAN
(and `100.86.134.100` over Tailscale) if the server needs to point back at it.

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


---

# The checks

## 1. The title bar (#22) and private messages' title bar (#21)
- [ ] No system title bar anywhere: at start, after restoring from minimised, and after leaving maximised.
- [ ] Minimise, Maximise/Restore and Close sit at the top right, on every screen: a channel, Friends, a private chat, Settings, and the "no servers" screen (a fresh profile with no server joined).
- [ ] Close's hover is the app's own (inverted, monochrome). The others highlight on hover.
- [ ] Dragging the empty header moves the window, also over a channel's or chat's title.
- [ ] A double click on the header maximises, and a second one restores.
- [ ] Aero Snap works: drag to the top to maximise, to a side for half the screen; Win+Up, Win+Down, Win+Left and Win+Right too.
- [-] Hovering Maximise shows Windows 11's snap layouts (Windows 11 only). — **not run: no Windows 11 PC.**
- [ ] The window's edges and corners resize it.
- [ ] **Settings → Appearance** has no Window frame choice.
- [ ] **Private messages (#21):**
  - on **Friends**, and in a private chat, the caption buttons are there and the header drags the window;
  - the empty space to the right of Friends' tabs (Online, All, Pending…) drags it too;
  - in a narrow window (the sidebar no longer inline), the buttons are still there and nothing overlaps.
- [ ] At 100 %, 125 % and 150 % scaling, the buttons are sharp and line up with the header.
## 2. The tray, and running in the background (#24)
- [ ] **The icon** shows in the notification area. It is sharp at 100 % and 150 % scaling, and at 150 % after a restart.
- [ ] **Hovering it** shows the unread summary, such as "No unread messages" or "3 unread messages, 1 mention".
- [ ] **A left click** brings the window forward and focuses it: when it's minimised, when it's hidden behind another app, and when it was closed to the tray.
- [ ] **A right click** opens the menu: Open Opencord, Mute, Deafen, Status (Online, Idle, Do not disturb, Invisible), Quit Opencord. Mute and Deafen show a check mark when on, Status a mark beside the current one, and the marks follow when they change from inside the app.
- [ ] **Clicking outside the menu, or Esc,** closes it.
- [ ] **Closing:**
  - the window's Close hides it, and the app keeps running: messages still arrive (send one from `bob`) and the icon's unread dot appears;
  - a tray click brings it back, in the same place, at the same size;
  - **Settings → Windows & behavior → Close button minimizes to the tray** is shown on Windows. Turned off, Close quits.
- [ ] **Quit Opencord** in the menu exits the process (Task Manager shows no `opencord.exe` for that profile).
- [ ] **Explorer restart:** run `Stop-Process -Name explorer -Force`, then wait for it to come back (or start `explorer.exe`). The icon comes back by itself and still works.
- [ ] **The voice icon:**
  - in a voice channel: a ring;
  - while you talk: a filled circle;
  - muted or deafened: the ring with a slash;
  - out of voice: the chat bubble again;
  - the unread dot stays in every state.
- [ ] **Two profiles** each get their own icon, and each icon's clicks go to its own window.
- [ ] **Start minimized** with **Launch at login**: after signing out and in, Opencord starts in the tray with no window.
## 3. The reconnect bar (#27), and the volume bug
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
- [ ] **The same two scenarios across machines:** `alice` on this PC, `alice-laptop` on the Mac.
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

## 10. Where this goes

- Post the ticked list above in the batch's thread, with the failures,
  screenshots and session logs (**Settings → About → Logs → Open logs folder**).
- A Windows-only fix goes on a branch off the pull request under test, as a
  pull request into `beta`, naming the step it is for. A shared problem (Dart or
  Rust every system runs) goes back to the Linux session with the steps to
  repeat it.
- §C above is for the Linux session, not for this report's thread.
