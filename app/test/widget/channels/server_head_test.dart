import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/events.dart';
import 'package:opencord/features/channels/channel_row.dart';
import 'package:opencord/features/channels/server_head.dart';
import 'package:opencord/features/channels/voice_channel_row.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/theme/oc_theme.dart';
import 'package:opencord/ui/widgets/badges.dart';
import 'package:opencord/ui/widgets/channel_glyph.dart';
import 'package:opencord/ui/widgets/key_hint.dart';
import 'package:opencord/ui/widgets/oc_float.dart';

import '../../support/app.dart';

void main() {
  group('Server head and channel list (§5.3, §5.4, Task 12)', () {
    testWidgets('head_is_its_own_float', (tester) async {
      final app = await MockApp.pump(tester, size: const Size(1280, 800));
      await tester.pumpAndSettle();

      final headFinder = find.byKey(ShellKeys.head);
      expect(headFinder, findsOneWidget);

      final rect = tester.getRect(headFinder);
      expect(rect.left, 78);
      expect(rect.top, 40);
      expect(rect.width, 288);
      expect(rect.height, 52);

      final colors = tester.element(headFinder).oc;

      // When not hovered, the float's color is sidebar
      final floatWidgetBefore = tester.widget<OcFloat>(headFinder);
      expect(floatWidgetBefore.color, colors.sidebar);

      // Hover over the float
      final gesture = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await gesture.addPointer(location: Offset.zero);
      await gesture.moveTo(tester.getCenter(headFinder));
      await tester.pump(const Duration(milliseconds: 120));

      // Hover color across the whole float
      final floatWidgetAfter = tester.widget<OcFloat>(headFinder);
      expect(floatWidgetAfter.color, colors.hover);

      await gesture.removePointer();
      await app.dispose(tester);
    });

    testWidgets('head_menu_is_as_wide_as_the_float', (tester) async {
      final app = await MockApp.pump(tester, size: const Size(1280, 800));
      await tester.pumpAndSettle();

      final headFinder = find.byKey(ShellKeys.head);

      // Tap 1: open menu
      await tester.tap(headFinder);
      await tester.pump(const Duration(milliseconds: 140));

      final menuFinder = find.byKey(ShellKeys.srvMenu);
      expect(menuFinder, findsOneWidget);

      final menuRect = tester.getRect(menuFinder);
      expect(menuRect.left, 78);
      expect(menuRect.top, 98); // 40 + 52 + 6
      expect(menuRect.width, 288);

      // Arrow turns 180° (0.5 turns) over 140 ms
      final rotation1 = tester.widget<AnimatedRotation>(
        find.descendant(
          of: headFinder,
          matching: find.byType(AnimatedRotation),
        ),
      );
      expect(rotation1.turns, 0.5);

      // Close menu
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pumpAndSettle();

      final rotationClosed1 = tester.widget<AnimatedRotation>(
        find.descendant(
          of: headFinder,
          matching: find.byType(AnimatedRotation),
        ),
      );
      expect(rotationClosed1.turns, 0.0);

      // Tap 2 (twice): open again
      await tester.tap(headFinder);
      await tester.pump(const Duration(milliseconds: 140));

      expect(find.byKey(ShellKeys.srvMenu), findsOneWidget);
      final rotation2 = tester.widget<AnimatedRotation>(
        find.descendant(
          of: headFinder,
          matching: find.byType(AnimatedRotation),
        ),
      );
      expect(rotation2.turns, 0.5);

      // Close menu again
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pumpAndSettle();

      final rotationClosed2 = tester.widget<AnimatedRotation>(
        find.descendant(
          of: headFinder,
          matching: find.byType(AnimatedRotation),
        ),
      );
      expect(rotationClosed2.turns, 0.0);

      await app.dispose(tester);
    });

    testWidgets('head_menu_is_as_wide_as_the_float_at_240', (tester) async {
      // Test when float is 240 wide
      final app = await MockApp.pump(tester, size: const Size(1280, 800));
      await tester.pumpAndSettle();

      // Pump ServerHead in a 240-wide container with OcTheme
      await tester.pumpWidget(
        UncontrolledProviderScope(
          container: app.container,
          child: MaterialApp(
            debugShowCheckedModeBanner: false,
            theme: buildTheme(OcColors.dark),
            home: const Scaffold(
              body: Align(
                alignment: Alignment.topLeft,
                child: SizedBox(width: 240, height: 52, child: ServerHead()),
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      final headFinder = find.byKey(ShellKeys.head);
      await tester.tap(headFinder);
      await tester.pump(const Duration(milliseconds: 140));

      final menuFinder = find.byKey(ShellKeys.srvMenu);
      expect(menuFinder, findsOneWidget);

      final menuRect = tester.getRect(menuFinder);
      expect(menuRect.width, 240);

      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pumpAndSettle();
      await app.dispose(tester);
    });

    testWidgets('head_says_when_it_is_not_connected', (tester) async {
      final app = await MockApp.pump(tester);
      await tester.pumpAndSettle();

      final server = app.read(currentServerProvider)!;

      // Connecting
      app.container
          .read(serverProvider(server).notifier)
          .apply(
            ConnectionChanged(server, const ConnectionStatus.connecting()),
          );
      await tester.pump();
      expect(find.text('Connecting…'), findsOneWidget);

      // Reconnecting
      app.container
          .read(serverProvider(server).notifier)
          .apply(
            ConnectionChanged(
              server,
              ConnectionStatus.reconnecting(
                attempt: 1,
                retryAt: DateTime.now().add(const Duration(seconds: 5)),
              ),
            ),
          );
      await tester.pump();
      expect(find.text('Reconnecting…'), findsOneWidget);

      // Connection failed
      app.container
          .read(serverProvider(server).notifier)
          .apply(
            ConnectionChanged(
              server,
              const ConnectionStatus.failed(
                FailureReason.rejected,
                'Connection failed',
              ),
            ),
          );
      await tester.pump();
      expect(find.text('Connection failed'), findsOneWidget);

      await app.dispose(tester);
    });

    testWidgets('search_pill_numbers', (tester) async {
      final app = await MockApp.pump(tester);
      await tester.pumpAndSettle();

      final searchFinder = find.byKey(ShellKeys.search);
      expect(searchFinder, findsOneWidget);

      final container = tester.widget<AnimatedContainer>(searchFinder);
      expect(container.constraints?.minHeight, 36);
      expect(container.padding, const EdgeInsets.symmetric(horizontal: 12));
      final decoration = container.decoration as BoxDecoration;
      expect(decoration.borderRadius, BorderRadius.circular(18));

      // KeyHint numbers: JetBrains Mono 11, padding 1 5, 1 px border, radius 5, textMuted
      final keyHintFinder = find.byType(KeyHint);
      expect(keyHintFinder, findsOneWidget);

      final keyHintContainer = tester.widget<Container>(
        find
            .descendant(of: keyHintFinder, matching: find.byType(Container))
            .first,
      );
      expect(
        keyHintContainer.padding,
        const EdgeInsets.symmetric(horizontal: 5, vertical: 1),
      );
      final keyHintDeco = keyHintContainer.decoration as BoxDecoration;
      expect(keyHintDeco.borderRadius, BorderRadius.circular(5));
      expect(keyHintDeco.border?.top.width, 1);

      final keyHintText = tester.widget<Text>(
        find.descendant(of: keyHintFinder, matching: find.byType(Text)),
      );
      expect(keyHintText.style?.fontSize, 11);
      expect(keyHintText.style?.fontFamily, OcText.monoFamily);

      await app.dispose(tester);
    });

    testWidgets('category_plus_only_on_hover', (tester) async {
      final app = await MockApp.pump(tester);
      await tester.pumpAndSettle();

      final catTextFinder = find.text('TEXT CHANNELS');
      expect(catTextFinder, findsOneWidget);

      // Category toggle label numbers: 11.5/700, spacing 0.7, uppercase, textMuted
      final catText = tester.widget<Text>(catTextFinder);
      expect(catText.style?.fontSize, 11.5);
      expect(catText.style?.fontWeight, FontWeight.w700);
      expect(catText.style?.letterSpacing, 0.7);

      // '+' button is only visible on hover or focus
      final catRowFinder = find.ancestor(
        of: catTextFinder,
        matching: find.byWidgetPredicate(
          (w) => w.runtimeType.toString() == '_CategoryRow',
        ),
      );
      expect(catRowFinder, findsOneWidget);

      final addOpacityFinder = find.descendant(
        of: catRowFinder,
        matching: find.byType(AnimatedOpacity),
      );
      expect(addOpacityFinder, findsOneWidget);

      final opacityBefore = tester.widget<AnimatedOpacity>(addOpacityFinder);
      expect(opacityBefore.opacity, 0.0);

      // Hover over category row
      final gesture = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await gesture.addPointer(location: Offset.zero);
      await gesture.moveTo(tester.getCenter(catTextFinder));
      await tester.pump(const Duration(milliseconds: 120));

      final opacityAfter = tester.widget<AnimatedOpacity>(addOpacityFinder);
      expect(opacityAfter.opacity, 1.0);

      // Tap to collapse: chevron turns -90° (turn = -0.25) over 150 ms
      await tester.tap(catTextFinder);
      await tester.pump(const Duration(milliseconds: 150));

      final rotation = tester.widget<AnimatedRotation>(
        find.descendant(
          of: catRowFinder,
          matching: find.byType(AnimatedRotation),
        ),
      );
      expect(rotation.turns, -0.25);

      await gesture.removePointer();
      await app.dispose(tester);
    });

    testWidgets('row_numbers', (tester) async {
      final app = await MockApp.pump(tester);
      await tester.pumpAndSettle();

      // Channel row: height 56, margin 0 8 2, padding 0 8, radius 10
      final generalRowFinder = find.byWidgetPredicate(
        (w) => w is ChannelRowView && w.name == 'general',
      );
      expect(generalRowFinder, findsOneWidget);

      final rowContainer = tester.widget<AnimatedContainer>(
        find
            .descendant(
              of: generalRowFinder,
              matching: find.byType(AnimatedContainer),
            )
            .first,
      );
      expect(rowContainer.constraints?.minHeight, 56);
      expect(rowContainer.margin, const EdgeInsets.fromLTRB(8, 0, 8, 2));
      expect(rowContainer.padding, const EdgeInsets.symmetric(horizontal: 8));
      final rowDeco = rowContainer.decoration as BoxDecoration;
      expect(rowDeco.borderRadius, BorderRadius.circular(10));

      // Glyph 40, radius 12, lock chip 16 circle at right/bottom -3
      final lockedRowFinder = find.byWidgetPredicate(
        (w) => w is ChannelRowView && w.name == 'welcome',
      );
      final glyphFinder = find.descendant(
        of: lockedRowFinder,
        matching: find.byType(ChannelGlyph),
      );
      expect(glyphFinder, findsOneWidget);
      final glyph = tester.widget<ChannelGlyph>(glyphFinder);
      expect(glyph.size, 40);

      // Lock chip
      final lockContainer = tester.widget<Container>(
        find.descendant(
          of: glyphFinder,
          matching: find.byWidgetPredicate(
            (w) => w is Container && w.constraints?.minWidth == 16,
          ),
        ),
      );
      expect(lockContainer.constraints?.minHeight, 16);

      // Badges: height and min width 18, padding 0 5, radius 9, 11/600
      final unreadBadgeFinder = find.byType(UnreadBadge);
      expect(unreadBadgeFinder, findsWidgets);
      final unreadContainer = tester.widget<Container>(
        find.descendant(
          of: unreadBadgeFinder.first,
          matching: find.byType(Container),
        ),
      );
      expect(unreadContainer.constraints?.minWidth, 18);
      expect(unreadContainer.constraints?.minHeight, 18);
      expect(
        unreadContainer.padding,
        const EdgeInsets.symmetric(horizontal: 5),
      );
      final unreadDeco = unreadContainer.decoration as BoxDecoration;
      expect(unreadDeco.borderRadius, BorderRadius.circular(9));

      final unreadText = tester.widget<Text>(
        find.descendant(
          of: unreadBadgeFinder.first,
          matching: find.byType(Text),
        ),
      );
      expect(unreadText.style?.fontSize, 11);
      expect(unreadText.style?.fontWeight, FontWeight.w600);

      // Voice row: margin 0 8 2, height 36, padding 0 10, radius 10, icon 18
      final voiceFinder = find.byType(VoiceChannelRow).first;
      expect(voiceFinder, findsOneWidget);

      final voiceHeaderFinder = find
          .descendant(of: voiceFinder, matching: find.byType(AnimatedContainer))
          .first;
      final voiceContainer = tester.widget<AnimatedContainer>(
        voiceHeaderFinder,
      );
      expect(voiceContainer.constraints?.minHeight, 36);
      expect(voiceContainer.margin, const EdgeInsets.fromLTRB(8, 0, 8, 2));
      expect(
        voiceContainer.padding,
        const EdgeInsets.symmetric(horizontal: 10),
      );
      final voiceDeco = voiceContainer.decoration as BoxDecoration;
      expect(voiceDeco.borderRadius, BorderRadius.circular(10));

      final voiceIcon = tester.widget<Icon>(
        find.descendant(
          of: voiceHeaderFinder,
          matching: find.byIcon(OcIcons.volumeUp),
        ),
      );
      expect(voiceIcon.size, 18);

      await app.dispose(tester);
    });

    testWidgets('channel_rows_are_58_apart', (tester) async {
      final app = await MockApp.pump(tester, size: const Size(1280, 800));
      await tester.pumpAndSettle();

      final announcements = tester.getRect(
        find.widgetWithText(ChannelRowView, 'announcements'),
      );
      final welcome = tester.getRect(
        find.widgetWithText(ChannelRowView, 'welcome'),
      );
      expect(welcome.top - announcements.top, 58.0);

      await app.dispose(tester);
    });

    testWidgets('person_in_voice_numbers', (tester) async {
      final app = await MockApp.pump(tester);
      await tester.pumpAndSettle();

      // Person in voice: height 30, margin 0 16 0 42, 13 textSecondary
      final personTextFinder = find.descendant(
        of: find.byType(VoiceChannelRow),
        matching: find.text('Tomás Ruiz'),
      );
      expect(personTextFinder, findsOneWidget);

      final personText = tester.widget<Text>(personTextFinder);
      expect(personText.style?.fontSize, 13);

      final personRowFinder = find.ancestor(
        of: find.byType(LivePill).first,
        matching: find.byWidgetPredicate(
          (w) => w is Container && w.constraints?.minHeight == 30,
        ),
      );
      expect(personRowFinder, findsOneWidget);

      final personContainer = tester.widget<Container>(personRowFinder);
      expect(personContainer.margin, const EdgeInsets.fromLTRB(42, 0, 16, 0));

      // LIVE badge: 10.5/700, spacing 0.4, padding 1 6, radius 5
      final livePillFinder = find.byType(LivePill);
      expect(livePillFinder, findsOneWidget);

      final liveContainer = tester.widget<Container>(
        find.descendant(of: livePillFinder, matching: find.byType(Container)),
      );
      expect(
        liveContainer.padding,
        const EdgeInsets.symmetric(horizontal: 6, vertical: 1),
      );
      final liveDeco = liveContainer.decoration as BoxDecoration;
      expect(liveDeco.borderRadius, BorderRadius.circular(5));

      final liveText = tester.widget<Text>(
        find.descendant(of: livePillFinder, matching: find.byType(Text)),
      );
      expect(liveText.style?.fontSize, 10.5);
      expect(liveText.style?.fontWeight, FontWeight.w700);
      expect(liveText.style?.letterSpacing, 0.4);

      await app.dispose(tester);
    });
  });
}
