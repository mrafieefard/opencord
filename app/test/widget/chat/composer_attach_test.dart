import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/chat/composer.dart';
import 'package:opencord/features/chat/composer_state.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';

import '../../support/app.dart';
import '../../support/clipboard.dart';

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

Finder get _input => find.descendant(
  of: find.byType(Composer),
  matching: find.byType(TextField),
);

void main() {
  testWidgets('staging shows tiles and blocks send on problems', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    final channel = await _open(tester, app, 'general');
    final composer = app.read(composerProvider(channel).notifier);

    // Stage a 60 MiB file (server limit is 50 MiB)
    composer.stageFile(
      const StagedFile(id: 'big-1', name: 'giant.zip', size: 60 * 1024 * 1024),
    );
    await _pumpFor(tester, const Duration(milliseconds: 200));

    // The tile shows problem text
    expect(find.textContaining('Larger than your'), findsOneWidget);

    // Send button is disabled
    final sendBtn = tester.widget<OcIconButton>(
      find.byWidgetPredicate((w) => w is OcIconButton && w.tooltip == 'Send'),
    );
    expect(sendBtn.onPressed, isNull);

    // Remove the problem file via its X button
    await tester.tap(find.byTooltip('Remove giant.zip'));
    await _pumpFor(tester, const Duration(milliseconds: 200));

    expect(find.textContaining('Larger than your'), findsNothing);

    await app.dispose(tester);
  });

  testWidgets('an attached file uploads at once and sends without uploading '
      'again', (tester) async {
    final app = await MockApp.pump(tester);
    final channel = await _open(tester, app, 'general');
    final composer = app.read(composerProvider(channel).notifier);
    final before = app.repository.uploadsStarted;

    composer.stageFile(
      const StagedFile(id: 'early-1', name: 'notes.txt', size: 4096),
    );
    await tester.pump();
    expect(find.text('Preparing…'), findsOneWidget);
    await _pumpFor(tester, const Duration(milliseconds: 700));
    expect(find.text('Uploaded · 4.0 KB'), findsOneWidget);
    expect(app.repository.uploadsStarted, before + 1);

    await tester.tap(find.byTooltip('Send'));
    await _pumpFor(tester, const Duration(milliseconds: 300));
    final messages = app.read(channelMessagesProvider(channel)).messages;
    expect(
      messages.any((m) => m.attachments.any((a) => a.filename == 'notes.txt')),
      isTrue,
    );
    expect(app.repository.uploadsStarted, before + 1, reason: 'taken over');
    await app.dispose(tester);
  });

  testWidgets('removing an attached file drops its upload; a file with a '
      'problem never uploads', (tester) async {
    final app = await MockApp.pump(tester);
    final channel = await _open(tester, app, 'general');
    final composer = app.read(composerProvider(channel).notifier);
    final before = app.repository.uploadsStarted;

    composer
      ..stageFile(const StagedFile(id: 'drop-1', name: 'draft.txt', size: 100))
      ..stageFile(
        const StagedFile(id: 'big-2', name: 'giant.zip', size: 60 << 20),
      );
    await _pumpFor(tester, const Duration(milliseconds: 700));
    expect(app.repository.uploadsStarted, before + 1, reason: 'not giant.zip');

    await tester.tap(find.byTooltip('Remove draft.txt'));
    await _pumpFor(tester, const Duration(milliseconds: 100));
    expect(app.repository.discardedStaged, ['drop-1']);
    await app.dispose(tester);
  });

  testWidgets('pasting an image stages it', (tester) async {
    ClipboardSpy(tester);
    final app = await MockApp.pump(tester);
    final channel = await _open(tester, app, 'general');

    // Put image in mock repo's clipboard
    await app.repository.copyImage(Uint8List.fromList([1, 2, 3, 4]));

    // Focus input and press Ctrl+V
    await tester.tap(_input);
    await tester.pump();
    await tester.sendKeyDownEvent(LogicalKeyboardKey.control);
    await tester.sendKeyEvent(LogicalKeyboardKey.keyV);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.control);
    await _pumpFor(tester, const Duration(milliseconds: 300));

    // The pasted file should be staged
    final staged = app.read(composerProvider(channel)).staged;
    expect(staged, isNotEmpty);
    expect(staged.first.name, startsWith('pasted-image-'));
    expect(staged.first.name, endsWith('.png'));

    await app.dispose(tester);
  });

  testWidgets('sending shows progress then the sent message', (tester) async {
    final app = await MockApp.pump(tester);
    final channel = await _open(tester, app, 'general');
    final composer = app.read(composerProvider(channel).notifier);

    composer.stageFile(
      const StagedFile(id: 'img-1', name: 'photo.png', size: 2048),
    );
    await _pumpFor(tester, const Duration(milliseconds: 200));

    // Send button should be enabled
    await tester.tap(find.byTooltip('Send'));
    await tester.pump();

    // Progress bar or upload status should be visible
    expect(find.text('photo.png'), findsOneWidget);

    // Advance mock time for upload to finish
    await _pumpFor(tester, const Duration(milliseconds: 500));

    // Sent message appears
    final messages = app.read(channelMessagesProvider(channel)).messages;
    expect(
      messages.any((m) => m.attachments.any((a) => a.filename == 'photo.png')),
      isTrue,
    );

    await app.dispose(tester);
  });

  testWidgets('cancelling a pending upload removes the message', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    final channel = await _open(tester, app, 'general');
    final composer = app.read(composerProvider(channel).notifier);

    composer.stageFile(
      const StagedFile(id: 'cancel-1', name: 'huge.dat', size: 4096),
    );
    await _pumpFor(tester, const Duration(milliseconds: 200));

    await tester.tap(find.byTooltip('Send'));
    await tester.pump();

    // Cancel via X on the pending upload
    final cancelBtn = find.byTooltip('Cancel upload');
    if (cancelBtn.evaluate().isNotEmpty) {
      await tester.tap(cancelBtn.first);
    } else {
      // Find close icon on the pending upload
      await tester.tap(find.byIcon(OcIcons.close).first);
    }
    await _pumpFor(tester, const Duration(milliseconds: 300));

    final pending = app.read(channelMessagesProvider(channel)).pending;
    expect(
      pending.where((m) => m.uploads.any((u) => u.name == 'huge.dat')),
      isEmpty,
    );

    await app.dispose(tester);
  });
  testWidgets('a_blocked_file_names_the_blocked_extension', (tester) async {
    final app = await MockApp.pump(tester);
    final channel = await _open(tester, app, 'general');
    unawaited(
      app
          .read(repositoryProvider)
          .updateUploadSettings(_server, blockedExtensions: ['exe', 'tar.gz']),
    );
    await _pumpFor(tester, const Duration(milliseconds: 300));
    final composer = app.read(composerProvider(channel).notifier);

    composer
      ..stageFile(const StagedFile(id: 'a', name: 'setup.EXE', size: 1024))
      ..stageFile(const StagedFile(id: 'b', name: 'logs.tar.gz', size: 1024));
    await _pumpFor(tester, const Duration(milliseconds: 200));

    expect(find.text('exe files are blocked here'), findsOneWidget);
    expect(find.text('tar.gz files are blocked here'), findsOneWidget);

    await app.dispose(tester);
  });
}
