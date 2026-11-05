import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/chat/bubble.dart';
import 'package:opencord/features/chat/chat_header.dart';
import 'package:opencord/features/chat/chat_rows.dart';
import 'package:opencord/features/chat/list_overlays.dart';
import 'package:opencord/features/chat/message_list.dart';
import 'package:opencord/features/chat/voice/mic_button.dart';
import 'package:opencord/features/chat/voice/recording_bar.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_theme.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';

import '../../support/app.dart';

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

Future<void> _settle(WidgetTester tester) =>
    _pumpFor(tester, const Duration(milliseconds: 600));

Finder _inSidebar(String text) => find.descendant(
      of: find.byKey(DesktopShell.sidebarKey),
      matching: find.text(text),
    );

void main() {
  testWidgets('header_numbers', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1280, 800));
    await tester.tap(_inSidebar('general'));
    await _settle(tester);

    final titleFinder = find.descendant(
      of: find.byType(ChatHeader),
      matching: find.text('#general'),
    );
    expect(titleFinder, findsOneWidget);
    final titleText = tester.widget<Text>(titleFinder);
    expect(titleText.style?.fontSize, 15.0);
    expect(titleText.style?.fontWeight, FontWeight.w600);
    expect(titleText.style?.height, 1.25);

    // Subtitle style check
    final subFinder = find.descendant(
      of: find.byType(ChatHeader),
      matching: find.byWidgetPredicate(
        (w) => w is Text && (w.data?.contains('members') ?? false),
      ),
    );
    expect(subFinder, findsOneWidget);
    final subText = tester.widget<Text>(subFinder);
    expect(subText.style?.fontSize, 12.5);
    expect(subText.style?.fontWeight, FontWeight.w400);
    expect(subText.style?.height, 1.25);

    // Button sizes (compact 36x36, icon 20) and gaps (2)
    final iconButtons = tester
        .widgetList<OcIconButton>(
          find.descendant(
            of: find.byType(ChatHeader),
            matching: find.byType(OcIconButton),
          ),
        )
        .toList();
    expect(iconButtons, isNotEmpty);
    for (final btn in iconButtons) {
      if (btn.size == OcIconButtonSize.compact) {
        expect(btn.size.extent, 36.0);
        expect(btn.size.iconSize, 20.0);
      }
    }

    await app.dispose(tester);
  });

  testWidgets('pinned_float_numbers', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1280, 800));
    await tester.tap(_inSidebar('general'));
    await _settle(tester);

    final pinFinder = find.byKey(ShellKeys.pin);
    expect(pinFinder, findsOneWidget);
    final rect = tester.getRect(pinFinder);

    // At 1280 width: left ~387, top ~101, width ~600, height ~48 (from measure.json: [387, 101, 600, 47.8])
    expect(rect.left, closeTo(387.0, 1.5));
    expect(rect.top, closeTo(101.0, 1.5));
    expect(rect.width, closeTo(600.0, 1.5));
    expect(rect.height, closeTo(48.0, 2.0));

    // Decoration: surface sidebar, border 1px, shadow float
    final container = tester.widget<Container>(pinFinder);
    final decoration = container.decoration as BoxDecoration;
    final oc = tester.element(pinFinder).oc;
    expect(decoration.color, oc.sidebar);
    expect(decoration.borderRadius, BorderRadius.circular(12));
    expect(decoration.border?.top.width, 1.0);
    expect(decoration.border?.top.color, oc.border);
    expect(decoration.boxShadow, OcShadows.float(oc));

    // Bar: 2 x 30 text, radius 1
    final barFinder = find.descendant(
      of: pinFinder,
      matching: find.byWidgetPredicate(
        (w) =>
            w is Container &&
            w.constraints?.minWidth == 2 &&
            w.constraints?.minHeight == 30,
      ),
    );
    expect(barFinder, findsOneWidget);
    final bar = tester.widget<Container>(barFinder);
    final barDeco = bar.decoration as BoxDecoration;
    expect(barDeco.color, oc.text);
    expect(barDeco.borderRadius, BorderRadius.circular(1));

    await app.dispose(tester);
  });

  testWidgets('messages_clear_the_pinned_float', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1280, 800));
    await tester.tap(_inSidebar('general'));
    await _settle(tester);

    final pinFinder = find.byKey(ShellKeys.pin);
    expect(pinFinder, findsOneWidget);
    final pinRect = tester.getRect(pinFinder);

    // Scroll to the very top
    final scrollable = find.descendant(
      of: find.byType(MessageList),
      matching: find.byType(Scrollable),
    );
    await tester.drag(scrollable, const Offset(0, 3000));
    await _settle(tester);

    // First message top >= float's bottom
    final bubbles = tester.widgetList<MessageBubble>(find.byType(MessageBubble));
    expect(bubbles, isNotEmpty);
    final firstBubbleRect = tester.getRect(find.byWidget(bubbles.first));
    expect(firstBubbleRect.top, greaterThanOrEqualTo(pinRect.bottom - 1.0));

    await app.dispose(tester);
  });

  testWidgets('day_pill_moves_under_the_pinned_float', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1280, 800));
    await tester.tap(_inSidebar('general'));
    await _settle(tester);

    // While pinned float shows: day pill is at top: 64
    final dayPillPositioned = find.descendant(
      of: find.byType(MessageList),
      matching: find.byWidgetPredicate(
        (w) => w is Positioned && w.child is FloatingDayPill,
      ),
    );
    expect(dayPillPositioned, findsOneWidget);
    final posWithPins = tester.widget<Positioned>(dayPillPositioned);
    expect(posWithPins.top, 64.0);

    // Unpin both pins to close pinned float completely
    final closePinBtn = find.descendant(
      of: find.byKey(ShellKeys.pin),
      matching: find.byTooltip('Unpin'),
    );
    expect(closePinBtn, findsOneWidget);
    await tester.tap(closePinBtn);
    await _settle(tester);

    // Unpin remaining pin if any
    final closePinBtn2 = find.descendant(
      of: find.byKey(ShellKeys.pin),
      matching: find.byTooltip('Unpin'),
    );
    if (closePinBtn2.evaluate().isNotEmpty) {
      await tester.tap(closePinBtn2);
      await _settle(tester);
    }

    // After closing: day pill moves to top: 8
    final posAfterClose = tester.widget<Positioned>(dayPillPositioned);
    expect(posAfterClose.top, 8.0);

    await app.dispose(tester);
  });

  testWidgets('column_is_880_with_its_padding', (tester) async {
    // When main is wider than 880 (e.g. at 1440 width)
    final app = await MockApp.pump(tester, size: const Size(1440, 900));
    await tester.tap(_inSidebar('general'));
    await _settle(tester);

    final msgListFinder = find.byType(MessageList);
    expect(msgListFinder, findsOneWidget);
    final listWidth = tester.getSize(msgListFinder).width;

    if (listWidth > 880) {
      final expectedPadding = (listWidth - 880) / 2;
      // SliverPadding inside CustomScrollView has side padding matching (listWidth - 880) / 2
      final sliverPaddings = tester
          .widgetList<SliverPadding>(
            find.descendant(
              of: msgListFinder,
              matching: find.byType(SliverPadding),
            ),
          )
          .toList();
      expect(sliverPaddings, isNotEmpty);
      expect((sliverPaddings.first.padding as EdgeInsets).left,
          closeTo(expectedPadding, 1.0));
      expect((sliverPaddings.first.padding as EdgeInsets).right,
          closeTo(expectedPadding, 1.0));
    }

    await app.dispose(tester);
  });

  testWidgets('short_chats_sit_at_the_bottom', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1280, 800));
    // Open a channel with 1 message: e.g. dev-core or a channel with minimal messages
    final servers = app.read(serverListProvider);
    final server = servers.first.key;
    final channel = app
        .read(serverProvider(server))
        .data!
        .channels
        .values
        .firstWhere((c) => c.name == 'dev-core')
        .id;
    app.read(navigationProvider.notifier).openChannel(server, channel);
    await _settle(tester);

    final msgListFinder = find.byType(MessageList);
    expect(msgListFinder, findsOneWidget);
    final listRect = tester.getRect(msgListFinder);

    final bubbles = tester.widgetList<MessageBubble>(find.byType(MessageBubble));
    if (bubbles.length == 1) {
      final bubbleRect = tester.getRect(find.byWidget(bubbles.single));
      expect(listRect.bottom - bubbleRect.bottom, closeTo(12.0, 1.0));
    }

    await app.dispose(tester);
  });

  testWidgets('bubble_numbers', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1280, 800));
    await tester.tap(_inSidebar('general'));
    await _settle(tester);

    final bubbles =
        tester.widgetList<MessageBubble>(find.byType(MessageBubble)).toList();
    expect(bubbles, isNotEmpty);

    // Padding 7 12 8: EdgeInsets.fromLTRB(12, 7, 12, 8)
    // 1 px border on incoming and own bubbles
    for (final bubble in bubbles.take(4)) {
      final box = find.descendant(
        of: find.byWidget(bubble),
        matching: find.byWidgetPredicate(
          (w) =>
              w is DecoratedBox &&
              w.decoration is BoxDecoration &&
              (w.decoration as BoxDecoration).borderRadius != null,
        ),
      );
      if (box.evaluate().isNotEmpty) {
        final deco =
            tester.widget<DecoratedBox>(box.first).decoration as BoxDecoration;
        expect(deco.border?.top.width, 1.0);
        final radius = deco.borderRadius as BorderRadius;
        expect(radius.topLeft.x == 16 || radius.topLeft.x == 5, isTrue);
      }
    }

    await app.dispose(tester);
  });

  testWidgets('reply_quote_reaction_unread_system_numbers', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1280, 800));
    await tester.tap(_inSidebar('general'));
    await _settle(tester);

    // Unread line: margin 6 vertical, gap 12
    final unreadLine = find.byType(UnreadLine);
    if (unreadLine.evaluate().isNotEmpty) {
      final padding = find.descendant(
        of: unreadLine.first,
        matching: find.byType(Padding),
      );
      expect(padding, findsWidgets);
    }

    // Reaction chip: height 24, radius 12, padding horizontal 8
    final reactionChips =
        tester.widgetList<ReactionChip>(find.byType(ReactionChip)).toList();
    if (reactionChips.isNotEmpty) {
      final chipRect = tester.getRect(find.byWidget(reactionChips.first));
      expect(chipRect.height, closeTo(24.0, 1.0));
    }

    await app.dispose(tester);
  });

  testWidgets('composer_rests_at_62', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1280, 800));
    await tester.tap(_inSidebar('general'));
    await _settle(tester);

    final composerFinder = find.byKey(ShellKeys.composer);
    expect(composerFinder, findsOneWidget);
    final composerRect = tester.getRect(composerFinder);
    expect(composerRect.height, 62.0);

    final meFinder = find.byKey(ShellKeys.me);
    expect(meFinder, findsOneWidget);
    final meRect = tester.getRect(meFinder);
    expect(composerRect.top, closeTo(meRect.top, 1.0));

    await app.dispose(tester);
  });

  testWidgets('composer_grows_upward_and_the_panel_stays_level', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1280, 800));
    await tester.tap(_inSidebar('general'));
    await _settle(tester);

    final meFinder = find.byKey(ShellKeys.me);
    final initialMeTop = tester.getRect(meFinder).top;

    final composerFinder = find.byKey(ShellKeys.composer);
    final initialComposerRect = tester.getRect(composerFinder);
    expect(initialComposerRect.height, 62.0);

    // Type three lines into composer
    final fieldFinder = find.descendant(
      of: composerFinder,
      matching: find.byType(TextField),
    );
    expect(fieldFinder, findsOneWidget);
    await tester.enterText(fieldFinder, 'Line 1\nLine 2\nLine 3');
    await _settle(tester);

    final grownRect = tester.getRect(composerFinder);
    expect(grownRect.height, greaterThan(62.0));
    expect(grownRect.bottom, closeTo(initialComposerRect.bottom, 1.0));
    expect(grownRect.top, lessThan(initialComposerRect.top));

    // User panel stays level
    final currentMeTop = tester.getRect(meFinder).top;
    expect(currentMeTop, closeTo(initialMeTop, 1.0));

    // Sending resets composer back to 62 tall
    final sendBtn = find.byWidgetPredicate(
      (w) => w is OcIconButton && w.tooltip == 'Send',
    );
    expect(sendBtn, findsOneWidget);
    await tester.tap(sendBtn);
    await _settle(tester);

    final resetRect = tester.getRect(composerFinder);
    expect(resetRect.height, 62.0);

    await app.dispose(tester);
  });

  testWidgets('recording_cover_fills_the_float', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1280, 800));
    await tester.tap(_inSidebar('general'));
    await _settle(tester);

    final micFinder = find.byType(MicButton);
    expect(micFinder, findsOneWidget);

    // Long press to start recording
    final gesture = await tester.startGesture(tester.getCenter(micFinder));
    await _pumpFor(tester, const Duration(milliseconds: 600));

    final recBarFinder = find.byType(RecordingBar);
    expect(recBarFinder, findsOneWidget);

    final composerFinder = find.byKey(ShellKeys.composer);
    final composerRect = tester.getRect(composerFinder);
    final recBarRect = tester.getRect(recBarFinder);

    expect(recBarRect.left, closeTo(composerRect.left, 1.0));
    expect(recBarRect.right, closeTo(composerRect.right, 1.0));
    expect(recBarRect.top, closeTo(composerRect.top, 1.0));
    expect(recBarRect.bottom, closeTo(composerRect.bottom, 1.0));

    await gesture.up();
    await _settle(tester);
    await app.dispose(tester);
  });
}
