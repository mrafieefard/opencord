// The music UI (Phase 3 plan §19): the Music buttons, the right-side
// panel's modes, the now-playing card, the music panel and its states,
// on the mock repository.

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/music_session.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/channels/voice_panel.dart';
import 'package:opencord/features/chat/chat_header.dart';
import 'package:opencord/features/members/member_panel.dart';
import 'package:opencord/features/music/music_controls.dart';
import 'package:opencord/features/music/music_panel.dart';
import 'package:opencord/features/music/music_queue.dart';
import 'package:opencord/features/music/now_playing_card.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/shell/pane_switch.dart';
import 'package:opencord/features/shell/shortcuts.dart';
import 'package:opencord/features/voice/voice_chat_slot.dart';
import 'package:opencord/features/voice/voice_controls.dart';
import 'package:opencord/features/voice/voice_tile.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_theme.dart';
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

/// The right pane's own button for `Members` or `Music`.
Finder _paneButton(String tooltip) => find.descendant(
  of: find.byKey(DesktopShell.membersKey),
  matching: find.byWidgetPredicate(
    (widget) =>
        (widget is PaneSwitchButton &&
            (widget.label == tooltip ||
                (tooltip == 'Members' && widget.label == 'People'))) ||
        (widget is OcIconButton && widget.tooltip == tooltip),
  ),
);

/// Opens the right pane where it is shut, and picks `Members` or `Music`.
Future<void> _paneOn(WidgetTester tester, String tooltip) async {
  final open = find.byTooltip('Open the right pane');
  if (open.evaluate().isNotEmpty) {
    await tester.tap(open);
    await _settle(tester);
  } else if (find.byType(VoiceChatSlot).evaluate().isNotEmpty) {
    Actions.invoke(
      tester.element(find.byType(VoiceChatSlot)),
      const ToggleMembersIntent(),
    );
    await _settle(tester);
  }
  await tester.tap(_paneButton(tooltip));
  await _settle(tester);
}

/// Joins General and opens its music panel from the right pane.
Future<void> _openMusic(WidgetTester tester) async {
  await tester.tap(_inSidebar('General'));
  await _settle(tester);
  await _paneOn(tester, 'Music');
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

    // The pane's Members button switches back.
    await _paneOn(tester, 'Members');
    expect(find.byType(MemberPanel), findsOneWidget);
    expect(
      tester.getSize(find.byKey(DesktopShell.membersKey)).width,
      OcSize.memberPanelWidth,
    );
    await app.dispose(tester);
  });

  testWidgets('the progress bar seeks, from 8 px off it as well', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openMusic(tester);
    await _search(tester, 'lofi');
    await tester.tap(_inPanel(find.byTooltip('Add to queue')).first);
    await _settle(tester);

    final bar = tester.getCenter(
      find.descendant(
        of: find.byType(MusicProgress),
        matching: find.byType(Slider),
      ),
    );
    await tester.tapAt(bar + const Offset(0, -8));
    await _settle(tester);

    final session = _session(app)!;
    final length = session.nowPlaying!.track.duration.inMilliseconds;
    expect(
      session.positionAt(DateTime.now()).inMilliseconds / length,
      closeTo(.5, .05),
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
    expect(_inPanel(find.text('UP NEXT')), findsOneWidget);
    expect(find.byType(MusicQueueRow), findsOneWidget);

    // Not a participant: no tile, and the card only in the users list.
    expect(find.byType(NowPlayingCard), findsNothing);
    await _paneOn(tester, 'Members');
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

  testWidgets('the header has no Members or Music in a voice channel', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await tester.tap(_inSidebar('General'));
    await _settle(tester);

    final header = find.byType(ChatHeader);
    expect(
      find.descendant(of: header, matching: find.byTooltip('Members')),
      findsNothing,
    );
    expect(
      find.descendant(of: header, matching: find.byTooltip('Music')),
      findsNothing,
    );
    expect(
      find.descendant(of: header, matching: find.byTooltip('More')),
      findsOneWidget,
    );

    await app.dispose(tester);
  });

  testWidgets(
    'Members and Music buttons track active panel state and clicking already-selected icon leaves it selected',
    (tester) async {
      final app = await MockApp.pump(tester);
      await tester.tap(_inSidebar('General'));
      await _settle(tester);

      bool active(String tooltip) =>
          (tester.widget(_paneButton(tooltip)) as dynamic).active as bool;

      // Initially Members panel is shown and Members icon is active.
      expect(find.byType(MemberPanel), findsOneWidget);
      expect(find.byType(MusicPanel), findsNothing);
      expect(active('Members'), isTrue);
      expect(active('Music'), isFalse);

      // Clicking already-selected Members does not close or switch the panel.
      await tester.tap(_paneButton('Members'));
      await _settle(tester);
      expect(find.byType(MemberPanel), findsOneWidget);
      expect(active('Members'), isTrue);
      expect(active('Music'), isFalse);

      // Clicking Music selects Music and replaces Members.
      await tester.tap(_paneButton('Music'));
      await _settle(tester);
      expect(find.byType(MusicPanel), findsOneWidget);
      expect(find.byType(MemberPanel), findsNothing);
      expect(active('Members'), isFalse);
      expect(active('Music'), isTrue);

      // Clicking already-selected Music leaves Music selected.
      await tester.tap(_paneButton('Music'));
      await _settle(tester);
      expect(find.byType(MusicPanel), findsOneWidget);
      expect(active('Members'), isFalse);
      expect(active('Music'), isTrue);

      await app.dispose(tester);
    },
  );

  testWidgets(
    'closing the panel via close button shuts it; the pane button opens it on the mode it had',
    (tester) async {
      final app = await MockApp.pump(tester);
      await tester.tap(_inSidebar('General'));
      await _settle(tester);

      final paneButton = find.byTooltip('Open the right pane');

      // Open Music first.
      await _paneOn(tester, 'Music');
      expect(find.byType(MusicPanel), findsOneWidget);

      // Close Music via the panel's Close button.
      final musicClose = find.descendant(
        of: find.byType(MusicPanel),
        matching: find.byTooltip('Close'),
      );
      expect(musicClose, findsOneWidget);
      await tester.tap(musicClose);
      await _settle(tester);

      // Panel is closed and its buttons with it.
      expect(find.byType(MusicPanel), findsNothing);
      expect(find.byType(MemberPanel), findsNothing);
      expect(paneButton, findsOneWidget);

      // Reopening opens Music.
      await tester.tap(paneButton);
      await _settle(tester);
      expect(find.byType(MusicPanel), findsOneWidget);
      expect((tester.widget(_paneButton('Music')) as dynamic).active, isTrue);

      // Close again.
      await tester.tap(musicClose);
      await _settle(tester);
      expect(find.byType(MusicPanel), findsNothing);

      // Reopening and picking Members opens Members.
      await _paneOn(tester, 'Members');
      expect(find.byType(MemberPanel), findsOneWidget);
      expect((tester.widget(_paneButton('Members')) as dynamic).active, isTrue);

      // Close MemberPanel via its close button.
      final memberClose = find.descendant(
        of: find.byType(MemberPanel),
        matching: find.byTooltip('Close'),
      );
      expect(memberClose, findsOneWidget);
      await tester.tap(memberClose);
      await _settle(tester);
      expect(find.byType(MemberPanel), findsNothing);

      // Reopening now opens Members.
      await tester.tap(paneButton);
      await _settle(tester);
      expect(find.byType(MemberPanel), findsOneWidget);

      await app.dispose(tester);
    },
  );

  testWidgets('the header has the PaneSwitch when music is offered', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await tester.tap(_inSidebar('General'));
    await _settle(tester);

    // In MemberPanel: PaneSwitch shows.
    expect(find.byType(MemberPanel), findsOneWidget);
    expect(find.byType(PaneSwitch), findsOneWidget);

    // In MusicPanel: PaneSwitch shows.
    await _paneOn(tester, 'Music');
    expect(find.byType(MusicPanel), findsOneWidget);
    expect(find.byType(PaneSwitch), findsOneWidget);

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
    await _paneOn(tester, 'Members');

    expect(find.byType(MemberPanel), findsOneWidget);
    expect(find.byType(NowPlayingCard), findsOneWidget);
    expect((tester.widget(_paneButton('Members')) as dynamic).active, isTrue);
    expect((tester.widget(_paneButton('Music')) as dynamic).active, isFalse);

    // Tap NowPlayingCard -> opens Music.
    await tester.tap(find.byType(NowPlayingCard));
    await _settle(tester);

    expect(find.byType(MusicPanel), findsOneWidget);
    expect(find.byType(MemberPanel), findsNothing);
    expect((tester.widget(_paneButton('Music')) as dynamic).active, isTrue);
    expect((tester.widget(_paneButton('Members')) as dynamic).active, isFalse);

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
        find.descendant(
          of: voicePanel,
          matching: find.bySemanticsLabel('Music'),
        ),
        findsNothing,
      );
      expect(
        find.descendant(
          of: voicePanel,
          matching: find.bySemanticsLabel('Camera'),
        ),
        findsOneWidget,
      );
      expect(
        find.descendant(
          of: voicePanel,
          matching: find.bySemanticsLabel('Screen'),
        ),
        findsOneWidget,
      );
      expect(
        find.descendant(
          of: voicePanel,
          matching: find.bySemanticsLabel('Soundboard'),
        ),
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
        find.descendant(
          of: controlBar,
          matching: find.bySemanticsLabel('Music'),
        ),
        findsNothing,
      );
      expect(
        find.descendant(
          of: controlBar,
          matching: find.bySemanticsLabel('Mute'),
        ),
        findsOneWidget,
      );
      expect(
        find.descendant(
          of: controlBar,
          matching: find.bySemanticsLabel('Turn on camera'),
        ),
        findsOneWidget,
      );
      expect(
        find.descendant(
          of: controlBar,
          matching: find.bySemanticsLabel('Share your screen'),
        ),
        findsOneWidget,
      );
      expect(
        find.descendant(
          of: controlBar,
          matching: find.bySemanticsLabel('Soundboard'),
        ),
        findsOneWidget,
      );
      expect(
        find.descendant(
          of: controlBar,
          matching: find.bySemanticsLabel('Deafen'),
        ),
        findsOneWidget,
      );

      await app.dispose(tester);
    },
  );

  testWidgets('in a text channel the card still opens the call\'s music', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _openMusic(tester);
    await _search(tester, 'lofi');
    await tester.tap(_inPanel(find.byTooltip('Add to queue')).first);
    await _settle(tester);
    await tester.tap(_inSidebar('general'));
    await _settle(tester);
    await _paneOn(tester, 'Members');
    expect(find.byType(NowPlayingCard), findsOneWidget);

    await tester.tap(find.byType(NowPlayingCard));
    await _settle(tester);

    expect(find.byType(MusicPanel), findsOneWidget, reason: 'the card works');
    expect(find.byType(MemberPanel), findsNothing);
    await app.dispose(tester);
  });

  testWidgets('with the voice chat open, Music and Members take its place', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await tester.tap(_inSidebar('General'));
    await _settle(tester);
    await tester.tap(
      find.descendant(
        of: find.byType(VoiceControlBar),
        matching: find.byTooltip('Show chat'),
      ),
    );
    await _settle(tester);
    expect(find.byType(MemberPanel), findsNothing);

    await _paneOn(tester, 'Music');
    expect(find.byType(MusicPanel), findsOneWidget);
    expect((tester.widget(_paneButton('Music')) as dynamic).active, isTrue);

    await _paneOn(tester, 'Members');
    expect(find.byType(MemberPanel), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('in text channels the pane offers no Music', (tester) async {
    final app = await MockApp.pump(tester);
    // General text channel.
    await tester.tap(_inSidebar('general'));
    await _settle(tester);

    expect(find.byType(MemberPanel), findsOneWidget);
    expect(_paneButton('Music'), findsNothing);
    expect(
      find.descendant(
        of: find.byType(ChatHeader),
        matching: find.byTooltip('Music'),
      ),
      findsNothing,
    );

    await app.dispose(tester);
  });

  testWidgets(
    'in narrow overlay mode, the overlay header provides PaneSwitch',
    (tester) async {
      // Medium window where membersInline is false (ShellLayout.medium).
      final app = await MockApp.pump(tester, size: const Size(1000, 900));
      await tester.tap(_inSidebar('General'));
      await _settle(tester);

      // The pane button opens the drawer.
      await tester.tap(find.byTooltip('Open the right pane'));
      await _settle(tester);

      // MemberPanel is in overlay. Its header has PaneSwitch.
      expect(find.byType(MemberPanel), findsOneWidget);
      final overlayMusic = find.descendant(
        of: find.byType(MemberPanel),
        matching: find.byWidgetPredicate(
          (w) => w is PaneSwitchButton && w.label == 'Music',
        ),
      );
      expect(overlayMusic, findsOneWidget);

      // Tap overlay Music button -> switches to MusicPanel in the overlay.
      await tester.tap(overlayMusic);
      await _settle(tester);

      expect(find.byType(MusicPanel), findsOneWidget);
      expect(find.byType(MemberPanel), findsNothing);

      // Overlay MusicPanel has PaneSwitch with People button.
      final overlayMembers = find.descendant(
        of: find.byType(MusicPanel),
        matching: find.byWidgetPredicate(
          (w) => w is PaneSwitchButton && w.label == 'People',
        ),
      );
      expect(overlayMembers, findsOneWidget);

      // Tap overlay People button -> switches back to MemberPanel.
      await tester.tap(overlayMembers);
      await _settle(tester);
      expect(find.byType(MemberPanel), findsOneWidget);

      // Close the overlay.
      await tester.tap(
        find.descendant(
          of: find.byType(MemberPanel),
          matching: find.byTooltip('Close'),
        ),
      );
      await _settle(tester);
      expect(find.byType(MemberPanel), findsNothing);

      await app.dispose(tester);
    },
  );

  group('MusicVolumeBar (§3, Task 2)', () {
    testWidgets('volume_needs_no_popover', (tester) async {
      final app = await MockApp.pump(tester);
      await _openMusic(tester);

      expect(find.byType(MusicVolumeBar), findsOneWidget);
      expect(find.byType(MusicVolumeControls), findsNothing);

      final sliderFinder = find.descendant(
        of: find.byType(MusicVolumeBar),
        matching: find.byType(Slider),
      );
      expect(sliderFinder, findsOneWidget);

      final initialVolume = app.read(musicListeningProvider).volume;
      expect(initialVolume, 100);

      // Drag the slider
      await tester.drag(sliderFinder, const Offset(60, 0));
      await tester.pump();

      final newVolume = app.read(musicListeningProvider).volume;
      expect(newVolume, isNot(100));
      expect(newVolume, greaterThan(100));

      // No route was pushed
      expect(find.byType(MusicVolumeControls), findsNothing);

      await app.dispose(tester);
    });

    testWidgets('volume_row_numbers', (tester) async {
      final app = await MockApp.pump(tester);
      await _openMusic(tester);

      final barFinder = find.byType(MusicVolumeBar);
      expect(barFinder, findsOneWidget);

      // Height is 32
      final barSize = tester.getSize(barFinder);
      expect(barSize.height, 32.0);

      // Children: Speaker icon button, gap 8, Slider, gap 8, Value text
      final speakerFinder = find
          .descendant(of: barFinder, matching: find.byType(Tooltip))
          .first;
      final sliderFinder = find.descendant(
        of: barFinder,
        matching: find.byType(Slider),
      );
      final valueBoxFinder = find.descendant(
        of: barFinder,
        matching: find.byWidgetPredicate(
          (w) => w is SizedBox && w.width == 36.0,
        ),
      );
      expect(valueBoxFinder, findsOneWidget);

      final speakerRect = tester.getRect(speakerFinder);
      final sliderRect = tester.getRect(sliderFinder);
      final valueRect = tester.getRect(valueBoxFinder);

      expect(sliderRect.left - speakerRect.right, 8.0);
      expect(valueRect.left - sliderRect.right, 8.0);
      expect(valueRect.width, 36.0);

      // Value text style: 11, tabular, textMuted, right-aligned in 36
      final textFinder = find.descendant(
        of: valueBoxFinder,
        matching: find.byType(Text),
      );
      expect(textFinder, findsOneWidget);
      final textWidget = tester.widget<Text>(textFinder);
      expect(textWidget.textAlign, TextAlign.right);
      expect(textWidget.style?.fontSize, 11.0);
      expect(textWidget.style?.color, tester.element(barFinder).oc.textMuted);
      expect(
        textWidget.style?.fontFeatures?.contains(
          const FontFeature.tabularFigures(),
        ),
        isTrue,
      );

      await app.dispose(tester);
    });

    testWidgets('speaker_mutes_and_unmutes', (tester) async {
      final app = await MockApp.pump(tester);
      await _openMusic(tester);

      final barFinder = find.byType(MusicVolumeBar);
      final speakerFinder = find
          .descendant(of: barFinder, matching: find.byType(Tooltip))
          .first;
      final valueTextFinder = find.descendant(
        of: barFinder,
        matching: find.byType(Text),
      );

      // 1. Initial unmuted: volumeUp, 'Mute music', '100%'
      expect(
        find.descendant(
          of: speakerFinder,
          matching: find.byIcon(OcIcons.volumeUp),
        ),
        findsOneWidget,
      );
      expect(tester.widget<Tooltip>(speakerFinder).message, 'Mute music');
      expect(tester.widget<Text>(valueTextFinder).data, '100%');

      // Click 1: mute -> volumeOff, 'Unmute music', 'Muted'
      await tester.tap(speakerFinder);
      await tester.pump();
      expect(
        find.descendant(
          of: speakerFinder,
          matching: find.byIcon(OcIcons.volumeOff),
        ),
        findsOneWidget,
      );
      expect(tester.widget<Tooltip>(speakerFinder).message, 'Unmute music');
      expect(tester.widget<Text>(valueTextFinder).data, 'Muted');

      // Click 2: unmute -> volumeUp, 'Mute music', '100%'
      await tester.tap(speakerFinder);
      await tester.pump();
      expect(
        find.descendant(
          of: speakerFinder,
          matching: find.byIcon(OcIcons.volumeUp),
        ),
        findsOneWidget,
      );
      expect(tester.widget<Tooltip>(speakerFinder).message, 'Mute music');
      expect(tester.widget<Text>(valueTextFinder).data, '100%');

      // Click 3: mute again -> volumeOff, 'Unmute music', 'Muted'
      await tester.tap(speakerFinder);
      await tester.pump();
      expect(
        find.descendant(
          of: speakerFinder,
          matching: find.byIcon(OcIcons.volumeOff),
        ),
        findsOneWidget,
      );
      expect(tester.widget<Tooltip>(speakerFinder).message, 'Unmute music');
      expect(tester.widget<Text>(valueTextFinder).data, 'Muted');

      // Click 4: unmute again -> volumeUp, 'Mute music', '100%'
      await tester.tap(speakerFinder);
      await tester.pump();
      expect(
        find.descendant(
          of: speakerFinder,
          matching: find.byIcon(OcIcons.volumeUp),
        ),
        findsOneWidget,
      );
      expect(tester.widget<Tooltip>(speakerFinder).message, 'Mute music');
      expect(tester.widget<Text>(valueTextFinder).data, '100%');

      await app.dispose(tester);
    });

    testWidgets('dragging_while_muted_unmutes', (tester) async {
      final app = await MockApp.pump(tester);
      await _openMusic(tester);

      final barFinder = find.byType(MusicVolumeBar);
      final speakerFinder = find
          .descendant(of: barFinder, matching: find.byType(Tooltip))
          .first;
      final sliderFinder = find.descendant(
        of: barFinder,
        matching: find.byType(Slider),
      );
      final valueTextFinder = find.descendant(
        of: barFinder,
        matching: find.byType(Text),
      );

      // Mute first
      await tester.tap(speakerFinder);
      await tester.pump();
      expect(app.read(musicListeningProvider).muted, isTrue);
      expect(tester.widget<Text>(valueTextFinder).data, 'Muted');

      // Drag the slider
      await tester.drag(sliderFinder, const Offset(-30, 0));
      await tester.pump();

      // Dragging unmutes
      expect(app.read(musicListeningProvider).muted, isFalse);
      final vol = app.read(musicListeningProvider).volume;
      expect(tester.widget<Text>(valueTextFinder).data, '$vol%');
      expect(
        find.descendant(
          of: speakerFinder,
          matching: find.byIcon(OcIcons.volumeUp),
        ),
        findsOneWidget,
      );
      expect(tester.widget<Tooltip>(speakerFinder).message, 'Mute music');

      await app.dispose(tester);
    });

    testWidgets('the_row_follows_the_hotkey', (tester) async {
      final app = await MockApp.pump(tester);
      await _openMusic(tester);

      final barFinder = find.byType(MusicVolumeBar);
      final speakerFinder = find
          .descendant(of: barFinder, matching: find.byType(Tooltip))
          .first;
      final valueTextFinder = find.descendant(
        of: barFinder,
        matching: find.byType(Text),
      );

      // Change from outside to 75%
      app.container
          .read(musicListeningProvider.notifier)
          .update((l) => l.copyWith(volume: 75, muted: false));
      await tester.pump();
      expect(tester.widget<Text>(valueTextFinder).data, '75%');
      expect(
        find.descendant(
          of: speakerFinder,
          matching: find.byIcon(OcIcons.volumeUp),
        ),
        findsOneWidget,
      );
      expect(tester.widget<Tooltip>(speakerFinder).message, 'Mute music');

      // Change from outside to muted
      app.container
          .read(musicListeningProvider.notifier)
          .update((l) => l.copyWith(muted: true));
      await tester.pump();
      expect(tester.widget<Text>(valueTextFinder).data, 'Muted');
      expect(
        find.descendant(
          of: speakerFinder,
          matching: find.byIcon(OcIcons.volumeOff),
        ),
        findsOneWidget,
      );
      expect(tester.widget<Tooltip>(speakerFinder).message, 'Unmute music');

      // Change from outside to 0 volume and unmuted: icon is volumeOff, tooltip is 'Mute music'
      app.container
          .read(musicListeningProvider.notifier)
          .update((l) => l.copyWith(volume: 0, muted: false));
      await tester.pump();
      expect(tester.widget<Text>(valueTextFinder).data, '0%');
      expect(
        find.descendant(
          of: speakerFinder,
          matching: find.byIcon(OcIcons.volumeOff),
        ),
        findsOneWidget,
      );
      expect(tester.widget<Tooltip>(speakerFinder).message, 'Mute music');

      await app.dispose(tester);
    });

    testWidgets('it_fits_the_narrowest_pane', (tester) async {
      // 300 wide pane: pump MusicVolumeBar in a 300 wide container
      final app = await MockApp.pump(tester);
      await tester.pumpWidget(
        UncontrolledProviderScope(
          container: app.container,
          child: MaterialApp(
            debugShowCheckedModeBanner: false,
            theme: buildTheme(OcColors.dark),
            home: const Scaffold(
              body: SizedBox(
                width: 300,
                child: Padding(
                  padding: EdgeInsets.symmetric(horizontal: 12),
                  child: MusicVolumeBar(),
                ),
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(tester.takeException(), isNull);
      expect(find.byType(MusicVolumeBar), findsOneWidget);
      final size = tester.getSize(find.byType(MusicVolumeBar));
      expect(size.width, 276.0); // 300 - 24
      expect(size.height, 32.0);

      await app.dispose(tester);
    });

    testWidgets('the_transport_stays_centred', (tester) async {
      final app = await MockApp.pump(tester);
      await _openMusic(tester);

      final panelFinder = find.byType(MusicPanel);
      final playFinder = find.descendant(
        of: find.byType(MusicTransport),
        matching: find.byIcon(OcIcons.playArrow),
      );
      expect(playFinder, findsOneWidget);

      final panelCenter = tester.getCenter(panelFinder);
      final playCenter = tester.getCenter(playFinder);

      expect((playCenter.dx - panelCenter.dx).abs(), lessThan(1.0));

      await app.dispose(tester);
    });

    testWidgets('the_cards_menu_keeps_its_controls', (tester) async {
      final app = await MockApp.pump(tester);
      await _openMusic(tester);
      await _search(tester, 'lofi');
      await tester.tap(_inPanel(find.byTooltip('Add to queue')).first);
      await _settle(tester);

      // Switch to text channel so NowPlayingCard shows in member list
      await tester.tap(_inSidebar('general'));
      await _settle(tester);
      await _paneOn(tester, 'Members');

      final cardFinder = find.byType(NowPlayingCard);
      expect(cardFinder, findsOneWidget);

      // Right-click the card
      await tester.tap(cardFinder, buttons: kSecondaryMouseButton);
      await tester.pumpAndSettle();

      // MusicVolumeControls still opens from the now-playing card's right click
      expect(find.byType(MusicVolumeControls), findsOneWidget);

      await app.dispose(tester);
    });
  });
}
