import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/mock/mock_repository.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/channels/voice_panel.dart';
import 'package:opencord/features/shell/desktop_shell.dart';

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

    expect(atStart, const AudioConfig());
    expect(
      app.repository.audio,
      const AudioConfig(
        inputDevice: 'mock:usb-microphone',
        pushToTalk: true,
        inputVolume: 150,
      ),
    );
    await app.dispose(tester);
  });
}
