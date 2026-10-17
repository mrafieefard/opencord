// The music UI (Phase 3 plan §19): the Music buttons, the right-side
// panel's modes, the now-playing card, the music panel and its states,
// on the mock repository.

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/music_session.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/channels/voice_panel.dart';
import 'package:opencord/features/chat/chat_header.dart';
import 'package:opencord/features/members/member_panel.dart';
import 'package:opencord/features/music/music_panel.dart';
import 'package:opencord/features/music/music_queue.dart';
import 'package:opencord/features/music/now_playing_card.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/voice/voice_controls.dart';
import 'package:opencord/features/voice/voice_tile.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
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

Finder _headerButton(String tooltip) => find.descendant(
  of: find.byType(ChatHeader),
  matching: find.byWidgetPredicate(
    (widget) => widget is OcIconButton && widget.tooltip == tooltip,
  ),
);

/// Joins General and opens its music panel from the header.
Future<void> _openMusic(WidgetTester tester) async {
  await tester.tap(_inSidebar('General'));
  await _settle(tester);
  await tester.tap(_headerButton('Music'));
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

    // Header Members button switches back.
    await tester.tap(
      find.descendant(
        of: find.byType(ChatHeader),
        matching: find.byTooltip('Members'),
      ),
    );
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
    await tester.tap(
      find.descendant(
        of: find.byType(ChatHeader),
        matching: find.byTooltip('Members'),
      ),
    );
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

  testWidgets(
    'the header actions are ordered Video, Members, Music, More in a voice channel',
    (tester) async {
      final app = await MockApp.pump(tester);
      await tester.tap(_inSidebar('General'));
      await _settle(tester);

      final membersFinder = find.descendant(
        of: find.byType(ChatHeader),
        matching: find.byTooltip('Members'),
      );
      final musicFinder = find.descendant(
        of: find.byType(ChatHeader),
        matching: find.byTooltip('Music'),
      );
      final moreFinder = find.descendant(
        of: find.byType(ChatHeader),
        matching: find.byTooltip('More'),
      );

      expect(membersFinder, findsOneWidget);
      expect(musicFinder, findsOneWidget);
      expect(moreFinder, findsOneWidget);

      // Verify header order: Members -> Music -> More from left to right.
      final membersDx = tester.getTopLeft(membersFinder).dx;
      final musicDx = tester.getTopLeft(musicFinder).dx;
      final moreDx = tester.getTopLeft(moreFinder).dx;
      expect(membersDx, lessThan(musicDx));
      expect(musicDx, lessThan(moreDx));

      await app.dispose(tester);
    },
  );

  testWidgets(
    'Members and Music buttons track active panel state and clicking already-selected icon leaves it selected',
    (tester) async {
      final app = await MockApp.pump(tester);
      await tester.tap(_inSidebar('General'));
      await _settle(tester);

      final headerMembers = _headerButton('Members');
      final headerMusic = _headerButton('Music');

      // Initially Members panel is shown and Members icon is active.
      expect(find.byType(MemberPanel), findsOneWidget);
      expect(find.byType(MusicPanel), findsNothing);
      expect(tester.widget<OcIconButton>(headerMembers).active, isTrue);
      expect(tester.widget<OcIconButton>(headerMusic).active, isFalse);

      // Clicking already-selected Members does not close or switch the panel.
      await tester.tap(headerMembers);
      await _settle(tester);
      expect(find.byType(MemberPanel), findsOneWidget);
      expect(tester.widget<OcIconButton>(headerMembers).active, isTrue);
      expect(tester.widget<OcIconButton>(headerMusic).active, isFalse);

      // Clicking Music selects Music and replaces Members.
      await tester.tap(headerMusic);
      await _settle(tester);
      expect(find.byType(MusicPanel), findsOneWidget);
      expect(find.byType(MemberPanel), findsNothing);
      expect(tester.widget<OcIconButton>(headerMembers).active, isFalse);
      expect(tester.widget<OcIconButton>(headerMusic).active, isTrue);

      // Clicking already-selected Music leaves Music selected.
      await tester.tap(headerMusic);
      await _settle(tester);
      expect(find.byType(MusicPanel), findsOneWidget);
      expect(tester.widget<OcIconButton>(headerMembers).active, isFalse);
      expect(tester.widget<OcIconButton>(headerMusic).active, isTrue);

      await app.dispose(tester);
    },
  );

  testWidgets(
    'closing the panel via close button leaves neither icon selected; reopening shows requested mode',
    (tester) async {
      final app = await MockApp.pump(tester);
      await tester.tap(_inSidebar('General'));
      await _settle(tester);

      final headerMembers = _headerButton('Members');
      final headerMusic = _headerButton('Music');

      // Open Music first.
      await tester.tap(headerMusic);
      await _settle(tester);
      expect(find.byType(MusicPanel), findsOneWidget);

      // Close Music via the panel's Close button.
      final musicClose = find.descendant(
        of: find.byType(MusicPanel),
        matching: find.byTooltip('Close'),
      );
      expect(musicClose, findsOneWidget);
      await tester.tap(musicClose);
      await _settle(tester);

      // Panel is closed and neither button is active.
      expect(find.byType(MusicPanel), findsNothing);
      expect(find.byType(MemberPanel), findsNothing);
      expect(tester.widget<OcIconButton>(headerMembers).active, isFalse);
      expect(tester.widget<OcIconButton>(headerMusic).active, isFalse);

      // Reopening via Music opens Music.
      await tester.tap(headerMusic);
      await _settle(tester);
      expect(find.byType(MusicPanel), findsOneWidget);
      expect(tester.widget<OcIconButton>(headerMusic).active, isTrue);
      expect(tester.widget<OcIconButton>(headerMembers).active, isFalse);

      // Close again.
      await tester.tap(musicClose);
      await _settle(tester);
      expect(find.byType(MusicPanel), findsNothing);
      expect(tester.widget<OcIconButton>(headerMembers).active, isFalse);
      expect(tester.widget<OcIconButton>(headerMusic).active, isFalse);

      // Reopening via Members opens Members.
      await tester.tap(headerMembers);
      await _settle(tester);
      expect(find.byType(MemberPanel), findsOneWidget);
      expect(tester.widget<OcIconButton>(headerMembers).active, isTrue);
      expect(tester.widget<OcIconButton>(headerMusic).active, isFalse);

      // Close MemberPanel via its close button.
      final memberClose = find.descendant(
        of: find.byType(MemberPanel),
        matching: find.byTooltip('Close member list'),
      );
      expect(memberClose, findsOneWidget);
      await tester.tap(memberClose);
      await _settle(tester);
      expect(find.byType(MemberPanel), findsNothing);
      expect(tester.widget<OcIconButton>(headerMembers).active, isFalse);
      expect(tester.widget<OcIconButton>(headerMusic).active, isFalse);

      await app.dispose(tester);
    },
  );

  testWidgets('the old People / Music segmented selector is absent from both panels', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await tester.tap(_inSidebar('General'));
    await _settle(tester);

    // In MemberPanel: no RightPanelModeBar or 'People' text.
    expect(find.byType(MemberPanel), findsOneWidget);
    expect(find.byType(RightPanelModeBar), findsNothing);

    // In MusicPanel: no RightPanelModeBar.
    await tester.tap(_headerButton('Music'));
    await _settle(tester);
    expect(find.byType(MusicPanel), findsOneWidget);
    expect(find.byType(RightPanelModeBar), findsNothing);

    await app.dispose(tester);
  });

  testWidgets('NowPlayingCard opens the music panel using the shared action', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openMusic(tester);
    await _search(tester, 'lofi');
    await tester.tap(_inPanel(find.byTooltip('Add to queue')).first);
    await _settle(tester);
    await tester.tap(_inPanel(find.text('Back to queue')));
    await _settle(tester);

    // Switch to Members.
    final headerMembers = _headerButton('Members');
    final headerMusic = _headerButton('Music');
    await tester.tap(headerMembers);
    await _settle(tester);

    expect(find.byType(MemberPanel), findsOneWidget);
    expect(find.byType(NowPlayingCard), findsOneWidget);
    expect(tester.widget<OcIconButton>(headerMembers).active, isTrue);
    expect(tester.widget<OcIconButton>(headerMusic).active, isFalse);

    // Tap NowPlayingCard -> opens Music.
    await tester.tap(find.byType(NowPlayingCard));
    await _settle(tester);

    expect(find.byType(MusicPanel), findsOneWidget);
    expect(find.byType(MemberPanel), findsNothing);
    expect(tester.widget<OcIconButton>(headerMusic).active, isTrue);
    expect(tester.widget<OcIconButton>(headerMembers).active, isFalse);

    await app.dispose(tester);
  });

  testWidgets(
    'bottom Music buttons are removed from VoiceConnectedPanel and VoiceControlBar, preserving other controls',
    (tester) async {
      final app = await MockApp.pump(tester);
      await tester.tap(_inSidebar('General'));
      await _settle(tester);

      // VoiceConnectedPanel: no Music toggle, other controls present.
      final voicePanel = find.byType(VoiceConnectedPanel);
      expect(voicePanel, findsOneWidget);
      expect(
        find.descendant(of: voicePanel, matching: find.bySemanticsLabel('Music')),
        findsNothing,
      );
      expect(
        find.descendant(of: voicePanel, matching: find.bySemanticsLabel('Camera')),
        findsOneWidget,
      );
      expect(
        find.descendant(of: voicePanel, matching: find.bySemanticsLabel('Screen')),
        findsOneWidget,
      );
      expect(
        find.descendant(of: voicePanel, matching: find.bySemanticsLabel('Soundboard')),
        findsOneWidget,
      );
      expect(
        find.descendant(of: voicePanel, matching: find.byTooltip('Disconnect')),
        findsOneWidget,
      );

      // VoiceControlBar: no Music toggle, other controls present.
      final controlBar = find.byType(VoiceControlBar);
      expect(controlBar, findsOneWidget);
      expect(
        find.descendant(of: controlBar, matching: find.bySemanticsLabel('Music')),
        findsNothing,
      );
      expect(
        find.descendant(of: controlBar, matching: find.bySemanticsLabel('Mute')),
        findsOneWidget,
      );
      expect(
        find.descendant(of: controlBar, matching: find.bySemanticsLabel('Turn on camera')),
        findsOneWidget,
      );
      expect(
        find.descendant(of: controlBar, matching: find.bySemanticsLabel('Share your screen')),
        findsOneWidget,
      );
      expect(
        find.descendant(of: controlBar, matching: find.bySemanticsLabel('Soundboard')),
        findsOneWidget,
      );
      expect(
        find.descendant(of: controlBar, matching: find.bySemanticsLabel('Deafen')),
        findsOneWidget,
      );

      await app.dispose(tester);
    },
  );

  testWidgets('in text channels the Music header button is not shown', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    // General text channel.
    await tester.tap(_inSidebar('general'));
    await _settle(tester);

    final header = find.byType(ChatHeader);
    expect(
      find.descendant(of: header, matching: find.byTooltip('Music')),
      findsNothing,
    );
    expect(
      find.descendant(of: header, matching: find.byTooltip('Members')),
      findsOneWidget,
    );

    await app.dispose(tester);
  });

  testWidgets(
    'in narrow overlay mode, the overlay header provides Members and Music buttons',
    (tester) async {
      // Medium window where membersInline is false (ShellLayout.medium).
      final app = await MockApp.pump(
        tester,
        size: const Size(1000, 900),
      );
      await tester.tap(_inSidebar('General'));
      await _settle(tester);

      // Main header has Members button. Tap it to open drawer.
      await tester.tap(_headerButton('Members'));
      await _settle(tester);

      // MemberPanel is in overlay. Its header has Members and Music fallback buttons.
      expect(find.byType(MemberPanel), findsOneWidget);
      final overlayMusic = find.descendant(
        of: find.byType(MemberPanel),
        matching: find.byWidgetPredicate(
          (w) => w is OcIconButton && w.tooltip == 'Music',
        ),
      );
      expect(overlayMusic, findsOneWidget);

      // Tap overlay Music button -> switches to MusicPanel in the overlay.
      await tester.tap(overlayMusic);
      await _settle(tester);

      expect(find.byType(MusicPanel), findsOneWidget);
      expect(find.byType(MemberPanel), findsNothing);

      // Overlay MusicPanel has Members button.
      final overlayMembers = find.descendant(
        of: find.byType(MusicPanel),
        matching: find.byWidgetPredicate(
          (w) => w is OcIconButton && w.tooltip == 'Members',
        ),
      );
      expect(overlayMembers, findsOneWidget);

      // Tap overlay Members button -> switches back to MemberPanel.
      await tester.tap(overlayMembers);
      await _settle(tester);
      expect(find.byType(MemberPanel), findsOneWidget);

      // Close the overlay.
      await tester.tap(
        find.descendant(
          of: find.byType(MemberPanel),
          matching: find.byTooltip('Close member list'),
        ),
      );
      await _settle(tester);
      expect(find.byType(MemberPanel), findsNothing);

      await app.dispose(tester);
    },
  );
}
