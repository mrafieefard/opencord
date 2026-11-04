# Windows check: the 2026-11-01 batch

> For the owner, or a Claude Code session on the Windows PC (`docs/plans/platforms.md` has the rules for working there). This file is everything the batch of pull requests #21 to #29 still needs on Windows. CI has built and tested each of them on Windows, but nobody has run any of it there by hand.
>
> It replaces `docs/plans/media-check-windows.md` (from #25 and #26): sections 5 to 7 here hold that file's checks.
>
> - **The PC:** Windows 10 22H2 (build 19045). Wherever a step says Windows 11, use a Windows 11 PC if there is one, or write "not run: no Windows 11 PC" in the report.
> - **Fixes:** fix Windows-only problems in Windows files only (`app/windows/**`, the `windows.rs` and `*_windows.*` files), on a branch from the one under test, as a pull request into `beta`. Run `cargo test` and `flutter test` for what you changed, plus `flutter analyze`, before pushing. Never push to `beta`, `canary` or `main`. Shared problems (Dart or Rust code that every system runs) go back to the Linux session, with steps to repeat them.

## 0. Before you start

### 0.1 The build

Test `beta` once the owner has merged the whole batch: everything below is then in one build.

```powershell
git fetch origin
git switch beta
git pull
flutter build windows --release
```

The app is then `app\build\windows\x64\runner\Release\opencord.exe`.

**If you test before the batch is merged,** each section names its pull request. Use that pull request's branch, or download its CI build:

```powershell
gh run list --branch <branch> --limit 1      # gives the run id
gh run download <run-id> -n opencord-windows-x64.zip -D C:\oc\<branch>
gh run download <run-id> -n opencord-server-windows-x64.zip -D C:\oc\server
```

| Section | Pull request | Branch |
|---|---|---|
| 1 | #21, #22 | `fix/private-messages-titlebar`, `feat/custom-titlebar-everywhere` |
| 2 | #24 | `fix/tray-background` |
| 3 | #27 | `feat/voice-reconnect-bar` |
| 4 | #23 | `feat/private-message-keys` |
| 5 to 7 | #25, #26 | `feat/media-viewer-player` (it carries #22, #24 and #25) |
| 8 | #28, #29 | `fix/compact-full-width`, `fix/private-chat-scroll` |

### 0.2 Two clients on one PC

Each profile has its own data, keychain entries and single-instance lock. Open two PowerShell windows:

```powershell
$env:OPENCORD_PROFILE = "alice"; & .\opencord.exe
```

```powershell
$env:OPENCORD_PROFILE = "bob"; & .\opencord.exe
```

Sections 3 and 4 also need a client on **another machine** (the Mac or the Linux dev machine), because they cut one machine's network.

### 0.3 The server

Use a server built from the same code as the app. The test server and opencord2 run older versions and lack #23's server changes.

- **On this PC:** `cargo run -p opencord-server -- --config target\test-server\opencord.toml` from the repo, or `opencord-server.exe --config C:\oc\test-server\opencord.toml` from the CI zip. The config is created with defaults the first time, and the server's data goes in a `data` folder beside it.
- **On the LAN:** the same command on the Linux dev machine (`--config target/test-server/opencord.toml`). Use this one for sections 3 and 4. Start it with `OPENCORD_PUBLIC_HOST=<its LAN address>`, or set `public_host` in its `opencord.toml`, so invite links and voice reach it from the other machines.

The first start prints an owner claim token and an invite link:

- `alice` adds the server with the claim token (**Add a server**, under "I'm the owner") and becomes its owner;
- every other profile joins with the invite link, and `bob` stays an ordinary member.

Uploads are on by default: files go to `data\attachments`.

### 0.4 Test material

In one text channel, as `alice`:

- a photo;
- a message with three photos and a video;
- a PNG with transparency;
- a voice message;
- a PDF;
- a video of a minute or more, with speech (for speed and picture-in-picture).

Also create a voice channel, and two channels with long histories (200+ messages each, for section 8). A script or a few pastes will do.

### 0.5 How to report

Tick each box that passes. For each one that fails, write what happened, with a screenshot. Section 10 says where the report goes.

## 1. The title bar (#22) and private messages' title bar (#21)

Windows is now always frameless, with the app's own caption buttons. The Window frame setting is gone.

- [ ] No system title bar anywhere: at start, after restoring from minimised, and after leaving maximised.
- [ ] Minimise, Maximise/Restore and Close sit at the top right, on every screen: a channel, Friends, a private chat, Settings, and the "no servers" screen (a fresh profile with no server joined).
- [ ] Close's hover is the app's own (inverted, monochrome). The others highlight on hover.
- [ ] Dragging the empty header moves the window, also over a channel's or chat's title.
- [ ] A double click on the header maximises, and a second one restores.
- [ ] Aero Snap works: drag to the top to maximise, to a side for half the screen; Win+Up, Win+Down, Win+Left and Win+Right too.
- [ ] Hovering Maximise shows Windows 11's snap layouts (Windows 11 only).
- [ ] The window's edges and corners resize it.
- [ ] **Settings → Appearance** has no Window frame choice.
- [ ] **Private messages (#21):**
  - on **Friends**, and in a private chat, the caption buttons are there and the header drags the window;
  - the empty space to the right of Friends' tabs (Online, All, Pending…) drags it too;
  - in a narrow window (the sidebar no longer inline), the buttons are still there and nothing overlaps.
- [ ] At 100 %, 125 % and 150 % scaling, the buttons are sharp and line up with the header.

## 2. The tray, and running in the background (#24)

The tray didn't exist on Windows before this pull request. Run every step on Windows 10, then again on Windows 11 if possible.

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

A bar across the top of the app offers to get back into a call this device left without meaning to. It names the channel, and has **Reconnect** and a ×. Put `alice` on this PC and `bob` on the other machine.

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

From `docs/private-messages.md` §Checking it by hand. Make `alice` and `bob` friends first (Scenario 1 there).

- [ ] **Scenario 3: a second device of one identity.**
  1. Make `alice-laptop` with `alice`'s identity: **Export identity backup** on `alice`, then **Import identity** on a new profile.
  2. Before approving, `bob` writes to `alice`:
     - only `alice`'s main profile gets it;
     - `bob`'s chat says `alice`'s keys changed;
     - `alice`'s **Friends** page shows "A device signed in as you with different keys."
  3. `alice` clicks **Approve**. `alice-laptop` shows the same 16 words under **Compare words**. Click **They match** on both.
  4. `bob` writes again: both of `alice`'s profiles show it, and `bob`'s chat says "Alice signed in on a new device."
  5. `alice-laptop` replies: the message shows on both of `alice`'s profiles (as sent) and on `bob`'s.
- [ ] **Scenario 3b: comparing words, and a stolen identity.**
  1. In `alice`'s chat with `bob` and in `bob`'s chat with `alice`, click the lock: both show the same 16 words. **They match** on both turns the lock into a shield.
  2. Make a `thief` profile with `alice`'s identity, and don't approve it.
  3. `bob` writes to `alice`:
     - only `alice`'s approved profiles get it;
     - `bob`'s chat says `alice`'s keys changed, with **Compare words** and **Use new keys anyway**;
     - `thief` shows nothing.
  4. `thief` writes to `bob`: `bob` sees "A message from keys you haven't accepted.", not the text.
- [ ] **The same two scenarios across machines:** `alice` on this PC, `alice-laptop` on the Mac.
- [ ] **Restart:** after a restart of every profile, the shield and the approvals are still there, and new messages still decrypt.
- [ ] **Fresh start:** a profile that had private chats before this build says once that old chats were removed. Friends, requests, blocks and settings are kept.

## 5. Media uploads (the upload plan's P1)

Never run on Windows (P1 in `docs/superpowers/plans/2026-10-18-media-uploads.md`).

- [ ] **FFmpeg.** The shipped FFmpeg DLLs contain the decoders and demuxers from the upload plan's Task 32, `libdav1d` included. Check with `ffmpeg -decoders` from the same build, or with the build's configure line. If any is missing, switch to an LGPL shared build that has them, and update `docs/packaging.md` (Windows).
- [ ] **Tests:** `cargo test --workspace --lib --bins --tests` and `flutter test` pass.
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
- [ ] **The clipboard** (upload plan Task 21). **A decision for the owner first.** `arboard` on Windows needs `clipboard-win` and `error-code`, both BSL-1.0, a licence the licence list doesn't include. Either:
  - allow BSL-1.0, add `arboard` to the Windows section of `crates/opencord-core/Cargo.toml`, and widen `clipboard.rs`'s `cfg`;
  - or write `image` and `set_image` for Windows with `windows-sys`.

  Then check pasting a screenshot (Win+Shift+S, then Ctrl+V in the composer) and **Copy image** on a photo.

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

Nothing in these is Windows code. A short pass is enough.

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

- **Notifications on Windows** (through the tray icon's balloon): planned, not built. Their checks will be added here.
- **Windows 11 only:** sharing a screen's sound needs build 20348 or later. It was never live-tested, because this PC runs Windows 10 (`docs/plans/windows.md`).

## 10. Report

- Post the ticked list in the batch's thread, with the failures, screenshots and session logs.
- Fixes for Windows-only problems go in a pull request from a branch off the one under test, into `beta` (see the top of this file). Name the step each fix is for.
- For a shared problem, write the steps to repeat it for the Linux session.
