import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/shell/navigation.dart';

import '../../support/app.dart';

const _dev = 'opencord.example:7710';
const _kai = 1001;

/// What the app asked screen readers to say.
List<String> _listen(WidgetTester tester) {
  final said = <String>[];
  tester.binding.defaultBinaryMessenger.setMockDecodedMessageHandler<Object?>(
    SystemChannels.accessibility,
    (message) async {
      if (message case {'type': 'announce', 'data': {'message': final text}}) {
        said.add('$text');
      }
      return null;
    },
  );
  addTearDown(
    () => tester.binding.defaultBinaryMessenger
        .setMockDecodedMessageHandler<Object?>(
          SystemChannels.accessibility,
          null,
        ),
  );
  return said;
}

int _channel(MockApp app, String name) => app
    .read(serverProvider(_dev))
    .data!
    .channels
    .values
    .firstWhere((channel) => channel.name == name)
    .id;

void main() {
  testWidgets('new messages in the open channel are read out (§9)', (
    tester,
  ) async {
    final said = _listen(tester);
    final app = await MockApp.pump(tester);
    final general = _channel(app, 'general');
    app.read(navigationProvider.notifier).openChannel(_dev, general);
    await tester.pump(const Duration(milliseconds: 800));
    said.clear();

    app.repository.debugPostAs(
      _dev,
      general,
      'Release notes are up in <#$general>',
      authorId: _kai,
    );
    await tester.pump();

    expect(said, ['Kai Nakamura: Release notes are up in #general']);
    await app.dispose(tester);
  });

  testWidgets('other channels and history stay quiet', (tester) async {
    final said = _listen(tester);
    final app = await MockApp.pump(tester);
    final general = _channel(app, 'general');
    app.read(navigationProvider.notifier).openChannel(_dev, general);
    await tester.pump(const Duration(milliseconds: 800));
    said.clear();

    app.repository.debugPostAs(
      _dev,
      _channel(app, 'dev-core'),
      'elsewhere',
      authorId: _kai,
    );
    await tester.pump();

    expect(said, isEmpty);
    await app.dispose(tester);
  });
}
