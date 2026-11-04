# The Media Viewer in Its Own Window: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Clicking a photo or video opens a new, see-through, full-screen OS window that behaves like Telegram Desktop's media viewer. It draws its own title buttons in the OS's style on Windows and Linux, and keeps the real traffic lights on macOS.

**Architecture:**
- **One engine.** The viewer is a second Flutter view on the running engine, so it shares the `ProviderContainer`, the repository, and video textures.
- **Runners.** Each runner makes the view with its engine's own API and a native window, behind a new `dev.opencord/media_window` method channel.
- **Dart side.** `main.dart` moves to `runWidget` with a `ViewCollection`: the implicit view for the app, plus the viewer's view while it's open.
- **Fallback.** The current in-window overlay stays as the fallback.

**Tech Stack:**
- Flutter 3.44 (multi-view: `View`, `ViewCollection`, `runWidget`) and Riverpod 3.
- **Win32:** `FlutterDesktopEngineCreateViewController`, DWM accent policy.
- **GTK 3:** `fl_view_new_for_engine`.
- **AppKit:** `FlutterViewController(engine:nibName:bundle:)`.

**Spec:** `docs/superpowers/specs/2026-11-02-media-viewer-window-design.md`

## Global Constraints

- **Base.** Branch `feat/media-viewer-window` is `beta` plus #22 and #24, already merged in. It's a pull request into `beta`, merged after #22 and #24, and held with the 2026-11-01 batch.
- **Commands.** Start every `cargo`, `flutter` and `dart` command with `source .superpowers/sdd/media/env.sh &&`. Run `.github/check.sh client` before each push.
- **Platforms.** Desktop only. Don't touch `mobile/`.
- **Background.** `#222222` at 92 % opacity: `Color(0xEB222222)`, painted by Flutter. The native window itself is fully transparent.
- **Normal window.** At least 480×360 logical px; the first time, 800×600, centred on the main window's monitor.
- **Fading controls.** They hide after 1100 ms without pointer movement, over 600 ms, and show again over 200 ms.
- **Telegram's code.** It's GPL. Copy behaviour, never code.
- **The remembered placement.** Store key `ui.mediaViewer`, JSON `{"mode": "fullscreen"|"normal", "width", "height", "x"?, "y"?}`.
- **The channel.** `dev.opencord/media_window`. Mode names on the wire are `fullscreen`, `normal` and `maximized` (the last one reported only, by Linux).

## Review Focus

1. **Opening while open.** Clicking another item while the viewer is open must reuse the window and bring it forward, never open a second one. Pinned by `opening_again_reuses_the_window` (Task 2).
2. **System close.** The system's close (Alt+F4, taskbar, Dock) must close only the viewer, and the main app must keep running. Pinned by `a_system_close_closes_only_the_viewer` (Task 2) and the runners' close handling (Tasks 5–7).
3. **Remove the view first.** The native view must not be destroyed while a `View` widget still renders into it. Pinned by `closing_removes_the_view_before_the_window` (Task 2).
4. **A placement that no longer fits.** A remembered normal window on a monitor that's gone must reopen on the main window's monitor, not off screen. The runners check this (Tasks 5–7). The Dart side is pinned by `a_broken_placement_opens_full_screen` (Task 2).
5. **Name clashes.** Download must never overwrite a file of the same name in Downloads. Pinned by `download_saves_beside_a_file_of_the_same_name` (Task 3).

---

### Task 1: The media window service

**Files:**
- Create: `app/lib/features/media_window/media_window_service.dart`
- Create: `app/test/support/fake_media_window.dart`
- Test: `app/test/unit/media_window/media_window_service_test.dart`

**Interfaces:**
- Produces:
  - `enum MediaWindowMode { fullscreen, normal }`.
  - `class MediaWindowPlacement`, with:
    - fields `MediaWindowMode mode` and `WindowGeometry? normal`;
    - `static MediaWindowPlacement fromJson(Object?)`, which gives full screen with no geometry for anything broken;
    - `Map<String, Object?> toJson()`.
  - `class MediaWindowStatus`, with:
    - fields `MediaWindowMode mode`, `bool maximized`, `bool focused`;
    - `factory MediaWindowStatus.fromMap(Object?)`, where `mode: "maximized"` means `normal` with `maximized: true`.
  - `abstract interface class MediaWindowService`:
    - `Future<int?> open(MediaWindowPlacement placement)`, which answers the new Flutter view id, or null when there's no window;
    - `Future<void> close()`;
    - `Future<void> setMode(MediaWindowMode mode)`;
    - `Future<void> minimize()`, `Future<void> focus()`, `Future<void> startDrag()`;
    - `Future<void> startResize(ResizeEdge edge)`;
    - `Stream<void> get closeRequests`;
    - `Stream<MediaWindowStatus> get status`;
    - `Stream<WindowGeometry> get geometry`.
  - `class ChannelMediaWindow implements MediaWindowService`, taking `[MethodChannel channel = const MethodChannel('dev.opencord/media_window')]`.
  - `class NullMediaWindow implements MediaWindowService`, whose `open` answers null.
  - `final mediaWindowServiceProvider = Provider<MediaWindowService>((ref) => const NullMediaWindow());`
  - In the test support file, `class FakeMediaWindow implements MediaWindowService`, which:
    - records calls in `List<String> calls`;
    - answers `int? nextViewId = 7` from `open`;
    - has `void requestClose()`, `void sendStatus(MediaWindowStatus)` and `void sendGeometry(WindowGeometry)`.

- [ ] **Step 1: Write the failing tests** in `media_window_service_test.dart`, using `TestDefaultBinaryMessengerBinding` mock handlers on the channel:
  - `open_sends_the_placement_and_the_engine`:
    - `open(MediaWindowPlacement(mode: normal, normal: WindowGeometry(width: 900, height: 700, x: 10, y: 20)))` sends `open` with `{'mode': 'normal', 'engineId': PlatformDispatcher.instance.engineId, 'width': 900, 'height': 700, 'x': 10, 'y': 20}`;
    - when the handler returns `5`, it answers `5`.
  - `open_without_a_runner_answers_null`: there's no handler (`MissingPluginException`), and a `PlatformException` also gives null.
  - `runner_events_reach_the_streams`:
    - `closeRequested` arrives on `closeRequests`;
    - `status {'mode': 'maximized', 'focused': true}` arrives as `normal`, `maximized: true`;
    - `geometry {'width': 800, 'height': 600}` arrives as a `WindowGeometry`.
  - `placement_round_trips_and_falls_back`:
    - `toJson`/`fromJson` round-trips;
    - `fromJson('x')`, `fromJson({'mode': 'nope'})` and `fromJson(null)` all give `mode: fullscreen, normal: null`.
- [ ] **Step 2:** Run `cd app && flutter test test/unit/media_window/media_window_service_test.dart`. Expected: it fails to compile, because the file doesn't exist.
- [ ] **Step 3: Implement.**
  - `ChannelMediaWindow` sets its own method call handler, as `ChannelNativeWindow` does (`native_window.dart`).
  - Its `_invoke` catches `MissingPluginException` and `PlatformException` and gives null.
  - Reuse `WindowGeometry.fromJson` and `toJson` for the normal window, and `ResizeEdge.name` on the wire.
- [ ] **Step 4:** Run the test. Expected: PASS.
- [ ] **Step 5: Commit** `feat(desktop): a channel for the media viewer's own window`.

### Task 2: The viewer session

**Files:**
- Create: `app/lib/features/media_window/media_viewer_session.dart`
- Test: `app/test/unit/media_window/media_viewer_session_test.dart`

**Interfaces:**
- Consumes Task 1.
- Produces:
  - `class MediaViewerRequest`, with fields `String serverKey`, `List<Attachment> media`, `int index`, `String? senderName` and `DateTime? sentAt`.
  - `class MediaViewerSession`, with fields `MediaViewerRequest request`, `int viewId`, `MediaWindowStatus status`, and `int generation`, which goes up with each new request so the viewer resets its index.
  - `class MediaViewerNotifier extends Notifier<MediaViewerSession?>`, with:
    - `Future<bool> open(MediaViewerRequest request)`, which is false when there's no window, so the caller falls back to the overlay;
    - `Future<void> close()`;
    - `Future<void> toggleMode()`.
  - `final mediaViewerProvider = NotifierProvider<MediaViewerNotifier, MediaViewerSession?>(MediaViewerNotifier.new);`

**Behaviour:**
- **Opening.** `open`:
  - reads the placement from `keyValueStoreProvider` under `ui.mediaViewer`;
  - calls `service.open`, and on a view id sets the session with status `MediaWindowStatus(mode: placement.mode, maximized: false, focused: true)`;
  - when already open, replaces the request, bumps `generation` and calls `focus()`, with no second `open`.
- **Closing.** `close`:
  - sets the state to null;
  - awaits `WidgetsBinding.instance.endOfFrame` when a binding exists, so the `View` leaves the tree first;
  - then calls `service.close()`.
- **System close.** A `closeRequests` event runs `close()`.
- **Status.** Status events update `session.status`. When the mode changes, the placement is saved.
- **Geometry.** Geometry events in normal mode, when not maximized, are saved after 500 ms of quiet, as `WindowGeometryKeeper` does.
- **Mode switch.** `toggleMode` calls `service.setMode` with the other mode, and saves the placement.

- [ ] **Step 1: Write the failing tests.** Use a `ProviderContainer` overriding `mediaWindowServiceProvider` with `FakeMediaWindow` and `keyValueStoreProvider` with the in-memory store the other tests use:
  - `opening_without_a_window_answers_false`: with `nextViewId = null`, `open` is false and the state stays null.
  - `opening_sets_the_session`: `open` is true, `state.viewId == 7`, and `calls == ['open:fullscreen']`.
  - `opening_again_reuses_the_window`: a second `open` gives `calls == ['open:fullscreen', 'focus']`, the new request, and `generation == 2`.
  - `a_system_close_closes_only_the_viewer`: `requestClose()` makes the state null and the calls end with `'close'`.
  - `closing_removes_the_view_before_the_window`: a listener records the state when `'close'` is called, and it's already null.
  - `the_mode_and_size_are_remembered`:
    - `toggleMode()` gives `calls` containing `'setMode:normal'`;
    - after `sendGeometry(WindowGeometry(width: 900, height: 700))` and 600 ms, the store holds `{"mode":"normal","width":900,"height":700}`;
    - the next `open` sends `open:normal`.
  - `a_broken_placement_opens_full_screen`: the store holds `'{'`, and `open` sends `open:fullscreen`.
- [ ] **Step 2:** Run it. Expected: it fails to compile.
- [ ] **Step 3: Implement.** Use a `Timer` for the 500 ms save, and cancel it with the subscriptions in `ref.onDispose`.
- [ ] **Step 4:** Run it. Expected: PASS.
- [ ] **Step 5: Commit** `feat(desktop): the media viewer's window session`.

### Task 3: The viewer, laid out like Telegram's

**Files:**
- Modify:
  - `app/lib/features/chat/attachments/media_viewer.dart`
  - `app/lib/features/chat/attachments/download.dart`
  - `app/lib/features/window/window_buttons.dart`, which makes `_GnomeButton` and `_Glyph` public as `GnomeWindowButton` and `WindowGlyph`
- Create: `app/lib/features/media_window/media_window_buttons.dart`
- Test:
  - `app/test/widget/chat/media_viewer_test.dart`, which keeps its tests and adds the ones below
  - `app/test/unit/chat/save_to_downloads_test.dart`

**Interfaces:**
- Consumes Tasks 1 and 2.
- Produces:
  - `MediaViewer`, with:
    - fields `serverKey`, `media`, `initialIndex`, `String? senderName`, `DateTime? sentAt` and `VoidCallback onClose`;
    - `bool inWindow = false`, where true means the viewer's own window: Telegram's mode rules, and `MediaWindowButtons`;
    - `MediaWindowMode mode = MediaWindowMode.fullscreen`.
  - `class MediaWindowButtons extends ConsumerWidget`, for the viewer window's title buttons on the current platform.
  - `Future<String?> saveToDownloads(Ref ref, String serverKey, Attachment attachment, {Future<Directory?> Function() downloads = getDownloadsDirectory})`, which answers the saved path, or null when there's no download folder. It throws `RepoException` from the repository.
  - `String mediaHeader(Attachment current, int index, int count)`, which gives "Photo 3 of 7" or "Video 2 of 3", or the file name when `count == 1`.

**Layout:**
- **No title strip.** The background is `Color(0xEB222222)` in window and overlay alike.
- **Previous and next.** Full-height strips 90 px wide at the left and right edges, from 47 px below the top to 47 px above the bottom. They show only when there's an item to go to. Each has a round 36 px hover behind a chevron.
- **Bottom left**, 14 px from the left edge:
  - at 47 px from the bottom, `mediaHeader` in `OcText.bodyStrong`;
  - at 26 px, `"$senderName · ${shortDate(sentAt, now)} at ${clockTime(sentAt)}"` in `OcText.small`, with each part shown only when known.
- **Bottom right.** One Download icon button, 46×54.
- **Top.**
  - In the window: `MediaWindowButtons` at the top right on Windows, on the layout's side on Linux, and nothing on macOS. On macOS, the app's controls stay 78 px clear of the top left.
  - In the overlay: today's `OverlayWindowButtons` at the top right.
- **Fading.** Everything except the media fades together.

**Window buttons:**
- **Windows.** Three 46×32 buttons with the Segoe Fluent glyphs of `WindowsCaptionButtons` (``, ``/``, ``), white at 10 pt, with `Shadow(color: Color(0x99000000), blurRadius: 4)`.
  - Hover is `Color(0x1AFFFFFF)` and press `Color(0x0FFFFFFF)`.
  - Close hovers `Color(0xFFC42B1C)` and presses `Color(0xFFB0281A)`.
  - Maximize shows restore (``) while full screen.
- **Linux.** `GnomeWindowButton`s in `buttonLayoutProvider`'s order and side. Maximize shows restore while full screen.
- **Actions.**
  - Minimize calls `service.minimize`.
  - Maximize calls `mediaViewerProvider.notifier.toggleMode`.
  - Close calls `onClose`.

**Behaviour:**
- **Clicks.** A click beside the picture closes the viewer when `!inWindow || mode == fullscreen`. In the normal window, a pan on the background calls `service.startDrag()`.
- **Top strip.** A double click on the top 32 px calls `toggleMode` in the window.
- **Keys.** Esc closes, the left and right arrows step, and Alt+Enter and Ctrl+Enter call `toggleMode` in the window.
- **Fading.** Controls fade after 1100 ms without pointer movement (600 ms) and come back on movement (200 ms). They don't fade while a pointer button is held or the pointer is over a control.
- **Download.**
  - `saveToDownloads` saves into the downloads folder under `freePathIn(dir, attachment.filename, …)`.
  - A note centred near the bottom shows "Saved to Downloads" for 2 s. Tapping it calls `launchUrl(Uri.directory(dir))`.
  - Errors show the `RepoException` message in the same note. A cancelled download shows nothing.
  - With no download folder, it says "There's no Downloads folder".

- [ ] **Step 1: Write the failing tests.**
  - `media_viewer_test.dart`:
    - `the_header_counts_the_items`: `mediaHeader` gives `'Video 1 of 2'` and `'Photo 2 of 2'`, and `'clip.mp4'` for one.
    - `the_bottom_line_shows_the_sender_and_time`: `senderName: 'Ana'` and `sentAt: DateTime(2026, 1, 1, 9, 5)` show `'Ana · 1 Jan at 09:05'`. The fixed clock is 2026-03-01.
    - `controls_fade_after_a_second_without_movement`:
      - after 1100 ms + 600 ms the Download button's `Opacity` is 0;
      - a hover move brings it back to 1 after 200 ms;
      - pumped with `fake_async`, or `tester.pump(duration)`.
    - `in_the_normal_window_a_click_beside_does_not_close`: with `inWindow: true` and `mode: normal`, a tap at (5, 300) doesn't call `onClose`. In full screen it does.
    - `alt_enter_switches_the_window_mode`: `FakeMediaWindow.calls` ends with `'setMode:normal'`.
    - `windows_buttons_are_windows_caption_buttons`: under `TargetPlatformVariant.only(TargetPlatform.windows)` with `inWindow: true`, it finds the text `''`, `''` (full screen) and `''`, and no `GnomeWindowButton`.
    - `macos_draws_no_window_buttons`: under the macOS variant, it finds no `MediaWindowButtons` glyphs.
  - `save_to_downloads_test.dart`:
    - `download_saves_beside_a_file_of_the_same_name`: in a temp dir holding `clip.mp4`, it saves to `clip (1).mp4`, through `MockRepository.downloadAttachment`;
    - `no_downloads_folder_answers_null`.
- [ ] **Step 2:** Run `flutter test test/widget/chat/media_viewer_test.dart test/unit/chat/save_to_downloads_test.dart`. Expected: FAIL.
- [ ] **Step 3: Implement.** Keep `_buildMediaContent`, zoom and the playback hooks as they are.
- [ ] **Step 4:** Run those tests and `test/widget/chat/media_playback_test.dart`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(desktop): the media viewer laid out like Telegram's`.

### Task 4: Two views: the app, and the viewer's window

**Files:**
- Create: `app/lib/app_views.dart`
- Modify:
  - `app/lib/main.dart`, from `runApp` to `runWidget`;
  - `app/lib/app_start.dart`, where `startupWidget(…, {bool multiView = false})` wraps the app in `AppViews` inside the `UncontrolledProviderScope`, and the failure app in `View(view: implicitView)`; it also overrides `mediaWindowServiceProvider` with `ChannelMediaWindow()` on Linux, Windows and macOS;
  - `app/lib/features/chat/attachments/media_viewer.dart` (`showMediaViewer`);
  - `app/lib/features/chat/attachments/attachment_view.dart` and `image_grid.dart`, which pass the sender name and time.
- Test: `app/test/widget/media_window/app_views_test.dart`

**Interfaces:**
- Consumes Tasks 1–3.
- Produces:
  - `class AppViews extends StatefulWidget`, with:
    - fields `Widget app`;
    - `FlutterView? Function(int viewId) findView`, defaulting to looking in `PlatformDispatcher.instance.views`;
    - a rebuild on `didChangeMetrics`.
  - `class MediaViewerWindow extends ConsumerWidget`. It's the viewer view's root: `MaterialApp(debugShowCheckedModeBanner: false, theme: buildTheme(OcColors.dark), home: MediaViewer(… inWindow: true, mode: session.status.mode, onClose: notifier.close))`, keyed by `session.generation`.
  - `Future<void> showMediaViewer(BuildContext context, {required String serverKey, required List<Attachment> media, required int index, String? senderName, DateTime? sentAt})`. It tries `mediaViewerProvider.notifier.open`, and on false pushes today's overlay route.

- [ ] **Step 1: Write the failing tests.** Use `pumpWidget(wrapWithView: false, …)` with `findView` returning `tester.view` for the implicit view's id only.
  - `the_app_renders_alone_while_the_viewer_is_closed`: one `View` and the app.
  - `without_a_view_the_viewer_falls_back_to_the_overlay`: `FakeMediaWindow.nextViewId = null`, and tapping a video in `AttachmentsView` shows `MediaViewer` in the main view's route (`ModalRoute.of` is not null).
  - `with_a_window_the_viewer_does_not_open_in_the_main_window`: with `nextViewId = 7` and `findView(7)` null, there's no `MediaViewer` in the tree and `calls` holds `'open:fullscreen'`.
- [ ] **Step 2:** Run it. Expected: FAIL.
- [ ] **Step 3: Implement.**
  - The implicit view is `PlatformDispatcher.instance.implicitView!`. `main.dart` calls `runWidget(await startupWidget(…, multiView: true))`.
  - Check `OcBinding`'s wheel smoother (`core/smooth_scroll.dart`): the events it replays must keep the `viewId` of the event they came from. Add a unit test `replayed_wheel_events_keep_their_view` if they don't.
- [ ] **Step 4:** Run `flutter test` (all of it) and `flutter analyze`. Expected: PASS, no issues.
- [ ] **Step 5: Commit** `feat(desktop): the media viewer opens in a view of its own`.

### Task 5: Linux runner

**Files:**
- Create: `app/linux/runner/media_window.cc` and `media_window.h`
- Modify:
  - `app/linux/runner/window_channel.cc` and `.h`, adding `GdkEvent* window_channel_last_press()` and `GtkWindow* window_channel_window()`;
  - `my_application.cc`, which calls `media_window_register(window, view)` after `window_channel_register`;
  - `CMakeLists.txt`.

**Interfaces:**
- Consumes the channel from Task 1.
- Produces `void media_window_register(GtkWindow* main_window, FlView* main_view);`

**Behaviour:**
- **`open`.**
  - A new `GtkWindow`, `gtk_window_set_application` to the main window's application, titled "Opencord media viewer", undecorated, with the RGBA visual and `app_paintable` when composited.
  - Inside it, `fl_view_new_for_engine(fl_view_get_engine(main_view))` with a transparent background, shown and realized.
  - It answers `fl_view_get_id`.
  - **Full screen:** `gtk_window_fullscreen_on_monitor` on the main window's monitor.
  - **Normal:** `gtk_window_resize` to the given size, at least 480×360, and on X11 `gtk_window_move` when the point is on a monitor, otherwise centred.
  - Then `gtk_window_present`.
- **`setMode`.** Full screen or unfullscreen. Back in normal, it restores the last normal size.
- **Other calls.**
  - `close`: `gtk_widget_destroy`.
  - `minimize`: `gtk_window_iconify`.
  - `focus`: `gtk_window_present`.
  - `startDrag` and `startResize`: through `window_channel_last_press()`, as the main window does.
- **Events.**
  - `delete-event` sends `closeRequested` and returns TRUE.
  - `window-state-event` sends `status` with `mode` (`fullscreen` / `maximized` / `normal`) and `focused`.
  - `configure-event` in normal mode, not maximized, sends `geometry`, with x and y only off Wayland.
- **Resize band.** No shadow margin: the Dart viewer puts 6 px resize handles inside the edges in normal mode on Linux. Reuse `WindowFrame`'s `_handles` logic by making it public as `WindowResizeHandles`, and have them call `MediaWindowService.startResize`.

- [ ] **Step 1: Build.** Run `cd app && flutter build linux --debug`. Expected: it builds.
- [ ] **Step 2: Hand check on KDE Plasma 6 Wayland** with `build/linux/x64/debug/bundle/opencord --mock`:
  1. Open a photo. A second window exists: a KWin script printing `workspace.windowList()` entries for the app's pid shows two.
  2. It covers the panel, and the desktop shows through. Spectacle's screenshot of the area shows the wallpaper dimmed.
  3. The buttons follow the desktop layout.
  4. Maximize gives a normal window, which can be moved by dragging the background and resized from its edges.
  5. Close and reopen: it comes back normal at the same size.
  6. Esc closes. Alt+F4 on the viewer leaves the app running.
  7. Quit from the tray with the viewer open: the process exits 0.
- [ ] **Step 3: Commit** `feat(linux): the media viewer's own window`.

### Task 6: Windows runner

**Files:**
- Create: `app/windows/runner/media_window.cpp` and `media_window.h`
- Modify:
  - `app/windows/runner/flutter_window.cpp` and `.h`, which create `media_window_` after `tray_channel_` and destroy it first in `OnDestroy`;
  - `CMakeLists.txt`.

**Interfaces:**
- Consumes the channel from Task 1.
- Produces `class MediaWindowChannel { MediaWindowChannel(HWND main, flutter::BinaryMessenger* messenger); ~MediaWindowChannel(); }`.
- Declared locally (from the engine's `flutter_windows_internal.h`; exported by `flutter_windows.dll`):
  - `struct FlutterDesktopViewControllerProperties { int width; int height; }`;
  - `FlutterDesktopEngineRef FlutterDesktopEngineForId(int64_t)`;
  - `FlutterDesktopViewControllerRef FlutterDesktopEngineCreateViewController(FlutterDesktopEngineRef, const FlutterDesktopViewControllerProperties*)`.

**Behaviour:**
- **`open`.**
  - Registers class `OPENCORD_MEDIA_VIEWER` with the app icon (`IDI_APP_ICON`).
  - Creates a top-level, unowned `WS_OVERLAPPEDWINDOW` window, so it has its own taskbar button.
  - Sets the accent policy `ACCENT_ENABLE_TRANSPARENTGRADIENT` with flags 2 and gradient colour 0, through `SetWindowCompositionAttribute` from `user32` (`GetProcAddress`), and `DwmExtendFrameIntoClientArea({0,0,1,0})`.
  - Creates the view controller on `FlutterDesktopEngineForId(engineId)`, parents its HWND (`FlutterDesktopViewGetHWND(FlutterDesktopViewControllerGetView(c))`), and answers `FlutterDesktopViewControllerGetViewId`.
- **Normal mode.** `WM_NCCALCSIZE` keeps only the side and bottom borders, like `WindowChannel`. `WM_NCHITTEST` gives `HTTOP` along the top band.
- **Full screen.** The style loses `WS_OVERLAPPEDWINDOW`, and the window covers `rcMonitor` of `MonitorFromWindow(main)`.
- **System maximize.** `WM_SIZE` with `SIZE_MAXIMIZED` (Aero Snap, Win+Up) posts a restore and a switch to full screen.
- **Close.** `WM_CLOSE` sends `closeRequested` and doesn't destroy.
- **Status and geometry.** `WM_ACTIVATE` focuses the view and sends `status`. `WM_SIZE` and `WM_MOVE` in normal mode send `geometry`: logical size and physical place, like `WindowChannel::SendGeometry`. `WM_GETMINMAXINFO` sets the 480×360 minimum.
- **Messages.** Every message goes first to `FlutterDesktopViewControllerHandleTopLevelWindowProc`.
- **`close`.** Destroys the controller, then the window.
- **`startResize`.** Does nothing: Windows resizes from its own borders.

- [ ] **Step 1:** Lint what can be linted from Linux: `clang-format --dry-run` if present. CI's Windows build is the compile check.
- [ ] **Step 2: Commit** `feat(windows): the media viewer's own window`. After pushing, read the Windows job and fix it on the branch.

### Task 7: macOS runner

**Files:**
- Modify: `app/macos/Runner/MainFlutterWindow.swift`, with a new `final class MediaWindowChannel` and `final class MediaViewerNSWindow: NSWindow`, created in `awakeFromNib` after `trayChannel`.

**Behaviour:**
- **`open`.**
  - Turns on multi-view once. `class_getInstanceVariable(type(of: engine), "_multiViewEnabled")` must exist, and then `engine.setValue(true, forKey: "multiViewEnabled")`. If the ivar is missing, it answers `nil`, and the Dart side uses the overlay.
  - Never calls `enableMultiView`, which asserts while the main view exists.
  - Creates `FlutterViewController(engine: engine, nibName: nil, bundle: nil)` with `backgroundColor = .clear`.
  - The window has style `[.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView]`, a transparent title bar, hidden title, `isOpaque = false`, `backgroundColor = .clear` and `isReleasedWhenClosed = false`.
  - It answers `controller.viewIdentifier`.
- **Full screen.**
  - `setFrame(mainWindow.screen.frame)`, level `.popUpMenu`, no shadow, `collectionBehavior = [.moveToActiveSpace, .fullScreenNone]`.
  - `constrainFrameRect(_:to:)` returns the rect unchanged, so the window can cover the menu bar.
- **Normal.** Level `.normal`, a shadow, and the remembered frame if it's on a screen, otherwise 800×600 centred.
- **Green.** `windowShouldZoom(_:toFrame:)` switches the mode and returns false.
- **Close.** `windowShouldClose` sends `closeRequested` and returns false.
- **`close`.** `orderOut`, `contentViewController = nil`, drop the window. Never `close()`: the app delegate quits after the last window closes, and the main window may be hidden in the tray.
- **Status and geometry.** `windowDidBecomeKey` and `windowDidResignKey` send `status`. `windowDidMove` and `windowDidResize` in normal mode send `geometry`.
- **`startDrag`.** `performDrag(with: NSApp.currentEvent)`.

- [ ] **Step 1: Commit** `feat(macos): the media viewer's own window`. CI's macOS build is the compile check.

### Task 8: Docs, review, pull request

**Files:**
- Modify:
  - `docs/decisions.md`: D86, "The media viewer in its own window (2026-11-02)", in D85's shape. It covers the second view on one engine, the internal Windows functions and the macOS ivar, the fallback, and Download without a dialog.
  - `docs/plans/desktop-ui.md`, the viewer's mention.
  - `~/.claude/projects/-home-amin-dev-opencord/memory/bug-batch-2026-11-01.md`: #25 and its hand tests.
  - The spec's hand-test list: drop "Snap Layouts on the maximize button", because the viewer's maximize switches modes and has no snap layouts.

- [ ] **Step 1:** Run `df -h /`, then `.github/check.sh client`. Expected: everything passes.
- [ ] **Step 2:** Review the whole branch diff against `origin/feat/custom-titlebar-everywhere` merged with `origin/fix/tray-background`, using one fresh reviewer.
- [ ] **Step 3:** Push, then `gh pr create --base beta`. The body says:
  - what changed, and why;
  - that it's stacked on #22 and #24, to merge after them;
  - what was tested, and what couldn't be (Windows and macOS);
  - the Windows transparency risk.
- [ ] **Step 4:** Follow `gh pr checks` and fix failures on the branch.

### Task 9: Windows check (on the Windows PC; not run by this session)

Written to `docs/plans/media-check-windows.md`, for the owner or a Claude session on the PC. It's the upload plan's unrun P1 plus this viewer.

1. Check out `feat/media-viewer-window`, or `beta` once the batch is merged. Then `flutter build windows` and run against a server with uploads.
2. Run upload plan P1 steps 1–2 (`docs/superpowers/plans/2026-10-18-media-uploads.md`, "P1: Windows"):
   - the FFmpeg DLLs' decoders;
   - `cargo test`, `flutter test`;
   - the live checks from A5–A7: upload, the progress bar, an image grid, a video plays, a voice message, Download.
3. Check the viewer:
   - a photo opens in a new window covering the taskbar, with the chat visible through it (or note the solid-dark fallback);
   - the three caption buttons, and close's red hover;
   - maximize gives a normal window; Win+Up and dragging to the top turn it into full screen;
   - the mode and size are remembered;
   - Esc closes, and so does a click beside the picture in full screen only;
   - Alt+F4 closes only the viewer;
   - Download lands in Downloads, and the note opens the folder;
   - a video plays in the viewer;
   - DPI of 100 % and 150 %, and two monitors;
   - closing the main window to the tray with the viewer open.
4. Report what failed, with screenshots, in the batch's thread.

### Task 10: macOS check (on the Mac; not run by this session)

Written to `docs/plans/media-check-macos.md`. It's the upload plan's unrun P2 plus this viewer.

1. Video playback on macOS needs upload plan P2 steps 1–3 first: the universal FFmpeg build, the `ffmpeg-next` dependency and the decision entry. Until then, the viewer shows "can't play" for videos.
2. Run P2 steps 4–6: the microphone prompt, the clipboard, the tests and the A5–A7 live checks.
3. Check the viewer:
   - a photo opens covering the menu bar and Dock, on the current Space, with the chat visible through it;
   - the real traffic lights at the top left;
   - green switches between full screen and a normal window, and never macOS's own full screen;
   - the mode and size are remembered;
   - Esc closes, and the red light closes only the viewer;
   - Download lands in Downloads;
   - Cmd+Q with the viewer open quits cleanly;
   - with the main window hidden to the menu bar, closing the viewer keeps the app running;
   - Retina and a second non-Retina screen.
4. Report what failed, in the batch's thread.
