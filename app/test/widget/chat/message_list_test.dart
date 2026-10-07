import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/settings/app_settings.dart';
import 'package:opencord/features/chat/bubble.dart';
import 'package:opencord/features/chat/compact_message.dart';
import 'package:opencord/features/chat/list_overlays.dart';
import 'package:opencord/features/chat/message_list.dart';
import 'package:opencord/features/shell/navigation.dart';

import '../../support/app.dart';

const _server = 'opencord.example:7710';

int _channelId(MockApp app, String name) => app
    .read(serverProvider(_server))
    .data!
    .channels
    .values
    .firstWhere((channel) => channel.name == name)
    .id;

Future<void> _settle(WidgetTester tester) async {
  for (var i = 0; i < 5; i++) {
    await tester.pump(const Duration(milliseconds: 50));
  }
}

Future<void> _open(WidgetTester tester, MockApp app, String name) async {
  app
      .read(navigationProvider.notifier)
      .openChannel(_server, _channelId(app, name));
  await _settle(tester);
}

/// Adds `#scroll-test` (10 000 messages, all read) and opens it.
Future<int> _openStress(WidgetTester tester, MockApp app) async {
  app.repository.debugStressChannel(_server);
  await _settle(tester);
  await _open(tester, app, 'scroll-test');
  return _channelId(app, 'scroll-test');
}

Rect _listRect(WidgetTester tester) => tester.getRect(find.byType(MessageList));

/// Whether [text] is drawn inside the message list's visible area.
bool _inView(WidgetTester tester, String text) {
  final found = find.textContaining(text, findRichText: true);
  if (found.evaluate().isEmpty) return false;
  final rect = tester.getRect(found.first);
  final list = _listRect(tester);
  return rect.top >= list.top - 1 && rect.bottom <= list.bottom + 1;
}

/// The first bubble entirely in view, as a finder that keeps finding it.
Finder _firstVisibleBubble(WidgetTester tester) {
  final list = _listRect(tester);
  for (final element in find.byType(MessageBubble).evaluate()) {
    final bubble = element.widget as MessageBubble;
    final finder = find.byWidgetPredicate(
      (widget) =>
          widget is MessageBubble && widget.message.id == bubble.message.id,
    );
    final rect = tester.getRect(finder);
    if (rect.top >= list.top && rect.bottom <= list.bottom) return finder;
  }
  throw StateError('no bubble in view');
}

Future<void> _scrollUp(WidgetTester tester, double by) async {
  await tester.drag(
    find.byType(MessageList),
    Offset(0, by),
    warnIfMissed: false,
  );
  await tester.pumpAndSettle();
}

void main() {
  testWidgets('a channel with unread messages opens at the unread line', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'general');

    expect(_inView(tester, 'Unread messages'), isTrue);
    await app.dispose(tester);
  });

  testWidgets('compact density draws lines instead of bubbles', (tester) async {
    final app = await MockApp.pump(
      tester,
      settings: (settings) =>
          settings.copyWith(density: MessageDensity.compact),
    );
    await _open(tester, app, 'general');

    expect(find.byType(CompactMessage), findsWidgets);
    expect(find.byType(MessageBubble), findsNothing);
    await app.dispose(tester);
  });

  testWidgets('a read channel opens at the newest message', (tester) async {
    final app = await MockApp.pump(tester);
    await _openStress(tester, app);

    expect(_inView(tester, 'Message 9999'), isTrue);
    expect(find.text('Unread messages'), findsNothing);
    await app.dispose(tester);
  });

  testWidgets('new messages are followed while at the newest one', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    final channel = await _openStress(tester, app);

    app.repository.debugPostAs(_server, channel, 'Fresh off the press');
    await _settle(tester);

    expect(_inView(tester, 'Fresh off the press'), isTrue);
    await app.dispose(tester);
  });

  testWidgets('reading older messages is not disturbed by new ones', (
    tester,
  ) async {
    final semantics = tester.ensureSemantics();
    final app = await MockApp.pump(tester);
    final channel = await _openStress(tester, app);
    await _scrollUp(tester, 1500);
    final reading = _firstVisibleBubble(tester);
    final before = tester.getTopLeft(reading).dy;

    app.repository.debugPostAs(_server, channel, 'While you were away');
    await _settle(tester);

    expect(tester.getTopLeft(reading).dy, before);
    expect(
      find.bySemanticsLabel(RegExp('Jump to the newest message, 1 new')),
      findsOneWidget,
    );

    await tester.tap(find.byType(JumpToBottomButton));
    await _settle(tester);

    expect(_inView(tester, 'While you were away'), isTrue);
    semantics.dispose();
    await app.dispose(tester);
  });

  testWidgets('older history loads near the top', (tester) async {
    final app = await MockApp.pump(tester);
    final channel = await _openStress(tester, app);
    final ref = (server: _server, channel: channel);
    final loaded = app.read(channelMessagesProvider(ref)).messages.length;

    for (var i = 0; i < 4; i++) {
      await _scrollUp(tester, 2000);
    }

    expect(
      app.read(channelMessagesProvider(ref)).messages.length,
      greaterThan(loaded),
    );
    await app.dispose(tester);
  });

  testWidgets('older history arriving keeps the view still', (tester) async {
    final app = await MockApp.pump(tester);
    final channel = await _openStress(tester, app);
    final ref = (server: _server, channel: channel);
    await _scrollUp(tester, 600);
    final reading = _firstVisibleBubble(tester);
    final before = tester.getTopLeft(reading).dy;
    final count = app.read(channelMessagesProvider(ref)).messages.length;

    await app.read(channelMessagesProvider(ref).notifier).loadOlder();
    await _settle(tester);

    expect(
      app.read(channelMessagesProvider(ref)).messages.length,
      greaterThan(count),
    );
    expect(tester.getTopLeft(reading).dy, before);
    await app.dispose(tester);
  });

  testWidgets('reaching the newest message marks the channel read', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'general');
    final general = _channelId(app, 'general');

    expect(app.read(activityProvider(_server)).of(general).read.unread, 0);
    expect(app.read(activityProvider(_server)).focused, general);
    await app.dispose(tester);
  });

  testWidgets('coming back to a channel returns to where the reader was', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openStress(tester, app);
    await _scrollUp(tester, 1500);
    final reading = _firstVisibleBubble(tester);
    final before = tester.getTopLeft(reading).dy;

    await _open(tester, app, 'general');
    await _open(tester, app, 'scroll-test');

    expect(tester.getTopLeft(reading).dy, closeTo(before, 2));
    await app.dispose(tester);
  });

  testWidgets('a reply quote jumps to the original and highlights it', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'general');
    final semantics = tester.ensureSemantics();
    await tester.tap(find.bySemanticsLabel(RegExp('^Reply to Priya Shah')));
    await tester.pumpAndSettle();
    semantics.dispose();

    final highlighted = tester
        .widgetList<MessageBubble>(find.byType(MessageBubble))
        .where((bubble) => bubble.highlighted);
    expect(highlighted, hasLength(1));
    expect(
      highlighted.single.message.content,
      contains('could you look at the invite dialog copy'),
    );
    final original = tester.getRect(
      find.byWidgetPredicate(
        (widget) => widget is MessageBubble && widget.highlighted,
      ),
    );
    final list = _listRect(tester);
    expect(original.top, greaterThanOrEqualTo(list.top));
    expect(original.bottom, lessThanOrEqualTo(list.bottom));
    await tester.pump(const Duration(seconds: 2));
    await app.dispose(tester);
  });

  testWidgets('a deleted message fades out, then its row goes', (tester) async {
    final app = await MockApp.pump(tester);
    final channel = await _openStress(tester, app);
    final ref = (server: _server, channel: channel);
    final last = app.read(channelMessagesProvider(ref)).messages.last;

    // The mock answers after a simulated delay of at most 300 ms; the
    // event lands within that.
    app.repository.deleteMessage(_server, channel, last.id);
    for (var i = 0; i < 31; i++) {
      await tester.pump(const Duration(milliseconds: 10));
      if (app.read(channelMessagesProvider(ref)).messages.last.id != last.id) {
        break;
      }
    }
    await tester.pump();

    expect(find.textContaining('Message 9999', findRichText: true), findsOne);
    final fade = tester.widget<AnimatedOpacity>(
      find
          .ancestor(
            of: find.textContaining('Message 9999', findRichText: true),
            matching: find.byType(AnimatedOpacity),
          )
          .first,
    );
    expect(fade.opacity, 0);

    await _settle(tester);
    expect(
      find.textContaining('Message 9999', findRichText: true),
      findsNothing,
    );
    await app.dispose(tester);
  });

  testWidgets('the date of what is at the top floats while scrolling', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openStress(tester, app);

    await tester.drag(find.byType(MessageList), const Offset(0, 1500));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 200));

    final pill = tester.widget<FloatingDayPill>(find.byType(FloatingDayPill));
    expect(pill.label, isNotNull);

    await tester.pump(const Duration(seconds: 2));
    expect(
      tester.widget<FloatingDayPill>(find.byType(FloatingDayPill)).label,
      isNull,
    );
    await app.dispose(tester);
  });
}
