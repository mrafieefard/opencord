// Songs dropped on the music player (design 2026-10-24): their progress,
// cancel, the queue they join, Local in search and removing them, on the
// mock repository.

import 'dart:async';

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/events.dart';
import 'package:opencord/features/chat/chat_header.dart';
import 'package:opencord/features/music/music_drop.dart';
import 'package:opencord/features/music/music_panel.dart';
import 'package:opencord/features/music/music_search.dart';
import 'package:opencord/features/music/music_uploads.dart';
import 'package:opencord/features/settings/server/music/local_songs_section.dart';
import 'package:opencord/features/settings/server/permission_info.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';

import '../../support/app.dart';

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

Future<void> _settle(WidgetTester tester) =>
    _pumpFor(tester, const Duration(milliseconds: 600));

Finder _inPanel(Finder finder) =>
    find.descendant(of: find.byType(MusicPanel), matching: finder);

/// Joins General, opens its music panel; the channel it plays in.
Future<MusicChannel> _openMusic(WidgetTester tester, MockApp app) async {
  await tester.tap(
    find.descendant(
      of: find.byKey(DesktopShell.sidebarKey),
      matching: find.text('General'),
    ),
  );
  await _settle(tester);
  await tester.tap(
    find.descendant(
      of: find.byType(ChatHeader),
      matching: find.byTooltip('Music'),
    ),
  );
  await _settle(tester);
  final voice = app.read(voiceSessionProvider);
  return (server: voice.serverKey!, channel: voice.channelId!);
}

List<String> _drop(MockApp app, MusicChannel channel, List<String> paths) =>
    app.read(musicUploadsProvider(channel).notifier).drop(paths);

String? _playing(MockApp app, MusicChannel channel) =>
    app.read(musicSessionProvider(channel)).value?.nowPlaying?.track.title;

void main() {
  testWidgets('a dropped song shows its progress, then joins the queue', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    final channel = await _openMusic(tester, app);

    expect(_drop(app, channel, ['/home/me/Night Drive.mp3']), isEmpty);
    await tester.pump();
    expect(_inPanel(find.text('Night Drive.mp3')), findsOneWidget);
    expect(_inPanel(find.text('Checking…')), findsOneWidget);

    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(
      _inPanel(find.byType(LinearProgressIndicator)),
      findsOneWidget,
      reason: 'a song the server lacks is uploaded',
    );

    await _pumpFor(tester, const Duration(seconds: 2));
    expect(find.byType(MusicUploadRow), findsNothing);
    expect(_playing(app, channel), 'Night Drive');
    final track = app.read(musicSessionProvider(channel)).value!.nowPlaying!;
    expect(track.track.sourceName, 'Local');
    expect(track.track.isLocal, isTrue);
    await app.dispose(tester);
  });

  testWidgets('dropped again, a song is queued without uploading', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    final channel = await _openMusic(tester, app);
    _drop(app, channel, ['/home/me/Loop.flac']);
    await _pumpFor(tester, const Duration(seconds: 2));

    final phases = <UploadPhase>[];
    final events = app.repository.events.listen((event) {
      if (event is MusicUploadEvent) phases.add(event.phase);
    });
    _drop(app, channel, ['/home/me/Loop.flac']);
    await _pumpFor(tester, const Duration(seconds: 2));
    // Not awaited: under the test's clock the cancel never completes.
    unawaited(events.cancel());

    expect(phases, [UploadPhase.preparing, UploadPhase.done]);
    final session = app.read(musicSessionProvider(channel)).value!;
    expect(session.queue.map((item) => item.track.title), ['Loop']);
    await app.dispose(tester);
  });

  testWidgets('Cancel stops a song on its way', (tester) async {
    final app = await MockApp.pump(tester);
    final channel = await _openMusic(tester, app);
    _drop(app, channel, ['/home/me/Long.mp3']);
    await tester.pump();

    await tester.tap(_inPanel(find.byTooltip('Cancel upload')));
    await tester.pump();
    expect(find.byType(MusicUploadRow), findsNothing);
    await _pumpFor(tester, const Duration(seconds: 2));
    expect(_playing(app, channel), isNull);
    await app.dispose(tester);
  });

  testWidgets('files that are not songs are left out', (tester) async {
    final app = await MockApp.pump(tester);
    final channel = await _openMusic(tester, app);

    final refused = _drop(app, channel, ['/home/me/notes.txt', '/a/b.wav']);
    await tester.pump();
    expect(refused, ['/home/me/notes.txt']);
    expect(find.byType(MusicUploadRow), findsOneWidget);
    await _pumpFor(tester, const Duration(seconds: 2));
    await app.dispose(tester);
  });

  testWidgets('uploaded songs show as Local in search, and come off the '
      'server from their menu', (tester) async {
    final app = await MockApp.pump(tester);
    final channel = await _openMusic(tester, app);
    _drop(app, channel, ['/home/me/Sunrise Theme.ogg']);
    await _pumpFor(tester, const Duration(seconds: 2));

    await tester.enterText(
      _inPanel(find.bySemanticsLabel('Search for music')).first,
      'sunrise',
    );
    await tester.testTextInput.receiveAction(TextInputAction.search);
    await _settle(tester);
    final local = find.ancestor(
      of: _inPanel(find.text('Local')),
      matching: find.byType(MusicResultRow),
    );
    expect(local, findsOneWidget);

    await tester.tap(local, buttons: kSecondaryButton);
    await tester.pump();
    await tester.tap(
      find.descendant(
        of: find.byType(OcMenuPanel),
        matching: find.text('Remove from server'),
      ),
    );
    await _settle(tester);
    await tester.tap(
      find.descendant(
        of: find.byType(OcDialog),
        matching: find.text('Remove from server'),
      ),
    );
    await _settle(tester);

    expect(
      find.ancestor(
        of: _inPanel(find.text('Local')),
        matching: find.byType(MusicResultRow),
      ),
      findsNothing,
    );
    await app.dispose(tester);
  });

  testWidgets('administrators find every uploaded song in the music settings', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    final channel = await _openMusic(tester, app);
    _drop(app, channel, ['/home/me/Anthem.mp3']);
    await _pumpFor(tester, const Duration(seconds: 2));

    await tester.tap(
      find.descendant(
        of: find.byKey(DesktopShell.sidebarKey),
        matching: find.text('Opencord Dev'),
      ),
    );
    await _settle(tester);
    await tester.tap(find.text('Server settings'));
    await _settle(tester);
    await tester.tap(
      find
          .descendant(
            of: find.byType(SettingsDialog),
            matching: find.text('Music'),
          )
          .first,
    );
    await _settle(tester);
    await tester.scrollUntilVisible(
      find.byType(LocalSongsSection),
      300,
      scrollable: find
          .descendant(
            of: find.byType(SettingsDialog),
            matching: find.byType(Scrollable),
          )
          .first,
    );
    await _settle(tester);
    expect(
      find.descendant(
        of: find.byType(LocalSongsSection),
        matching: find.text('Anthem'),
      ),
      findsOneWidget,
    );
    await app.dispose(tester);
  });

  test('Access all uploaded songs is in the role editor', () {
    final infos = [
      for (final (_, infos) in permissionGroups)
        for (final info in infos)
          if (info.bit == Permissions.musicAllLocalSongs) info,
    ];
    expect(infos.single.label, 'Access all uploaded songs');
  });
}
