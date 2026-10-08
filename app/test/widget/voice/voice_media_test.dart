import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/mock/mock_repository.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/channels/voice_panel.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/features/voice/input_level_meter.dart';

import '../../support/app.dart';

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  const step = Duration(milliseconds: 50);
  for (var waited = Duration.zero; waited < total; waited += step) {
    await tester.pump(step);
  }
}

Future<void> _joinGeneral(WidgetTester tester) async {
  await tester.tap(
    find.descendant(
      of: find.byKey(DesktopShell.sidebarKey),
      matching: find.text('General'),
    ),
  );
  await _pumpFor(tester, const Duration(milliseconds: 300));
}

/// Repository events arrive in a microtask; the frame after shows them.
Future<void> _settle(WidgetTester tester) async {
  await tester.pump();
  await tester.pump();
}

Finder _inPanel(Finder finder) =>
    find.descendant(of: find.byType(VoiceConnectedPanel), matching: finder);

void main() {
  testWidgets('the voice panel says how the voice connection is doing', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _joinGeneral(tester);
    final connected = _inPanel(find.text('Voice connected'));

    app.repository.debugVoiceConnection(
      const VoiceConnectionStatus(VoiceConnectionPhase.noRoute),
    );
    await _settle(tester);
    final noRoute = _inPanel(find.text('No route'));
    final hint = find.byTooltip(
      "UDP port 7711 may be blocked by your network or the server's firewall",
    );
    expect(noRoute, findsOneWidget);
    expect(hint, findsOneWidget);
    app.repository.debugVoiceConnection(
      const VoiceConnectionStatus(VoiceConnectionPhase.reconnecting),
    );
    await _settle(tester);

    expect(_inPanel(find.text('Reconnecting')), findsOneWidget);
    expect(connected, findsNothing);
    await app.dispose(tester);
  });

  testWidgets('a voice connection starts out connected on the mock', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);

    await _joinGeneral(tester);

    expect(_inPanel(find.text('Voice connected')), findsOneWidget);
    await app.dispose(tester);
  });

  testWidgets('a missing microphone falls back to the default with a toast', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);

    app.repository.debugDeviceFellBack(
      output: false,
      device: 'Built-in microphone',
    );
    await _settle(tester);

    expect(
      find.text('Microphone unavailable. Using Built-in microphone.'),
      findsOneWidget,
    );
    await _pumpFor(tester, const Duration(seconds: 3));
    await app.dispose(tester);
  });

  testWidgets('audio choices reach voice media at start and on change', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    final atStart = app.repository.audio;

    app
        .read(audioSettingsProvider.notifier)
        .update(
          (audio) => audio
              .withInput(MockRepository.devices.inputs[1])
              .copyWith(inputVolume: 150, inputMode: InputMode.pushToTalk),
        );

    // The first run's choice of noise suppression is in (the mock is fast).
    expect(atStart, const AudioConfig(noiseSuppression: NoiseSuppression.high));
    expect(
      app.repository.audio,
      const AudioConfig(
        inputDevice: 'mock:usb-microphone',
        pushToTalk: true,
        noiseSuppression: NoiseSuppression.high,
        inputVolume: 150,
      ),
    );
    await app.dispose(tester);
  });

  testWidgets('the first run chooses noise suppression once', (tester) async {
    final app = await MockApp.pump(tester);
    await _settle(tester);

    expect(
      app.read(audioSettingsProvider).noiseSuppression,
      NoiseSuppression.high,
    );
    expect(app.repository.audio?.noiseSuppression, NoiseSuppression.high);
    await app.dispose(tester);
  });

  testWidgets("speaking while muted says you're muted, and Unmute unmutes", (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _joinGeneral(tester);
    await app.read(voiceSessionProvider.notifier).toggleMute();
    await _settle(tester);

    app.repository.debugSpokeWhileMuted();
    await _settle(tester);
    expect(find.text("You're muted"), findsOneWidget);
    await tester.tap(find.text('Unmute'));
    await _settle(tester);

    expect(app.read(voiceSessionProvider).muted, isFalse);
    await _pumpFor(tester, const Duration(seconds: 6));
    await app.dispose(tester);
  });

  testWidgets('deafened, speaking brings no reminder', (tester) async {
    final app = await MockApp.pump(tester);
    await _joinGeneral(tester);
    await app.read(voiceSessionProvider.notifier).toggleDeafen();
    await _settle(tester);

    app.repository.debugSpokeWhileMuted();
    await _settle(tester);

    expect(find.text("You're muted"), findsNothing);
    await app.dispose(tester);
  });

  testWidgets('High noise suppression giving way says so and keeps Standard', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _settle(tester);

    app.repository.debugNoiseSuppressionFellBack();
    await _settle(tester);

    expect(
      find.text('High noise suppression was too slow here. Using Standard.'),
      findsOneWidget,
    );
    expect(
      app.read(audioSettingsProvider).noiseSuppression,
      NoiseSuppression.standard,
    );
    await _pumpFor(tester, const Duration(seconds: 3));
    await app.dispose(tester);
  });

  testWidgets('the quick audio menu shows the microphone while it is open', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await tester.tap(find.bySemanticsLabel('Audio options').first);
    await tester.pumpAndSettle();
    final opened = app.repository.levelMeterOn;

    app.repository.debugInputLevel(-30);
    await _settle(tester);
    final fill = tester.widget<FractionallySizedBox>(
      find.descendant(
        of: find.byType(InputLevelMeter),
        matching: find.byType(FractionallySizedBox),
      ),
    );
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pumpAndSettle();

    expect(opened, isTrue);
    expect(fill.widthFactor, closeTo(0.5, 0.01), reason: '-30 dBFS of 60');
    expect(app.repository.levelMeterOn, isFalse);
    await app.dispose(tester);
  });
}
