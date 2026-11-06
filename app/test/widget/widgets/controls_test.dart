import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/presence.dart';
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

  testWidgets('buttons are as wide as their label unless asked to expand', (
    tester,
  ) async {
    await pumpThemed(
      tester,
      surface: const Size(600, 200),
      const Align(
        alignment: Alignment.topLeft,
        child: SizedBox(
          width: 500,
          child: Wrap(
            children: [
              OcButton(label: 'Cancel', key: Key('cancel')),
              SizedBox(
                width: 500,
                child: OcButton(label: 'Wide', expand: true, key: Key('wide')),
              ),
            ],
          ),
        ),
      ),
    );

    expect(
      tester.getSize(find.byKey(const Key('cancel'))).width,
      lessThan(120),
    );
    expect(tester.getSize(find.byKey(const Key('wide'))).width, 500);
  });

  testWidgets('badge_is_18_tall', (tester) async {
    await pumpThemed(
      tester,
      const Column(children: [UnreadBadge(count: 1), MentionBadge()]),
    );

    final unread = tester.widget<Container>(
      find.descendant(
        of: find.byType(UnreadBadge),
        matching: find.byType(Container),
      ),
    );
    expect(tester.getSize(find.byType(UnreadBadge)).height, 18);
    expect(unread.constraints?.minWidth, 18);
    expect(unread.padding, const EdgeInsets.symmetric(horizontal: 5));

    final unreadText = tester.widget<Text>(
      find.descendant(
        of: find.byType(UnreadBadge),
        matching: find.byType(Text),
      ),
    );
    expect(unreadText.style?.fontSize, 11);
    expect(unreadText.style?.fontWeight, FontWeight.w600);

    final mention = tester.widget<Container>(
      find.descendant(
        of: find.byType(MentionBadge),
        matching: find.byType(Container),
      ),
    );
    expect(tester.getSize(find.byType(MentionBadge)).height, 18);
    expect(mention.constraints?.minWidth, 18);
    expect(mention.padding, const EdgeInsets.symmetric(horizontal: 5));
    expect(
      (mention.decoration! as BoxDecoration).borderRadius,
      BorderRadius.circular(9),
    );

    // In a row, as everywhere it's used: the "@" and its padding, not a circle.
    await pumpThemed(tester, const Row(children: [MentionBadge()]));
    expect(tester.getSize(find.byType(MentionBadge)).width, closeTo(21, 1));
    final mentionText = tester.widget<Text>(
      find.descendant(
        of: find.byType(MentionBadge),
        matching: find.byType(Text),
      ),
    );
    expect(mentionText.style?.fontSize, 11);
    expect(mentionText.style?.fontWeight, FontWeight.w600);
  });

  testWidgets('initials_follow_the_spec', (tester) async {
    final sizes = <double, double>{
      20.0: 8.0,
      22.0: 8.5,
      26.0: 9.5,
      32.0: 12.0,
      34.0: 12.0,
      36.0: 12.5,
      40.0: 13.5,
      72.0: 24.0,
      // Not one of the design's: the line through them.
      28.0: 0.31 * 28 + 1.7,
    };
    for (final MapEntry(key: size, value: fontSize) in sizes.entries) {
      await pumpThemed(tester, OcAvatar(id: 'u1', name: 'Kai', size: size));
      final text = tester.widget<Text>(
        find.descendant(of: find.byType(OcAvatar), matching: find.byType(Text)),
      );
      expect(text.style?.fontSize, fontSize);
      expect(text.style?.fontWeight, FontWeight.w600);
    }
  });

  testWidgets('presence_ring_is_two', (tester) async {
    expect(PresenceBadge.ring, 2.0);
    await pumpThemed(
      tester,
      const OcAvatar(id: 'u1', name: 'Kai', presence: Presence.online),
    );
    final avatar = tester.getRect(find.byType(OcAvatar));
    final badge = tester.getRect(find.byType(PresenceBadge));
    // The disc, inside its ring, reaches 2 past the avatar (`.pres`).
    final disc = badge.deflate(PresenceBadge.ring);
    expect(disc.width, 12);
    expect(disc.right, avatar.right + 2);
    expect(disc.bottom, avatar.bottom + 2);
  });

  testWidgets('disabled_button_uses_the_given_colour', (tester) async {
    const disabled = Color(0xFF556677);
    await pumpThemed(
      tester,
      const OcIconButton(
        icon: Icons.star,
        tooltip: 'Star',
        onPressed: null,
        disabledColor: disabled,
      ),
    );
    final icon = tester.widget<Icon>(find.byType(Icon));
    expect(icon.color, disabled);
  });
}
