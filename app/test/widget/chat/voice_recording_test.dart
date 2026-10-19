import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/chat/voice/mic_button.dart';
import 'package:opencord/features/chat/voice/recording_bar.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/theme/oc_icons.dart';

import '../../support/app.dart';

const _server = 'opencord.example:7710';

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

int _channelId(MockApp app, String name, {String server = _server}) => app
    .read(serverProvider(server))
    .data!
    .channels
    .values
    .firstWhere((channel) => channel.name == name)
    .id;

Future<ChannelRef> _open(
  WidgetTester tester,
  MockApp app,
  String name, {
  String server = _server,
}) async {
  final id = _channelId(app, name, server: server);
  app.read(navigationProvider.notifier).openChannel(server, id);
  await _pumpFor(tester, const Duration(milliseconds: 300));
  return (server: server, channel: id);
}

void main() {
  testWidgets('hold_shows_the_bar_and_release_sends', (tester) async {
    final app = await MockApp.pump(tester);
    final channel = await _open(tester, app, 'general');

    final micFinder = find.byType(MicButton);
    expect(micFinder, findsOneWidget);

    final gesture = await tester.startGesture(tester.getCenter(micFinder));
    await tester.pump(const Duration(milliseconds: 100));

    expect(find.byType(RecordingBar), findsOneWidget);

    // Hold for > 500 ms
    await _pumpFor(tester, const Duration(milliseconds: 600));

    await gesture.up();
    await _pumpFor(tester, const Duration(milliseconds: 200));

    expect(find.byType(RecordingBar), findsNothing);

    // Verify message sent with voice attachment
    final messages = app.read(channelMessagesProvider(channel)).all;
    final lastMessage = messages.last;
    expect(lastMessage.attachments, isNotEmpty);
    expect(lastMessage.attachments.first.kind, AttachmentKind.voiceMessage);
    await app.dispose(tester);
  });

  testWidgets('a_short_press_shows_hold_to_record', (tester) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'general');

    final micFinder = find.byType(MicButton);
    expect(micFinder, findsOneWidget);

    final gesture = await tester.startGesture(tester.getCenter(micFinder));
    await tester.pump(const Duration(milliseconds: 100));
    await gesture.up();
    await _pumpFor(tester, const Duration(milliseconds: 200));

    expect(find.text('Hold to record'), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('dragging_left_cancels', (tester) async {
    final app = await MockApp.pump(tester);
    final channel = await _open(tester, app, 'general');

    final micFinder = find.byType(MicButton);
    expect(micFinder, findsOneWidget);

    final gesture = await tester.startGesture(tester.getCenter(micFinder));
    await tester.pump(const Duration(milliseconds: 100));
    expect(find.byType(RecordingBar), findsOneWidget);

    // Drag left dx < -120
    await gesture.moveBy(const Offset(-130, 0));
    await tester.pump();

    expect(find.text('Release to cancel'), findsOneWidget);

    await gesture.up();
    await _pumpFor(tester, const Duration(milliseconds: 200));

    expect(find.byType(RecordingBar), findsNothing);
    final messages = app.read(channelMessagesProvider(channel)).all;
    expect(
      messages.where(
        (m) => m.attachments.any((a) => a.kind == AttachmentKind.voiceMessage),
      ),
      isEmpty,
    );
    await app.dispose(tester);
  });

  testWidgets('dragging_up_locks_and_enter_sends', (tester) async {
    final app = await MockApp.pump(tester);
    final channel = await _open(tester, app, 'general');

    final micFinder = find.byType(MicButton);
    expect(micFinder, findsOneWidget);

    final gesture = await tester.startGesture(tester.getCenter(micFinder));
    await tester.pump(const Duration(milliseconds: 100));

    // Drag up dy < -60
    await gesture.moveBy(const Offset(0, -70));
    await tester.pump();
    await gesture.up();
    await tester.pump();

    // Locked state shows Delete, Stop, Send icons
    expect(find.byIcon(OcIcons.delete), findsOneWidget);
    expect(find.byIcon(OcIcons.stop), findsOneWidget);
    expect(find.byIcon(OcIcons.send), findsOneWidget);

    // Press Enter to send
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await _pumpFor(tester, const Duration(milliseconds: 200));

    expect(find.byType(RecordingBar), findsNothing);
    final messages = app.read(channelMessagesProvider(channel)).all;
    expect(
      messages.last.attachments.any(
        (a) => a.kind == AttachmentKind.voiceMessage,
      ),
      isTrue,
    );
    await app.dispose(tester);
  });

  testWidgets('max_length_while_locked_opens_the_preview', (tester) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'general');

    final micFinder = find.byType(MicButton);
    final gesture = await tester.startGesture(tester.getCenter(micFinder));
    await tester.pump(const Duration(milliseconds: 100));

    // Drag up to lock
    await gesture.moveBy(const Offset(0, -70));
    await tester.pump();
    await gesture.up();
    await tester.pump();

    // Trigger max length stop
    app.repository.emitVoiceRecordStopped(VoiceStopReason.maxLength);
    await _pumpFor(tester, const Duration(milliseconds: 200));

    // In preview mode: play/pause button (playArrow icon) and Delete / Send buttons
    expect(find.byIcon(OcIcons.playArrow), findsOneWidget);
    expect(find.byIcon(OcIcons.delete), findsOneWidget);
    expect(find.byIcon(OcIcons.send), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('the_preview_plays_the_draft', (tester) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'general');

    final gesture = await tester.startGesture(
      tester.getCenter(find.byType(MicButton)),
    );
    await tester.pump(const Duration(milliseconds: 100));
    await gesture.moveBy(const Offset(0, -70));
    await tester.pump();
    await gesture.up();
    await tester.pump();
    await tester.tap(find.byTooltip('Stop'));
    await _pumpFor(tester, const Duration(milliseconds: 200));

    await tester.tap(find.byIcon(OcIcons.playArrow));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(app.read(playbackProvider).item, isA<DraftPlayback>());
    expect(find.byIcon(OcIcons.pause), findsOneWidget);

    // Deleting the draft stops it playing.
    await tester.tap(find.byTooltip('Delete'));
    await tester.pump();
    expect(app.read(playbackProvider).item, isNull);
    await app.dispose(tester);
  });

  testWidgets('a_lost_microphone_keeps_the_recording', (tester) async {
    final app = await MockApp.pump(tester);
    await _open(tester, app, 'general');

    final gesture = await tester.startGesture(
      tester.getCenter(find.byType(MicButton)),
    );
    await _pumpFor(tester, const Duration(milliseconds: 600));
    app.repository.emitVoiceRecordStopped(VoiceStopReason.deviceLost);
    await _pumpFor(tester, const Duration(milliseconds: 200));

    // The preview holds what was recorded, and the member is told why.
    expect(find.byTooltip('Send'), findsOneWidget);
    expect(find.textContaining('The microphone stopped'), findsOneWidget);
    await gesture.up();
    await tester.pump();
    expect(find.byTooltip('Send'), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('no_permission_hides_the_mic', (tester) async {
    final app = await MockApp.pump(tester);
    final channel = await _open(tester, app, 'general');

    // Deny sendVoiceMessages
    final serverData = app.read(serverProvider(channel.server)).data!;
    final currentPermissions =
        serverData.channelPermissions[channel.channel] ?? Permissions.all;
    final updatedPermissions =
        currentPermissions - Permissions.sendVoiceMessages;
    app
        .read(serverProvider(channel.server).notifier)
        .apply(
          PermissionsChanged(channel.server, serverData.serverPermissions, {
            ...serverData.channelPermissions,
            channel.channel: updatedPermissions,
          }),
        );

    await _pumpFor(tester, const Duration(milliseconds: 200));
    expect(find.byType(MicButton), findsNothing);
    await app.dispose(tester);
  });
}
