# Windows check: media uploads and the media viewer's window

> For the owner, or a Claude Code session on the Windows PC (`docs/plans/platforms.md` has the rules for working there). Nothing in this list has been run on Windows yet:
> - the media uploads plan's Windows task (P1 in `docs/superpowers/plans/2026-10-18-media-uploads.md`), which was never done;
> - the media viewer's own window (D86, pull request #25), which only CI has compiled;
> - the viewer's player, rotate and picture-in-picture (D87, branch `feat/media-viewer-player`), which only CI has compiled.
>
> Fix Windows-only problems in Windows files only, on a branch from the one under test, as a pull request into `beta`. Shared problems go back to the Linux session.

## 0. Get the build

1. Check out `feat/media-viewer-player` (it carries `feat/media-viewer-window`), or `beta` once the 2026-11-01 batch is merged.
2. `flutter build windows`, or run the installer from the pull request's CI artifacts.
3. Use a server that has uploads switched on: the test server, or `opencord-server` on the LAN. Use one channel with at least:
   - a photo;
   - a message with three photos and a video;
   - a PNG with transparency;
   - a voice message;
   - a PDF.

## 1. Media uploads (upload plan P1)

1. **FFmpeg.** The shipped FFmpeg DLLs contain the decoders and demuxers from Task 32, `libdav1d` included. Check with `ffmpeg -decoders` from the same build, or with the build's configure line. If any is missing, switch to an LGPL shared build that has them, and update `docs/packaging.md` (Windows).
2. **Tests.** `cargo test --workspace --lib --bins --tests` and `flutter test` pass.
3. **Sending.**
   - Drop a photo, a video and a PDF on the composer.
   - The upload bar moves, and Cancel stops one.
   - The sent photo shows in the grid at its size, with no jump.
   - The video tile shows its first frame and length.
4. **Playing.**
   - A video plays in the chat, then in the viewer, with sound.
   - Seeking works.
   - A voice message records and plays.
   - Playing a voice message pauses the video.
5. **Saving.**
   - Download from a file card asks where to save, and saves there.
   - "Save all" puts several files in one folder, beside the files already there.
6. **The clipboard** (upload plan Task 21). `arboard` on Windows needs `clipboard-win` and `error-code`, both BSL-1.0, a licence the licence list doesn't include. Ask the owner which way to go:
   - allow BSL-1.0, add `arboard` to the Windows section of `crates/opencord-core/Cargo.toml`, and widen `clipboard.rs`'s `cfg`;
   - or write `image` and `set_image` for Windows with `windows-sys`.

   Then check pasting a screenshot and "Copy image".

## 2. The media viewer's window (D86)

1. **Opening.**
   - Click a photo: a new window opens and covers the whole monitor, taskbar included.
   - It has a taskbar button of its own.
   - The chat and desktop show through behind the photo, dimmed. If the background is solid dark instead, transparency failed: note it, with a screenshot. That's the known risk.
2. **Buttons.**
   - Top right: Windows' minimize, restore and close, white.
   - Close hovers inverted, as the main window's does.
3. **Modes.**
   - Restore gives a normal window, 800×600 the first time. Its borders resize it, down to 480×360, and dragging the background moves it.
   - Maximize, Win+Up, dragging it to the top of the screen, and Aero Snap all turn it back into full screen.
   - A double click on the top strip, or Alt+Enter, switches too.
4. **Remembering.** Close the viewer in the normal window and open another photo: it opens normal, at the same place and size. Do the same after restarting the app.
5. **Closing.**
   - Esc closes the viewer.
   - In full screen, a click beside the photo closes it. In the normal window it doesn't.
   - Alt+F4 and the taskbar button's Close close only the viewer; the app stays.
6. **Moving around.**
   - In the three-photo message: the left and right arrows, and the side strips, step through it.
   - "Photo 2 of 4" and "name · date at time" show at the bottom left.
   - The controls fade after a second without moving the mouse, and come back on movement.
7. **The video.** It plays in the viewer with its own controls, and the viewer's text is not over them.
8. **Download** saves to Downloads without asking. The note says "Saved to Downloads", and clicking it opens the folder. A second Download of the same file adds " (2)".
9. **Opening again.** Clicking another photo while the viewer is open shows it in the same window and brings it forward.
10. **The tray.**
    - Close the main window to the tray with the viewer open: the viewer stays.
    - Close the viewer: the app keeps running in the tray.
    - Quit from the tray with the viewer open: everything exits.
11. **Display settings.**
    - At 100 % and 150 % scaling.
    - With two monitors: full screen opens on the main window's monitor.

## 3. The viewer's player and picture-in-picture (D87, the pull request after #25)

1. **Rotate.** Beside Download at the bottom right, for photos and videos. Each click turns a quarter left; the next item starts unturned.
2. **The bar** (a video): narrow, centred at the bottom, see-through, and never over the video. Volume, play, full screen, picture-in-picture and the speed on the first row; the time played, the seek bar and the time left on the second. It fades with the rest when the mouse rests.
3. **Speed.** 1x, 1.5x, 2x, 3x and round again; voices keep their pitch, and the times stay in the video's own time.
4. **Full screen.** The video fills the screen at its own shape, only the bar and the title buttons left, fading. Esc ends it first, then closes. From a normal window the window goes full screen and comes back normal, and the next viewer still opens normal.
5. **Picture-in-picture.**
   - The button turns the viewer into a small window, 320 wide with the video's shape, at the bottom right of the screen, playing on without a restart.
   - It stays above other windows, also when another app is active.
   - Its own controls: volume top left, back and close top right, play in the middle, the times and seek bar at the bottom; they fade.
   - A drag moves it; its edges resize it, keeping its shape.
   - Back, or a double click, returns to the window it was, still playing; close stops the video.
   - Opening another photo or video brings the viewer back with it.
   - Close it somewhere else and open picture-in-picture again: it comes back there, at that size.

## 4. Report

List what failed, with screenshots, in the batch's thread, or as a pull request with the fixes.
