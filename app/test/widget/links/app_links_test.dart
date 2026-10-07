import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/mock/mock_world.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/chat/bubble.dart';
import 'package:opencord/features/dialogs/add_server_dialog.dart';
import 'package:opencord/features/links/app_links.dart';
import 'package:opencord/features/shell/navigation.dart';

import '../../support/app.dart';

const _dev = 'opencord.example:7710';
const _berlin = 'rust-berlin.example:7710';

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

int _channel(MockApp app, String name) => app
    .read(serverProvider(_dev))
    .data!
    .channels
    .values
    .firstWhere((channel) => channel.name == name)
    .id;

Future<void> _receive(WidgetTester tester, MockApp app, String link) async {
  app.read(appLinkInboxProvider.notifier).add(link);
  await _pumpFor(tester, const Duration(milliseconds: 800));
}

void main() {
  testWidgets('a channel link opens the channel', (tester) async {
    final app = await MockApp.pump(tester);
    final devCore = _channel(app, 'dev-core');

    await _receive(tester, app, 'opencord://$_dev/c/$devCore');

    expect(app.read(currentChannelProvider), devCore);
    expect(app.read(appLinkInboxProvider), isEmpty);
    await app.dispose(tester);
  });

  testWidgets('a message link opens its channel at the message', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    final general = _channel(app, 'general');
    app.read(navigationProvider.notifier).openChannel(_dev, general);
    await _pumpFor(tester, const Duration(milliseconds: 800));
    final oldest = app
        .read(channelMessagesProvider((server: _dev, channel: general)))
        .messages
        .first;
    app
        .read(navigationProvider.notifier)
        .openChannel(_dev, _channel(app, 'dev-core'));
    await _pumpFor(tester, const Duration(milliseconds: 800));

    await _receive(tester, app, 'opencord://$_dev/c/$general/${oldest.id}');
    await _pumpFor(tester, const Duration(milliseconds: 400));

    expect(app.read(currentChannelProvider), general);
    final highlighted = tester
        .widgetList<MessageBubble>(find.byType(MessageBubble))
        .where((bubble) => bubble.highlighted);
    expect(highlighted.map((bubble) => bubble.message.id), [oldest.id]);
    await app.dispose(tester);
  });

  testWidgets('an invite link opens Add server with the link filled in', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    const invite = 'opencord://new.example:7710/invite/Xy12';

    await _receive(tester, app, invite);

    expect(find.byType(AddServerDialog), findsOneWidget);
    final field = tester.widget<TextField>(
      find
          .descendant(
            of: find.byType(AddServerDialog),
            matching: find.byType(TextField),
          )
          .first,
    );
    expect(field.controller!.text, invite);
    await app.dispose(tester);
  });

  testWidgets('an invite to a server already added opens it', (tester) async {
    final app = await MockApp.pump(tester);

    await _receive(tester, app, 'opencord://$_berlin/invite/Zz99');

    expect(app.read(currentServerProvider), _berlin);
    expect(find.byType(AddServerDialog), findsNothing);
    expect(find.text('You are already in Rust Berlin.'), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('a link to a server you are not in says so', (tester) async {
    final app = await MockApp.pump(tester);
    final before = app.read(currentChannelProvider);

    await _receive(tester, app, 'opencord://elsewhere.example:7710/c/1');

    expect(app.read(currentChannelProvider), before);
    expect(find.text('You are not in that server.'), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('links given at launch are opened once the shell is up', (
    tester,
  ) async {
    final devCore = buildMockWorld(DateTime(2026)).servers
        .firstWhere((server) => server.key == _dev)
        .channels
        .values
        .firstWhere((channel) => channel.name == 'dev-core')
        .id;
    final app = await MockApp.pump(
      tester,
      links: ['opencord://$_dev/c/$devCore'],
    );
    await _pumpFor(tester, const Duration(milliseconds: 800));

    expect(app.read(currentChannelProvider), devCore);
    await app.dispose(tester);
  });
}
