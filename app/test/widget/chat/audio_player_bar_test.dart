// The audio player bar, as Telegram Desktop's: it shows while a voice
// message or an audio file from the chat plays, steps through the
// channel's ones, goes on to the next at the end, opens the message, and
// shows the queue on hover.

import 'package:flutter/gestures.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/chat/audio_player_bar.dart';
import 'package:opencord/features/shell/navigation.dart';

import '../../support/app.dart';

const _server = 'opencord.example:7710';

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

int _channelId(MockApp app, String name) => app
    .read(serverProvider(_server))
    .data!
    .channels
    .values
    .firstWhere((channel) => channel.name == name)
    .id;

/// #general with a voice message, an audio file and another voice
/// message, in that order; their entries.
Future<List<AudioEntry>> _channelWithAudio(
  WidgetTester tester,
  MockApp app,
) async {
  final general = _channelId(app, 'general');
  app.read(navigationProvider.notifier).openChannel(_server, general);
  await _pumpFor(tester, const Duration(milliseconds: 300));
  final self = app.read(serverProvider(_server)).data!.self.id;
  final attachments = [
    const Attachment(
      id: 9101,
      filename: 'voice-message.ogg',
      contentType: 'audio/ogg',
      sizeBytes: 9000,
      kind: AttachmentKind.voiceMessage,
      durationMs: 3000,
    ),
    const Attachment(
      id: 9102,
      filename: 'song.m4a',
      contentType: 'audio/mp4',
      sizeBytes: 90000,
      kind: AttachmentKind.audio,
      durationMs: 3000,
    ),
    const Attachment(
      id: 9103,
      filename: 'voice-message.ogg',
      contentType: 'audio/ogg',
      sizeBytes: 9000,
      kind: AttachmentKind.voiceMessage,
      durationMs: 3000,
    ),
  ];
  final messages = app.read(
    channelMessagesProvider((server: _server, channel: general)).notifier,
  );
  final entries = <AudioEntry>[];
  for (final (index, attachment) in attachments.indexed) {
    final message = Message(
      id: 9001 + index,
      channelId: general,
      authorId: self,
      content: '',
      createdAt: DateTime(2026, 10, 20, 12, index),
      attachments: [attachment],
    );
    messages.apply(MessageCreated(_server, message));
    entries.add(
      AudioEntry(
        serverKey: _server,
        channelId: general,
        messageId: message.id,
        authorId: self,
        attachment: attachment,
      ),
    );
  }
  await tester.pump();
  return entries;
}

Finder _inBar(Finder finder) =>
    find.descendant(of: find.byType(AudioPlayerBar), matching: finder);

void main() {
  testWidgets('the bar steps through the channel\'s voice messages and '
      'audio files, and × stops', (tester) async {
    final app = await MockApp.pump(tester);
    final entries = await _channelWithAudio(tester, app);
    final playback = app.read(playbackProvider.notifier);
    expect(_inBar(find.byTooltip('Stop')), findsNothing, reason: 'no bar yet');

    await playback.playEntry(entries[0]);
    await _pumpFor(tester, const Duration(milliseconds: 200));
    expect(_inBar(find.textContaining('Voice message')), findsOneWidget);
    expect(playback.previousEntry, isNull);

    await tester.tap(_inBar(find.byTooltip('Next')));
    await _pumpFor(tester, const Duration(milliseconds: 600));
    expect(app.read(playbackProvider).item, entries[1].item);
    expect(_inBar(find.text('song.m4a')), findsOneWidget);

    await tester.tap(_inBar(find.byTooltip('Next')));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(app.read(playbackProvider).item, entries[2].item);
    expect(playback.nextEntry, isNull);

    await tester.tap(_inBar(find.byTooltip('Previous')));
    await _pumpFor(tester, const Duration(milliseconds: 600));
    expect(app.read(playbackProvider).item, entries[1].item);

    await tester.tap(_inBar(find.byTooltip('Stop')));
    await _pumpFor(tester, const Duration(milliseconds: 100));
    expect(app.read(playbackProvider).item, isNull);
    expect(_inBar(find.byTooltip('Stop')), findsNothing);
    await app.dispose(tester);
  });

  testWidgets('when one ends, the next one plays', (tester) async {
    final app = await MockApp.pump(tester);
    final entries = await _channelWithAudio(tester, app);

    await app.read(playbackProvider.notifier).playEntry(entries[0]);
    await _pumpFor(tester, const Duration(milliseconds: 3600));
    expect(app.read(playbackProvider).item, entries[1].item);
    expect(app.read(playbackProvider).playing, isTrue);
    await app.dispose(tester);
  });

  testWidgets('its name opens the message\'s channel', (tester) async {
    final app = await MockApp.pump(tester);
    final entries = await _channelWithAudio(tester, app);
    await app.read(playbackProvider.notifier).playEntry(entries[0]);
    await _pumpFor(tester, const Duration(milliseconds: 200));

    final random = _channelId(app, 'off-topic');
    app.read(navigationProvider.notifier).openChannel(_server, random);
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(app.read(currentChannelProvider), random);

    await tester.tap(_inBar(find.textContaining('Voice message')));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(app.read(currentChannelProvider), entries[0].channelId);
    await app.dispose(tester);
  });

  testWidgets('hovering the bar shows the queue', (tester) async {
    final app = await MockApp.pump(tester);
    final entries = await _channelWithAudio(tester, app);
    await app.read(playbackProvider.notifier).playEntry(entries[0]);
    await _pumpFor(tester, const Duration(milliseconds: 200));

    final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
    addTearDown(mouse.removePointer);
    await mouse.addPointer(location: Offset.zero);
    await mouse.moveTo(tester.getCenter(find.byType(AudioPlayerBar)));
    await _pumpFor(tester, const Duration(milliseconds: 500));
    expect(find.text('song.m4a'), findsOneWidget, reason: 'in the queue');
    expect(find.textContaining('Voice message'), findsAtLeastNWidgets(3));

    // A click in the queue plays it.
    await tester.tap(find.text('song.m4a'));
    await _pumpFor(tester, const Duration(milliseconds: 600));
    expect(app.read(playbackProvider).item, entries[1].item);
    await app.dispose(tester);
  });
}
