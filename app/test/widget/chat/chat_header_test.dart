import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/format.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/chat/bubble.dart';
import 'package:opencord/features/chat/pinned_bar.dart';
import 'package:opencord/features/chat/typing_dots.dart';
import 'package:opencord/features/shell/navigation.dart';

import '../../support/app.dart';

const _server = 'opencord.example:7710';
const _kai = 1001;
const _mira = 1002;
const _jonas = 1003;

int _channelId(MockApp app, String name) => app
    .read(serverProvider(_server))
    .data!
    .channels
    .values
    .firstWhere((channel) => channel.name == name)
    .id;

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

Future<int> _openGeneral(WidgetTester tester, MockApp app) async {
  final general = _channelId(app, 'general');
  app.read(navigationProvider.notifier).openChannel(_server, general);
  await _pumpFor(tester, const Duration(milliseconds: 300));
  return general;
}

String _pinLabel(WidgetTester tester) => tester
    .widgetList<Text>(
      find.descendant(of: find.byType(PinnedBar), matching: find.byType(Text)),
    )
    .first
    .data!;

void main() {
  testWidgets('the subtitle counts members and who is online, then the topic', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    final general = await _openGeneral(tester, app);
    final audience = app.read(
      channelAudienceProvider((server: _server, channel: general)),
    );

    expect(
      find.text(
        '${countLabel(audience.members)} members, '
        '${countLabel(audience.online)} online · '
        'Everything Opencord, one message at a time',
      ),
      findsOneWidget,
    );
    await app.dispose(tester);
  });

  testWidgets('someone typing takes over the subtitle for ten seconds', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    final general = await _openGeneral(tester, app);

    app.repository.debugTyping(_server, general, _kai);
    await _pumpFor(tester, const Duration(milliseconds: 100));
    expect(find.text('Kai Nakamura is typing'), findsOneWidget);
    expect(find.byType(TypingDots), findsOneWidget);

    app.repository.debugTyping(_server, general, _mira);
    await _pumpFor(tester, const Duration(milliseconds: 100));
    expect(
      find.text('Kai Nakamura and Mira Okafor are typing'),
      findsOneWidget,
    );

    app.repository.debugTyping(_server, general, _jonas);
    await _pumpFor(tester, const Duration(milliseconds: 100));
    expect(find.text('Several people are typing'), findsOneWidget);

    await _pumpFor(tester, const Duration(seconds: 11));
    expect(find.byType(TypingDots), findsNothing);
    expect(find.textContaining('members, '), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('the More menu mutes the channel', (tester) async {
    final app = await MockApp.pump(tester);
    final general = await _openGeneral(tester, app);

    await tester.tap(find.byTooltip('More'));
    await _pumpFor(tester, const Duration(milliseconds: 200));
    expect(find.text('Copy link'), findsOneWidget);
    expect(find.text('Mark as read'), findsOneWidget);
    await tester.tap(find.text('Mute channel'));
    await _pumpFor(tester, const Duration(milliseconds: 200));

    expect(
      app.read(notificationPrefsProvider).channelMuted(_server, general),
      isTrue,
    );
    await app.dispose(tester);
  });

  testWidgets('the pinned bar starts at the newest pin and steps back', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openGeneral(tester, app);

    expect(_pinLabel(tester), 'Pinned message 2 of 2');
    expect(
      find.descendant(
        of: find.byType(PinnedBar),
        matching: find.textContaining('Release checklist for M5'),
      ),
      findsOneWidget,
    );

    await tester.tap(find.byType(PinnedBar));
    await tester.pumpAndSettle();

    expect(_pinLabel(tester), 'Pinned message 1 of 2');
    final highlighted = tester
        .widgetList<MessageBubble>(find.byType(MessageBubble))
        .where((bubble) => bubble.highlighted);
    expect(
      highlighted.single.message.content,
      startsWith('Release checklist for M5'),
    );
    await tester.pump(const Duration(seconds: 2));
    await app.dispose(tester);
  });

  testWidgets('unpinning offers Undo instead of asking first', (tester) async {
    final app = await MockApp.pump(tester);
    final general = await _openGeneral(tester, app);
    final pins = pinsProvider((server: _server, channel: general));

    await tester.tap(
      find.descendant(
        of: find.byType(PinnedBar),
        matching: find.byTooltip('Unpin'),
      ),
    );
    await _pumpFor(tester, const Duration(milliseconds: 400));

    expect(app.read(pins), hasLength(1));
    expect(_pinLabel(tester), 'Pinned message');
    expect(find.text('Message unpinned'), findsOneWidget);

    await tester.tap(find.text('Undo'));
    await _pumpFor(tester, const Duration(milliseconds: 400));

    expect(app.read(pins), hasLength(2));
    await _pumpFor(tester, const Duration(seconds: 6));
    await app.dispose(tester);
  });

  testWidgets('the pin button lists every pin and jumps to the chosen one', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openGeneral(tester, app);

    await tester.tap(find.byTooltip('Pinned messages'));
    await _pumpFor(tester, const Duration(milliseconds: 200));
    final list = find.byType(PinnedList);
    expect(list, findsOneWidget);
    await tester.tap(
      find.descendant(
        of: list,
        matching: find.textContaining('the Berlin meetup is on Thursday'),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.byType(PinnedList), findsNothing);
    final highlighted = tester
        .widgetList<MessageBubble>(find.byType(MessageBubble))
        .where((bubble) => bubble.highlighted);
    expect(highlighted.single.message.content, contains('Berlin meetup'));
    await tester.pump(const Duration(seconds: 2));
    await app.dispose(tester);
  });
}
