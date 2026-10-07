import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/chat/message_list.dart';
import 'package:opencord/features/shell/navigation.dart';

import '../../support/app.dart';

const _dev = 'opencord.example:7710';
const _note = 'Attachments are coming later';

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

/// What the desktop_drop plugin sends while files are dragged.
Future<void> _drop(WidgetTester tester, String method, Object arguments) =>
    tester.binding.defaultBinaryMessenger.handlePlatformMessage(
      'desktop_drop',
      const StandardMethodCodec().encodeMethodCall(
        MethodCall(method, arguments),
      ),
      (_) {},
    );

Future<MockApp> _openGeneral(WidgetTester tester) async {
  final app = await MockApp.pump(tester);
  final general = app
      .read(serverProvider(_dev))
      .data!
      .channels
      .values
      .firstWhere((channel) => channel.name == 'general')
      .id;
  app.read(navigationProvider.notifier).openChannel(_dev, general);
  await _pumpFor(tester, const Duration(milliseconds: 600));
  return app;
}

List<double> _overChat(WidgetTester tester) {
  final center = tester.getCenter(find.byType(MessageList));
  return [center.dx, center.dy];
}

void main() {
  testWidgets('files dragged over the chat say attachments are coming', (
    tester,
  ) async {
    final app = await _openGeneral(tester);

    await _drop(tester, 'entered', _overChat(tester));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(find.text(_note), findsOneWidget);

    await _drop(tester, 'exited', _overChat(tester));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(find.text(_note), findsNothing);
    await app.dispose(tester);
  });

  testWidgets('a drop shows the note a moment and sends nothing (§16)', (
    tester,
  ) async {
    final app = await _openGeneral(tester);
    final channel = (server: _dev, channel: app.read(currentChannelProvider)!);
    final before = app.read(channelMessagesProvider(channel)).all.length;

    await _drop(tester, 'entered', _overChat(tester));
    await tester.pump();
    await _drop(tester, 'performOperation', ['/home/a/photo.png']);
    await _pumpFor(tester, const Duration(milliseconds: 500));
    expect(find.text(_note), findsOneWidget);

    await _pumpFor(tester, const Duration(seconds: 4));
    expect(find.text(_note), findsNothing);
    expect(app.read(channelMessagesProvider(channel)).all, hasLength(before));
    await app.dispose(tester);
  });
}
