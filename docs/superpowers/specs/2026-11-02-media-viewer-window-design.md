# The media viewer in its own window: design

Written 2026-11-02, for the bug batch of 2026-11-01. It goes in as its own pull request into `beta` and stays open with the batch.

**The branch is built on #22 and #24.** It relies on two open batch pull requests:
- #22 (`feat/custom-titlebar-everywhere`), the app's own frame everywhere;
- #24 (`fix/tray-background`), with the Windows and macOS tray, closing to the tray, and Linux's close handling connected before Flutter's.

So `feat/media-viewer-window` starts from `beta` with both merged in. They merge cleanly together except for `docs/decisions.md`, where each adds its own entry. The pull request says to merge it after #22 and #24. Until then its diff carries their commits too.

## Why

Clicking a photo or video in a chat shows it in a layer drawn inside the main window (`app/lib/features/chat/attachments/media_viewer.dart`). The owner had asked for a viewer in a window of its own, the way Telegram Desktop does it, and the overlay isn't that. The owner also pointed out the title bar: on Windows the overlay shows round GNOME-style buttons (`OverlayWindowButtons`), not Windows' own.

What the owner wants, in their words: "a new window opens when user open a media like telegram and it has transparent background and OS titlebar buttons", "a default fullscreened new window", and "the titlebar shouldn't be the default one from os (except mac os) it needs to have same buttons of that OS". After reading how Telegram's viewer works (summary below), the owner said "I want the same thing".

## What Telegram Desktop does (read from its source, 2026-11-02)

From `Telegram/SourceFiles/media/view/media_view_overlay_widget.cpp`, `platform/platform_overlay_widget.cpp` and `platform/mac/overlay_widget_mac.mm`:

- **The window.** A top-level window of its own, with a translucent background: `#222222` at 92 % opacity (`#222222eb`), so whatever is behind it shows through, dimmed.
- **Three modes.** Full screen (the default), maximized, and a normal window. It remembers the last mode, and the normal window's position and size, across openings and restarts. The normal window is at least 480×360 and starts at 800×600.
- **Switching modes.** The maximize button switches between full screen and the normal window. So do double-clicking the top 32 px strip and Alt+Enter or Ctrl+Enter. On Windows, the system's own maximize (Aero Snap, dragging to the top, Win+Up) also becomes full screen.
- **Placement.** Opening in full screen puts the viewer on the screen the main window is on.
- **Window buttons on Windows and Linux.** Telegram draws its own minimize, maximize/restore and close buttons, 44×32, as white icons with a soft shadow and a faint white hover. On Linux they follow the desktop's button side and order.
- **Window buttons on macOS.** Telegram hides the real traffic lights and draws look-alikes.
- **No title strip.** The buttons float over the media at the top. The bottom left has a bold line ("Photo 3 of 7") with "Name · date" under it. The bottom right has a row of action icons, each 46×54. Previous and next are full-height strips 90 px wide at the window's left and right edges.
- **Fading controls.** All controls, the window buttons included, fade out after 1.1 s without mouse movement (600 ms fade) and come back on movement (200 ms). They stay while a menu is open or the mouse button is held.
- **Clicks.** In full screen, a click beside the picture closes the viewer. In the normal window it doesn't, and dragging the background moves the window. Esc closes in every mode.
- **Leaving.** Actions that leave the viewer (show in folder, forward, opening the message) close it in full screen, not in the normal window.
- **Save.** Saving a photo or video doesn't open a dialog. It goes straight into the download folder (`downloadMedia`, then `saveToFile`), and a note in the viewer says "Saved to Downloads". A dialog opens only through "Save as…" in the ⋮ menu or Ctrl+Shift+S, or when the user has turned on Telegram's "ask download path" setting.
- **macOS full screen.** A borderless window above the menu bar and Dock (`NSPopUpMenuWindowLevel`) that moves to the active Space and has no shadow. It's not macOS's own full screen with its Space and slide animation. While a file dialog is open, Telegram lowers it to the modal-panel level so the dialog shows.

Telegram Desktop is GPL. Opencord copies how it behaves, never its code.

## Goals

1. A photo or video opens in a new OS window, separate from the main window, with its own taskbar or Dock entry.
2. The window is see-through: the chat and desktop show behind the media, dimmed by the dark background.
3. It opens full screen by default and behaves like Telegram's: the three modes, how they switch, remembering them, clicks, fading controls and keyboard.
4. **Title buttons on Windows.** The app draws Windows' own caption buttons (minimize, maximize/restore, close) in Windows' shapes, sizes and hovers, with a red hover on close. They're drawn white for the dark background, with a soft shadow so they show over a bright photo.
5. **Title buttons on Linux.** The same buttons the main window draws (`LinuxWindowButtons`), in the desktop's button layout, side and order.
6. **Title buttons on macOS.** The system's real traffic lights, which is the owner's exception. Green switches between full screen and the normal window.
7. Everything the viewer does now keeps working in the window: zoom with the wheel, previous and next, video playback with its controls, Download, and the error and retry state.

## Non-goals

- The phone apps (`mobile/`). The owner keeps them out of the batch until told.
- New actions Opencord doesn't have: forwarding, rotating, drawing, text recognition, stories, picture-in-picture. The bottom right shows only what the viewer has now (Download). The layout must leave room for more icons later.
- Opening chats in separate windows. Only the media viewer gets one.

## Anti-goals (a failure even if it works)

- On Windows or Linux, a title bar drawn by the system, with its title text and frame. Or buttons that don't look like the OS's own, as the GNOME-style buttons on Windows do now.
- On macOS, look-alike traffic lights instead of the real ones.
- A viewer that loads media twice, logs in again, or loses the video's playback state because it runs as a second app or a second Flutter engine.
- A viewer window left open, or the app still running with no main window, after the user quits Opencord.
- Breaking the main window's behaviour from #22 and #24: its own title bar, closing to the tray and the tray bringing it back.

## Behaviour

### Opening

- A click on a photo or video in a message opens the viewer window on that item. It steps through the message's photos and videos in order, as now.
- If the viewer is already open, the window shows the new item and comes forward. No second window opens.
- The window takes the keyboard focus when it opens.
- The window's mode is the last one used. The first time, it's full screen.

### Modes

| Mode | Windows | Linux | macOS |
|---|---|---|---|
| Full screen (default) | Covers the whole monitor, taskbar included, on the main window's monitor | `gtk_window_fullscreen_on_monitor` on the main window's monitor | Covers the whole screen, menu bar and Dock included: borderless-looking, above them, on the active Space, no shadow; traffic lights still shown |
| Normal window | App-drawn frame like the main window's: resizable borders, DWM shadow, at least 480×360, first 800×600 | Undecorated, resizable from its edges, shadow margin like the main window's | Titled window with a transparent full-size title bar and a shadow, at the normal level |
| Maximized | System maximize turns it into full screen, like Telegram | Stays maximized (a tiling window manager or the desktop's maximize); restore goes back to the normal window | Not used: green and zoom go to full screen |

- The maximize button (green on macOS), double-clicking the top 32 px, Alt+Enter and Ctrl+Enter switch between full screen and the normal window.
- The normal window's last position and size, and the last mode, are remembered across openings and restarts. On Wayland, only the size can be restored.
- Minimize minimizes it to its own taskbar or Dock entry.

### Layout

The layout is Telegram's, with Opencord's tokens and icons.

- **Window buttons.** At the top: on Windows at the top right; on Linux where the desktop's layout puts them; on macOS the traffic lights at the top left. The app's controls keep clear of them.
- **Bottom left.** A bold line: "Photo 3 of 7" or "Video 2 of 3", or the file name when the message has just one. Under it: the sender's name · the message's time.
- **Bottom right.** Download, room for more icons later.
- **Previous and next.** Full-height strips at the left and right edges, shown only when there is one to go to, each with a round hover and a chevron.
- **Content.** The picture or video is centred. Video keeps its own playback controls.
- **Theme.** Always dark, as now.

### Fading controls

- Every control fades out after 1.1 s without mouse movement (600 ms) and comes back on movement (200 ms). That includes the window buttons, but not the macOS traffic lights.
- They don't fade while a menu or dialog is open, while a mouse button is held, or while the pointer is over a control.
- On macOS the traffic lights stay. They're the system's, and hiding them only to bring them back is not worth the risk.

### Input

- **Closing.** Esc, the close button, or a click beside the picture in full screen. In the normal window a click beside the picture does nothing, and a drag on the background moves the window.
- **Keys.** Left and right arrows step through the items. Alt+Enter and Ctrl+Enter switch modes.
- **Wheel.** The mouse wheel zooms, as now.
- **System close.** The system's close (Alt+F4, the Dock's or taskbar's Close, the window manager's) closes the viewer only, never the app.

### Lifetime

- Closing the viewer destroys its window and view, and stops the video.
- Quitting the app (tray Quit, Cmd+Q, or closing with close-to-tray off) closes the viewer first.
- **The main window hiding to the tray (#24).** The viewer stays open (my call). If the user then closes the viewer, the app keeps running in the tray as before.

### Download

Download in the viewer works as Telegram's save does. It doesn't open a dialog.

- It saves the original file straight into the system's download folder (`path_provider`'s `getDownloadsDirectory`), under its file name. If that name is taken, it adds " (1)", " (2)" and so on before the extension.
- When done, a note shows in the viewer for about 2 s: "Saved to Downloads". Clicking it opens the folder (`url_launcher` on the folder's URI).
- If saving fails, the note says so.

**Why no dialog.** `file_selector`'s save dialog attaches to the main window on all three systems:
- **macOS:** a sheet on the main window, below a full-screen viewer;
- **Windows:** owned by the main window and placed over its monitor;
- **Linux:** transient for the main window, which may be hidden in the tray.

A dialog would show behind the viewer, or hang off a hidden window.

Download in the chat itself (outside the viewer) keeps its save dialog, as now.

## How it's made

### One engine, a second view

The decisive constraint: the viewer has to share the running app's state, so videos, downloads and login keep working without loading anything twice. So the viewer is a second **Flutter view on the same engine**, in a native window, not a second app or engine.

Flutter 3.44's own multi-window API (`RegularWindowController`, `widgets/_window.dart`) is `@internal`. It's behind the `windowing` feature flag, which is available only on the master channel, so it's not usable. Each runner instead makes the view with the engine's own API:

- **Windows:**
  - `FlutterDesktopEngineCreateViewController(engine, {width, height})`, which is exported from `flutter_windows.dll` but declared only in the engine's internal header `flutter_windows_internal.h`. The runner declares it itself.
  - It gets the engine with `FlutterDesktopEngineForId(PlatformDispatcher.instance.engineId)`, also exported, or from the runner's own controller. That choice is the builder's.
  - The view's child HWND goes into a new top-level window. The runner forwards that window's messages through `FlutterDesktopViewControllerHandleTopLevelWindowProc`, as `FlutterWindow` does for the main window.
  - `FlutterDesktopViewControllerDestroy` removes the view.
- **macOS:**
  - `FlutterViewController(engine: mainController.engine, nibName: nil, bundle: nil)` in a new `NSWindow`, removed by closing that window.
  - Transparent with `isOpaque = false`, `backgroundColor = .clear`, and the view controller's `backgroundColor = .clear`.
- **Linux:**
  - `fl_view_new_for_engine(fl_view_get_engine(main_view))` in a new `GtkWindow`, with the RGBA visual and `app_paintable`, as the main window has, and `fl_view_set_background_color` transparent.

Each runner gets a `dev.opencord/media_window` method channel next to `dev.opencord/window` and `dev.opencord/tray`:

- **Dart calls the runner:**
  - `open {mode, normal geometry, monitor of the main window}`, which answers the new view id;
  - `close`;
  - `setMode`;
  - `minimize`;
  - `startDrag` and `startResize` on Linux;
  - `focus`.
- **The runner calls Dart:**
  - `closeRequested` (the system's close);
  - `state {mode, maximized, active}`;
  - `geometry` (to remember the normal window).
- The exact method list is the builder's.

### Dart side

- `main.dart` changes from `runApp` to `runWidget`, with a root that renders a `ViewCollection`:
  - the implicit view with the app as now;
  - while the viewer is open, a `View` for the viewer's `FlutterView`, found by id in `PlatformDispatcher.instance.views` once the runner has added it.
- The viewer's `View` sits under the same `UncontrolledProviderScope`, so it shares the `ProviderContainer`.
- **Startup failure.** The startup-failed app keeps working as a single view.
- **The service.** A `MediaWindowService` interface, like `TrayService`:
  - a `ChannelMediaWindow` implementation;
  - a fake for tests;
  - a fallback that keeps today's in-app overlay. It's used when the runner has no such channel or `open` fails, and in widget tests that don't use the window.
- **The provider.** One provider holds what's open:
  - server key, items, index, sender name, message time;
  - the mode;
  - the view id.
- **The call sites.** `showMediaViewer` and its two callers (`image_grid.dart`, `attachment_view.dart`) pass the sender and the time along. `attachment_view.dart` has the message.
- **The widget.** One viewer widget, laid out as above, serves both the window and the fallback overlay.
- **Remembered state.** The mode and the normal window's geometry go in the key-value store (`keyValueStoreProvider`).

### Things to check while building

- `OcBinding`'s wheel smoother (`core/smooth_scroll.dart`) replays wheel events; the replayed events must keep their view id, or the viewer's zoom lands in the main window.
- `View.of(context)` users (`message_item.dart`, `message_list.dart`) still get the main view.
- Keyboard focus and Esc in the second view on all three platforms.
- The video texture (irondash, `Texture(textureId:)`) shows in the second view. Textures are per engine, so it should.

## Risks

1. **Windows transparency.** The engine renders through ANGLE with an 8-bit alpha EGL config and clears to transparent (`compositor_opengl.cc`). Whether DWM shows the desktop through a child HWND's swap chain still has to be proved. The intended route is `DwmExtendFrameIntoClientArea` with margins of −1 on the top-level window. If it can't be made to work, the Windows background falls back to an opaque `#222222`, and the PR says so. It isn't hidden.
2. **Windows internal functions.** They come from an internal header, so a Flutter upgrade may change them. CI's Windows build fails loudly if they go away. Note it in the decision entry.
3. **macOS above the menu bar.** A titled window is normally kept below the menu bar. The window subclass overrides `constrainFrameRect(_:to:)` and sets the level above the menu bar. Native full screen is turned off (`collectionBehavior` without `.fullScreenPrimary`), and green is caught to switch modes.
4. **Testing other systems.** Windows and macOS can only be compiled in CI from here. Their behaviour goes on the batch's hand-test list.

## Testing

- **Dart unit and widget tests.**
  - The channel's messages, against a fake channel.
  - The provider: opening, opening again while open, closing, the system's close, mode switches, and remembered mode and geometry.
  - The layout, with "n of m", the sender and the time.
  - Fading after 1.1 s and staying while hovered.
  - Clicks beside the picture closing only in full screen, and the keys (Esc, arrows, Alt+Enter).
  - The fallback overlay when the runner has no window.
  - Rendering into a second test view, if `flutter_test` allows it cleanly. That's the builder's call.
- **Linux, by hand on the dev machine (KDE Plasma 6, Wayland).**
  - Open a photo and a video in the mock app. Check that a second window exists (KWin scripting by pid, as for the tray), that full screen covers the panel, and that the desktop shows through.
  - Check the buttons follow the desktop layout, and that the mode switch, the normal window and the remembered size work.
  - Check that Esc, closing and quitting with the viewer open behave.
- **Windows and macOS.** CI compiles them. Hand tests go on the batch checklist:
  - transparency;
  - covering the taskbar or the menu bar and Dock;
  - Snap and Win+Up turning into full screen;
  - Snap Layouts on the maximize button;
  - the traffic lights and green;
  - Download saving to the download folder and the note opening it;
  - DPI and several monitors.
- `.github/check.sh client` before every push, then CI.

## Decided, and whose call

- **Telegram's behaviour as summarized above.** The owner's call ("I want the same thing").
- **macOS keeps the real traffic lights.** The owner's call.
- **The title bar is the app's own, with the OS's buttons, on Windows and Linux.** The owner's call.
- **Download in the viewer saves straight to the download folder, as Telegram's save does.** This follows from "the same thing".

My calls:
- The branch is built on #22 and #24, and is merged after them.
- One window, reused when another item opens.
- The traffic lights don't fade.
- The viewer stays open when the main window hides to the tray, and closes when the app quits.
- A second view on the same engine, through each runner's native API.
- Only today's actions at the bottom right.
- The fallback to today's overlay.
- On Linux, maximized stays maximized; on Windows it becomes full screen. This follows Telegram.
- Windows' buttons get a red hover on close: Windows' own look, where Telegram's doesn't.

## Left to the builder

- The channel's exact methods and arguments, file names, and how the root widget is split.
- How the runner gets the engine on Windows.
- Whether the Windows full screen is a `WS_POPUP` window or the main window's fullscreen route.
- Icon choices from `OcIcons`, and spacing from `OcSpace`.
- Whether a second-view widget test is practical.
- The decision entry in `docs/decisions.md`. It takes the next free number after the batch's D83–D85, so D86, and notes the internal Windows functions. The desktop UI plan's mention of the viewer is updated to match.
