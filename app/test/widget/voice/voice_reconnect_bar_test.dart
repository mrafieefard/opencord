// The reconnect bar across the top of the app (D88).

import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/providers/voice_reconnect.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/features/voice/voice_reconnect_bar.dart';

import '../../support/app.dart';

/// The first server's first channel of [kind].
(String, Channel) _channel(MockApp app, ChannelKind kind) {
  final server = app.repository.servers.first.key;
  final channel = app
      .read(serverProvider(server))
      .data!
      .channels
      .values
      .firstWhere((channel) => channel.kind == kind);
  return (server, channel);
}

/// The app, started after it closed in [channelId] on [server].
Future<MockApp> _startedAfterClosingIn(
  WidgetTester tester,
  String server,
  int channelId,
) {
  final store = MemoryKeyValueStore()
    ..write(lastCallKey, jsonEncode({'server': server, 'channel': channelId}));
  return MockApp.pump(tester, store: store);
}

void main() {
  testWidgets('no bar without a call to get back to', (tester) async {
    final app = await MockApp.pump(tester);
    expect(find.byType(VoiceReconnectBar), findsOneWidget);
    expect(find.text('Reconnect'), findsNothing);
    await tester.pump(const Duration(seconds: 1));
    await app.dispose(tester);
  });

  testWidgets('another device taking the call offers it back', (tester) async {
    final app = await MockApp.pump(tester);
    final (server, voice) = _channel(app, ChannelKind.voice);
    await app.read(voiceSessionProvider.notifier).join(server, voice.id);
    await tester.pump();

    app.repository.debugVoiceElsewhere();
    await tester.pump();

    expect(
      find.text('Your call in ${voice.name} moved to another device.'),
      findsOneWidget,
    );
    await tester.tap(find.text('Reconnect'));
    await tester.pump(const Duration(seconds: 1));

    final session = app.read(voiceSessionProvider);
    expect((session.serverKey, session.channelId), (server, voice.id));
    expect(find.text('Reconnect'), findsNothing);
    await tester.pump(const Duration(seconds: 1));
    await app.dispose(tester);
  });

  testWidgets('a start after closing in a call names it', (tester) async {
    final probe = await MockApp.pump(tester);
    final (server, voice) = _channel(probe, ChannelKind.voice);
    await probe.dispose(tester);

    final app = await _startedAfterClosingIn(tester, server, voice.id);

    expect(
      find.text('Opencord closed while you were in ${voice.name}.'),
      findsOneWidget,
    );
    expect(find.text('Reconnect'), findsOneWidget);
    await tester.pump(const Duration(seconds: 1));
    await app.dispose(tester);
  });

  testWidgets('a refused rejoin says why', (tester) async {
    final app = await MockApp.pump(tester);
    final (server, voice) = _channel(app, ChannelKind.voice);
    await app.read(voiceSessionProvider.notifier).join(server, voice.id);
    await tester.pump();

    app.repository.debugRejoinRefused('missing permission: connect');
    await tester.pump();

    expect(
      find.text(
        "Couldn't get back into ${voice.name}: missing permission: connect",
      ),
      findsOneWidget,
    );
    await tester.pump(const Duration(seconds: 1));
    await app.dispose(tester);
  });

  testWidgets('dismissing hides the bar', (tester) async {
    final app = await MockApp.pump(tester);
    final (server, voice) = _channel(app, ChannelKind.voice);
    await app.read(voiceSessionProvider.notifier).join(server, voice.id);
    await tester.pump();
    app.repository.debugVoiceElsewhere();
    await tester.pump();

    await tester.tap(find.byTooltip('Dismiss'));
    await tester.pump();

    expect(find.text('Reconnect'), findsNothing);
    expect(app.read(voiceSessionProvider).connected, isFalse);
    await tester.pump(const Duration(seconds: 1));
    await app.dispose(tester);
  });

  testWidgets('a failed reconnect says why and keeps the bar', (tester) async {
    final probe = await MockApp.pump(tester);
    final (server, text) = _channel(probe, ChannelKind.text);
    await probe.dispose(tester);
    final app = await _startedAfterClosingIn(tester, server, text.id);

    await tester.tap(find.text('Reconnect'));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));

    expect(find.text('That is not a voice channel.'), findsOneWidget);
    expect(find.text('Reconnect'), findsOneWidget);
    await tester.pump(const Duration(seconds: 10));
    await app.dispose(tester);
  });

  testWidgets('a call in a channel that is gone offers nothing', (
    tester,
  ) async {
    final probe = await MockApp.pump(tester);
    final server = probe.repository.servers.first.key;
    await probe.dispose(tester);
    final app = await _startedAfterClosingIn(tester, server, 999999);

    expect(find.text('Reconnect'), findsNothing);
    await tester.pump(const Duration(seconds: 1));
    await app.dispose(tester);
  });

  testWidgets('the bar sits above the whole shell', (tester) async {
    final app = await MockApp.pump(tester);
    final (server, voice) = _channel(app, ChannelKind.voice);
    await app.read(voiceSessionProvider.notifier).join(server, voice.id);
    await tester.pump();
    app.repository.debugVoiceElsewhere();
    await tester.pump();

    final bar = tester.getRect(find.byKey(VoiceReconnectBar.barKey));
    expect(bar.width, 1440);
    expect(
      tester.getTopLeft(find.byKey(const Key('shell-rail'))).dy,
      bar.bottom,
    );
    await tester.pump(const Duration(seconds: 1));
    await app.dispose(tester);
  });
}
