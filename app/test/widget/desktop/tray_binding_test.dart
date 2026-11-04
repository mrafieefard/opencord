import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/desktop/tray.dart';
import 'package:opencord/features/desktop/tray_menu.dart';

import '../../support/app.dart';
import '../../support/fake_tray.dart';
import '../../support/fake_window.dart';

const _dev = 'opencord.example:7710';

Future<MockApp> _pump(
  WidgetTester tester, {
  required FakeTray tray,
  FakeNativeWindow? window,
  bool closeToTray = true,
  TargetPlatform? platform,
}) => MockApp.pump(
  tester,
  window: window,
  platform: platform,
  settings: (s) => s.copyWith(closeToTray: closeToTray),
  overrides: [trayServiceProvider.overrideWithValue(tray)],
);

void main() {
  testWidgets('the tray shows the unread count, voice and status (§15)', (
    tester,
  ) async {
    final tray = FakeTray();
    final app = await _pump(tester, tray: tray);
    await tester.pump(const Duration(milliseconds: 500));

    expect(tray.shown.last.unread, app.read(unreadTotalProvider));
    expect(tray.shown.last.unread, greaterThan(0));
    expect(tray.shown.last.mentions, greaterThan(0));

    await app.read(voiceSessionProvider.notifier).toggleMute();
    app.read(selfPresenceProvider.notifier).choose(SelfPresence.idle);
    await tester.pump();

    expect(tray.shown.last.muted, isTrue);
    expect(tray.shown.last.presence, SelfPresence.idle);
    await app.dispose(tester);
  });

  testWidgets('tray menu choices are carried out', (tester) async {
    final tray = FakeTray();
    final window = FakeNativeWindow();
    final app = await _pump(tester, tray: tray, window: window);

    tray
      ..click(const ToggleDeafen())
      ..click(const SetPresence(SelfPresence.doNotDisturb))
      ..click(const OpenWindow())
      ..click(const QuitApp());
    await tester.pump();

    expect(app.read(voiceSessionProvider).deafened, isTrue);
    expect(app.read(selfPresenceProvider), SelfPresence.doNotDisturb);
    expect(window.calls, containsAllInOrder(['show', 'quit']));
    await app.dispose(tester);
  });

  testWidgets('the tray shows whether you are in voice and talking', (
    tester,
  ) async {
    final tray = FakeTray();
    final app = await _pump(tester, tray: tray);
    await tester.pump();
    expect(tray.shown.last.voice, TrayVoice.none);

    final channel = app
        .read(serverProvider(_dev))
        .data!
        .channels
        .values
        .firstWhere((channel) => channel.name == 'General')
        .id;
    await app.read(voiceSessionProvider.notifier).join(_dev, channel);
    await tester.pump(const Duration(milliseconds: 300));
    expect(tray.shown.last.voice, TrayVoice.quiet);

    final me = app.read(serverProvider(_dev)).data!.self.id;
    app.read(speakingProvider(_dev).notifier).set({me});
    await tester.pump();
    expect(tray.shown.last.voice, TrayVoice.speaking);

    await app.read(voiceSessionProvider.notifier).toggleMute();
    await tester.pump();
    expect(tray.shown.last.voice, TrayVoice.muted);

    await app.read(voiceSessionProvider.notifier).leave();
    await tester.pump();
    expect(tray.shown.last.voice, TrayVoice.none);
    // Leaving's own timers run out.
    await tester.pump(const Duration(seconds: 1));
    await app.dispose(tester);
  });

  testWidgets('a click on the icon brings the window forward', (tester) async {
    final tray = FakeTray();
    final window = FakeNativeWindow();
    final app = await _pump(tester, tray: tray, window: window);

    tray.click(const OpenWindow(activationToken: 'token-1'));
    await tester.pump();

    expect(window.calls, contains('show:token-1'));
    await app.dispose(tester);
  });

  group('closing the window', () {
    testWidgets('hides it to the tray when there is one', (tester) async {
      final window = FakeNativeWindow();
      final app = await _pump(tester, tray: FakeTray(), window: window);

      window.closeEvents.add(null);
      await tester.pump();

      expect(window.calls, contains('hide'));
      expect(window.calls, isNot(contains('quit')));
      await app.dispose(tester);
    });

    // Launching the app again brings the hidden window back.
    testWidgets('keeps running in the background with no tray', (tester) async {
      final window = FakeNativeWindow();
      final app = await _pump(
        tester,
        tray: FakeTray(available: false),
        window: window,
      );

      window.closeEvents.add(null);
      await tester.pump();

      expect(window.calls, contains('hide'));
      expect(window.calls, isNot(contains('quit')));
      await app.dispose(tester);
    });

    testWidgets('on macOS hides it: the Dock icon brings it back', (
      tester,
    ) async {
      final window = FakeNativeWindow();
      final app = await _pump(
        tester,
        tray: FakeTray(available: false),
        window: window,
        closeToTray: false,
        platform: TargetPlatform.macOS,
      );

      window.closeEvents.add(null);
      await tester.pump();

      expect(window.calls, contains('hide'));
      expect(window.calls, isNot(contains('quit')));
      await app.dispose(tester);
    });

    testWidgets('quits when the setting is off', (tester) async {
      final window = FakeNativeWindow();
      final app = await _pump(
        tester,
        tray: FakeTray(),
        window: window,
        closeToTray: false,
      );

      window.closeEvents.add(null);
      await tester.pump();

      expect(window.calls, contains('quit'));
      await app.dispose(tester);
    });
  });
}
