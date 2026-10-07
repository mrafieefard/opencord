import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/features/chat/bubble.dart';
import 'package:opencord/features/chat/composer_state.dart';
import 'package:opencord/features/chat/hover_bar.dart';
import 'package:opencord/features/chat/markdown_view.dart';
import 'package:opencord/features/chat/message_item.dart';
import 'package:opencord/features/chat/message_list.dart';
import 'package:opencord/features/chat/reaction_pill.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';

import '../../support/app.dart';

const _server = 'opencord.example:7710';
const _ping = 'could you look at the invite dialog copy';
const _mine = 'On it. The fingerprint note needs to be shorter.';

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

Future<ChannelRef> _openGeneral(WidgetTester tester, MockApp app) async {
  final general = app
      .read(serverProvider(_server))
      .data!
      .channels
      .values
      .firstWhere((channel) => channel.name == 'general')
      .id;
  app.read(navigationProvider.notifier).openChannel(_server, general);
  await _pumpFor(tester, const Duration(milliseconds: 300));
  return (server: _server, channel: general);
}

Finder _bubble(String containing) => find.byWidgetPredicate(
  (widget) =>
      widget is MessageBubble && widget.message.content.contains(containing),
);

/// The middle of a message's text: not a chip, link or quote.
Offset _textOf(WidgetTester tester, String containing) => tester.getCenter(
  find.descendant(of: _bubble(containing), matching: find.byType(MarkdownView)),
);

Future<void> _rightClick(WidgetTester tester, Offset at) async {
  await tester.tapAt(
    at,
    buttons: kSecondaryMouseButton,
    kind: PointerDeviceKind.mouse,
  );
  await _pumpFor(tester, const Duration(milliseconds: 200));
}

List<String> _menuLabels(WidgetTester tester) => [
  for (final panel in tester.widgetList<OcMenuPanel>(find.byType(OcMenuPanel)))
    for (final entry in panel.entries)
      if (entry is OcMenuItem) entry.label,
];

String? _copied;

void _mockClipboard(WidgetTester tester) {
  _copied = null;
  tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
    SystemChannels.platform,
    (call) async {
      if (call.method == 'Clipboard.setData') {
        _copied = (call.arguments as Map)['text'] as String;
      }
      return null;
    },
  );
  addTearDown(
    () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
      SystemChannels.platform,
      null,
    ),
  );
}

void main() {
  testWidgets('the menu of someone else’s message lists what applies', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openGeneral(tester, app);

    await _rightClick(tester, _textOf(tester, _ping));

    expect(_menuLabels(tester), [
      'Reply',
      'Add reaction',
      'Copy text',
      'Pin',
      'Copy message link',
      'Copy message ID',
      'Delete',
    ]);
    await app.dispose(tester);
  });

  testWidgets('your own messages can be edited from the menu', (tester) async {
    final app = await MockApp.pump(tester);
    final channel = await _openGeneral(tester, app);

    await _rightClick(tester, _textOf(tester, _mine));
    expect(_menuLabels(tester), contains('Edit'));
    await tester.tap(find.text('Edit'));
    await _pumpFor(tester, const Duration(milliseconds: 200));

    expect(app.read(composerProvider(channel)).editing?.content, _mine);
    await app.dispose(tester);
  });

  testWidgets('double-click replies', (tester) async {
    final app = await MockApp.pump(tester);
    final channel = await _openGeneral(tester, app);

    await tester.tapAt(_textOf(tester, _ping));
    await tester.pump(const Duration(milliseconds: 80));
    await tester.tapAt(_textOf(tester, _ping));
    await _pumpFor(tester, const Duration(milliseconds: 200));

    expect(
      app.read(composerProvider(channel)).replyTo?.content,
      contains(_ping),
    );
    await app.dispose(tester);
  });

  testWidgets('two slow clicks are not a double-click', (tester) async {
    final app = await MockApp.pump(tester);
    final channel = await _openGeneral(tester, app);

    await tester.tapAt(_textOf(tester, _ping));
    await tester.pump(const Duration(milliseconds: 500));
    await tester.tapAt(_textOf(tester, _ping));
    await _pumpFor(tester, const Duration(milliseconds: 200));

    expect(app.read(composerProvider(channel)).replyTo, isNull);
    await app.dispose(tester);
  });

  testWidgets('double-clicking a reaction chip does not reply', (tester) async {
    final app = await MockApp.pump(tester);
    final channel = await _openGeneral(tester, app);
    final chip = find.descendant(of: _bubble(_ping), matching: find.text('👀'));

    await tester.tap(chip);
    await tester.pump(const Duration(milliseconds: 80));
    await tester.tap(chip);
    await _pumpFor(tester, const Duration(milliseconds: 400));

    expect(app.read(composerProvider(channel)).replyTo, isNull);
    await app.dispose(tester);
  });

  testWidgets('the hover bar lingers so the pointer can reach it', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openGeneral(tester, app);
    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    // Over the channel sidebar, away from any message.
    await mouse.addPointer(location: const Offset(200, 400));
    addTearDown(mouse.removePointer);

    await mouse.moveTo(_textOf(tester, _ping));
    await tester.pump();
    expect(find.byType(HoverActionBar), findsOneWidget);

    // Over to the bar itself: it stays.
    await mouse.moveTo(tester.getCenter(find.byType(HoverActionBar)));
    await tester.pump(const Duration(milliseconds: 300));
    expect(find.byType(HoverActionBar), findsOneWidget);

    // Away from both: it goes after the short delay.
    await mouse.moveTo(const Offset(5, 795));
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.byType(HoverActionBar), findsOneWidget);
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.byType(HoverActionBar), findsNothing);
    await app.dispose(tester);
  });

  for (final density in MessageDensity.values) {
    testWidgets(
      'the hover bar always stays inside the list (${density.name})',
      (tester) async {
        final app = await MockApp.pump(
          tester,
          size: const Size(900, 700),
          settings: (settings) => settings.copyWith(density: density),
        );
        await _openGeneral(tester, app);
        final list = tester.getRect(find.byType(MessageList));
        final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
        // Over the server rail, away from any message.
        await mouse.addPointer(location: const Offset(30, 650));
        addTearDown(mouse.removePointer);
        final texts = [
          for (final element in find.byType(MarkdownView).evaluate())
            tester.getCenter(find.byWidget(element.widget)),
        ].where((point) => list.deflate(4).contains(point)).toList();
        expect(texts.length, greaterThan(4));

        for (final point in texts) {
          await mouse.moveTo(point);
          await tester.pump();
          final bar = tester.getRect(find.byType(HoverActionBar));
          expect(
            list.inflate(1).contains(bar.topLeft),
            isTrue,
            reason: '$point',
          );
          expect(
            list.inflate(1).contains(bar.bottomRight),
            isTrue,
            reason: '$point',
          );
          await mouse.moveTo(const Offset(30, 650));
          await tester.pump(const Duration(milliseconds: 200));
        }
        await app.dispose(tester);
      },
    );
  }

  testWidgets('a reaction picked from the hover bar is added', (tester) async {
    final app = await MockApp.pump(tester);
    final channel = await _openGeneral(tester, app);
    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    // Over the channel sidebar, away from any message.
    await mouse.addPointer(location: const Offset(200, 400));
    addTearDown(mouse.removePointer);
    await mouse.moveTo(_textOf(tester, _ping));
    await tester.pump();

    await tester.tap(
      find.byWidgetPredicate(
        (widget) => widget is OcIconButton && widget.tooltip == 'Add reaction',
      ),
    );
    await _pumpFor(tester, const Duration(milliseconds: 200));
    expect(find.byType(QuickReactions), findsOneWidget);
    await tester.tap(
      find.descendant(
        of: find.byType(QuickReactions),
        matching: find.text('🎉'),
      ),
    );
    await _pumpFor(tester, const Duration(milliseconds: 500));

    final message = app
        .read(channelMessagesProvider(channel))
        .messages
        .firstWhere((m) => m.content.contains(_ping));
    expect(
      message.reactions.where((r) => r.emoji == '🎉' && r.me),
      hasLength(1),
    );
    await app.dispose(tester);
  });

  testWidgets('deleting asks first', (tester) async {
    final app = await MockApp.pump(tester);
    await _openGeneral(tester, app);

    await _rightClick(tester, _textOf(tester, _ping));
    await tester.tap(find.text('Delete'));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(find.text('Delete message?'), findsOneWidget);
    await tester.tap(find.widgetWithText(OcButton, 'Cancel'));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(_bubble(_ping), findsOneWidget);

    await _rightClick(tester, _textOf(tester, _ping));
    await tester.tap(find.text('Delete'));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    await tester.tap(find.widgetWithText(OcButton, 'Delete'));
    await _pumpFor(tester, const Duration(milliseconds: 800));

    expect(_bubble(_ping), findsNothing);
    await app.dispose(tester);
  });

  testWidgets('Copy message link copies an opencord link', (tester) async {
    _mockClipboard(tester);
    final app = await MockApp.pump(tester);
    final channel = await _openGeneral(tester, app);
    final message = tester.widget<MessageBubble>(_bubble(_ping)).message;

    await _rightClick(tester, _textOf(tester, _ping));
    await tester.tap(find.text('Copy message link'));
    await _pumpFor(tester, const Duration(milliseconds: 200));

    expect(_copied, 'opencord://$_server/c/${channel.channel}/${message.id}');
    await _pumpFor(tester, const Duration(seconds: 2));
    await app.dispose(tester);
  });

  testWidgets('Shift+F10 opens the menu of the focused message', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openGeneral(tester, app);
    final item = find.ancestor(
      of: _bubble(_ping),
      matching: find.byType(MessageItem),
    );
    Focus.of(
      tester.element(
        find.descendant(of: item, matching: find.byType(Listener)).first,
      ),
    ).requestFocus();
    await tester.pump();

    await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    await tester.sendKeyEvent(LogicalKeyboardKey.f10);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    await _pumpFor(tester, const Duration(milliseconds: 200));

    expect(_menuLabels(tester), contains('Reply'));
    await app.dispose(tester);
  });

  testWidgets('selected text can be copied on its own', (tester) async {
    _mockClipboard(tester);
    final app = await MockApp.pump(tester);
    await _openGeneral(tester, app);
    final text = find.descendant(
      of: _bubble(_mine),
      matching: find.byType(MarkdownView),
    );
    final rect = tester.getRect(text);

    final mouse = await tester.startGesture(
      rect.centerLeft + const Offset(2, 0),
      kind: PointerDeviceKind.mouse,
    );
    await tester.pump();
    await mouse.moveTo(rect.center);
    await tester.pump();
    await mouse.up();
    await tester.pump(const Duration(milliseconds: 400));

    await _rightClick(tester, rect.center);
    expect(_menuLabels(tester).first, 'Copy selection');
    await tester.tap(find.text('Copy selection'));
    await _pumpFor(tester, const Duration(milliseconds: 200));

    expect(_copied, isNotEmpty);
    expect(_mine, contains(_copied!));
    expect(_copied!.length, lessThan(_mine.length));
    await _pumpFor(tester, const Duration(seconds: 2));
    await app.dispose(tester);
  });
}
