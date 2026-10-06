import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/widgets/avatar.dart';
import 'package:opencord/ui/widgets/badges.dart';
import 'package:opencord/ui/widgets/key_hint.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_spinner.dart';
import 'package:opencord/ui/widgets/oc_switch.dart';

import '../../support/pump.dart';

Color? fillOf(WidgetTester tester, Finder widget) {
  final container = tester.widget<AnimatedContainer>(
    find.descendant(of: widget, matching: find.byType(AnimatedContainer)).first,
  );
  return (container.decoration as BoxDecoration?)?.color;
}

void main() {
  group('OcIconButton', () {
    testWidgets('presses, and is labelled by its tooltip', (tester) async {
      var presses = 0;
      await pumpThemed(
        tester,
        Center(
          child: OcIconButton(
            icon: OcIcons.search,
            tooltip: 'Search',
            onPressed: () => presses++,
          ),
        ),
      );

      await tester.tap(find.byType(OcIconButton));

      expect(presses, 1);
      expect(find.bySemanticsLabel('Search'), findsOneWidget);
    });

    testWidgets('shows its tooltip with the shortcut after a pause', (
      tester,
    ) async {
      await pumpThemed(
        tester,
        Center(
          child: OcIconButton(
            icon: OcIcons.search,
            tooltip: 'Search',
            shortcut: const SingleActivator(
              LogicalKeyboardKey.keyK,
              control: true,
            ),
            onPressed: () {},
          ),
        ),
      );
      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      addTearDown(mouse.removePointer);
      await mouse.addPointer(location: Offset.zero);
      await mouse.moveTo(tester.getCenter(find.byType(OcIconButton)));
      await tester.pump(const Duration(milliseconds: 500));

      expect(find.text('Search (Ctrl+K)'), findsOneWidget);
    });

    testWidgets('an active inverted button uses the accent', (tester) async {
      await pumpThemed(
        tester,
        Center(
          child: OcIconButton(
            icon: OcIcons.mic,
            tooltip: 'Mute',
            active: true,
            activeStyle: OcActiveStyle.inverted,
            onPressed: () {},
          ),
        ),
      );

      expect(fillOf(tester, find.byType(OcIconButton)), OcColors.dark.accent);
    });

    testWidgets('without a callback it does nothing', (tester) async {
      await pumpThemed(
        tester,
        const Center(
          child: OcIconButton(icon: OcIcons.mic, tooltip: 'Mute'),
        ),
      );

      await tester.tap(find.byType(OcIconButton), warnIfMissed: false);

      expect(tester.takeException(), isNull);
    });
  });

  group('OcButton', () {
    testWidgets('primary buttons are inverted', (tester) async {
      await pumpThemed(
        tester,
        Center(
          child: OcButton.primary(label: 'Join', onPressed: () {}),
        ),
      );

      final label = tester.widget<Text>(find.text('Join'));
      expect(fillOf(tester, find.byType(OcButton)), OcColors.dark.accent);
      expect(label.style?.color, OcColors.dark.onAccent);
    });

    testWidgets('a busy button shows a spinner and ignores taps', (
      tester,
    ) async {
      var presses = 0;
      await pumpThemed(
        tester,
        Center(
          child: OcButton.primary(
            label: 'Connect',
            busy: true,
            onPressed: () => presses++,
          ),
        ),
      );

      await tester.tap(find.byType(OcButton));

      expect(presses, 0);
      expect(find.byType(OcSpinner), findsOneWidget);
    });

    testWidgets('a disabled button ignores taps', (tester) async {
      await pumpThemed(tester, const Center(child: OcButton(label: 'Save')));

      await tester.tap(find.byType(OcButton), warnIfMissed: false);

      expect(tester.takeException(), isNull);
    });
  });

  group('OcSwitch', () {
    testWidgets('toggles and reports its state', (tester) async {
      bool? changedTo;
      await pumpThemed(
        tester,
        Center(
          child: OcSwitch(
            value: false,
            semanticLabel: 'Reduce motion',
            onChanged: (value) => changedTo = value,
          ),
        ),
      );

      await tester.tap(find.byType(OcSwitch));

      expect(changedTo, isTrue);
      expect(
        tester.getSemantics(find.byType(OcSwitch)),
        matchesSemantics(
          label: 'Reduce motion',
          hasToggledState: true,
          isToggled: false,
          hasEnabledState: true,
          isEnabled: true,
          isFocusable: true,
          hasTapAction: true,
          hasFocusAction: true,
        ),
      );
    });
  });

  group('shortcut labels', () {
    const search = SingleActivator(LogicalKeyboardKey.keyK, control: true);
    const mute = SingleActivator(
      LogicalKeyboardKey.keyM,
      control: true,
      shift: true,
    );
    const previous = SingleActivator(LogicalKeyboardKey.arrowUp, alt: true);

    test('read naturally on Linux and Windows', () {
      expect(shortcutLabel(search, TargetPlatform.linux), 'Ctrl+K');
      expect(shortcutLabel(mute, TargetPlatform.windows), 'Ctrl+Shift+M');
      expect(shortcutLabel(previous, TargetPlatform.linux), 'Alt+↑');
      expect(
        shortcutLabel(search, TargetPlatform.linux, separator: ' '),
        'Ctrl K',
      );
    });

    test('use the Mac symbols on macOS, with Ctrl becoming ⌘', () {
      expect(shortcutLabel(search, TargetPlatform.macOS), '⌘K');
      expect(shortcutLabel(mute, TargetPlatform.macOS), '⇧⌘M');
      expect(shortcutLabel(previous, TargetPlatform.macOS), '⌥↑');
    });

    test('the app shortcut uses ⌘ on macOS and Ctrl elsewhere', () {
      final mac = appShortcut(LogicalKeyboardKey.keyK, TargetPlatform.macOS);
      final linux = appShortcut(LogicalKeyboardKey.keyK, TargetPlatform.linux);

      expect(mac.meta && !mac.control, isTrue);
      expect(linux.control && !linux.meta, isTrue);
    });
  });

  group('initials', () {
    test('take the first letters of the first and last word', () {
      expect(initialsOf('Opencord Dev'), 'OD');
      expect(initialsOf('kai'), 'K');
      expect(initialsOf('  mira  van der berg '), 'MB');
      expect(initialsOf('élodie'), 'É');
      expect(initialsOf('   '), '?');
      expect(initialsOf('#general'), '#');
    });
  });

  group('counts', () {
    test('compact large numbers like Telegram', () {
      expect(formatCount(7), '7');
      expect(formatCount(999), '999');
      expect(formatCount(1000), '1K');
      expect(formatCount(1250), '1.2K');
      expect(formatCount(10500), '10K');
      expect(formatCount(1200000), '1.2M');
    });
  });
}
