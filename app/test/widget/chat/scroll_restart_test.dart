import 'package:flutter_test/flutter_test.dart';
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
}
