import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/video.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/model/stream.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/features/channels/voice_panel.dart';
import 'package:opencord/features/chat/chat_header.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/voice/voice_controls.dart';
import 'package:opencord/features/voice/voice_tile.dart';
import 'package:opencord/features/voice/voice_view.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

import '../../support/app.dart';

const _dev = 'opencord.example:7710';
const _berlin = 'rust-berlin.example:7710';
const _mira = 1002, _priya = 1005, _tomas = 1006;

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

Finder _inView(Finder finder) =>
    find.descendant(of: find.byType(VoiceView), matching: finder);

Finder _control(String label) => find.descendant(
  of: find.byType(VoiceControlBar),
  matching: find.bySemanticsLabel(label),
);

Finder _tileOf(int userId, {bool screen = false}) => find.byWidgetPredicate(
  (widget) =>
      widget is VoiceTileView &&
      widget.userId == userId &&
      widget.screen == screen,
);

int _voiceChannel(MockApp app) => app
    .read(serverProvider(_dev))
    .data!
    .channels
    .values
    .firstWhere((channel) => channel.name == 'General')
    .id;

/// Clicks the General voice channel, which opens and joins it.
Future<void> _joinGeneral(WidgetTester tester) async {
  await tester.tap(
    find.descendant(
      of: find.byKey(DesktopShell.sidebarKey),
      matching: find.text('General'),
    ),
  );
  await _pumpFor(tester, const Duration(milliseconds: 300));
}

void main() {
  testWidgets('a joined voice channel shows a tile per person and screen', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _joinGeneral(tester);

    // Mira, Tomás (and his screen), Priya, Jonas, and you.
    expect(_inView(find.byType(VoiceTileView)), findsNWidgets(6));
    expect(_inView(find.text('LIVE')), findsOneWidget);
    expect(_inView(find.text('Alex Rivera')), findsOneWidget);
    expect(_inView(find.byType(VoiceControlBar)), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('the header counts who is connected', (tester) async {
    final app = await MockApp.pump(tester);
    await _joinGeneral(tester);

    expect(
      find.descendant(
        of: find.byType(ChatHeader),
        matching: find.text('5 connected'),
      ),
      findsOneWidget,
    );
    await app.dispose(tester);
  });

  testWidgets('a speaking participant is outlined', (tester) async {
    final app = await MockApp.pump(tester);
    await _joinGeneral(tester);

    app.read(speakingProvider(_dev).notifier).set({_mira});
    await tester.pump();

    expect(tester.widget<VoiceTileView>(_tileOf(_mira)).speaking, isTrue);
    expect(tester.widget<VoiceTileView>(_tileOf(_priya)).speaking, isFalse);
    await app.dispose(tester);
  });

  testWidgets('a clicked tile is focused, and Esc returns to the grid', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _joinGeneral(tester);
    final gridWidth = tester.getSize(_tileOf(_priya)).width;

    await tester.tap(_tileOf(_priya));
    await tester.pump();

    expect(tester.getSize(_tileOf(_priya)).width, greaterThan(gridWidth));
    expect(tester.getSize(_tileOf(_mira)).width, 160);
    expect(tester.widget<VoiceTileView>(_tileOf(_priya)).focused, isTrue);

    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pump();

    expect(tester.getSize(_tileOf(_priya)).width, gridWidth);
    expect(tester.getSize(_tileOf(_mira)).width, gridWidth);
    await app.dispose(tester);
  });

  testWidgets('the control bar mutes, turns the camera on and deafens', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _joinGeneral(tester);

    await tester.tap(_control('Mute'));
    await tester.pump();
    expect(app.read(voiceSessionProvider).muted, isTrue);

    await tester.tap(_control('Turn on camera'));
    await tester.pump();
    expect(app.read(voiceSessionProvider).camera, isTrue);
    expect(tester.widget<VoiceTileView>(_tileOf(1000)).camera, isTrue);

    await tester.tap(_control('Deafen'));
    await tester.pump();
    expect(app.read(voiceSessionProvider).deafened, isTrue);
    expect(tester.widget<VoiceTileView>(_tileOf(1000)).deafened, isTrue);
    await app.dispose(tester);
  });

  testWidgets('Disconnect leaves for the join prompt; Join voice returns', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _joinGeneral(tester);

    await tester.tap(_inView(find.text('Disconnect')));
    await tester.pump();

    expect(app.read(voiceSessionProvider).connected, isFalse);
    expect(_inView(find.text('4 connected')), findsOneWidget);
    expect(_inView(find.byType(VoiceTileView)), findsNothing);

    await tester.tap(_inView(find.widgetWithText(OcButton, 'Join voice')));
    await tester.pump();

    expect(app.read(voiceSessionProvider).channelId, _voiceChannel(app));
    expect(_inView(find.byType(VoiceTileView)), findsNWidgets(6));
    await app.dispose(tester);
  });

  testWidgets('a voice channel opened without joining shows the prompt', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);

    app.read(navigationProvider.notifier).openChannel(_dev, _voiceChannel(app));
    await _pumpFor(tester, const Duration(milliseconds: 300));

    expect(_inView(find.text('General')), findsOneWidget);
    expect(_inView(find.text('4 connected')), findsOneWidget);
    expect(
      _inView(find.widgetWithText(OcButton, 'Join voice')),
      findsOneWidget,
    );
    await app.dispose(tester);
  });

  group('screenshare', () {
    testWidgets('asks for a quality within the server limit, then goes live', (
      tester,
    ) async {
      final app = await MockApp.pump(tester);
      await _joinGeneral(tester);

      await tester.tap(_control('Share your screen'));
      await _pumpFor(tester, const Duration(milliseconds: 300));
      expect(find.text('Share your screen'), findsWidgets);
      // The mock server allows 720p at 30 fps.
      expect(find.text('Server limit: 720p · 30 fps'), findsOneWidget);
      await tester.tap(find.text('1080p'));
      await tester.tap(find.text('60 fps'));
      await tester.tap(find.text('15 fps'));
      await tester.pump();
      await tester.tap(find.widgetWithText(OcButton, 'Go live'));
      await _pumpFor(tester, const Duration(milliseconds: 300));

      expect(app.read(voiceSessionProvider).screensharing, isTrue);
      expect(
        app.read(lastScreenQualityProvider(_dev)),
        const ScreenShareQuality(ScreenShareResolution.p720, 15),
      );
      expect(_tileOf(1000, screen: true), findsOneWidget);

      await tester.tap(_control('Screen share options'));
      await _pumpFor(tester, const Duration(milliseconds: 300));
      await tester.tap(find.text('Stop sharing'));
      await _pumpFor(tester, const Duration(milliseconds: 300));

      expect(app.read(voiceSessionProvider).screensharing, isFalse);
      expect(_tileOf(1000, screen: true), findsNothing);
      await app.dispose(tester);
    });

    testWidgets('leaves the source to the desktop on Linux', (tester) async {
      final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
      await _joinGeneral(tester);

      await tester.tap(_control('Share your screen'));
      await _pumpFor(tester, const Duration(milliseconds: 300));

      expect(find.textContaining('Your desktop asks'), findsOneWidget);
      await app.dispose(tester);
    });

    testWidgets('the voice panel Screen toggle asks too', (tester) async {
      final app = await MockApp.pump(tester);
      await _joinGeneral(tester);

      await tester.tap(
        find.descendant(
          of: find.byType(VoiceConnectedPanel),
          matching: find.text('Screen'),
        ),
      );
      await _pumpFor(tester, const Duration(milliseconds: 300));

      expect(find.widgetWithText(OcButton, 'Go live'), findsOneWidget);
      expect(app.read(voiceSessionProvider).screensharing, isFalse);
      await app.dispose(tester);
    });

    testWidgets("others' streams come only once watched, and open large", (
      tester,
    ) async {
      final app = await MockApp.pump(tester);
      await _joinGeneral(tester);
      final key = 'stream:${_voiceChannel(app)}:$_tomas';

      expect(app.repository.watched, isEmpty);
      await tester.tap(_inView(find.widgetWithText(OcButton, 'Watch')));
      await _pumpFor(tester, const Duration(milliseconds: 300));

      expect(app.repository.watched, {key});
      expect(
        tester.widget<VoiceTileView>(_tileOf(_tomas, screen: true)).focused,
        isTrue,
      );

      await tester.tap(_inView(find.widgetWithText(OcButton, 'Stop watching')));
      await _pumpFor(tester, const Duration(milliseconds: 300));

      expect(app.repository.watched, isEmpty);
      expect(
        tester.widget<VoiceTileView>(_tileOf(_tomas, screen: true)).focused,
        isFalse,
      );
      await app.dispose(tester);
    });

    testWidgets('a full stream says so', (tester) async {
      final app = await MockApp.pump(tester);
      await _joinGeneral(tester);
      app.repository.watchError = const RepoException(
        RepoErrorKind.streamFull,
        'This stream is full (50 viewers)',
      );

      await tester.tap(_inView(find.widgetWithText(OcButton, 'Watch')));
      await _pumpFor(tester, const Duration(milliseconds: 300));

      expect(find.text('This stream is full (50 viewers)'), findsOneWidget);
      expect(app.repository.watched, isEmpty);
      await app.dispose(tester);
    });
  });

  testWidgets('the voice view fits the smallest window at large text', (
    tester,
  ) async {
    final app = await MockApp.pump(
      tester,
      size: OcSize.minWindow,
      textScale: 1.5,
    );
    // The voice rows are below the fold of the sidebar here.
    final channel = _voiceChannel(app);
    app.read(navigationProvider.notifier).openChannel(_dev, channel);
    await app.read(voiceSessionProvider.notifier).join(_dev, channel);
    await _pumpFor(tester, const Duration(milliseconds: 300));
    expect(_inView(find.byType(VoiceTileView)), findsNWidgets(6));
    await tester.tap(_tileOf(_priya));
    await tester.pump();

    expect(tester.takeException(), isNull);
    await app.dispose(tester);
  });

  testWidgets('a full channel says so instead of joining', (tester) async {
    final app = await MockApp.pump(tester);
    app.read(navigationProvider.notifier).openServer(_berlin);
    await _pumpFor(tester, const Duration(milliseconds: 300));

    await tester.tap(
      find.descendant(
        of: find.byKey(DesktopShell.sidebarKey),
        matching: find.text('Lounge'),
      ),
    );
    await _pumpFor(tester, const Duration(milliseconds: 600));

    expect(find.text('This voice channel is full.'), findsOneWidget);
    expect(app.read(voiceSessionProvider).connected, isFalse);
    await _pumpFor(tester, const Duration(seconds: 3));
    await app.dispose(tester);
  });

  testWidgets('without camera or screen share, the controls leave them out', (
    tester,
  ) async {
    final app = await MockApp.pump(
      tester,
      capabilities: const RepoCapabilities(voice: true),
    );
    await _joinGeneral(tester);

    expect(_control('Mute'), findsOneWidget);
    expect(_control('Turn on camera'), findsNothing);
    expect(_control('Share your screen'), findsNothing);
    final panel = find.byType(VoiceConnectedPanel);
    expect(
      find.descendant(of: panel, matching: find.text('Camera')),
      findsNothing,
    );
    expect(
      find.descendant(of: panel, matching: find.text('Screen')),
      findsNothing,
    );
    await app.dispose(tester);
  });

  group('video (Phase 2 V5)', () {
    const kira = VideoFeed(
      trackId: 'camera-mira',
      textureId: 11,
      width: 1280,
      height: 720,
    );
    const own = VideoFeed(
      trackId: 'camera-me',
      textureId: 12,
      width: 1280,
      height: 720,
      mirrored: true,
    );

    Future<int> showCameras(MockApp app, WidgetTester tester) async {
      await _joinGeneral(tester);
      final channel = _voiceChannel(app);
      final feeds = app.read(videoFeedsProvider.notifier);
      feeds.apply(VideoTrackAdded(_dev, channel, _mira, kira));
      feeds.setOwn(own);
      await _pumpFor(tester, const Duration(milliseconds: 100));
      return app.read(serverProvider(_dev)).data!.self.id;
    }

    Finder textureIn(Finder tile, int id) => find.descendant(
      of: tile,
      matching: find.byWidgetPredicate(
        (widget) => widget is Texture && widget.textureId == id,
      ),
    );

    testWidgets('a camera shows its video, and this device sees itself '
        'mirrored', (tester) async {
      final app = await MockApp.pump(tester);
      final self = await showCameras(app, tester);

      expect(textureIn(_tileOf(_mira), 11), findsOneWidget);
      expect(textureIn(_tileOf(self), 12), findsOneWidget);
      final mirrors = find.descendant(
        of: _tileOf(self),
        matching: find.byWidgetPredicate(
          (widget) => widget is Transform && widget.transform.storage[0] < 0,
        ),
      );
      expect(mirrors, findsOneWidget);
      expect(
        find.descendant(
          of: _tileOf(_mira),
          matching: find.byWidgetPredicate(
            (widget) => widget is Transform && widget.transform.storage[0] < 0,
          ),
        ),
        findsNothing,
      );
      await app.dispose(tester);
    });

    testWidgets('tiles showing video ask for it at their size, and stop '
        'when the view closes', (tester) async {
      final app = await MockApp.pump(tester);
      final self = await showCameras(app, tester);

      final wants = {
        for (final want in app.repository.videoWants) want.trackId: want,
      };
      final tile = tester.getSize(_tileOf(_mira));
      expect(wants.keys, unorderedEquals(['camera-mira', 'camera-me']));
      expect(wants['camera-mira']!.width, tile.width.round());
      expect(wants['camera-mira']!.height, tile.height.round());
      expect(
        wants['camera-me']!.width,
        tester.getSize(_tileOf(self)).width.round(),
      );

      await tester.tap(
        find.descendant(
          of: find.byKey(DesktopShell.sidebarKey),
          matching: find.text('general'),
        ),
      );
      await _pumpFor(tester, const Duration(milliseconds: 300));
      expect(find.byType(VoiceView), findsNothing);
      expect(app.repository.videoWants, isEmpty);
      await app.dispose(tester);
    });

    testWidgets('a camera that cannot start says why and stays off', (
      tester,
    ) async {
      final app = await MockApp.pump(tester);
      await _joinGeneral(tester);
      app.repository.cameraError = const RepoException(
        RepoErrorKind.cameraLimit,
        'too many cameras',
      );

      await tester.tap(_control('Turn on camera'));
      await _pumpFor(tester, const Duration(milliseconds: 300));

      expect(find.text('Camera limit reached in this channel'), findsOneWidget);
      expect(app.read(voiceSessionProvider).camera, isFalse);
      expect(_control('Turn on camera'), findsOneWidget);
      await app.dispose(tester);
    });
  });
}
