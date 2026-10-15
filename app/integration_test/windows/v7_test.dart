import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/voice/screen_source_picker.dart';
import 'package:opencord/features/voice/voice_controls.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

import '../support/e2e.dart';

// Windows plan W7, step 4 — §10.7's "no double audio" test. The app shares
// a screen (or a window) **with its sound**, while a Talker voicebot talks
// in the same call. What goes out must carry the shared sound and not the
// call's own voice. `check_v7.ps1` plays the tones and measures the stream;
// this test puts the app in the state and holds it there.
//
// **This has never run**: process loopback needs Windows build 20348 and
// the PC it was written on is 19045 (windows.md §7).
//
// One phase per run:
//   screen    a whole monitor, shared with sound
//   mutation  the same, with the backend excluding another process instead
//             of Opencord's — the call's voice must then be in the stream,
//             which is what proves the check can fail. The script sets
//             OPENCORD_TEST_EXCLUDE_PID; nothing changes here.
//   window    an ffplay window, shared with its own sound only
const _invite = String.fromEnvironment('OPENCORD_E2E_INVITE');

const _phase = String.fromEnvironment(
  'OPENCORD_E2E_PHASE',
  defaultValue: 'screen',
);

/// The app's speaker: the cable, so the call is never audible.
const _outputId = String.fromEnvironment('OPENCORD_E2E_OUTPUT_ID');

/// The app's microphone: the cable's loopback.
const _inputId = String.fromEnvironment('OPENCORD_E2E_INPUT_ID');

/// Part of the title of the window to share, in the window phase.
const _window = String.fromEnvironment(
  'OPENCORD_E2E_WINDOW',
  defaultValue: 'ffplay',
);

const _holdSeconds = int.fromEnvironment('OPENCORD_E2E_HOLD', defaultValue: 14);

const _profile = 'e2e_v7';

Future<void> _hold(WidgetTester tester, Duration total) async {
  final until = DateTime.now().add(total);
  while (DateTime.now().isBefore(until)) {
    await tester.pump(const Duration(milliseconds: 50));
    await Future<void>.delayed(const Duration(milliseconds: 50));
  }
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('sharing a $_phase with its sound', skip: _invite.isEmpty, (
    tester,
  ) async {
    await SecureIdentityStore(profile: _profile).clear();
    final container = await launch(tester, profile: _profile);
    try {
      await createIdentity(tester, 'Sharer');
      await startAddingServer(tester, _invite);
      await tester.tap(find.widgetWithText(OcButton, 'Join'));
      await waitFor(tester, inSidebar('General'));

      // Everything on the cable, so nothing is audible.
      container
          .read(audioSettingsProvider.notifier)
          .update(
            (audio) => audio
                .withInput(AudioDevice(id: _inputId, name: 'Cable'))
                .withOutput(AudioDevice(id: _outputId, name: 'Cable')),
          );

      await tester.tap(inSidebar('General'));
      await waitUntil(
        tester,
        () =>
            container.read(voiceConnectionProvider)?.phase ==
            VoiceConnectionPhase.connected,
        reason: 'voice media connecting',
      );
      await waitFor(tester, inSidebar('Talker'));
      await waitFor(tester, inSidebar('Watcher'));

      await tester.tap(
        find.descendant(
          of: find.byType(VoiceControlBar),
          matching: find.bySemanticsLabel('Share your screen'),
        ),
      );
      await _hold(tester, const Duration(milliseconds: 800));
      expect(find.byType(ScreenSourcePicker), findsOneWidget);

      // Which source.
      final window = _phase == 'window';
      if (window) {
        await tester.tap(find.textContaining('Windows ('));
        await _hold(tester, const Duration(milliseconds: 600));
      }
      final wanted = window
          ? find.textContaining(_window)
          : find.textContaining('Screen 1 · ');
      await waitFor(tester, wanted);
      await tester.tap(wanted.first);
      await _hold(tester, const Duration(milliseconds: 400));

      // The whole point of W7: the sound goes with it.
      final sound = find.text('Share sound');
      expect(sound, findsOneWidget, reason: 'the sound switch is there');
      final soundSwitch = find.descendant(
        of: find.ancestor(of: sound, matching: find.byType(Row)).first,
        matching: find.byType(Switch),
      );
      if (soundSwitch.evaluate().isNotEmpty &&
          !tester.widget<Switch>(soundSwitch.first).value) {
        await tester.tap(soundSwitch.first);
        await _hold(tester, const Duration(milliseconds: 300));
      }

      await tester.tap(find.widgetWithText(OcButton, 'Go live'));
      await waitUntil(
        tester,
        () => container.read(voiceSessionProvider).screensharing,
        reason: 'going live',
      );
      final share = container.read(ownScreenShareProvider);
      // ignore: avoid_print
      print(
        'V7: phase=$_phase source=${share?.source} audio=${share?.audio} '
        'audioFrom=${share?.audioFrom}',
      );
      expect(
        share?.audio,
        isTrue,
        reason: 'the share went live with its sound',
      );

      await _hold(tester, Duration(seconds: _holdSeconds));
      expect(container.read(voiceSessionProvider).screensharing, isTrue);

      await container.read(voiceSessionProvider.notifier).leave();
    } finally {
      await SecureIdentityStore(profile: _profile).clear();
    }
  });
}
