import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/shell/navigation.dart';

import '../../support/app.dart';

const _dev = 'opencord.example:7710';
const _kai = 1001;

int _channel(MockApp app, String name) => app
    .read(serverProvider(_dev))
    .data!
    .channels
    .values
    .firstWhere((channel) => channel.name == name)
    .id;

void main() {
  // Servers pass message content through as sent: whatever another member
  // types has to show without breaking the channel for everyone else.
  testWidgets('a mention whose id does not fit 64 bits shows as typed', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    final general = _channel(app, 'general');
    app.read(navigationProvider.notifier).openChannel(_dev, general);
    await tester.pump(const Duration(milliseconds: 800));

    const typed = 'hello <@99999999999999999999>';
    app.repository
      ..debugPostAs(_dev, general, typed, authorId: _kai)
      // Not open: its sidebar row previews the message.
      ..debugPostAs(_dev, _channel(app, 'dev-core'), typed, authorId: _kai);
    await tester.pump(const Duration(milliseconds: 500));

    expect(tester.takeException(), isNull);
    expect(find.textContaining(typed, findRichText: true), findsWidgets);
    await app.dispose(tester);
  });
}
