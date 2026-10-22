import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/channels/channel_row.dart';
import 'package:opencord/features/chat/bubble.dart';
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
  testWidgets('a channel link someone pasted opens that channel (§15)', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    final general = _channel(app, 'general');
    final devCore = _channel(app, 'dev-core');
    app.read(navigationProvider.notifier).openChannel(_dev, general);
    await tester.pump(const Duration(milliseconds: 800));
    final link = channelLink(_dev, devCore);

    app.repository.debugPostAs(_dev, general, 'see $link', authorId: _kai);
    await tester.pump(const Duration(milliseconds: 500));
    await tester.tapOnText(
      find.textRange.ofSubstring(
        link,
        descendentOf: find.byType(MessageBubble),
      ),
    );
    await tester.pumpAndSettle();

    expect(app.read(currentChannelProvider), devCore);
    await app.dispose(tester);
  });
}
