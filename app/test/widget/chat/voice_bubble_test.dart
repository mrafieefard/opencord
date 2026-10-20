import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/mock/mock_repository.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/providers/playback_state.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/chat/attachments/attachment_view.dart';
import 'package:opencord/features/chat/voice/voice_bubble.dart';
import 'package:opencord/features/chat/voice/waveform_painter.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_theme.dart';

Message _msg(Attachment attachment) => Message(
  id: 1,
  channelId: 10,
  authorId: 100,
  content: '',
  createdAt: DateTime(2026, 1, 1),
  attachments: [attachment],
);

void main() {
  testWidgets('voice_bubble_plays_seeks_and_changes_speed', (tester) async {
    final repository = MockRepository();
    addTearDown(repository.dispose);

    final attachment = Attachment(
      id: 42,
      filename: 'voice-message.ogg',
      contentType: 'audio/ogg',
      sizeBytes: 12000,
      kind: AttachmentKind.voiceMessage,
      durationMs: 3000,
      waveform: Uint8List.fromList(List.filled(128, 120)),
    );

    await tester.pumpWidget(
      ProviderScope(
        overrides: [repositoryProvider.overrideWithValue(repository)],
        child: MaterialApp(
          theme: buildTheme(OcColors.dark),
          home: Scaffold(
            body: AttachmentsView(
              serverKey: 'server.example:7710',
              message: _msg(attachment),
            ),
          ),
        ),
      ),
    );

    // Initial state: VoiceBubble rendered, shows Play button and total duration "0:03"
    expect(find.byType(VoiceBubble), findsOneWidget);
    expect(find.byIcon(OcIcons.playArrow), findsOneWidget);
    expect(find.text('0:03'), findsOneWidget);
    expect(find.text('1×'), findsOneWidget);

    // Tap play
    await tester.tap(find.byIcon(OcIcons.playArrow));
    await tester.pump();

    // Now playing: button shows Pause, time displays "0:00 / 0:03"
    expect(find.byIcon(OcIcons.pause), findsOneWidget);
    expect(find.text('0:00 / 0:03'), findsOneWidget);

    // Tap speed chip: cycles 1× -> 1.5×
    await tester.tap(find.text('1×'));
    await tester.pump();
    expect(find.text('1.5×'), findsOneWidget);
    expect(repository.lastVoiceSpeed, 1.5);

    // Tap speed chip again: cycles 1.5× -> 2×
    await tester.tap(find.text('1.5×'));
    await tester.pump();
    expect(find.text('2×'), findsOneWidget);
    expect(repository.lastVoiceSpeed, 2.0);

    // Tap speed chip again: cycles 2× -> 1×
    await tester.tap(find.text('2×'));
    await tester.pump();
    expect(find.text('1×'), findsOneWidget);
    expect(repository.lastVoiceSpeed, 1.0);

    // Tap on waveform to seek
    final waveformFinder = find.byType(CustomPaint);
    expect(waveformFinder, findsWidgets);
    // Tap center of waveform
    await tester.tap(
      find.byWidgetPredicate(
        (w) => w is CustomPaint && w.painter is WaveformPainter,
      ),
    );
    await tester.pump();

    // Tap pause
    await tester.tap(find.byIcon(OcIcons.pause));
    await tester.pump();
    expect(find.byIcon(OcIcons.playArrow), findsOneWidget);
  });

  testWidgets('a_voice_message_plays_again_after_it_ends', (tester) async {
    final repository = MockRepository();
    addTearDown(repository.dispose);
    final attachment = Attachment(
      id: 42,
      filename: 'voice-message.ogg',
      contentType: 'audio/ogg',
      sizeBytes: 12000,
      kind: AttachmentKind.voiceMessage,
      durationMs: 3000,
      waveform: Uint8List.fromList(List.filled(128, 120)),
    );
    await tester.pumpWidget(
      ProviderScope(
        overrides: [repositoryProvider.overrideWithValue(repository)],
        child: MaterialApp(
          theme: buildTheme(OcColors.dark),
          home: Scaffold(
            body: AttachmentsView(
              serverKey: 'server.example:7710',
              message: _msg(attachment),
            ),
          ),
        ),
      ),
    );

    // The core's playback events reach the playback state, as in the app.
    final container = ProviderScope.containerOf(
      tester.element(find.byType(VoiceBubble)),
    );
    final routed = repository.events.listen((event) {
      if (event is PlaybackUpdate ||
          event is PlaybackEnded ||
          event is PlaybackFailed) {
        container.read(playbackProvider.notifier).apply(event);
      }
    });
    addTearDown(routed.cancel);

    await tester.tap(find.byIcon(OcIcons.playArrow));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 3200));
    await tester.pump();
    expect(find.byIcon(OcIcons.playArrow), findsOneWidget);

    // Played to the end: Play starts it again from the start.
    await tester.tap(find.byIcon(OcIcons.playArrow));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 1100));
    await tester.pump();
    expect(find.byIcon(OcIcons.pause), findsOneWidget);
    expect(find.text('0:01 / 0:03'), findsOneWidget);

    await tester.tap(find.byIcon(OcIcons.pause));
    await tester.pump();
  });
}
