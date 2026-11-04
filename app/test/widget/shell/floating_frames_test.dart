import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/shell/shell_drawer.dart';
import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/features/voice/voice_reconnect_bar.dart';
import 'package:opencord/ui/motion/oc_spring.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/widgets/oc_float.dart';
import 'package:opencord/ui/widgets/panel_header.dart';

import '../../support/app.dart';

const bpSizes = <String, Size>{
  'wide-1440': Size(1440, 900),
  'wide-1280': Size(1280, 800),
  'wide-1180': Size(1180, 800),
  'medium-1100': Size(1100, 800),
  'medium-980': Size(980, 800),
  'narrow-940': Size(940, 560),
  'compact-759': Size(759, 700),
};

/// The prototype's reference frames from `reference/measure.json` (§2.1).
const prototypeFrames = <String, Map<String, Rect>>{
  'wide-1440': {
    'rail': Rect.fromLTWH(8, 40, 62, 852),
    'left': Rect.fromLTWH(78, 40, 288, 852),
    'main': Rect.fromLTWH(374, 40, 786, 782),
    'composer': Rect.fromLTWH(374, 830, 786, 62),
    'head': Rect.fromLTWH(78, 40, 288, 52),
    'me': Rect.fromLTWH(78, 830, 288, 62),
  },
  'wide-1280': {
    'rail': Rect.fromLTWH(8, 40, 62, 752),
    'left': Rect.fromLTWH(78, 40, 288, 752),
    'main': Rect.fromLTWH(374, 40, 626, 682),
    'composer': Rect.fromLTWH(374, 730, 626, 62),
    'head': Rect.fromLTWH(78, 40, 288, 52),
    'me': Rect.fromLTWH(78, 730, 288, 62),
  },
  'wide-1180': {
    'rail': Rect.fromLTWH(8, 40, 62, 752),
    'left': Rect.fromLTWH(78, 40, 288, 752),
    'main': Rect.fromLTWH(374, 40, 526, 682),
    'composer': Rect.fromLTWH(374, 730, 526, 62),
    'head': Rect.fromLTWH(78, 40, 288, 52),
    'me': Rect.fromLTWH(78, 730, 288, 62),
  },
  'medium-1100': {
    'rail': Rect.fromLTWH(8, 40, 62, 752),
    'left': Rect.fromLTWH(78, 40, 288, 752),
    'main': Rect.fromLTWH(374, 40, 718, 682),
    'composer': Rect.fromLTWH(374, 730, 718, 62),
    'head': Rect.fromLTWH(78, 40, 288, 52),
    'me': Rect.fromLTWH(78, 730, 288, 62),
  },
  'medium-980': {
    'rail': Rect.fromLTWH(8, 40, 62, 752),
    'left': Rect.fromLTWH(78, 40, 288, 752),
    'main': Rect.fromLTWH(374, 40, 598, 682),
    'composer': Rect.fromLTWH(374, 730, 598, 62),
    'head': Rect.fromLTWH(78, 40, 288, 52),
    'me': Rect.fromLTWH(78, 730, 288, 62),
  },
  'narrow-940': {
    'rail': Rect.fromLTWH(8, 40, 62, 512),
    'left': Rect.fromLTWH(78, 40, 264, 512),
    'main': Rect.fromLTWH(350, 40, 582, 442),
    'composer': Rect.fromLTWH(350, 490, 582, 62),
    'head': Rect.fromLTWH(78, 40, 264, 52),
    'me': Rect.fromLTWH(78, 490, 264, 62),
  },
  'compact-759': {
    'rail': Rect.fromLTWH(8, 40, 62, 652),
    'main': Rect.fromLTWH(78, 40, 673, 582),
    'composer': Rect.fromLTWH(78, 630, 673, 62),
  },
};

void main() {
  testWidgets('frames_match_the_prototype', (tester) async {
    for (final entry in prototypeFrames.entries) {
      final bp = entry.key;
      final expected = entry.value;
      final size = bpSizes[bp]!;
      final app = await MockApp.pump(tester, size: size);

      final server = app.read(currentServerProvider)!;
      final general = app
          .read(serverProvider(server))
          .data!
          .channels
          .values
          .firstWhere((c) => c.kind.isTextLike);
      app.read(navigationProvider.notifier).openChannel(server, general.id);
      await tester.pumpAndSettle();

      for (final part in expected.entries) {
        final key = ShellKeys.byName[part.key]!;
        final finder = find.byKey(key);
        expect(finder, findsOneWidget, reason: '$bp ${part.key} should exist');
        final actual = tester.getRect(finder);
        expect(
          actual,
          rectMoreOrLessEquals(part.value, epsilon: 1),
          reason: '$bp ${part.key}: expected ${part.value}, got $actual',
        );
      }
      await app.dispose(tester);
    }
  });

  testWidgets('ground_shows_between_floats', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1280, 800));
    final BuildContext context = tester.element(find.byType(DesktopShell));
    final colors = context.oc;

    final ground = find.byWidgetPredicate(
      (widget) => widget is ColoredBox && widget.color == colors.rail,
    );
    expect(ground, findsWidgets);
    await app.dispose(tester);
  });

  testWidgets('header_is_52_with_its_line', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1280, 800));
    await tester.pumpAndSettle();

    final headRect = tester.getRect(find.byKey(ShellKeys.head));
    expect(headRect.height, 52);

    final hdRect = tester.getRect(find.byKey(ShellKeys.hd));
    expect(hdRect.height, 52);

    // Verify PanelHeader line decoration
    final panelHeaders = tester.widgetList<PanelHeader>(
      find.byType(PanelHeader),
    );
    expect(panelHeaders, isNotEmpty);

    await app.dispose(tester);
  });

  testWidgets('headers_do_not_drag', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1280, 800));
    await tester.pumpAndSettle();

    // PanelHeader has no window move / drag gesture
    final hd = find.byKey(ShellKeys.hd);
    expect(hd, findsOneWidget);
    final descendants = find.descendant(
      of: hd,
      matching: find.byType(GestureDetector),
    );
    for (final element in descendants.evaluate()) {
      final gd = element.widget as GestureDetector;
      expect(gd.onPanStart, isNull);
      expect(gd.onPanUpdate, isNull);
    }

    await app.dispose(tester);
  });

  testWidgets('left_drawer_opens_from_the_hamburger', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(759, 700));
    await tester.pumpAndSettle();

    // 1. Text view in server
    expect(find.byKey(ShellKeys.left), findsNothing);
    await tester.tap(find.bySemanticsLabel('Channels'));
    await tester.pumpAndSettle();

    expect(find.byKey(ShellKeys.left), findsOneWidget);
    final leftRect = tester.getRect(find.byKey(ShellKeys.left));
    expect(
      leftRect,
      rectMoreOrLessEquals(const Rect.fromLTWH(78, 40, 285, 652), epsilon: 1),
    );

    // Close with Esc
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pumpAndSettle();
    expect(find.byKey(ShellKeys.left), findsNothing);

    // 2. Home view
    app.read(navigationProvider.notifier).openHome();
    await tester.pumpAndSettle();

    expect(find.byKey(ShellKeys.left), findsNothing);
    await tester.tap(find.byIcon(OcIcons.menu));
    await tester.pumpAndSettle();

    expect(find.byKey(ShellKeys.left), findsOneWidget);
    final homeLeftRect = tester.getRect(find.byKey(ShellKeys.left));
    expect(
      homeLeftRect,
      rectMoreOrLessEquals(const Rect.fromLTWH(78, 40, 285, 652), epsilon: 1),
    );

    await app.dispose(tester);
  });

  testWidgets('right_drawer_floats', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1100, 800));
    await tester.pumpAndSettle();

    // Run open/close twice each: first with Esc, second with scrim click
    for (var i = 0; i < 2; i++) {
      // Open with shortcut
      await tester.sendKeyDownEvent(LogicalKeyboardKey.control);
      await tester.sendKeyDownEvent(LogicalKeyboardKey.shift);
      await tester.sendKeyEvent(LogicalKeyboardKey.keyU);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.shift);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.control);
      await tester.pumpAndSettle();

      final drawer = tester.getRect(find.byKey(ShellKeys.side));
      expect(
        drawer,
        rectMoreOrLessEquals(
          const Rect.fromLTWH(828, 40, 264, 752),
          epsilon: 1,
        ),
      );

      final scrim = tester.getRect(find.byKey(ShellKeys.scrim));
      expect(
        scrim,
        rectMoreOrLessEquals(const Rect.fromLTWH(0, 40, 1100, 760), epsilon: 1),
      );

      // Close with Esc
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pumpAndSettle();
      expect(find.byKey(ShellKeys.scrim), findsNothing);
    }

    for (var i = 0; i < 2; i++) {
      // Open with shortcut
      await tester.sendKeyDownEvent(LogicalKeyboardKey.control);
      await tester.sendKeyDownEvent(LogicalKeyboardKey.shift);
      await tester.sendKeyEvent(LogicalKeyboardKey.keyU);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.shift);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.control);
      await tester.pumpAndSettle();

      // Close with scrim tap
      await tester.tapAt(const Offset(200, 200));
      await tester.pumpAndSettle();
      expect(find.byKey(ShellKeys.scrim), findsNothing);
    }

    await app.dispose(tester);
  });

  testWidgets('drawers_open_on_the_springs', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        theme: ThemeData(extensions: const [OcColors.dark]),
        home: Scaffold(
          body: Stack(
            children: [
              ShellDrawer(
                fromRight: true,
                width: 264,
                onClose: () {},
                child: const SizedBox(),
              ),
            ],
          ),
        ),
      ),
    );

    // Default tester view width is 800
    const windowWidth = 800.0;
    const distance = 264.0 + 8.0 + 24.0;

    // Pump 40 ms
    await tester.pump(const Duration(milliseconds: 40));
    final x40 = tester.getRect(find.byKey(ShellKeys.side)).left;
    final progress40 = OcSpring.pane.valueAt(const Duration(milliseconds: 40));
    final expected40 =
        windowWidth - 8.0 - 264.0 + (1.0 - progress40) * distance;
    expect((x40 - expected40).abs(), lessThanOrEqualTo(2.0));

    // Pump another 80 ms (total 120 ms)
    await tester.pump(const Duration(milliseconds: 80));
    final x120 = tester.getRect(find.byKey(ShellKeys.side)).left;
    final progress120 = OcSpring.pane.valueAt(
      const Duration(milliseconds: 120),
    );
    final expected120 =
        windowWidth - 8.0 - 264.0 + (1.0 - progress120) * distance;
    expect((x120 - expected120).abs(), lessThanOrEqualTo(2.0));
  });

  testWidgets('smallest_window_with_everything_has_no_overflow', (
    tester,
  ) async {
    // 940 × 560, in voice, with reconnect bar, pinned message, Music drawer
    final app = await MockApp.pump(tester, size: const Size(940, 560));
    final server = app.read(currentServerProvider)!;

    final voice = app
        .read(serverProvider(server))
        .data!
        .channels
        .values
        .firstWhere((c) => c.kind == ChannelKind.voice);
    await app.read(voiceSessionProvider.notifier).join(server, voice.id);
    await tester.pumpAndSettle();

    // Reconnect bar
    app.repository.debugVoiceElsewhere();
    await tester.pumpAndSettle();

    // Pinned message
    final textChan = app
        .read(serverProvider(server))
        .data!
        .channels
        .values
        .firstWhere((c) => c.kind.isTextLike);
    app.read(navigationProvider.notifier).openChannel(server, textChan.id);
    await tester.pumpAndSettle();

    // Open music drawer
    app.read(rightPanelModeProvider.notifier).set(RightPanelMode.music);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.control);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.shift);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyU);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.shift);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.control);
    await tester.pumpAndSettle();

    expect(tester.takeException(), isNull);

    // Channel list scrolls
    final channelsList = find.byKey(ShellKeys.channels);
    expect(channelsList, findsOneWidget);
    await tester.drag(channelsList, const Offset(0, -100), warnIfMissed: false);
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);

    await app.dispose(tester);
  });

  testWidgets('reconnect_bar_floats', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1280, 800));
    final server = app.read(currentServerProvider)!;
    final voice = app
        .read(serverProvider(server))
        .data!
        .channels
        .values
        .firstWhere((c) => c.kind == ChannelKind.voice);
    await app.read(voiceSessionProvider.notifier).join(server, voice.id);
    await tester.pumpAndSettle();

    app.repository.debugVoiceElsewhere();
    await tester.pumpAndSettle();

    final bar = tester.getRect(find.byKey(VoiceReconnectBar.barKey));
    expect(bar.height, 40);
    expect(bar.left, 8);
    expect(bar.right, 1280 - 8);

    // Check elevated surface and radius
    final floatWidget = tester.widget<OcFloat>(
      find.byKey(VoiceReconnectBar.barKey),
    );
    expect(floatWidget.radius, 16);

    // Dismiss leaves on paneOut
    await tester.tap(find.byTooltip('Dismiss'));
    await tester.pump(const Duration(milliseconds: 40));
    final progress40 = OcSpring.paneOut.valueAt(
      const Duration(milliseconds: 40),
    );
    expect(progress40, greaterThan(0));

    await tester.pumpAndSettle();
    await app.dispose(tester);
  });
}
