import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/features/chat/compact_message.dart';
import 'package:opencord/features/chat/chat_scroll.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/features/chat/message_list.dart';
import 'package:opencord/features/shell/navigation.dart';

import '../../support/app.dart';

const _dev = 'opencord.example:7710';

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

void main() {
  testWidgets('where a channel was left is kept across a restart (§16)', (
    tester,
  ) async {
    final store = MemoryKeyValueStore();
    var app = await MockApp.pump(tester, store: store);
    final general = (
      server: _dev,
      channel: app
          .read(serverProvider(_dev))
          .data!
          .channels
          .values
          .firstWhere((channel) => channel.name == 'general')
          .id,
    );
    app.read(navigationProvider.notifier).openChannel(_dev, general.channel);
    await _pumpFor(tester, const Duration(milliseconds: 800));

    await tester.drag(find.byType(MessageList), const Offset(0, 500));
    await _pumpFor(tester, const Duration(seconds: 1));
    final left = app.read(scrollMemoryProvider).read(general);
    expect(left, isNotNull);
    await app.dispose(tester);

    app = await MockApp.pump(tester, store: store);
    expect(app.read(scrollMemoryProvider).read(general), left);
    await app.dispose(tester);
  });

  testWidgets('a place left further back than the newest page is reached', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    final stress = await _stress(tester, app);
    // As after a restart: only the newest page will load.
    final far = app.repository.debugMessages(_dev, stress.channel)[9800];
    app
        .read(scrollMemoryProvider)
        .save(stress, SavedScroll(messageId: far.id, fromTop: 120));

    app.read(navigationProvider.notifier).openChannel(_dev, stress.channel);
    await _pumpFor(tester, const Duration(seconds: 2));

    final row = find.byWidgetPredicate(
      (widget) => widget is CompactMessage && widget.message.id == far.id,
    );
    expect(row, findsOneWidget);
    final list = tester.getRect(find.byType(MessageList));
    // The row starts at 120.
    expect(tester.getTopLeft(row).dy - list.top, inInclusiveRange(120, 140));
    await app.dispose(tester);
  });

  testWidgets('leaving a channel before it opens keeps where it was left', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    final stress = await _stress(tester, app);
    final far = app.repository.debugMessages(_dev, stress.channel)[9800];
    final left = SavedScroll(messageId: far.id, fromTop: 120);
    app.read(scrollMemoryProvider).save(stress, left);

    app.read(navigationProvider.notifier).openChannel(_dev, stress.channel);
    await tester.pump();
    app.read(navigationProvider.notifier).openChannel(_dev, _general(app));
    await _pumpFor(tester, const Duration(seconds: 1));

    expect(app.read(scrollMemoryProvider).read(stress), left);
    await app.dispose(tester);
  });
}

/// Adds `#scroll-test` (10 000 messages), then goes to `#general`.
Future<ChannelRef> _stress(WidgetTester tester, MockApp app) async {
  app.repository.debugStressChannel(_dev);
  await _pumpFor(tester, const Duration(milliseconds: 200));
  app.read(navigationProvider.notifier).openChannel(_dev, _general(app));
  await _pumpFor(tester, const Duration(milliseconds: 200));
  return (
    server: _dev,
    channel: app
        .read(serverProvider(_dev))
        .data!
        .channels
        .values
        .firstWhere((channel) => channel.name == 'scroll-test')
        .id,
  );
}

int _general(MockApp app) => app
    .read(serverProvider(_dev))
    .data!
    .channels
    .values
    .firstWhere((channel) => channel.name == 'general')
    .id;
