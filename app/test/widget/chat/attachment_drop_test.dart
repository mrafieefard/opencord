import 'dart:io';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/chat/composer_state.dart';
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
  testWidgets('files dragged over the chat show drop target overlay', (
    tester,
  ) async {
    final app = await _openGeneral(tester);

    await _drop(tester, 'entered', _overChat(tester));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(find.text('Drop to send to #general'), findsOneWidget);
    expect(find.textContaining('Up to 50.0 MB each'), findsOneWidget);

    await _drop(tester, 'exited', _overChat(tester));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(find.text('Drop to send to #general'), findsNothing);
    await app.dispose(tester);
  });

  testWidgets('a drop stages the files in composer', (tester) async {
    final app = await _openGeneral(tester);
    final channel = (server: _dev, channel: app.read(currentChannelProvider)!);

    final dir = Directory.systemTemp.createTempSync('oc_drop_test_');
    final sampleFile = File('${dir.path}/dropped.png')
      ..writeAsBytesSync([1, 2, 3]);

    await _drop(tester, 'entered', _overChat(tester));
    await tester.pump();
    await tester.runAsync(() async {
      await _drop(tester, 'performOperation', [sampleFile.path]);
      // Give async I/O time to read file stats
      for (var i = 0; i < 20; i++) {
        if (app.read(composerProvider(channel)).staged.isNotEmpty) break;
        await Future.delayed(const Duration(milliseconds: 20));
      }
    });
    await tester.pump();

    expect(find.text('Drop to send to #general'), findsNothing);
    final staged = app.read(composerProvider(channel)).staged;
    expect(staged, hasLength(1));
    expect(staged.first.name, 'dropped.png');

    dir.deleteSync(recursive: true);
    await app.dispose(tester);
  });
}
