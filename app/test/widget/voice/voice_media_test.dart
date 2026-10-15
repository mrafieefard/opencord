import 'package:flutter/services.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/mock/mock_repository.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/channels/voice_panel.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/voice/connection_info.dart';
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

  testWidgets('the voice panel shows the connection as bars, and a click '
      'graphs its ping, loss and jitter', (tester) async {
    final app = await MockApp.pump(tester);
    await _joinGeneral(tester);
    final session = app.read(voiceSessionProvider);
    final status = _inPanel(
      find.bySemanticsLabel(RegExp('Show the connection graph')),
    );
    expect(
      _inPanel(find.bySemanticsLabel(RegExp('Connection good'))),
      findsNothing,
    );

    app.repository.debugVoiceStats(
      session.serverKey!,
      session.channelId!,
      const VoiceStats(
        pingMs: 42,
        jitterMs: 3.25,
        lossInPercent: 0.5,
        lossOutPercent: 0,
        bitrateIn: 96000,
        bitrateOut: 1500000,
        voiceNode: 'opencord.example:7710',
        cameraEncoder: 'VaApi (hardware)',
      ),
    );
    await _settle(tester);
    expect(
      _inPanel(find.bySemanticsLabel(RegExp('Connection good'))),
      findsOneWidget,
    );
    await tester.tap(status);
    await _settle(tester);

    expect(find.byType(ConnectionGraph), findsOneWidget);
    expect(find.text('Ping'), findsOneWidget);
    expect(find.text('42 ms'), findsOneWidget);
    expect(find.text('Packet loss'), findsOneWidget);
    expect(find.text('0.5 %'), findsOneWidget);
    expect(find.text('Jitter'), findsOneWidget);
    expect(find.text('3 ms'), findsOneWidget);
    // Those three only.
    expect(find.text('96 kbps'), findsNothing);
    expect(find.text('opencord.example:7710'), findsNothing);
    expect(find.textContaining('VaApi'), findsNothing);

    // The graph follows the reports while it is open.
    app.repository.debugVoiceStats(
      session.serverKey!,
      session.channelId!,
      const VoiceStats(pingMs: 90, lossInPercent: 12, lossOutPercent: 1),
    );
    await _settle(tester);
    expect(find.text('90 ms'), findsOneWidget);
    expect(find.text('12.0 %'), findsOneWidget);
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await _settle(tester);

    expect(
      _inPanel(find.bySemanticsLabel(RegExp('Connection poor'))),
      findsOneWidget,
    );
    await app.dispose(tester);
  });

  testWidgets("the voice panel's channel line opens the call", (tester) async {
    final app = await MockApp.pump(tester);
    await _joinGeneral(tester);
    final voiceChannel = app.read(voiceSessionProvider).channelId;
    await tester.tap(
      find.descendant(
        of: find.byKey(DesktopShell.sidebarKey),
        matching: find.text('general'),
      ),
    );
    await _settle(tester);
    expect(app.read(currentChannelProvider), isNot(voiceChannel));

    await tester.tap(
      _inPanel(find.bySemanticsLabel(RegExp('Open the voice view'))),
    );
    await _settle(tester);

    expect(app.read(currentChannelProvider), voiceChannel);
    await app.dispose(tester);
  });

  testWidgets('the graph keeps the last minute, and forgets it on leaving', (
    tester,
  ) async {
    final app = await MockApp.pump(tester);
    await _joinGeneral(tester);
    final session = app.read(voiceSessionProvider);

    for (var ping = 1; ping <= 65; ping++) {
      app.repository.debugVoiceStats(
        session.serverKey!,
        session.channelId!,
        // The same report twice in a row still counts as a second.
        VoiceStats(pingMs: ping == 65 ? 64 : ping),
      );
      await _settle(tester);
    }
    final history = app.read(voiceStatsHistoryProvider);
    expect(history, hasLength(VoiceStatsHistoryNotifier.length));
    expect(history.first.pingMs, 6);
    expect(history.last.pingMs, 64);

    await app.read(voiceSessionProvider.notifier).leave();
    await _settle(tester);
    expect(app.read(voiceStatsHistoryProvider), isEmpty);
    await app.dispose(tester);
  });

  testWidgets("a headset's two halves going at once make one toast", (
    tester,
  ) async {
    final app = await MockApp.pump(tester);

    app.repository.debugDeviceFellBack(output: false, device: 'Built-in');
    await tester.pump(const Duration(milliseconds: 100));
    app.repository.debugDeviceFellBack(output: true, device: 'Built-in');
    await _settle(tester);

    expect(
      find.text('Microphone and speaker unavailable. Using Built-in.'),
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
    await tester.tap(find.bySemanticsLabel('Input options'));
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

  testWidgets('device lists show four at a time and scroll the rest', (
    tester,
  ) async {
    final app = await MockApp.pump(tester, size: const Size(1280, 640));
    app.repository.deviceList = AudioDeviceList(
      inputs: [
        for (var i = 0; i < 12; i++)
          AudioDevice(id: 'mock:in-$i', name: 'Microphone $i'),
      ],
      outputs: [
        for (var i = 0; i < 12; i++)
          AudioDevice(id: 'mock:out-$i', name: 'Speaker $i'),
      ],
    );

    for (final (side, first, last) in [
      ('Input options', 'Microphone 0', 'Microphone 11'),
      ('Output options', 'Speaker 0', 'Speaker 11'),
    ]) {
      await tester.tap(find.bySemanticsLabel(side));
      await tester.pumpAndSettle();
      final list = find.ancestor(
        of: find.text(first),
        matching: find.byType(ListView),
      );
      final overflow = tester.takeException();

      expect(overflow, isNull);
      expect(tester.getSize(list).height, 4 * 30, reason: side);
      expect(find.text(last).hitTestable(), findsNothing, reason: side);
      await tester.drag(find.text(first), const Offset(0, -2000));
      await tester.pumpAndSettle();
      expect(find.text(last).hitTestable(), findsOneWidget, reason: side);
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pumpAndSettle();
    }
    await app.dispose(tester);
  });
}
