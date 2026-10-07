import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/settings/local_prefs.dart';
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
const _mira = 1002, _priya = 1005;

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
    testWidgets('asks what to share and how well, then goes live', (
      tester,
    ) async {
      final app = await MockApp.pump(tester);
      await _joinGeneral(tester);

      await tester.tap(_control('Share your screen'));
      await _pumpFor(tester, const Duration(milliseconds: 300));
      expect(find.text('Share your screen'), findsWidgets);
      await tester.tap(find.text('Screen 2'));
      await tester.tap(find.text('1080p60'));
      await tester.pump();
      await tester.tap(find.widgetWithText(OcButton, 'Go live'));
      await _pumpFor(tester, const Duration(milliseconds: 300));

      expect(app.read(voiceSessionProvider).screensharing, isTrue);
      expect(app.read(screenQualityProvider), ScreenQuality.hd1080p60);
      expect(_tileOf(1000, screen: true), findsOneWidget);

      await tester.tap(_control('Stop sharing'));
      await _pumpFor(tester, const Duration(milliseconds: 300));

      expect(app.read(voiceSessionProvider).screensharing, isFalse);
      expect(find.text('Go live'), findsNothing);
      await app.dispose(tester);
    });

    testWidgets('leaves the source to the desktop on Linux', (tester) async {
      final app = await MockApp.pump(tester, platform: TargetPlatform.linux);
      await _joinGeneral(tester);

      await tester.tap(_control('Share your screen'));
      await _pumpFor(tester, const Duration(milliseconds: 300));

      expect(find.textContaining('Your desktop asks'), findsOneWidget);
      expect(find.text('Screen 1'), findsNothing);
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
}
