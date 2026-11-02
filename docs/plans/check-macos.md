# macOS check: the 2026-11-01 batch

> For the owner, or a Claude Code session on the Mac (`docs/plans/platforms.md` has the rules for working there). This file is everything the batch of pull requests #21 to #29 still needs on macOS. CI has built and tested each of them on macOS, but nobody has run any of it there by hand.
>
> It replaces `docs/plans/media-check-macos.md` (from #25 and #26): sections 5 to 8 here hold that file's work and checks.
>
> - **The Mac:** an Intel MacBook Pro (2019, i7-9750H, macOS 15.8.1). Tools come from MacPorts (`/opt/local/bin`), not Homebrew.
> - **Two parts of this file are building, not checking:** video playback (§5) and the clipboard (§6.2). Videos can't play on macOS until §5 is done, so §7's video steps and all of §8 wait for it.
> - **Fixes:** fix macOS-only problems in macOS files only (`app/macos/**`, the `macos.rs` and `macos/` files, `app/tool/macos/`), on a branch from the one under test, as a pull request into `beta`. Run `cargo test` and `flutter test` for what you changed, plus `flutter analyze`, before pushing. Never push to `beta`, `canary` or `main`. Shared problems (Dart or Rust code that every system runs) go back to the Linux session, with steps to repeat them.

## 0. Before you start

### 0.1 The build

Test `beta` once the owner has merged the whole batch: everything below is then in one build.

```sh
git fetch origin
git switch beta
git pull
flutter build macos --release
```

The app is then `app/build/macos/Build/Products/Release/Opencord.app`.

**If you test before the batch is merged,** each section names its pull request. Use that pull request's branch, or download its CI build:

```sh
gh run list --branch <branch> --limit 1      # gives the run id
gh run download <run-id> -n opencord-macos.zip -D ~/oc/<branch>
gh run download <run-id> -n opencord-server-macos.tar.gz -D ~/oc/server
xattr -dr com.apple.quarantine ~/oc/<branch>/Opencord.app   # the CI build isn't notarised
```

| Section | Pull request | Branch |
|---|---|---|
| 1 | #21, #22 | `fix/private-messages-titlebar`, `feat/custom-titlebar-everywhere` |
| 2 | #24 | `fix/tray-background` |
| 3 | #27 | `feat/voice-reconnect-bar` |
| 4 | #23 | `feat/private-message-keys` |
| 5 to 8 | #25, #26 | `feat/media-viewer-player` (it carries #22, #24 and #25) |
| 9 | #28, #29 | `fix/compact-full-width`, `fix/private-chat-scroll` |

### 0.2 Two clients on one Mac

Each profile has its own data and keychain entries. Start the app's binary from two Terminal windows; `open` would bring back the one already running.

```sh
OPENCORD_PROFILE=alice ./Opencord.app/Contents/MacOS/Opencord
```

```sh
OPENCORD_PROFILE=bob ./Opencord.app/Contents/MacOS/Opencord
```

Allow the keychain prompts the first time each profile starts. Sections 3 and 4 also need a client on **another machine** (the Windows PC or the Linux dev machine), because they cut one machine's network.

### 0.3 The server

Use a server built from the same code as the app. The test server and opencord2 run older versions and lack #23's server changes.

- **On this Mac:** `cargo run -p opencord-server -- --config target/test-server/opencord.toml` from the repo, or `./opencord-server --config ~/oc/test-server/opencord.toml` from the CI archive. The config is created with defaults the first time, and the server's data goes in a `data` folder beside it.
- **On the LAN:** the same command on the Linux dev machine. Use this one for sections 3 and 4. Start it with `OPENCORD_PUBLIC_HOST=<its LAN address>`, or set `public_host` in its `opencord.toml`, so invite links and voice reach it from the other machines.

The first start prints an owner claim token and an invite link:

- `alice` adds the server with the claim token (**Add a server**, under "I'm the owner") and becomes its owner;
- every other profile joins with the invite link, and `bob` stays an ordinary member.

Uploads are on by default: files go to `data/attachments`.

### 0.4 Test material

In one text channel, as `alice`:

- a photo;
- a message with three photos and a video;
- a PNG with transparency;
- a voice message;
- a PDF;
- a video of a minute or more, with speech (for speed and picture-in-picture).

Also create a voice channel, and two channels with long histories (200+ messages each, for section 9). A script or a few pastes will do.

### 0.5 How to report

Tick each box that passes. For each one that fails, write what happened, with a screenshot. Section 11 says where the report goes.

## 1. The title bar (#22) and private messages' title bar (#21)

macOS keeps its real traffic lights (the owner's choice). They sit over the server rail at the top left, on a hidden, transparent title bar, as in Discord. The option to bring back the full system title bar is gone.

- [ ] No system title bar or title text, on every screen: a channel, Friends, a private chat, Settings, and the "no servers" screen (a fresh profile with no server joined).
- [ ] The traffic lights are at the top left, centred on the header's height, and nothing of the app's own draws under or over them.
- [ ] Red closes the window (section 2 says what happens next), yellow minimises it, and green zooms. Holding Option on green, or hovering it, offers the tiling choices.
- [ ] Dragging the empty header moves the window, also over a channel's or chat's title.
- [ ] A double click on the header does what **System Settings → Desktop & Dock → Double-click a window's title bar to** says (zoom by default).
- [ ] The window's edges and corners resize it.
- [ ] In macOS full screen (green, or Ctrl+Cmd+F), the header isn't cut, and the traffic lights come back when you point at the top of the screen.
- [ ] **Settings → Appearance** has no Window frame choice.
- [ ] **Private messages (#21):**
  - on **Friends**, and in a private chat, the header drags the window;
  - the empty space to the right of Friends' tabs (Online, All, Pending…) drags it too;
  - in a narrow window (the sidebar no longer inline), nothing overlaps the traffic lights.
- [ ] On the Retina screen and on a second screen that isn't Retina, the traffic lights and the header line up.

## 2. The menu bar icon, and running in the background (#24)

The menu bar icon didn't exist on macOS before this pull request. It is an `NSStatusItem` with a template image.

- [ ] **The icon** shows in the menu bar, sharp on Retina and on a non-Retina screen.
- [ ] **Dark and light menu bars:** it follows **System Settings → Appearance** (Light, Dark, Auto), and a light wallpaper with a dark menu bar.
- [ ] **A click** opens the window and brings Opencord forward: when it's minimised, hidden behind another app, or closed with red.
- [ ] **A right click** opens the menu: Open Opencord, Mute, Deafen, Status (Online, Idle, Do not disturb, Invisible), Quit Opencord. Mute and Deafen show a check mark when on, Status a mark beside the current one, and the marks follow when they change from inside the app.
- [ ] **Closing:**
  - red closes the window, and the app keeps running in the Dock: messages still arrive (send one from `bob`) and the icon's unread dot appears;
  - a Dock click or a menu bar click brings it back, in the same place, at the same size;
  - **Settings → Windows & behavior** shows no "Close button minimizes to the tray" on a Mac. That's expected.
- [ ] **Quit Opencord** in the menu and **⌘Q** both quit the process (Activity Monitor shows no Opencord for that profile).
- [ ] **The voice icon:**
  - in a voice channel: a ring;
  - while you talk: a filled circle;
  - muted or deafened: the ring with a slash;
  - out of voice: the chat bubble again;
  - the unread dot stays in every state.
- [ ] **Two profiles** each get their own icon, and each icon's clicks go to its own window.
- [ ] **A crowded menu bar:** with many icons, macOS may hide Opencord's. Note whether it does; the app keeps working without it.

## 3. The reconnect bar (#27), and the volume bug

A bar across the top of the app offers to get back into a call this device left without meaning to. It names the channel, and has **Reconnect** and a ×. Put `alice` on this Mac and `bob` on the other machine.

- [ ] **The app closed in a call.**
  1. `alice` joins the voice channel.
  2. Force quit her app: Activity Monitor → Force Quit, or `kill -9 <pid>` (`pgrep -l Opencord` lists them).
  3. Start it again: the bar says "Opencord closed while you were in <channel>."
  4. **Reconnect** puts her back in the call, and the bar goes.
- [ ] **Offered once:** force quit again while out of the call, and start it again. No bar.
- [ ] **The × hides the bar**, and doesn't come back after a restart.
- [ ] **Joining any call hides it:** joining another voice channel while the bar shows.
- [ ] **Another device took the call.**
  1. Make a second profile with `alice`'s identity: export it from **Settings → Identity & keys → Backup → Export identity backup**, start `OPENCORD_PROFILE=alice2`, and use **Import identity** there.
  2. `alice` joins the voice channel, then `alice2` joins the same one.
  3. `alice` shows "Your call in <channel> moved to another device."
  4. **Reconnect** on `alice` takes the call back, and `alice2` is out of it.
- [ ] **No bar after:**
  - leaving the call;
  - `bob`, given Move Members, disconnects `alice` from the member menu;
  - `bob` moves `alice` to another channel;
  - **Quit Opencord** from the menu bar menu while in a call, then a restart.
- [ ] **Known gap, confirm it:** ⌘Q while in a call, then a restart, *does* show "Opencord closed while you were in <channel>." ⌘Q doesn't go through the app's quit path (D88). Note it, but it isn't a failure.
- [ ] **Getting back in was refused.**
  1. In the voice channel's settings set **User limit** to 1, and remove Move Members from `alice`'s roles.
  2. `alice` joins it.
  3. Turn Wi-Fi off (or unplug the network), and wait until `bob` sees `alice` leave the channel.
  4. `bob` (the owner, who can pass the limit) joins the channel.
  5. Turn Wi-Fi on: `alice` shows "Couldn't get back into <channel>: <the server's reason>".
  6. **Reconnect** while the channel is still full keeps the bar and shows the reason in a toast.
  7. When `bob` leaves, **Reconnect** works.
- [ ] **The volume bug** (reported, never reproduced):
  1. In a call with `bob`, set his **User volume** in `alice`'s member menu to 40 %.
  2. Turn Wi-Fi off and on so `alice` drops and the app rejoins by itself (the voice panel shows reconnecting, then connected).
  3. His volume is still 40 %, and he sounds as quiet as before.
  4. Force quit in the call and use **Reconnect**: still 40 %.
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
- [ ] **The same two scenarios across machines:** `alice` on the Windows PC, `alice-laptop` on this Mac.
- [ ] **Restart:** after a restart of every profile, the shield and the approvals are still there, and new messages still decrypt. The keys live in the keychain: no prompt should ask for them again after the first time.
- [ ] **Fresh start:** a profile that had private chats before this build says once that old chats were removed. Friends, requests, blocks and settings are kept.

## 5. Build: video playback (the upload plan's P2, steps 1 to 3)

Videos don't play on macOS yet: `opencord-media`'s player is built only for Linux and Windows. Until this is done, the viewer shows "This video can't play on this computer yet" and offers Download. Do this on its own branch from the one under test, as a pull request into `beta`.

1. **Build FFmpeg.** Write a new `app/tool/macos/build_ffmpeg.sh` that builds a decode-only universal FFmpeg:
   - the same flags as the upload plan's Task 32 (`docs/superpowers/plans/2026-10-18-media-uploads.md`), plus `--enable-videotoolbox` and dav1d (`libdav1d`, built with meson and ninja from MacPorts);
   - built for arm64 and x86_64 (this Mac is Intel, so arm64 is a cross build with `-arch arm64`), then merged with `lipo`;
   - the dylibs go in `Opencord.app/Contents/Frameworks`, with `install_name_tool` ids `@rpath/…`, signed with the app;
   - LGPL only: no `--enable-gpl` or `--enable-nonfree`. Record the licence and the build in `docs/packaging.md` (macOS).
2. **The dependency.** `crates/opencord-media/Cargo.toml`'s macOS section gains `ffmpeg-next.workspace = true`, and `player`'s `cfg` includes macOS.
3. **CI.** The macOS jobs run the script (or restore its result from a cache) before building, as the Windows jobs do for their FFmpeg.
4. **The decision entry.** Write one in `docs/decisions.md` that replaces D34's "no FFmpeg on macOS" for playback only. Calls keep VideoToolbox. Take the next free D number when the pull request is opened.
5. **Check:**
   - [ ] `cargo test -p opencord-media` passes, with the player's tests running and not skipped;
   - [ ] `otool -L` on the app's binary and dylibs shows only `@rpath` and system paths;
   - [ ] the app, copied to another user account or Mac without MacPorts, still plays a video.

## 6. Media uploads (the upload plan's P2, steps 4 to 6)

1. - [ ] **Microphone.** The existing `NSMicrophoneUsageDescription` covers voice messages. The first recording shows the system prompt, and recording works after Allow.
2. **Build: the clipboard** (the upload plan's Task 21). Add `arboard = { version = "3.7.0", default-features = false, features = ["image-data"] }` to the macOS section of `crates/opencord-core/Cargo.toml`, and widen `clipboard.rs`'s `cfg` to `any(target_os = "linux", target_os = "macos")`. Then:
   - [ ] pasting a screenshot (Cmd+Ctrl+Shift+4, then Cmd+V in the composer) attaches it;
   - [ ] **Copy image** on a photo, then Cmd+V in Preview's **File → New from Clipboard**, gives the photo.
3. - [ ] **Tests:** `cargo test --workspace --lib --bins --tests` and `flutter test` pass.
4. - [ ] **Sending:**
   - drop a photo, a video and a PDF on the composer;
   - the upload bar moves, and Cancel stops one;
   - the sent photo shows in the grid at its size, with no jump;
   - the video tile shows its first frame and its length.
5. - [ ] **Playing** (needs §5):
   - a video plays in the chat, then in the viewer, with sound;
   - seeking works;
   - a voice message records and plays;
   - playing a voice message pauses the video.
6. - [ ] **Saving:**
   - Download on a file card asks where to save, and saves there;
   - **Save all** puts several files in one folder, beside the files already there.

## 7. The media viewer's window (#25, D86)

- [ ] **Opening:**
  - click a photo: a new window opens and covers the whole screen, menu bar and Dock included;
  - it opens on the current Space, without macOS's full-screen slide;
  - the chat and desktop show through, dimmed;
  - if the photo opens inside the main window instead, the engine's multi-view flag wasn't found: the log says "this Flutter engine has no multi-view flag". Note the Flutter version.
- [ ] **Buttons:** the real traffic lights at the top left, and nothing of the app's own over them.
- [ ] **Modes:**
  - green gives a normal window, 800×600 the first time, with a shadow. Its edges resize it, down to 480×360, and dragging the background moves it;
  - green again goes back to full screen. It's never macOS's own full screen;
  - a double click on the top strip, or Alt+Enter, switches too.
- [ ] **Remembering:** close the viewer in the normal window, then open another photo: it opens normal, at the same place and size. The same after restarting the app.
- [ ] **Closing:**
  - Esc and the red light close only the viewer. Esc works right after typing a letter in it too;
  - in full screen, a click beside the photo closes it. In the normal window it doesn't.
- [ ] **Switching apps:** Cmd+Tab to another app while it's full screen: the viewer steps aside, and comes back when Opencord is active again.
- [ ] **Moving around:**
  - the arrows and the side strips step through a message's photos;
  - the bottom-left text shows the count and the sender;
  - the controls fade, and come back on movement.
- [ ] **Zoom** works across the whole screen, not only the photo's first size.
- [ ] **Download** saves to Downloads without asking. The note opens the folder.
- [ ] **Opening again:** clicking another photo while the viewer is open shows it in the same window.
- [ ] **Hiding and quitting:**
  - close the main window with red while the viewer is open, then close the viewer: the app keeps running;
  - Cmd+Q with the viewer open quits cleanly;
  - **known, check whether it bites:** with the main window closed and a normal (not full screen) viewer open, a Dock click may not bring the main window back. Note what happens.
- [ ] **Screens:** Retina, plus a second screen that isn't: full screen opens on the main window's screen.
  - **known:** a remembered normal window that only just overlaps a screen still there (after unplugging one) may open nearly off-screen.

## 8. The viewer's player and picture-in-picture (#26, D87)

Videos need §5 first: until FFmpeg is built for macOS, only the first step can be run.

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
  - it floats above other apps, on every Space, also beside an app in macOS full screen. No traffic lights;
  - its own controls: volume top left, back and close top right, play in the middle, the times and seek bar at the bottom; they fade;
  - a drag moves it; its edges resize it, keeping its shape;
  - Back, or a double click, returns to the window it was, still playing; close stops the video;
  - opening another photo or video brings the viewer back with it;
  - close it somewhere else and open picture-in-picture again: it comes back there, at that size;
  - close from picture-in-picture with a normal window remembered, then open a photo: a normal window, laid out as the viewer, not as picture-in-picture.

## 9. Scrolling and compact mode (#28, #29)

Nothing in these is macOS code. A short pass is enough; also try trackpad scrolling, with its momentum.

- [ ] **Compact mode (#28):** in **Settings → Appearance → Message density**, choose **Compact**. On a wide window, a text channel's lines run the chat's whole width, 16 px in from each side, as in private chats. Comfortable bubbles stay in a centred column.
- [ ] **Private chats (#29):**
  - a chat opens at the newest message;
  - a new message from `bob` scrolls into view while you're at the newest one;
  - while you read older messages, new ones don't move you, also mid-swipe on the trackpad;
  - sending takes you to the newest message;
  - scrolling up loads older messages without a jump.
- [ ] **Remembering the place (#29):**
  - scroll halfway up a long channel, go to another channel and come back: the same place;
  - the same for a private chat, and each chat keeps its own place;
  - quit with ⌘Q, start again: both the channel and the chat open where you left them, even far back in the history.

## 10. Not in this list yet

- **Notifications on macOS** (through `UNUserNotificationCenter`): planned, not built. Their checks will be added here.

## 11. Report

- Post the ticked list in the batch's thread, with the failures, screenshots and session logs.
- The building work (§5, §6.2) and fixes for macOS-only problems go in pull requests from a branch off the one under test, into `beta` (see the top of this file). Name the step each one is for.
- For a shared problem, write the steps to repeat it for the Linux session.
