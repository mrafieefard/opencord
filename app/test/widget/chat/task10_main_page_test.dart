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
import 'package:opencord/features/home/private_message_list.dart';
import 'package:opencord/features/members/member_avatar.dart';
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
    // 16 in from main, inside its 1 px border, with no hamburger before it.
    expect(
      tester.getTopLeft(titleFinder).dx,
      moreOrLessEquals(
        tester.getTopLeft(find.byKey(ShellKeys.main)).dx + 1 + 16,
        epsilon: 0.5,
      ),
    );

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
    final bubbles = tester.widgetList<MessageBubble>(
      find.byType(MessageBubble),
    );
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
    final app = await MockApp.pump(tester, size: const Size(1440, 900));
    await tester.tap(_inSidebar('general'));
    await _settle(tester);

    // Close the right pane to measure at 1440 with pane shut
    app.read(memberPanelProvider.notifier).close();
    await _settle(tester);

    final msgListFinder = find.byType(MessageList);
    expect(msgListFinder, findsOneWidget);
    final listWidth = tester.getSize(msgListFinder).width;

    // Side padding is 16 + max(0, (w - 880) / 2)
    final expectedPadding = 16.0 + (listWidth - 880.0) / 2.0;
    final sliverPadding = tester.widget<SliverPadding>(
      find
          .descendant(of: msgListFinder, matching: find.byType(SliverPadding))
          .first,
    );
    expect(
      (sliverPadding.padding as EdgeInsets).left,
      closeTo(expectedPadding, 0.5),
    );
    expect(
      (sliverPadding.padding as EdgeInsets).right,
      closeTo(expectedPadding, 0.5),
    );

    // Avatar left edge is at msgsIn.x (463) + 16 = 479
    final avatarFinder = find.descendant(
      of: msgListFinder,
      matching: find.byType(MemberAvatar),
    );
    expect(avatarFinder, findsWidgets);
    final avatarLeft = tester.getRect(avatarFinder.first).left;
    expect(avatarLeft, closeTo(479.0, 1.5));

    await app.dispose(tester);
  });

  testWidgets('short_chats_sit_at_the_bottom', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1280, 800));

    // 1. Server channel with one message (welcome)
    final servers = app.read(serverListProvider);
    final server = servers.first.key;
    final channel = app
        .read(serverProvider(server))
        .data!
        .channels
        .values
        .firstWhere((c) => c.name == 'welcome')
        .id;
    app.read(navigationProvider.notifier).openChannel(server, channel);
    await _settle(tester);

    final serverListFinder = find.byType(MessageList);
    expect(serverListFinder, findsOneWidget);
    final serverListRect = tester.getRect(serverListFinder);

    final serverBubbles = tester
        .widgetList<MessageBubble>(find.byType(MessageBubble))
        .toList();
    expect(serverBubbles.length, 1);
    final serverBubbleRect = tester.getRect(
      find.byWidget(serverBubbles.single),
    );
    // The message sits at the bottom of the list (12 px bottom inset)
    expect(serverListRect.bottom - serverBubbleRect.bottom, closeTo(12.0, 1.0));

    // 2. Private chat with one message (Priya Shah)
    final contacts = app.read(privateContactsProvider);
    final priya = contacts.all.firstWhere((c) => c.name == 'Priya Shah');
    app.read(navigationProvider.notifier).openConversation(priya.peer);
    await _settle(tester);

    final dmListFinder = find.byType(PrivateMessageList);
    expect(dmListFinder, findsOneWidget);
    final dmListRect = tester.getRect(dmListFinder);

    final dmBubbles = tester
        .widgetList<MessageBubble>(
          find.descendant(
            of: dmListFinder,
            matching: find.byType(MessageBubble),
          ),
        )
        .toList();
    expect(dmBubbles.length, 1);
    final dmBubbleRect = tester.getRect(find.byWidget(dmBubbles.single));
    expect(dmListRect.bottom - dmBubbleRect.bottom, closeTo(12.0, 1.0));

    await app.dispose(tester);
  });

  testWidgets('bubble_numbers', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1280, 800));
    await tester.tap(_inSidebar('general'));
    await _settle(tester);

    final colors = tester.element(find.byType(DesktopShell)).oc;
    final bubbles = tester
        .widgetList<MessageBubble>(find.byType(MessageBubble))
        .toList();
    expect(bubbles, isNotEmpty);

    // Verify incoming bubble: padding 7 12 8 (12, 7, 12, 8), border 1px, corners
    final incoming = bubbles.firstWhere((b) => !b.own && !b.mentionsMe);
    final incomingBox = tester.widget<AnimatedContainer>(
      find
          .descendant(
            of: find.byWidget(incoming),
            matching: find.byType(AnimatedContainer),
          )
          .first,
    );
    final incomingDeco = incomingBox.decoration as BoxDecoration;
    expect(incomingDeco.color, colors.bubbleIn);
    expect(incomingDeco.border?.top.width, 1.0);
    expect(incomingDeco.border?.top.color, colors.border);
    final incomingRadius = incomingDeco.borderRadius as BorderRadius;
    expect(incomingRadius.topRight.x, 16.0);
    expect(incomingRadius.bottomRight.x, 16.0);

    final incomingPadding = tester.widget<Padding>(
      find
          .descendant(
            of: find.byWidget(incoming),
            matching: find.byWidgetPredicate(
              (w) =>
                  w is Padding &&
                  w.padding == const EdgeInsets.fromLTRB(12, 7, 12, 8),
            ),
          )
          .first,
    );
    expect(incomingPadding.padding, const EdgeInsets.fromLTRB(12, 7, 12, 8));

    // Verify own bubble: bubbleOut color, 1px border
    final ownBubble = bubbles.firstWhere((b) => b.own);
    final ownBox = tester.widget<AnimatedContainer>(
      find
          .descendant(
            of: find.byWidget(ownBubble),
            matching: find.byType(AnimatedContainer),
          )
          .first,
    );
    final ownDeco = ownBox.decoration as BoxDecoration;
    expect(ownDeco.color, colors.bubbleOut);
    expect(ownDeco.border?.top.width, 1.0);
    final ownRadius = ownDeco.borderRadius as BorderRadius;
    expect(ownRadius.topLeft.x, 16.0);
    expect(ownRadius.bottomLeft.x, 16.0);

    // Verify mention bubble: CustomPaint with _MentionBorderPainter, left padding 13
    final mentionBubble = bubbles.firstWhere((b) => b.mentionsMe);
    final mentionCustomPaint = find.descendant(
      of: find.byWidget(mentionBubble),
      matching: find.byWidgetPredicate(
        (w) => w is CustomPaint && w.foregroundPainter is CustomPainter,
      ),
    );
    expect(mentionCustomPaint, findsOneWidget);

    final mentionPadding = tester.widget<Padding>(
      find
          .descendant(
            of: find.byWidget(mentionBubble),
            matching: find.byWidgetPredicate(
              (w) =>
                  w is Padding &&
                  w.padding == const EdgeInsets.fromLTRB(13, 7, 12, 8),
            ),
          )
          .first,
    );
    expect(mentionPadding.padding, const EdgeInsets.fromLTRB(13, 7, 12, 8));

    await app.dispose(tester);
  });

  testWidgets('reply_quote_reaction_unread_system_numbers', (tester) async {
    final app = await MockApp.pump(tester, size: const Size(1280, 800));
    await tester.tap(_inSidebar('general'));
    await _settle(tester);

    final colors = tester.element(find.byType(DesktopShell)).oc;

    // 1. Unread line: margin 6 vertical, gap 12 above/below
    final unreadLine = find.byType(UnreadLine);
    expect(unreadLine, findsOneWidget);
    final unreadPadding = tester.widget<Padding>(
      find.ancestor(of: unreadLine, matching: find.byType(Padding)).first,
    );
    expect(unreadPadding.padding, const EdgeInsets.only(top: 12, bottom: 6));

    // Scroll up to reveal earlier messages (quote, system pill)
    final scrollable = find.descendant(
      of: find.byType(MessageList),
      matching: find.byType(Scrollable),
    );
    await tester.drag(scrollable, const Offset(0, 300));
    await _settle(tester);

    // 2. System pill: gap 6 (top 6 padding on row), icon 16
    final sysPill = find.byType(CenterPill);
    expect(sysPill, findsWidgets);
    final sysRowPadding = tester.widget<Padding>(
      find.ancestor(of: sysPill.first, matching: find.byType(Padding)).first,
    );
    expect(sysRowPadding.padding, const EdgeInsets.only(top: 6));

    // 3. Reaction chip: height 24, radius 12, horizontal padding 8, emoji to count gap 5
    final reactionChips = tester
        .widgetList<ReactionChip>(find.byType(ReactionChip))
        .toList();
    expect(reactionChips, isNotEmpty);
    final chipRect = tester.getRect(find.byWidget(reactionChips.first));
    expect(chipRect.height, 24.0);

    final chipContainer = tester.widget<AnimatedContainer>(
      find
          .descendant(
            of: find.byWidget(reactionChips.first),
            matching: find.byType(AnimatedContainer),
          )
          .first,
    );
    final chipDeco = chipContainer.decoration as BoxDecoration;
    expect(chipDeco.borderRadius, BorderRadius.circular(12));
    expect(chipContainer.padding, const EdgeInsets.symmetric(horizontal: 8));

    final chipGap = tester.widget<SizedBox>(
      find
          .descendant(
            of: find.byWidget(reactionChips.first),
            matching: find.byType(SizedBox),
          )
          .first,
    );
    expect(chipGap.width, 5.0);

    // 4. Quote: 2px border in textMuted, padding-left 8, margin 1 0 2
    final quoteContainers = find.byWidgetPredicate(
      (w) =>
          w is Container &&
          w.decoration is BoxDecoration &&
          ((w.decoration as BoxDecoration).border is Border &&
              ((w.decoration as BoxDecoration).border as Border).left.width ==
                  2.0 &&
              ((w.decoration as BoxDecoration).border as Border).top.width ==
                  0.0),
    );
    expect(quoteContainers, findsAtLeastNWidgets(1));
    final quoteBox = tester.widget<Container>(quoteContainers.first);
    final quoteDeco = quoteBox.decoration as BoxDecoration;
    final quoteBorder = quoteDeco.border as Border;
    expect(quoteBorder.left.width, 2.0);
    expect(quoteBorder.left.color, colors.textMuted);
    expect(quoteBox.padding, const EdgeInsets.only(left: 8));
    expect(quoteBox.margin, const EdgeInsets.fromLTRB(0, 1, 0, 2));

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

  testWidgets('composer_grows_upward_and_the_panel_stays_level', (
    tester,
  ) async {
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
