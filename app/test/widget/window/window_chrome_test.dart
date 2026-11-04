import 'package:fake_async/fake_async.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/features/chat/chat_header.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/features/shell/window_title.dart';
import 'package:opencord/features/window/app_title_bar.dart';
import 'package:opencord/features/window/native_window.dart';
import 'package:opencord/features/window/window_buttons.dart';
import 'package:opencord/features/window/window_frame.dart';
import 'package:opencord/features/window/window_mode.dart';
import 'package:opencord/features/window/window_providers.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';

import '../../support/app.dart';
import '../../support/fake_window.dart';
import '../../support/pump.dart';

const custom = WindowInfo(
  chrome: WindowChrome.custom,
  transparent: true,
  frameMargin: windowFrameMargin,
  buttonLayout: ButtonLayout(
    left: [],
    right: [WindowButton.minimize, WindowButton.maximize, WindowButton.close],
  ),
);

const _linux = TargetPlatformVariant({TargetPlatform.linux});
const _windows = TargetPlatformVariant({TargetPlatform.windows});
const _macos = TargetPlatformVariant({TargetPlatform.macOS});

const _leadingKey = Key('leading');
const _centerKey = Key('center');

/// The row alone, in a window [width] wide, as the top of a column.
Future<FakeNativeWindow> _pumpBar(
  WidgetTester tester, {
  WindowInfo info = custom,
  double width = 1280,
  Widget? leading,
  Widget? center,
}) async {
  final window = FakeNativeWindow();
  await pumpThemed(
    tester,
    ProviderScope(
      overrides: [
        nativeWindowProvider.overrideWithValue(window),
        windowInfoProvider.overrideWithValue(info),
      ],
      child: Column(
        children: [AppTitleBar(leading: leading, center: center)],
      ),
    ),
    surface: Size(width, 800),
  );
  return window;
}

/// The window's status arriving from the platform.
Future<void> _sendStatus(
  WidgetTester tester,
  FakeNativeWindow window,
  WindowStatus status,
) async {
  window.statusEvents.add(status);
  await tester.pump();
  await tester.pump();
}

/// Each Linux button's 24 px circle, left to right.
List<Rect> _circles(WidgetTester tester) {
  final circles = find.descendant(
    of: find.byType(GnomeWindowButton),
    matching: find.byType(AnimatedContainer),
  );
  return [
    for (final element in circles.evaluate())
      tester.getRect(find.byElementPredicate((e) => e == element)),
  ];
}

SizedBox _box(Key key, double width, double height) =>
    SizedBox(key: key, width: width, height: height);

void main() {
  for (final platform in [
    TargetPlatform.linux,
    TargetPlatform.windows,
    TargetPlatform.macOS,
  ]) {
    for (final (screen, problem) in [
      ('onboarding', null),
      (
        'the keyring screen',
        const RepoException(RepoErrorKind.other, 'No Secret Service.'),
      ),
    ]) {
      testWidgets('title_bar_is_40_and_flush: $screen on ${platform.name}', (
        tester,
      ) async {
        final app = await MockApp.pump(
          tester,
          size: const Size(1280, 800),
          window: FakeNativeWindow(),
          windowInfo: const WindowInfo(chrome: WindowChrome.custom),
          platform: platform,
          withIdentity: false,
          identityUnavailable: problem,
          settings: (s) => s.copyWith(theme: ThemePreference.dark),
        );
        final bar = find.byKey(ShellKeys.titlebar);

        expect(tester.getRect(bar), const Rect.fromLTWH(0, 0, 1280, 40));
        expect(tester.widget<ColoredBox>(bar).color, OcColors.dark.rail);
        expect(
          find.descendant(of: bar, matching: find.byType(OcIconButton)),
          findsNothing,
          reason: 'no arrows here',
        );
        await app.dispose(tester);
      });
    }
  }

  testWidgets('center_is_at_the_windows_centre', (tester) async {
    await _pumpBar(
      tester,
      leading: _box(_leadingKey, 400, 28),
      center: _box(_centerKey, 200, 20),
    );

    expect(
      tester.getRect(find.byKey(_centerKey)),
      const Rect.fromLTWH(540, 10, 200, 20),
    );
    expect(tester.getTopLeft(find.byKey(_leadingKey)).dx, 8);
  }, variant: _linux);

  testWidgets('the centre takes at most 56 % of the window', (tester) async {
    await _pumpBar(tester, center: _box(_centerKey, 2000, 20));

    expect(
      tester.getRect(find.byKey(_centerKey)),
      rectMoreOrLessEquals(const Rect.fromLTWH(281.6, 10, 716.8, 20)),
    );
  }, variant: _linux);

  testWidgets('linux_buttons_follow_the_layout: right-hand', (tester) async {
    await _pumpBar(tester, leading: _box(_leadingKey, 60, 28));

    final circles = _circles(tester);
    expect(circles, hasLength(3));
    expect(circles.last, const Rect.fromLTWH(1248, 8, 24, 24));
    expect(circles[1].right, circles.last.left - 6);
    expect(circles[0].right, circles[1].left - 6);
    expect(tester.getTopLeft(find.byKey(_leadingKey)).dx, 8);
  }, variant: _linux);

  testWidgets('linux_buttons_follow_the_layout: left-hand', (tester) async {
    await _pumpBar(
      tester,
      info: const WindowInfo(
        chrome: WindowChrome.custom,
        buttonLayout: ButtonLayout(
          left: [WindowButton.close, WindowButton.minimize],
          right: [],
        ),
      ),
      leading: _box(_leadingKey, 60, 28),
    );

    final circles = _circles(tester);
    expect(circles, hasLength(2));
    expect(circles.first, const Rect.fromLTWH(8, 8, 24, 24));
    expect(circles.last.left, circles.first.right + 6);
    expect(
      tester.getTopLeft(find.byKey(_leadingKey)).dx,
      circles.last.right + 3,
      reason: 'the arrows come after the buttons',
    );
  }, variant: _linux);

  testWidgets('GNOME buttons follow the button layout and the window state', (
    tester,
  ) async {
    final window = await _pumpBar(tester);

    await tester.tap(find.bySemanticsLabel('Minimize'));
    await tester.tap(find.bySemanticsLabel('Maximize'));
    await _sendStatus(tester, window, const WindowStatus(maximized: true));
    final restore = find.bySemanticsLabel('Restore').evaluate().length;
    await tester.tap(find.bySemanticsLabel('Restore'));
    await tester.tap(find.bySemanticsLabel('Close'));
    window.layoutEvents.add(ButtonLayout.parse('appmenu:close'));
    await tester.pump();
    await tester.pump();

    expect(restore, 1);
    expect(find.bySemanticsLabel('Minimize'), findsNothing);
    expect(find.bySemanticsLabel('Close'), findsOneWidget);
    expect(
      window.calls,
      containsAllInOrder([
        'minimize',
        'toggleMaximize',
        'toggleMaximize',
        'close',
      ]),
    );
  }, variant: _linux);

  testWidgets('windows_caption_is_flush_top_right', (tester) async {
    await _pumpBar(
      tester,
      info: const WindowInfo(chrome: WindowChrome.custom),
      leading: _box(_leadingKey, 60, 28),
      center: _box(_centerKey, 200, 20),
    );

    expect(
      tester.getRect(find.byType(WindowsCaptionButtons)),
      const Rect.fromLTWH(1280 - 138, 0, 138, 32),
    );
    expect(
      tester.getRect(find.bySemanticsLabel('Close')),
      const Rect.fromLTWH(1280 - 46, 0, 46, 32),
    );
    expect(tester.getTopLeft(find.byKey(_leadingKey)).dx, 8);
    expect(
      tester.getRect(find.byKey(_centerKey)),
      const Rect.fromLTWH(540, 10, 200, 20),
    );
    expect(find.byType(GnomeWindowButton), findsNothing);
  }, variant: _windows);

  testWidgets('macos_keeps_room_for_the_lights', (tester) async {
    final window = await _pumpBar(
      tester,
      info: const WindowInfo(
        chrome: WindowChrome.custom,
        buttonLayout: ButtonLayout(left: [], right: []),
      ),
      leading: _box(_leadingKey, 60, 28),
    );
    final leading = find.byKey(_leadingKey);

    final windowed = tester.getTopLeft(leading).dx;
    await _sendStatus(tester, window, const WindowStatus(fullscreen: true));
    final fullscreen = tester.getTopLeft(leading).dx;
    await _sendStatus(tester, window, const WindowStatus());
    final back = tester.getTopLeft(leading).dx;

    expect(windowed, 78);
    expect(fullscreen, 8);
    expect(back, 78);
    expect(find.byType(GnomeWindowButton), findsNothing);
    expect(find.byType(WindowsCaptionButtons), findsNothing);
  }, variant: _macos);

  testWidgets('drag_area_fills_the_empty_row', (tester) async {
    var arrowTaps = 0;
    final window = await _pumpBar(
      tester,
      leading: OcIconButton(
        icon: OcIcons.arrowBack,
        tooltip: 'Back',
        size: OcIconButtonSize.compact,
        onPressed: () => arrowTaps++,
      ),
      center: _box(_centerKey, 200, 20),
    );
    const space = Offset(300, 20);
    final arrow = tester.getCenter(find.byType(OcIconButton));
    final crumb = tester.getCenter(find.byKey(_centerKey));

    await tester.dragFrom(arrow, const Offset(40, 10));
    await tester.pump(kDoubleTapTimeout);
    final onArrow = List.of(window.calls);
    await tester.dragFrom(space, const Offset(40, 10));
    await tester.dragFrom(crumb, const Offset(40, 10));
    await tester.pump(kDoubleTapTimeout);
    final drags = window.calls.where((call) => call == 'startDrag').length;
    await tester.tapAt(space);
    await tester.pump(const Duration(milliseconds: 50));
    await tester.tapAt(space);
    await tester.pump(kDoubleTapTimeout);
    await tester.tapAt(space, buttons: kSecondaryButton);
    await tester.pump();
    await tester.tapAt(arrow);
    await tester.pump(kDoubleTapTimeout);

    expect(onArrow, isEmpty, reason: 'an arrow keeps its own pointer');
    expect(drags, 2, reason: 'the row and the crumb both drag');
    expect(
      window.calls.skip(2),
      containsAllInOrder(['toggleMaximize', 'showWindowMenu']),
    );
    expect(arrowTaps, 1);
  }, variant: _linux);

  testWidgets('fullscreen keeps the row, without window controls', (
    tester,
  ) async {
    final window = await _pumpBar(tester, leading: _box(_leadingKey, 60, 28));

    await _sendStatus(tester, window, const WindowStatus(fullscreen: true));
    final bar = tester.getRect(find.byKey(ShellKeys.titlebar));
    final buttons = find.byType(GnomeWindowButton).evaluate().length;
    await _sendStatus(tester, window, const WindowStatus());

    expect(bar, const Rect.fromLTWH(0, 0, 1280, 40));
    expect(buttons, 0);
    expect(find.byType(GnomeWindowButton), findsNWidgets(3));
  }, variant: _linux);

  testWidgets('fullscreen hides the Windows caption buttons', (tester) async {
    final window = await _pumpBar(
      tester,
      info: const WindowInfo(chrome: WindowChrome.custom),
    );

    await _sendStatus(tester, window, const WindowStatus(fullscreen: true));
    final caption = find.byType(WindowsCaptionButtons).evaluate().length;
    await _sendStatus(tester, window, const WindowStatus());

    expect(caption, 0);
    expect(find.byType(WindowsCaptionButtons), findsOneWidget);
  }, variant: _windows);

  testWidgets('a frame the platform draws gets no controls and no dragging', (
    tester,
  ) async {
    final window = await _pumpBar(
      tester,
      info: WindowInfo.none,
      leading: _box(_leadingKey, 60, 28),
    );

    await tester.dragFrom(const Offset(300, 20), const Offset(40, 10));

    expect(find.byType(GnomeWindowButton), findsNothing);
    expect(window.calls, isEmpty);
    expect(tester.getTopLeft(find.byKey(_leadingKey)).dx, 8);
  }, variant: _linux);

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
    final framed = tester.getTopLeft(find.byType(DesktopShell));

    await tester.tapAt(const Offset(1440 - windowFrameMargin + 3, 450));
    window.statusEvents.add(const WindowStatus(maximized: true));
    await tester.pump();
    await tester.pump();
    final maximized = tester.getTopLeft(find.byType(DesktopShell));

    expect(framed, const Offset(windowFrameMargin, windowFrameMargin));
    expect(window.calls, contains('startResize:right'));
    expect(maximized, Offset.zero);
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

  testWidgets('header content is centered in its row', (tester) async {
    final app = await MockApp.pump(
      tester,
      window: FakeNativeWindow(),
      windowInfo: custom,
      platform: TargetPlatform.linux,
    );
    final main = find.byKey(DesktopShell.mainKey);

    final header = tester.getRect(
      find.descendant(of: main, matching: find.byType(ChatHeader)),
    );
    // The channel name and the subtitle under it, as one block.
    final name = tester.getRect(
      find.descendant(of: main, matching: find.text('#announcements')),
    );
    final subtitle = tester.getRect(
      find.descendant(of: main, matching: find.textContaining(' online')),
    );
    final title = name.expandToInclude(subtitle);

    expect(title.center.dy, closeTo(header.center.dy, 1));
    await app.dispose(tester);
  });

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
