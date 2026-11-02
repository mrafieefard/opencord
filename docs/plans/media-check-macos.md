# macOS check: media uploads and the media viewer's window

> For the owner, or a Claude Code session on the Mac (`docs/plans/platforms.md` has the rules for working there). Nothing in this list has been run on macOS yet:
> - the media uploads plan's macOS task (P2 in `docs/superpowers/plans/2026-10-18-media-uploads.md`), which was never done;
> - the media viewer's own window (D86, pull request #25), which only CI has compiled;
> - the viewer's player, rotate and picture-in-picture (D87, branch `feat/media-viewer-player`), which only CI has compiled.
>
> Fix macOS-only problems in macOS files only, on a branch from the one under test, as a pull request into `beta`. Shared problems go back to the Linux session.

## 0. Get the build

1. Check out `feat/media-viewer-player` (it carries `feat/media-viewer-window`), or `beta` once the 2026-11-01 batch is merged.
2. `flutter build macos`, or the app from the pull request's CI artifacts.
3. Use the same server and channel as the Windows check (`docs/plans/media-check-windows.md` §0).

## 1. Video playback first (upload plan P2 steps 1 to 3)

Videos don't play on macOS yet: `opencord-media`'s player is built only for Linux and Windows. Until this is done, the viewer shows "This video can't play on this computer yet" and offers Download.

1. **Build FFmpeg.** Write a new `app/tool/macos/build_ffmpeg.sh` that builds a decode-only universal FFmpeg:
   - the same flags as Task 32, plus `--enable-videotoolbox` and dav1d;
   - built for arm64 and x86_64, then merged with `lipo`;
   - the dylibs go in `Opencord.app/Contents/Frameworks`, with `install_name_tool` ids `@rpath/…`, signed with the app.
2. **The dependency.** `crates/opencord-media/Cargo.toml`'s macOS section gains `ffmpeg-next.workspace = true`, and `player`'s `cfg` includes macOS.
3. **The decision entry.** Write one that supersedes D34's "no FFmpeg on macOS" for playback only. Calls keep VideoToolbox.

## 2. Media uploads (upload plan P2 steps 4 to 6)

1. **Microphone.** The existing `NSMicrophoneUsageDescription` covers voice messages. Check the prompt on the first recording.
2. **The clipboard** (Task 21). Add `arboard = { version = "3.7.0", default-features = false, features = ["image-data"] }` to the macOS section of `crates/opencord-core/Cargo.toml`, and widen `clipboard.rs`'s `cfg` to `any(target_os = "linux", target_os = "macos")`. Check pasting a screenshot and "Copy image".
3. **Tests.** `cargo test` and `flutter test` pass, then the same live checks as Windows §1 steps 3 to 5: sending, playing, saving.

## 3. The media viewer's window (D86)

1. **Opening.**
   - Click a photo: a new window opens and covers the whole screen, menu bar and Dock included.
   - It opens on the current Space, without macOS's full-screen slide.
   - The chat and desktop show through, dimmed.
   - If the photo opens inside the main window instead, the engine's multi-view flag wasn't found: the log says "this Flutter engine has no multi-view flag". Note the Flutter version.
2. **Buttons.** The real traffic lights at the top left, and nothing of the app's own over them.
3. **Modes.**
   - Green gives a normal window, 800×600 the first time, with a shadow. Its edges resize it, down to 480×360, and dragging the background moves it.
   - Green again goes back to full screen. It's never macOS's own full screen.
   - A double click on the top strip, or Alt+Enter, switches too.
4. **Remembering.** Close the viewer in the normal window, then open another photo: it opens normal, at the same place and size. Do the same after restarting the app.
5. **Closing.**
   - Esc and the red light close only the viewer.
   - In full screen, a click beside the photo closes it. In the normal window it doesn't.
6. **Switching apps.** Cmd+Tab to another app while it's full screen: the viewer steps aside, and comes back when Opencord is active again.
7. **Moving around.**
   - The arrows and the side strips step through a message's photos.
   - The bottom-left text shows the count and the sender.
   - The controls fade, and come back on movement.
8. **Download** saves to Downloads without asking. The note opens the folder.
9. **Opening again.** Clicking another photo while the viewer is open shows it in the same window.
10. **Hiding and quitting.**
    - Hide the main window to the menu bar with the viewer open, then close the viewer: the app keeps running.
    - Cmd+Q with the viewer open quits cleanly.
11. **Screens.** Retina, plus a second screen that isn't: full screen opens on the main window's screen.

## 4. The viewer's player and picture-in-picture (D87, the pull request after #25)

Videos need §1 first: until FFmpeg is built for macOS, steps 2 to 5 can't be run.

1. **Rotate.** Beside Download at the bottom right, for photos and videos. Each click turns a quarter left; the next item starts unturned.
2. **The bar** (a video): narrow, centred at the bottom, see-through, and never over the video. Volume, play, full screen, picture-in-picture and the speed on the first row; the time played, the seek bar and the time left on the second. It fades with the rest when the mouse rests.
3. **Speed.** 1x, 1.5x, 2x, 3x and round again; voices keep their pitch, and the times stay in the video's own time.
4. **Full screen.** The video fills the screen at its own shape, only the bar and the title buttons left, fading. Esc ends it first, then closes. From a normal window the window goes full screen and comes back normal, and the next viewer still opens normal.
5. **Picture-in-picture.**
   - The button turns the viewer into a small window, 320 wide with the video's shape, at the bottom right of the screen, playing on without a restart.
   - It floats above other apps, on every Space, also beside an app in macOS full screen. No traffic lights.
   - Its own controls: volume top left, back and close top right, play in the middle, the times and seek bar at the bottom; they fade.
   - A drag moves it; its edges resize it, keeping its shape.
   - Back, or a double click, returns to the window it was, still playing; close stops the video.
   - Opening another photo or video brings the viewer back with it.
   - Close it somewhere else and open picture-in-picture again: it comes back there, at that size.

## 5. Report

List what failed, with screenshots, in the batch's thread, or as a pull request with the fixes.
