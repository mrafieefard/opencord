import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/channels/user_panel.dart';
import 'package:opencord/features/channels/voice_panel.dart';
import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/widgets/avatar.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';

import '../../support/app.dart';

const _dev = 'opencord.example:7710';

int _voiceChannel(MockApp app) => app
    .read(serverProvider(_dev))
    .data!
    .channels
    .values
    .firstWhere((channel) => channel.name == 'General')
    .id;

Future<void> _settle(WidgetTester tester) async {
  for (var i = 0; i < 20; i++) {
    await tester.pump(const Duration(milliseconds: 50));
  }
}

void main() {
  group('Voice card and user panel (§5.5, §5.6, Task 14)', () {
    testWidgets('card_numbers', (tester) async {
      final app = await MockApp.pump(tester, size: const Size(1280, 800));
      await _settle(tester);

      final channelId = _voiceChannel(app);
      await app.read(voiceSessionProvider.notifier).join(_dev, channelId);
      await _settle(tester);

      final vcardFinder = find.byKey(ShellKeys.vcard);
      expect(vcardFinder, findsOneWidget);

      final vcardRect = tester.getRect(vcardFinder);
      expect(vcardRect.height, closeTo(103.4, 0.5));

      // Container padding inside VoiceConnectedPanel: 8 8 10
      final panelContainer = tester.widget<Container>(
        find
            .descendant(
              of: find.byType(VoiceConnectedPanel),
              matching: find.byType(Container),
            )
            .first,
      );
      expect(panelContainer.padding, const EdgeInsets.fromLTRB(8, 8, 8, 10));

      // Line 1: clickable link
      final statusIcon = tester.widget<Icon>(
        find.descendant(
          of: find.byType(VoiceConnectedPanel),
          matching: find.byIcon(OcIcons.graphicEq),
        ),
      );
      expect(statusIcon.size, 16);

      final statusText = tester.widget<Text>(
        find.descendant(
          of: find.byType(VoiceConnectedPanel),
          matching: find.text('Voice connected'),
        ),
      );
      expect(statusText.style?.fontSize, 13);
      expect(statusText.style?.fontWeight, FontWeight.w600);

      final subtitleText = tester.widget<Text>(
        find.descendant(
          of: find.byType(VoiceConnectedPanel),
          matching: find.text('General / Opencord Dev'),
        ),
      );
      expect(subtitleText.style?.fontSize, 12.5);
      expect(
        subtitleText.style?.color,
        tester.element(vcardFinder).oc.textMuted,
      );

      // Disconnect compact button: 28 x 28, icon 20
      final disconnectButton = tester.widget<OcIconButton>(
        find.descendant(
          of: find.byType(VoiceConnectedPanel),
          matching: find.byWidgetPredicate(
            (w) => w is OcIconButton && w.tooltip == 'Disconnect',
          ),
        ),
      );
      expect(disconnectButton.size, OcIconButtonSize.compact);
      expect(disconnectButton.iconSize, 20);

      // Toggles: equal widths, 8 apart, height 36, radius 10
      final toggleFinders = find.descendant(
        of: find.byType(VoiceConnectedPanel),
        matching: find.byType(AnimatedContainer),
      );
      // Filter for toggles with height 36
      final toggleContainers = <AnimatedContainer>[];
      for (final element in toggleFinders.evaluate()) {
        final widget = element.widget as AnimatedContainer;
        final constraints = widget.constraints;
        if (widget.decoration is BoxDecoration) {
          final box = widget.decoration! as BoxDecoration;
          if (box.borderRadius == BorderRadius.circular(10) &&
              (element.size?.height == 36 ||
                  (constraints != null && constraints.maxHeight == 36))) {
            toggleContainers.add(widget);
          }
        }
      }
      expect(toggleContainers.length, 3); // camera, screen, soundboard

      // Verify toggles have equal widths
      final firstToggle = tester.getRect(
        find.descendant(
          of: find.byType(VoiceConnectedPanel),
          matching: find.byTooltip('Camera'),
        ),
      );
      final secondToggle = tester.getRect(
        find.descendant(
          of: find.byType(VoiceConnectedPanel),
          matching: find.byTooltip('Screen'),
        ),
      );
      expect(firstToggle.height, 36);
      expect(secondToggle.height, 36);
      expect(firstToggle.width, closeTo(secondToggle.width, 0.5));
      expect(secondToggle.left - firstToggle.right, closeTo(8, 0.5));

      await app.dispose(tester);
    });

    testWidgets('card_leaves_on_the_curve', (tester) async {
      final app = await MockApp.pump(tester, size: const Size(1280, 800));
      await _settle(tester);

      final channelId = _voiceChannel(app);
      await app.read(voiceSessionProvider.notifier).join(_dev, channelId);
      await _settle(tester);

      final vcardFinder = find.byKey(ShellKeys.vcard);
      final channelsFinder = find.byKey(ShellKeys.channels);
      expect(vcardFinder, findsOneWidget);

      final initialVcardHeight = tester.getSize(vcardFinder).height;
      expect(initialVcardHeight, closeTo(103.4, 0.5));

      final initialChannelsHeight = tester.getSize(channelsFinder).height;

      // Start leaving
      await app.read(voiceSessionProvider.notifier).leave();
      await tester.pump(); // t = 0

      expect(tester.getSize(vcardFinder).height, closeTo(103.4, 0.5));

      // Sample curve from §4.3: 103.4, 66.5, 29.5, 11.4, 4.1, 1.4, 0, 0
      // at t = 0, 40, 80, 120, 160, 200, 280, 360 ms.
      const expectedHeights = [
        (40, 66.5),
        (80, 29.5),
        (120, 11.4),
        (160, 4.1),
        (200, 1.4),
      ];

      var elapsed = 0;
      for (final (ms, expected) in expectedHeights) {
        await tester.pump(Duration(milliseconds: ms - elapsed));
        elapsed = ms;

        final currentVcardHeight = tester.getSize(vcardFinder).height;
        expect(
          currentVcardHeight,
          closeTo(expected, 1.5),
          reason: 'vcard height at $ms ms',
        );

        // Channels grows by what vcard and its gap give up
        final currentChannelsHeight = tester.getSize(channelsFinder).height;
        final channelsGrowth = currentChannelsHeight - initialChannelsHeight;
        final vcardLost = initialVcardHeight - currentVcardHeight;
        // The slot gives up both vcard height and gap (8px scaled by (1 - factor))
        final factor = currentVcardHeight / 103.4;
        final expectedGrowth = vcardLost + (OcFloats.gap * (1 - factor));
        expect(
          channelsGrowth,
          closeTo(expectedGrowth, 1.5),
          reason: 'channels growth at $ms ms',
        );
      }

      // Settle the remainder (up to 360ms)
      await tester.pump(const Duration(milliseconds: 160));
      await _settle(tester);

      // Now vcard is gone and channels has grown by full vcard (103.4) + gap (8) = 111.4
      expect(find.byKey(ShellKeys.vcard), findsNothing);
      final finalChannelsHeight = tester.getSize(channelsFinder).height;
      expect(
        finalChannelsHeight - initialChannelsHeight,
        closeTo(103.4 + OcFloats.gap, 1.0),
      );

      await app.dispose(tester);
    });

    testWidgets('card_joins_and_leaves_twice', (tester) async {
      final app = await MockApp.pump(tester, size: const Size(1280, 800));
      await _settle(tester);

      final channelId = _voiceChannel(app);

      // First join
      await app.read(voiceSessionProvider.notifier).join(_dev, channelId);
      await _settle(tester);
      expect(find.byKey(ShellKeys.vcard), findsOneWidget);
      expect(
        tester.getSize(find.byKey(ShellKeys.vcard)).height,
        closeTo(103.4, 0.5),
      );

      // First leave
      await app.read(voiceSessionProvider.notifier).leave();
      await _settle(tester);
      expect(find.byKey(ShellKeys.vcard), findsNothing);

      // Second join
      await app.read(voiceSessionProvider.notifier).join(_dev, channelId);
      await _settle(tester);
      expect(find.byKey(ShellKeys.vcard), findsOneWidget);
      expect(
        tester.getSize(find.byKey(ShellKeys.vcard)).height,
        closeTo(103.4, 0.5),
      );

      // Second leave
      await app.read(voiceSessionProvider.notifier).leave();
      await _settle(tester);
      expect(find.byKey(ShellKeys.vcard), findsNothing);

      await app.dispose(tester);
    });

    testWidgets('it_keeps_its_channel_while_leaving', (tester) async {
      final app = await MockApp.pump(tester, size: const Size(1280, 800));
      await _settle(tester);

      final channelId = _voiceChannel(app);
      await app.read(voiceSessionProvider.notifier).join(_dev, channelId);
      await _settle(tester);

      expect(find.text('General / Opencord Dev'), findsOneWidget);

      // Start leaving
      await app.read(voiceSessionProvider.notifier).leave();
      // Pump 80 ms: midway through the exit animation
      await tester.pump(const Duration(milliseconds: 80));

      expect(find.byKey(ShellKeys.vcard), findsOneWidget);
      expect(find.text('General / Opencord Dev'), findsOneWidget);

      await _settle(tester);
      await app.dispose(tester);
    });

    testWidgets('panel_numbers', (tester) async {
      final app = await MockApp.pump(tester, size: const Size(1280, 800));
      await _settle(tester);

      final meFinder = find.byKey(ShellKeys.me);
      expect(meFinder, findsOneWidget);

      final meRect = tester.getRect(meFinder);
      expect(meRect.height, 62);

      // Container inside UserPanel has padding: 0 6 0 8
      final container = tester.widget<Container>(
        find
            .descendant(
              of: find.byType(UserPanel),
              matching: find.byType(Container),
            )
            .first,
      );
      expect(container.padding, const EdgeInsets.fromLTRB(8, 0, 6, 0));

      // Who row: avatar 32, name 13/600, status 11
      final avatarFinder = find.descendant(
        of: find.byType(UserPanel),
        matching: find.byType(OcAvatar),
      );
      expect(avatarFinder, findsOneWidget);
      expect(tester.getSize(avatarFinder).width, 32);
      expect(tester.getSize(avatarFinder).height, 32);

      final nameFinder = find.descendant(
        of: find.byType(UserPanel),
        matching: find.text('Alex Rivera'),
      );
      expect(nameFinder, findsOneWidget);
      final nameText = tester.widget<Text>(nameFinder);
      expect(nameText.style?.fontSize, 13);
      expect(nameText.style?.fontWeight, FontWeight.w600);

      final statusFinder = find.descendant(
        of: find.byType(UserPanel),
        matching: find.text('Online'),
      );
      expect(statusFinder, findsOneWidget);
      final statusText = tester.widget<Text>(statusFinder);
      expect(statusText.style?.fontSize, 11);

      // Chevrons: 14 x 28, radius 6, expandMore at 12
      final chevronContainers = find.descendant(
        of: find.byType(UserPanel),
        matching: find.byWidgetPredicate((w) {
          if (w is Container && w.decoration is BoxDecoration) {
            final box = w.decoration! as BoxDecoration;
            final c = w.constraints;
            return box.borderRadius == BorderRadius.circular(6) &&
                c != null &&
                c.maxWidth == 14 &&
                c.maxHeight == 28;
          }
          return false;
        }),
      );
      expect(
        chevronContainers,
        findsNWidgets(2),
      ); // mic chevron and speaker chevron

      final chevronIcons = find.descendant(
        of: find.byType(UserPanel),
        matching: find.byIcon(OcIcons.expandMore),
      );
      expect(chevronIcons, findsNWidgets(2));
      for (final element in chevronIcons.evaluate()) {
        final icon = element.widget as Icon;
        expect(icon.size, 12);
      }

      await app.dispose(tester);
    });

    testWidgets('muted_and_deafened_buttons_stay_active', (tester) async {
      final app = await MockApp.pump(tester, size: const Size(1280, 800));
      await _settle(tester);

      final muteFinder = find.descendant(
        of: find.byType(UserPanel),
        matching: find.byTooltip('Mute'),
      );
      expect(muteFinder, findsOneWidget);

      // Tap mute
      await tester.tap(muteFinder);
      await tester.pumpAndSettle();

      expect(app.read(voiceSessionProvider).muted, isTrue);

      final unmuteFinder = find.descendant(
        of: find.byType(UserPanel),
        matching: find.byWidgetPredicate(
          (w) => w is OcIconButton && w.tooltip == 'Unmute',
        ),
      );
      expect(unmuteFinder, findsOneWidget);

      final muteButton = tester.widget<OcIconButton>(unmuteFinder);
      expect(muteButton.active, isTrue);
      expect(muteButton.activeStyle, OcActiveStyle.selected);

      // Tap deafen
      final deafenFinder = find.descendant(
        of: find.byType(UserPanel),
        matching: find.byTooltip('Deafen'),
      );
      expect(deafenFinder, findsOneWidget);

      await tester.tap(deafenFinder);
      await tester.pumpAndSettle();

      expect(app.read(voiceSessionProvider).deafened, isTrue);

      final undeafenFinder = find.descendant(
        of: find.byType(UserPanel),
        matching: find.byWidgetPredicate(
          (w) => w is OcIconButton && w.tooltip == 'Undeafen',
        ),
      );
      expect(undeafenFinder, findsOneWidget);

      final deafenButton = tester.widget<OcIconButton>(undeafenFinder);
      expect(deafenButton.active, isTrue);
      expect(deafenButton.activeStyle, OcActiveStyle.selected);

      await app.dispose(tester);
    });
  });
}
