# The media viewer's player, rotate and picture-in-picture (D87)

The owner's list after their first look at the viewer's window (D86, pull request #25), with Telegram Desktop's player as the model (their screenshots, 2026-11-02).

## What the owner asked for

- **Rotate.** A button that rotates the photo or video. It's a general viewer button, beside Download at the bottom right, in the same place for photos and videos. Nothing else is added to the player's own controls.
- **The video's controls as Telegram's.**
  - A narrow, almost see-through bar, centred at the bottom.
  - The video is never under the bar.
  - It fades with the viewer's other controls when the mouse rests.
- **Speed.** The settings button of Telegram's bar becomes a speed button cycling 1x, 1.5x, 2x, 3x. The pitch stays, as Telegram's does (owner's answer).
- **Full screen.** A button that makes the video take the whole screen, keeping its shape. The bar fades there too.
- **Picture-in-picture, as Telegram's** (owner's answer):
  - a small window over other apps that keeps playing after the viewer closes;
  - it can be moved and resized, and has a way back to the viewer.
  - On Wayland an app can't keep its window above the others, so on Linux it may sit as an ordinary window (owner: acceptable).

## The bar (measured from the owner's screenshot)

- **Box:** 480 × 70, 7 above the window's bottom edge, centred, radius 6, black at half opacity.
- **First row**, its centres 22 from the bar's top:
  - the volume icon, then its slider (75 wide), on the left;
  - play/pause in the middle;
  - full screen, picture-in-picture and speed on the right, 36 apart.
- **Second row**, 54 from the top:
  - the time played on the left, as `00:00`;
  - the seek bar between;
  - the time left on the right, as `-00:11`.
- **Without picture-in-picture:** in the viewer inside the app, where there is no window of its own, the picture-in-picture button is left out.
- **Space for the bar:**
  - A video keeps 84 clear above and below itself (bar plus margins, the same at the top so it stays centred).
  - Photos keep the 54 of D86.
- **The bottom-left text** (what it is, who sent it) keeps to the left of the bar. Where less than 120 is left, it's hidden.

## Behaviour

- **Rotate**
  - Each click turns the media a quarter counter-clockwise, as Telegram's ↺.
  - It resets the zoom. Moving to another item resets the turn.
  - The zoom, its edges (the zoom fix on #25) and "a click beside the picture closes" all follow the turned picture.
- **Full screen**
  - The video fills the window at its own shape, under the bar.
  - The bottom-left text, the side arrows, Download and Rotate go. The title buttons stay, fading.
  - In a normal window, the window goes full screen too and comes back when full screen ends, so the remembered mode stays normal.
  - Esc ends full screen first, and closes the viewer only after that.
  - Leaving the video (arrows, closing) ends full screen.
- **Speed**
  - The speed button shows the current speed and goes to the next one on a click.
  - It starts at 1x for each video.
  - The time shown and the seek bar stay in the video's own time.
- **Picture-in-picture**
  - **Opening it:** the button turns the viewer's window into a small one, 320 wide with the video's shape, at the bottom right of the screen, 24 from its edges. It stays above other apps where the system allows.
  - **Playback:** the same playback carries on with no restart.
  - **Its controls** (owner's last screenshot): volume at the top left; back to the viewer and close at the top right; play in the middle; the times and the seek bar along the bottom. They fade like the viewer's.
  - **Moving and sizing:** a drag moves it, and its edges resize it. Its place and size are remembered, as the normal window's are.
  - **Leaving it:** a double click or the back button returns to the viewer in the mode it had, still playing. Close stops the video and closes the window.
  - **Opening other media** while it plays brings the viewer back with that media, and the old video stops, as there is one player.
  - Picture-in-picture is only offered for videos.

## How it's built

- **Rotate and the bar:** Dart only.
  - Rotation uses `RotatedBox` around the picture, inside the zoom.
  - The bar is `VideoPlayerView`'s, wrapped by the viewer, so it fades and counts as a control.
- **Speed:**
  - `opencord-media`'s player gets `set_speed`. The sound goes through `wsola`'s `TimeStretch`, as voice messages' does.
  - The media clock counts media time: sound heard times the speed, or the wall clock times the speed.
  - `opencord-core` gets `media_set_speed` for the bridge. The repository, the mock and `PlaybackNotifier.setSpeed` carry it.
- **Picture-in-picture:** a third mode of the viewer's own window (D86), `pip`. It goes over the same channel, `dev.opencord/media_window`, with `setMode("pip")` and the video's shape.
  - The runners size and place it, keep it above others where they can, and remember its frame:
    - Linux: `gtk_window_set_keep_above`;
    - Windows: `HWND_TOPMOST`;
    - macOS: the floating level.
  - The viewer keeps the same `VideoPlayerView` across the switch, under a `GlobalKey`, so playback isn't restarted.

## Left to the builder

- The icons.
- How the speed is spelled on the button (`1x`, `1.5x`).
- The fade timings, which are D86's.
- Whether the picture-in-picture window has a shadow.
