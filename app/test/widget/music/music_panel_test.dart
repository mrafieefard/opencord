// The music UI (Phase 3 plan §19): the Music buttons, the right-side
// panel's modes, the now-playing card, the music panel and its states,
// on the mock repository.

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/music_session.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/channels/voice_panel.dart';
import 'package:opencord/features/members/member_panel.dart';
import 'package:opencord/features/music/music_panel.dart';
import 'package:opencord/features/music/music_queue.dart';
import 'package:opencord/features/music/now_playing_card.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/voice/voice_tile.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
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

Finder _inSidebar(String text) => find.descendant(
  of: find.byKey(DesktopShell.sidebarKey),
  matching: find.text(text),
);

Finder _inPanel(Finder finder) =>
    find.descendant(of: find.byType(MusicPanel), matching: finder);

Finder _inMenu(Finder finder) =>
    find.descendant(of: find.byType(OcMenuPanel), matching: finder);

/// Joins General and opens its music panel from the voice panel.
Future<void> _openMusic(WidgetTester tester) async {
  await tester.tap(_inSidebar('General'));
  await _settle(tester);
  await tester.tap(
    find.descendant(
      of: find.byType(VoiceConnectedPanel),
      matching: find.bySemanticsLabel('Music'),
    ),
  );
  await _settle(tester);
}

Future<void> _search(WidgetTester tester, String query) async {
  await tester.enterText(
    _inPanel(find.bySemanticsLabel('Search for music')).first,
    query,
  );
  await tester.testTextInput.receiveAction(TextInputAction.search);
  await _settle(tester);
}

MusicSession? _session(MockApp app) {
  final voice = app.read(voiceSessionProvider);
  return app
      .read(
        musicSessionProvider((
          server: voice.serverKey!,
          channel: voice.channelId!,
        )),
      )
      .value;
}

void main() {
  testWidgets('the Music button swaps the users list for the music panel', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openMusic(tester);

    expect(find.byType(MusicPanel), findsOneWidget);
    expect(find.byType(MemberPanel), findsNothing);
    expect(
      tester.getSize(find.byKey(DesktopShell.membersKey)).width,
      OcSize.musicPanelWidth,
    );
    expect(_inPanel(find.text('Nothing playing')), findsOneWidget);

    // People · Music switches back.
    await tester.tap(_inPanel(find.text('People')));
    await _settle(tester);
    expect(find.byType(MemberPanel), findsOneWidget);
    expect(
      tester.getSize(find.byKey(DesktopShell.membersKey)).width,
      OcSize.memberPanelWidth,
    );
    await app.dispose(tester);
  });

  testWidgets('a search adds tracks; the card shows only in the users list', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openMusic(tester);
    await _search(tester, 'lofi');
    expect(_inPanel(find.text('lofi 1')), findsOneWidget);

    await tester.tap(_inPanel(find.byTooltip('Add to queue')).first);
    await _settle(tester);
    await tester.tap(_inPanel(find.byTooltip('Add to queue')).at(1));
    await _settle(tester);
    await tester.tap(_inPanel(find.text('Back to queue')));
    await _settle(tester);

    final session = _session(app)!;
    expect(session.nowPlaying?.track.title, 'lofi 1');
    expect(session.queue.map((item) => item.track.title), ['lofi 2']);
    expect(_inPanel(find.text('Up next')), findsOneWidget);
    expect(find.byType(MusicQueueRow), findsOneWidget);

    // Not a participant: no tile, and the card only in the users list.
    expect(find.byType(NowPlayingCard), findsNothing);
    await tester.tap(_inPanel(find.text('People')));
    await _settle(tester);
    expect(
      find.descendant(
        of: find.byType(MemberPanel),
        matching: find.byType(NowPlayingCard),
      ),
      findsOneWidget,
    );
    expect(
      find.descendant(
        of: find.byType(NowPlayingCard),
        matching: find.text('lofi 1'),
      ),
      findsOneWidget,
    );
    expect(
      find.descendant(
        of: find.byType(VoiceTileView),
        matching: find.textContaining('lofi'),
      ),
      findsNothing,
    );
    // The card opens the music panel again.
    await tester.tap(find.byType(NowPlayingCard));
    await _settle(tester);
    expect(find.byType(MusicPanel), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('play, pause, repeat and shuffle are shared controls', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openMusic(tester);
    await _search(tester, 'lofi');
    await tester.tap(
      _inPanel(find.text('Add all')).hitTestable().evaluate().isEmpty
          ? _inPanel(find.byTooltip('Add to queue')).first
          : _inPanel(find.text('Add all')),
    );
    await _settle(tester);
    await tester.tap(_inPanel(find.text('Back to queue')));
    await _settle(tester);

    await tester.tap(_inPanel(find.bySemanticsLabel('Pause')));
    await _settle(tester);
    expect(_session(app)!.state, MusicPlayState.paused);
    await tester.tap(_inPanel(find.bySemanticsLabel('Play')));
    await _settle(tester);
    expect(_session(app)!.state, MusicPlayState.playing);

    final repeat = _inPanel(find.bySemanticsLabel(RegExp('^Repeat')));
    await tester.tap(repeat);
    await _settle(tester);
    expect(_session(app)!.repeat, MusicRepeat.queue);
    await tester.tap(repeat);
    await _settle(tester);
    expect(_session(app)!.repeat, MusicRepeat.track);
    await tester.tap(repeat);
    await _settle(tester);
    expect(_session(app)!.repeat, MusicRepeat.off);

    await tester.tap(_inPanel(find.bySemanticsLabel(RegExp('^Shuffle'))));
    await _settle(tester);
    expect(_session(app)!.shuffle, isTrue);
    await app.dispose(tester);
  });

  testWidgets('the queue row menu moves tracks, sent as "before this item"', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openMusic(tester);
    await _search(tester, 'mix');
    for (var n = 0; n < 3; n++) {
      await tester.tap(_inPanel(find.byTooltip('Add to queue')).at(n));
      await _settle(tester);
    }
    await tester.tap(_inPanel(find.text('Back to queue')));
    await _settle(tester);
    expect(_session(app)!.queue.map((i) => i.track.title), ['mix 2', 'mix 3']);

    await tester.tap(
      find
          .descendant(
            of: find.byType(MusicQueueRow),
            matching: find.byTooltip('More'),
          )
          .last,
    );
    await _settle(tester);
    await tester.tap(_inMenu(find.text('Play next')));
    await _settle(tester);
    expect(_session(app)!.queue.map((i) => i.track.title), ['mix 3', 'mix 2']);

    await tester.tap(
      find
          .descendant(
            of: find.byType(MusicQueueRow),
            matching: find.byTooltip('More'),
          )
          .last,
    );
    await _settle(tester);
    await tester.tap(_inMenu(find.text('Remove')));
    await _settle(tester);
    expect(_session(app)!.queue.map((i) => i.track.title), ['mix 3']);
    await app.dispose(tester);
  });

  testWidgets('recently played tracks can be added again', (tester) async {
    final app = await MockApp.pump(tester);
    await _openMusic(tester);
    await _search(tester, 'song');
    await tester.tap(_inPanel(find.byTooltip('Add to queue')).first);
    await _settle(tester);
    await tester.tap(_inPanel(find.text('Back to queue')));
    await _settle(tester);
    await tester.tap(_inPanel(find.bySemanticsLabel('Next')));
    await _settle(tester);
    expect(_session(app)!.nowPlaying, isNull);

    await tester.tap(_inPanel(find.bySemanticsLabel('Show recently played')));
    await _settle(tester);
    await tester.tap(_inPanel(find.text('Add again')));
    await _settle(tester);
    expect(_session(app)!.nowPlaying?.track.title, 'song 1');
    await app.dispose(tester);
  });

  testWidgets('outside the channel the panel is read-only, with Join', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    // A server where this member doesn't manage music.
    const server = 'rust-berlin.example:7710';
    final voice = app
        .read(serverProvider(server))
        .data!
        .channels
        .values
        .firstWhere((channel) => channel.kind == ChannelKind.voice);
    app.read(navigationProvider.notifier).openChannel(server, voice.id);
    await _settle(tester);
    app.read(rightPanelModeProvider.notifier).set(RightPanelMode.music);
    await _settle(tester);

    expect(
      _inPanel(find.text('Join the voice channel to control music')),
      findsOneWidget,
    );
    expect(_inPanel(find.text('Join')), findsOneWidget);
    expect(
      tester
          .widget<TextField>(
            _inPanel(
              find.descendant(
                of: find.bySemanticsLabel('Search for music'),
                matching: find.byType(TextField),
              ),
            ),
          )
          .enabled,
      isFalse,
    );
    await app.dispose(tester);
  });

  testWidgets('without music set up, only who manages the server sees it', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    app.repository.debugSetMusicConfigured(
      app.read(serverListProvider).first.key,
      configured: false,
    );
    await _openMusic(tester);

    // The owner manages the server: Set up music.
    expect(_inPanel(find.text('Set up music')), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('your music volume is kept and given to voice media', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    app
        .read(musicListeningProvider.notifier)
        .update((l) => l.copyWith(volume: 150, duckWhileTalking: true));
    await tester.pump();
    expect(app.repository.musicListening.volume, 150);
    expect(app.repository.musicListening.duckWhileTalking, isTrue);
    expect(app.store.read(musicListeningKey), contains('150'));
    await app.dispose(tester);
  });
}
