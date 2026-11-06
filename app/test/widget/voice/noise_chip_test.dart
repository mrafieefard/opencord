import 'dart:ui' show PointerDeviceKind;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart' show RenderParagraph;
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/channels/voice_panel.dart';
import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/features/voice/input_level_meter.dart';
import 'package:opencord/features/voice/mic_test.dart';
import 'package:opencord/features/voice/mic_test_panel.dart';
import 'package:opencord/features/voice/noise_chip.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_segmented.dart';
import 'package:opencord/ui/widgets/oc_switch.dart';

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
  group('Noise chip and popover (§8.1, §8.2, §8.3, Task 10)', () {
    testWidgets('chip_sits_left_of_disconnect', (tester) async {
      final app = await MockApp.pump(tester, size: const Size(1280, 800));
      await _settle(tester);

      final channelId = _voiceChannel(app);
      await app.read(voiceSessionProvider.notifier).join(_dev, channelId);
      app
          .read(audioSettingsProvider.notifier)
          .update(
            (a) => a.copyWith(noiseSuppression: NoiseSuppression.standard),
          );
      await _settle(tester);

      final chipFinder = find.byType(NoiseChip);
      expect(chipFinder, findsOneWidget);

      final disconnectFinder = find.descendant(
        of: find.byType(VoiceConnectedPanel),
        matching: find.byWidgetPredicate(
          (w) => w is OcIconButton && w.tooltip == 'Disconnect',
        ),
      );
      expect(disconnectFinder, findsOneWidget);

      final chipRect = tester.getRect(chipFinder);
      final disconnectRect = tester.getRect(disconnectFinder);

      // Its right edge 6 from Disconnect's left
      expect(disconnectRect.left - chipRect.right, closeTo(6, 0.5));
      expect(chipRect.height, closeTo(24, 0.5));

      final container = tester.widget<AnimatedContainer>(
        find.descendant(
          of: chipFinder,
          matching: find.byType(AnimatedContainer),
        ),
      );
      final deco = container.decoration as BoxDecoration;
      expect(deco.borderRadius, BorderRadius.circular(12));
      final colors = tester.element(chipFinder).oc;
      expect(deco.color, colors.hover);

      // Tooltip and semantics
      expect(
        find.descendant(
          of: chipFinder,
          matching: find.byTooltip('Noise suppression: Standard'),
        ),
        findsOneWidget,
      );
      expect(
        find.descendant(
          of: chipFinder,
          matching: find.bySemanticsLabel('Noise suppression: Standard'),
        ),
        findsOneWidget,
      );

      // Hover measurement
      final gesture = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await gesture.addPointer(location: Offset.zero);
      await gesture.moveTo(tester.getCenter(chipFinder));
      await tester.pumpAndSettle();

      final hoveredContainer = tester.widget<AnimatedContainer>(
        find.descendant(
          of: chipFinder,
          matching: find.byType(AnimatedContainer),
        ),
      );
      final hoveredDeco = hoveredContainer.decoration as BoxDecoration;
      expect(hoveredDeco.color, colors.selected);

      final icon = tester.widget<Icon>(
        find.descendant(of: chipFinder, matching: find.byType(Icon)),
      );
      expect(icon.icon, OcIcons.graphicEq);
      expect(icon.color, colors.text);

      await gesture.removePointer();
      await app.dispose(tester);
    });

    testWidgets('chip_shows_the_mode', (tester) async {
      final app = await MockApp.pump(tester, size: const Size(1280, 800));
      await _settle(tester);

      final channelId = _voiceChannel(app);
      await app.read(voiceSessionProvider.notifier).join(_dev, channelId);
      app
          .read(audioSettingsProvider.notifier)
          .update(
            (a) => a.copyWith(noiseSuppression: NoiseSuppression.standard),
          );
      await _settle(tester);

      final colors = tester.element(find.byType(NoiseChip)).oc;

      // 1. Standard
      app
          .read(audioSettingsProvider.notifier)
          .update(
            (a) => a.copyWith(noiseSuppression: NoiseSuppression.standard),
          );
      await _settle(tester);
      var icon = tester.widget<Icon>(
        find.descendant(
          of: find.byType(NoiseChip),
          matching: find.byType(Icon),
        ),
      );
      expect(icon.icon, OcIcons.graphicEq);
      expect(icon.color, colors.textSecondary);
      expect(
        find.descendant(
          of: find.byType(NoiseChip),
          matching: find.byTooltip('Noise suppression: Standard'),
        ),
        findsOneWidget,
      );

      // 2. High
      app
          .read(audioSettingsProvider.notifier)
          .update((a) => a.copyWith(noiseSuppression: NoiseSuppression.high));
      await _settle(tester);
      icon = tester.widget<Icon>(
        find.descendant(
          of: find.byType(NoiseChip),
          matching: find.byType(Icon),
        ),
      );
      expect(icon.icon, OcIcons.graphicEq);
      expect(icon.color, colors.textSecondary);
      expect(
        find.descendant(
          of: find.byType(NoiseChip),
          matching: find.byTooltip('Noise suppression: High'),
        ),
        findsOneWidget,
      );

      // 3. Off
      app
          .read(audioSettingsProvider.notifier)
          .update((a) => a.copyWith(noiseSuppression: NoiseSuppression.off));
      await _settle(tester);
      icon = tester.widget<Icon>(
        find.descendant(
          of: find.byType(NoiseChip),
          matching: find.byType(Icon),
        ),
      );
      expect(icon.icon, OcIcons.graphicEq);
      expect(icon.color, colors.textMuted);
      expect(
        find.descendant(
          of: find.byType(NoiseChip),
          matching: find.byTooltip('Noise suppression: Off'),
        ),
        findsOneWidget,
      );

      await app.dispose(tester);
    });

    testWidgets('popover_opens_beside_it', (tester) async {
      final app = await MockApp.pump(tester, size: const Size(1280, 800));
      await _settle(tester);

      final channelId = _voiceChannel(app);
      await app.read(voiceSessionProvider.notifier).join(_dev, channelId);
      app
          .read(audioSettingsProvider.notifier)
          .update(
            (a) => a.copyWith(noiseSuppression: NoiseSuppression.standard),
          );
      await _settle(tester);

      // First open
      await tester.tap(find.byType(NoiseChip));
      await _settle(tester);

      expect(find.byType(NoiseSuppressionPopover), findsOneWidget);
      final chipRect = tester.getRect(find.byType(NoiseChip));
      final popoverRect = tester.getRect(find.byType(NoiseSuppressionPopover));
      expect(popoverRect.left, greaterThanOrEqualTo(chipRect.right));

      // `selected` while its popover is open, `hover` again once it shuts.
      final colors = tester.element(find.byType(NoiseChip)).oc;
      Color? chipColour() {
        final container = tester.widget<AnimatedContainer>(
          find.descendant(
            of: find.byType(NoiseChip),
            matching: find.byType(AnimatedContainer),
          ),
        );
        return (container.decoration! as BoxDecoration).color;
      }

      expect(chipColour(), colors.selected);

      // Close by clicking outside
      await tester.tapAt(const Offset(10, 10));
      await _settle(tester);
      expect(find.byType(NoiseSuppressionPopover), findsNothing);
      expect(chipColour(), colors.hover);

      // Second open (twice)
      await tester.tap(find.byType(NoiseChip));
      await _settle(tester);
      expect(find.byType(NoiseSuppressionPopover), findsOneWidget);

      await tester.tapAt(const Offset(10, 10));
      await _settle(tester);

      await app.dispose(tester);
    });

    testWidgets('popover_numbers', (tester) async {
      final app = await MockApp.pump(tester, size: const Size(1280, 800));
      await _settle(tester);

      final channelId = _voiceChannel(app);
      await app.read(voiceSessionProvider.notifier).join(_dev, channelId);
      app
          .read(audioSettingsProvider.notifier)
          .update(
            (a) => a.copyWith(noiseSuppression: NoiseSuppression.standard),
          );
      await _settle(tester);

      await tester.tap(find.byType(NoiseChip));
      await _settle(tester);

      final popoverFinder = find.byType(NoiseSuppressionPopover);
      final popoverRect = tester.getRect(popoverFinder);
      expect(popoverRect.width, 264.0);

      final titleFinder = find.text('Noise suppression');
      final segmentedFinder = find.byType(OcSegmented<NoiseSuppression>);
      final descFinder = find.text('Removes steady noise; very light');
      final dividerFinder = find.byWidgetPredicate(
        (w) =>
            w is Container && w.constraints?.maxHeight == 1 && w.color != null,
      );
      final micPanelFinder = find.byType(MicTestPanel);

      final titleRect = tester.getRect(titleFinder);
      final segmentedRect = tester.getRect(segmentedFinder);
      final descRect = tester.getRect(descFinder);
      final dividerRect = tester.getRect(dividerFinder);
      final micPanelRect = tester.getRect(micPanelFinder);

      // Gaps: 8, 6, 12, 12
      expect(segmentedRect.top - titleRect.bottom, closeTo(8, 1.0));
      expect(descRect.top - segmentedRect.bottom, closeTo(6, 1.0));
      expect(dividerRect.top - descRect.bottom, closeTo(12, 1.0));
      expect(micPanelRect.top - dividerRect.bottom, closeTo(12, 1.0));

      // Shrink-wrapped at the start, as the People and Music switch is:
      // 2 around and between the two segments.
      final segments = find.descendant(
        of: segmentedFinder,
        matching: find.byType(OcSegmentedButton),
      );
      final segmentsWidth =
          tester.getSize(segments.at(0)).width +
          tester.getSize(segments.at(1)).width;
      expect(segmentedRect.left, popoverRect.left);
      expect(segmentedRect.width, closeTo(segmentsWidth + 6, 0.5));

      final colors = tester.element(popoverFinder).oc;
      final titleText = tester.widget<Text>(titleFinder);
      expect(titleText.style?.fontSize, 14);
      expect(titleText.style?.fontWeight, FontWeight.w600);
      expect(titleText.style?.color, colors.text);

      final descText = tester.widget<Text>(descFinder);
      expect(descText.style?.fontSize, 12.5);
      expect(descText.style?.color, colors.textSecondary);

      await app.dispose(tester);
    });

    testWidgets('the_switch_picks_standard_and_high', (tester) async {
      final app = await MockApp.pump(tester, size: const Size(1280, 800));
      await _settle(tester);

      final channelId = _voiceChannel(app);
      await app.read(voiceSessionProvider.notifier).join(_dev, channelId);
      app
          .read(audioSettingsProvider.notifier)
          .update(
            (a) => a.copyWith(noiseSuppression: NoiseSuppression.standard),
          );
      await _settle(tester);

      await tester.tap(find.byType(NoiseChip));
      await _settle(tester);

      final highSegment = find.descendant(
        of: find.byType(OcSegmented<NoiseSuppression>),
        matching: find.text('High'),
      );
      final standardSegment = find.descendant(
        of: find.byType(OcSegmented<NoiseSuppression>),
        matching: find.text('Standard'),
      );

      // Round 1
      await tester.tap(highSegment);
      await _settle(tester);
      expect(
        app.read(audioSettingsProvider).noiseSuppression,
        NoiseSuppression.high,
      );
      expect(app.repository.audio?.noiseSuppression, NoiseSuppression.high);

      await tester.tap(standardSegment);
      await _settle(tester);
      expect(
        app.read(audioSettingsProvider).noiseSuppression,
        NoiseSuppression.standard,
      );
      expect(app.repository.audio?.noiseSuppression, NoiseSuppression.standard);

      // Round 2 (twice)
      await tester.tap(highSegment);
      await _settle(tester);
      expect(
        app.read(audioSettingsProvider).noiseSuppression,
        NoiseSuppression.high,
      );
      expect(app.repository.audio?.noiseSuppression, NoiseSuppression.high);

      await tester.tap(standardSegment);
      await _settle(tester);
      expect(
        app.read(audioSettingsProvider).noiseSuppression,
        NoiseSuppression.standard,
      );
      expect(app.repository.audio?.noiseSuppression, NoiseSuppression.standard);

      await app.dispose(tester);
    });

    testWidgets('turning_off_and_on_comes_back_to_the_same_mode', (
      tester,
    ) async {
      final app = await MockApp.pump(tester, size: const Size(1280, 800));
      await _settle(tester);

      final channelId = _voiceChannel(app);
      await app.read(voiceSessionProvider.notifier).join(_dev, channelId);
      app
          .read(audioSettingsProvider.notifier)
          .update(
            (a) => a.copyWith(noiseSuppression: NoiseSuppression.standard),
          );
      await _settle(tester);

      // Start with High
      app
          .read(audioSettingsProvider.notifier)
          .update((a) => a.copyWith(noiseSuppression: NoiseSuppression.high));
      await _settle(tester);

      await tester.tap(find.byType(NoiseChip));
      await _settle(tester);

      // Toggle switch off
      await tester.tap(find.byType(OcSwitch));
      await _settle(tester);
      expect(
        app.read(audioSettingsProvider).noiseSuppression,
        NoiseSuppression.off,
      );

      // Toggle switch on -> goes back to High
      await tester.tap(find.byType(OcSwitch));
      await _settle(tester);
      expect(
        app.read(audioSettingsProvider).noiseSuppression,
        NoiseSuppression.high,
      );

      // Close popover
      await tester.tapAt(const Offset(10, 10));
      await _settle(tester);

      // Fresh opening from Off
      app
          .read(audioSettingsProvider.notifier)
          .update((a) => a.copyWith(noiseSuppression: NoiseSuppression.off));
      await _settle(tester);

      await tester.tap(find.byType(NoiseChip));
      await _settle(tester);

      // Toggle on from fresh opening -> Standard
      await tester.tap(find.byType(OcSwitch));
      await _settle(tester);
      expect(
        app.read(audioSettingsProvider).noiseSuppression,
        NoiseSuppression.standard,
      );

      await app.dispose(tester);
    });

    testWidgets('tapping_a_mode_while_off_turns_it_on', (tester) async {
      final app = await MockApp.pump(tester, size: const Size(1280, 800));
      await _settle(tester);

      final channelId = _voiceChannel(app);
      await app.read(voiceSessionProvider.notifier).join(_dev, channelId);
      app
          .read(audioSettingsProvider.notifier)
          .update((a) => a.copyWith(noiseSuppression: NoiseSuppression.off));
      await _settle(tester);

      await tester.tap(find.byType(NoiseChip));
      await _settle(tester);

      final sw = tester.widget<OcSwitch>(find.byType(OcSwitch));
      expect(sw.value, isFalse);

      final highSegment = find.descendant(
        of: find.byType(OcSegmented<NoiseSuppression>),
        matching: find.text('High'),
      );
      await tester.tap(highSegment);
      await _settle(tester);

      final swOn = tester.widget<OcSwitch>(find.byType(OcSwitch));
      expect(swOn.value, isTrue);
      expect(
        app.read(audioSettingsProvider).noiseSuppression,
        NoiseSuppression.high,
      );

      await app.dispose(tester);
    });

    testWidgets('the_line_follows_the_mode', (tester) async {
      final app = await MockApp.pump(tester, size: const Size(1280, 800));
      await _settle(tester);

      final channelId = _voiceChannel(app);
      await app.read(voiceSessionProvider.notifier).join(_dev, channelId);
      app
          .read(audioSettingsProvider.notifier)
          .update(
            (a) => a.copyWith(noiseSuppression: NoiseSuppression.standard),
          );
      await _settle(tester);

      await tester.tap(find.byType(NoiseChip));
      await _settle(tester);

      // Standard
      expect(find.text('Removes steady noise; very light'), findsOneWidget);

      // High
      final highSegment = find.descendant(
        of: find.byType(OcSegmented<NoiseSuppression>),
        matching: find.text('High'),
      );
      await tester.tap(highSegment);
      await _settle(tester);
      expect(find.text('Also keyboards and dogs; heavier'), findsOneWidget);

      // Off
      await tester.tap(find.byType(OcSwitch));
      await _settle(tester);
      expect(find.text('Sends the microphone as it is'), findsOneWidget);

      await app.dispose(tester);
    });

    testWidgets('the_popover_follows_the_fallback', (tester) async {
      final app = await MockApp.pump(tester, size: const Size(1280, 800));
      await _settle(tester);

      final channelId = _voiceChannel(app);
      await app.read(voiceSessionProvider.notifier).join(_dev, channelId);
      app
          .read(audioSettingsProvider.notifier)
          .update((a) => a.copyWith(noiseSuppression: NoiseSuppression.high));
      await _settle(tester);

      await tester.tap(find.byType(NoiseChip));
      await _settle(tester);

      expect(find.text('Also keyboards and dogs; heavier'), findsOneWidget);

      app.repository.debugNoiseSuppressionFellBack();
      await _settle(tester);

      expect(find.text('Removes steady noise; very light'), findsOneWidget);
      expect(
        app.read(audioSettingsProvider).noiseSuppression,
        NoiseSuppression.standard,
      );

      await app.dispose(tester);
    });

    testWidgets('test_button_starts_and_stops', (tester) async {
      final app = await MockApp.pump(tester, size: const Size(1280, 800));
      await _settle(tester);

      final channelId = _voiceChannel(app);
      await app.read(voiceSessionProvider.notifier).join(_dev, channelId);
      app
          .read(audioSettingsProvider.notifier)
          .update(
            (a) => a.copyWith(noiseSuppression: NoiseSuppression.standard),
          );
      await _settle(tester);

      await tester.tap(find.byType(NoiseChip));
      await _settle(tester);

      final btnFinder = find.descendant(
        of: find.byType(MicTestPanel),
        matching: find.byType(OcButton),
      );

      // Round 1
      var btn = tester.widget<OcButton>(btnFinder);
      expect(btn.label, 'Test');
      expect(btn.variant, OcButtonVariant.secondary);
      expect(find.byType(InputLevelMeter), findsNothing);

      await tester.tap(btnFinder);
      await _settle(tester);

      btn = tester.widget<OcButton>(btnFinder);
      expect(btn.label, 'Stop test');
      expect(btn.variant, OcButtonVariant.primary);
      expect(find.byType(InputLevelMeter), findsOneWidget);
      expect(app.read(micTestProvider), isTrue);

      await tester.tap(btnFinder);
      await _settle(tester);

      btn = tester.widget<OcButton>(btnFinder);
      expect(btn.label, 'Test');
      expect(btn.variant, OcButtonVariant.secondary);
      expect(find.byType(InputLevelMeter), findsNothing);
      expect(app.read(micTestProvider), isFalse);

      // Round 2 (twice)
      await tester.tap(btnFinder);
      await _settle(tester);

      btn = tester.widget<OcButton>(btnFinder);
      expect(btn.label, 'Stop test');
      expect(btn.variant, OcButtonVariant.primary);
      expect(find.byType(InputLevelMeter), findsOneWidget);

      await tester.tap(btnFinder);
      await _settle(tester);

      btn = tester.widget<OcButton>(btnFinder);
      expect(btn.label, 'Test');
      expect(btn.variant, OcButtonVariant.secondary);
      expect(find.byType(InputLevelMeter), findsNothing);

      await app.dispose(tester);
    });

    testWidgets('closing_the_popover_ends_the_test', (tester) async {
      final app = await MockApp.pump(tester, size: const Size(1280, 800));
      await _settle(tester);

      final channelId = _voiceChannel(app);
      await app.read(voiceSessionProvider.notifier).join(_dev, channelId);
      app
          .read(audioSettingsProvider.notifier)
          .update(
            (a) => a.copyWith(noiseSuppression: NoiseSuppression.standard),
          );
      await _settle(tester);

      expect(app.read(voiceSessionProvider).deafened, isFalse);

      // 1. Outside click
      await tester.tap(find.byType(NoiseChip));
      await _settle(tester);

      await tester.tap(find.text('Test'));
      await _settle(tester);

      expect(app.read(micTestProvider), isTrue);
      expect(app.read(voiceSessionProvider).deafened, isTrue);

      // Click outside
      await tester.tapAt(const Offset(10, 10));
      await _settle(tester);

      expect(find.byType(NoiseSuppressionPopover), findsNothing);
      expect(app.read(micTestProvider), isFalse);
      expect(app.read(voiceSessionProvider).deafened, isFalse);

      // 2. Esc key
      await tester.tap(find.byType(NoiseChip));
      await _settle(tester);

      await tester.tap(find.text('Test'));
      await _settle(tester);

      expect(app.read(micTestProvider), isTrue);
      expect(app.read(voiceSessionProvider).deafened, isTrue);

      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await _settle(tester);

      expect(find.byType(NoiseSuppressionPopover), findsNothing);
      expect(app.read(micTestProvider), isFalse);
      expect(app.read(voiceSessionProvider).deafened, isFalse);

      await app.dispose(tester);
    });

    testWidgets('a_test_that_cant_start_says_why', (tester) async {
      final app = await MockApp.pump(tester, size: const Size(1280, 800));
      await _settle(tester);

      final channelId = _voiceChannel(app);
      await app.read(voiceSessionProvider.notifier).join(_dev, channelId);
      app
          .read(audioSettingsProvider.notifier)
          .update(
            (a) => a.copyWith(noiseSuppression: NoiseSuppression.standard),
          );
      await _settle(tester);

      app.repository.micTestError = const RepoException(
        RepoErrorKind.other,
        'Microphone busy',
      );

      await tester.tap(find.byType(NoiseChip));
      await _settle(tester);

      await tester.tap(find.text('Test'));
      await _settle(tester);

      expect(find.text('Microphone busy'), findsOneWidget);
      final btn = tester.widget<OcButton>(
        find.descendant(
          of: find.byType(MicTestPanel),
          matching: find.byType(OcButton),
        ),
      );
      expect(btn.label, 'Test');
      expect(app.read(micTestProvider), isFalse);

      await app.dispose(tester);
    });

    testWidgets('the_card_fits_its_narrowest', (tester) async {
      // Minimum column width 232, largest text scale
      final app = await MockApp.pump(
        tester,
        size: const Size(1280, 800),
        textScale: 1.5,
      );
      await _settle(tester);

      final channelId = _voiceChannel(app);
      await app.read(voiceSessionProvider.notifier).join(_dev, channelId);
      app
          .read(audioSettingsProvider.notifier)
          .update(
            (a) => a.copyWith(noiseSuppression: NoiseSuppression.standard),
          );
      await _settle(tester);

      // Drag splitter to left limit (232)
      final splitterCenter = tester.getCenter(find.byKey(ShellKeys.splitter));
      final drag = await tester.startGesture(splitterCenter);
      await drag.moveBy(const Offset(-100, 0));
      await drag.up();
      await _settle(tester);

      final vcardRect = tester.getRect(find.byKey(ShellKeys.vcard));
      expect(vcardRect.width, closeTo(232, 0.5));

      expect(tester.takeException(), isNull);
      expect(find.byType(NoiseChip), findsOneWidget);
      expect(find.byType(VoiceConnectedPanel), findsOneWidget);

      await app.dispose(tester);
    });

    testWidgets('the_first_row_fits_at_288_and_232', (tester) async {
      final app = await MockApp.pump(tester, size: const Size(1280, 800));
      await _settle(tester);

      final channelId = _voiceChannel(app);
      await app.read(voiceSessionProvider.notifier).join(_dev, channelId);
      await _settle(tester);

      final vcardFinder = find.byKey(ShellKeys.vcard);
      final status = find.descendant(
        of: find.byType(VoiceConnectedPanel),
        matching: find.text('Voice connected'),
      );
      final disconnect = find.descendant(
        of: find.byType(VoiceConnectedPanel),
        matching: find.byWidgetPredicate(
          (w) => w is OcIconButton && w.tooltip == 'Disconnect',
        ),
      );

      void expectRowFits(double width, {required bool ellipsized}) {
        final card = tester.getRect(vcardFinder);
        expect(card.width, closeTo(width, 0.5));
        final chip = tester.getRect(find.byType(NoiseChip));
        final end = tester.getRect(disconnect);
        expect(end.left - chip.right, closeTo(6, 0.5));
        expect(end.right, lessThanOrEqualTo(card.right));
        expect(tester.getRect(status).right, lessThanOrEqualTo(chip.left - 6));
        final paragraph = tester.renderObject<RenderParagraph>(status);
        expect(paragraph.didExceedMaxLines, ellipsized);
      }

      expectRowFits(288, ellipsized: false);

      final splitterCenter = tester.getCenter(find.byKey(ShellKeys.splitter));
      final drag = await tester.startGesture(splitterCenter);
      await drag.moveBy(const Offset(-100, 0));
      await drag.up();
      await _settle(tester);

      // With the compact 24x24 icon button, the status text fits without being ellipsized even at 232.
      expectRowFits(232, ellipsized: false);
      expect(tester.takeException(), isNull);

      await app.dispose(tester);
    });
  });
}
