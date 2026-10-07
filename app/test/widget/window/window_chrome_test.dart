import 'package:fake_async/fake_async.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/features/shell/window_title.dart';
import 'package:opencord/features/window/header_bar.dart';
import 'package:opencord/features/window/native_window.dart';
import 'package:opencord/features/window/window_frame.dart';
import 'package:opencord/features/window/window_mode.dart';
import 'package:opencord/features/window/window_providers.dart';

import '../../support/app.dart';
import '../../support/fake_window.dart';

const custom = WindowInfo(
  chrome: WindowChrome.custom,
  transparent: true,
  frameMargin: windowFrameMargin,
  buttonLayout: ButtonLayout(
    left: [],
    right: [WindowButton.minimize, WindowButton.maximize, WindowButton.close],
  ),
);

/// A point in the chat header's empty space.
Offset headerSpace(WidgetTester tester) {
  final main = tester.getRect(find.byKey(DesktopShell.mainKey));
  return Offset(main.center.dx, main.top + 30);
}

void main() {
  testWidgets('the header row drags, maximizes and opens the window menu', (
    tester,
  ) async {
    final window = FakeNativeWindow();
    final app = await MockApp.pump(
      tester,
      window: window,
      windowInfo: custom,
      platform: TargetPlatform.linux,
    );
    final point = headerSpace(tester);

    await tester.dragFrom(point, const Offset(40, 10));
    await tester.pump(kDoubleTapTimeout);
    await tester.tapAt(point);
    await tester.pump(const Duration(milliseconds: 50));
    await tester.tapAt(point);
    await tester.pump(kDoubleTapTimeout);
    await tester.tapAt(point, buttons: kSecondaryButton);
    await tester.pump();

    expect(
      window.calls,
      containsAllInOrder(['startDrag', 'toggleMaximize', 'showWindowMenu']),
    );
    await app.dispose(tester);
  });

  testWidgets('on a tiling compositor the headers are just headers', (
    tester,
  ) async {
    final window = FakeNativeWindow();
    final app = await MockApp.pump(
      tester,
      window: window,
      windowInfo: const WindowInfo(chrome: WindowChrome.bare),
      platform: TargetPlatform.linux,
    );

    await tester.dragFrom(headerSpace(tester), const Offset(40, 10));
    await tester.pump();

    expect(window.calls.where((call) => !call.startsWith('setTitle')), isEmpty);
    expect(find.bySemanticsLabel('Close'), findsNothing);
    await app.dispose(tester);
  });

  testWidgets('GNOME buttons follow the button layout and the window state', (
    tester,
  ) async {
    final window = FakeNativeWindow();
    final app = await MockApp.pump(
      tester,
      window: window,
      windowInfo: custom,
      platform: TargetPlatform.linux,
    );

    final membersHeader = tester.getRect(find.byKey(DesktopShell.membersKey));
    final close = tester.getCenter(find.bySemanticsLabel('Close'));
    await tester.tap(find.bySemanticsLabel('Minimize'));
    await tester.tap(find.bySemanticsLabel('Maximize'));
    window.statusEvents.add(const WindowStatus(maximized: true));
    await tester.pump();
    await tester.pump();
    final restore = find.bySemanticsLabel('Restore').evaluate().length;
    window.layoutEvents.add(ButtonLayout.parse('appmenu:close'));
    await tester.pump();
    await tester.pump();

    expect(
      membersHeader.contains(close),
      isTrue,
      reason: 'the rightmost header has them',
    );
    expect(restore, 1);
    expect(find.bySemanticsLabel('Minimize'), findsNothing);
    expect(window.calls, containsAllInOrder(['minimize', 'toggleMaximize']));
    await app.dispose(tester);
  });

  testWidgets('a left button layout puts the buttons in the first header', (
    tester,
  ) async {
    final app = await MockApp.pump(
      tester,
      window: FakeNativeWindow(),
      windowInfo: const WindowInfo(
        chrome: WindowChrome.custom,
        buttonLayout: ButtonLayout(left: [WindowButton.close], right: []),
      ),
      platform: TargetPlatform.linux,
    );

    final sidebar = tester.getRect(find.byKey(DesktopShell.sidebarKey));
    expect(
      sidebar.contains(tester.getCenter(find.bySemanticsLabel('Close'))),
      isTrue,
    );
    await app.dispose(tester);
  });

  testWidgets('a floating window gets the frame margin and resize handles', (
    tester,
  ) async {
    final window = FakeNativeWindow();
    final app = await MockApp.pump(
      tester,
      window: window,
      windowInfo: custom,
      platform: TargetPlatform.linux,
    );
    final framed = tester.getTopLeft(find.byKey(DesktopShell.railKey));

    await tester.tapAt(const Offset(1440 - windowFrameMargin + 3, 450));
    window.statusEvents.add(const WindowStatus(maximized: true));
    await tester.pump();
    await tester.pump();
    final maximized = tester.getTopLeft(find.byKey(DesktopShell.railKey));

    expect(framed, const Offset(windowFrameMargin, windowFrameMargin));
    expect(window.calls, contains('startResize:right'));
    expect(maximized, Offset.zero);
    await app.dispose(tester);
  });

  testWidgets('Windows caption buttons sit flush in the top right corner', (
    tester,
  ) async {
    final app = await MockApp.pump(
      tester,
      window: FakeNativeWindow(),
      windowInfo: const WindowInfo(chrome: WindowChrome.custom),
      platform: TargetPlatform.windows,
    );

    final close = tester.getRect(find.bySemanticsLabel('Close'));
    expect(close, const Rect.fromLTWH(1440 - 46, 0, 46, 32));
    await app.dispose(tester);
  });

  testWidgets('the window title follows the open channel', (tester) async {
    final window = FakeNativeWindow();
    final app = await MockApp.pump(
      tester,
      window: window,
      windowInfo: custom,
      platform: TargetPlatform.linux,
    );

    await tester.sendKeyDownEvent(LogicalKeyboardKey.altLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.arrowDown);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.altLeft);
    await tester.pump();

    final titles = window.calls
        .where((call) => call.startsWith('setTitle:'))
        .toList();
    expect(titles.length, greaterThanOrEqualTo(2));
    expect(titles.last, 'setTitle:${app.read(windowTitleProvider)}');
    await app.dispose(tester);
  });

  testWidgets('header content is centered in the 60 px row', (tester) async {
    final app = await MockApp.pump(
      tester,
      window: FakeNativeWindow(),
      windowInfo: custom,
      platform: TargetPlatform.linux,
    );
    final main = find.byKey(DesktopShell.mainKey);

    final header = tester.getRect(
      find.descendant(of: main, matching: find.byType(HeaderBar)),
    );
    final title = tester.getRect(
      find.descendant(of: main, matching: find.text('#announcements')),
    );

    expect(title.center.dy, closeTo(header.center.dy, 1));
    await app.dispose(tester);
  });

  testWidgets(
    'with window buttons beside it, the member list drops its own close',
    (tester) async {
      final app = await MockApp.pump(
        tester,
        window: FakeNativeWindow(),
        windowInfo: custom,
        platform: TargetPlatform.linux,
      );

      expect(find.bySemanticsLabel('Close member list'), findsNothing);
      expect(find.bySemanticsLabel('Close'), findsOneWidget);
      await app.dispose(tester);
    },
  );

  testWidgets('F11 toggles fullscreen', (tester) async {
    final window = FakeNativeWindow();
    final app = await MockApp.pump(
      tester,
      window: window,
      windowInfo: custom,
      platform: TargetPlatform.linux,
    );

    await tester.sendKeyEvent(LogicalKeyboardKey.f11);
    await tester.pump();

    expect(window.calls, contains('setFullscreen:true'));
    await app.dispose(tester);
  });

  test(
    'geometry is saved after it settles, and only for a floating window',
    () {
      fakeAsync((async) {
        final window = FakeNativeWindow();
        final store = MemoryKeyValueStore();
        final keeper = WindowGeometryKeeper(window: window, store: store);

        window.geometryEvents.add(
          const WindowGeometry(width: 1000, height: 700, x: 10, y: 20),
        );
        async.elapse(const Duration(milliseconds: 100));
        window.geometryEvents.add(
          const WindowGeometry(width: 1100, height: 750, x: 10, y: 20),
        );
        async.elapse(const Duration(milliseconds: 100));
        final early = store.read(windowGeometryKey);
        async.elapse(const Duration(seconds: 1));
        final floating = savedWindowGeometry(store);
        window.statusEvents.add(const WindowStatus(maximized: true));
        window.geometryEvents.add(
          const WindowGeometry(width: 1920, height: 1080),
        );
        async.elapse(const Duration(seconds: 1));
        final maximized = savedWindowGeometry(store);
        keeper.dispose();

        expect(early, isNull);
        expect(
          floating,
          const WindowGeometry(width: 1100, height: 750, x: 10, y: 20),
        );
        expect(
          maximized,
          const WindowGeometry(
            width: 1100,
            height: 750,
            x: 10,
            y: 20,
            maximized: true,
          ),
        );
      });
    },
  );
}
