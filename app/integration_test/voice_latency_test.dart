import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

import 'support/e2e.dart';

// Voice latency through real devices (plan §16.1), from
// tool/check_voice_latency.sh: the app plays into a PipeWire null sink and
// records that sink's monitor as its microphone, so whatever it hears goes
// straight back out. A voicebot beeps every second and times each beep's
// return. Echo cancellation, noise suppression and gain control are off,
// or the app would take the very echo being measured out. Without the
// script this test skips.
const _invite = String.fromEnvironment('OPENCORD_E2E_INVITE');
const _loop = String.fromEnvironment('OPENCORD_E2E_LOOP_SINK');
const _profile = 'e2e_voice_latency';
const _hold = Duration(seconds: 30);

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    'what the app hears goes back out through its devices',
    skip: _invite.isEmpty || _loop.isEmpty,
    (tester) async {
      await SecureIdentityStore(profile: _profile).clear();
      final container = await launch(tester, profile: _profile);
      try {
        await createIdentity(tester, 'Loop');
        await startAddingServer(tester, _invite);
        await tester.tap(find.widgetWithText(OcButton, 'Join'));
        await waitFor(tester, inSidebar('General'));
        final loop = AudioDevice(id: 'pipewire:$_loop', name: 'Loop');
        container
            .read(audioSettingsProvider.notifier)
            .update(
              (audio) => audio
                  .withInput(loop)
                  .withOutput(loop)
                  .copyWith(
                    echoCancellation: false,
                    noiseSuppression: NoiseSuppression.off,
                    automaticGain: false,
                    automaticSensitivity: false,
                    sensitivityDbfs: -70,
                  ),
            );
        await tester.tap(inSidebar('General'));
        await waitUntil(
          tester,
          () =>
              container.read(voiceConnectionProvider)?.phase ==
              VoiceConnectionPhase.connected,
          reason: 'voice media connecting',
        );
        await Future<void>.delayed(_hold);
        await container.read(voiceSessionProvider.notifier).leave();
      } finally {
        await SecureIdentityStore(profile: _profile).clear();
      }
    },
    timeout: const Timeout(Duration(minutes: 3)),
  );
}
