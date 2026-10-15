import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

import '../support/e2e.dart';

// Windows plan W1, step 5: the microphone path. A tone plays into CABLE
// Input; the app's microphone is CABLE Output, so the tone is what the app
// captures and sends. A Listener voicebot outside the app measures 440 Hz in
// what arrives, and app/tool/windows/check_microphone.ps1 reads that from the
// bot's log — this test only puts the app in one state and holds it there.
//
// There is no Linux twin to copy: V2 and V3 check that voice connects and
// that processing is chosen, but neither pushes a tone through the app's
// microphone. So the acceptance is a comparison between the phases rather
// than an absolute level, which is what makes it independent of how much the
// processing chain colours a pure sine.
//
// One phase per run, as check_m5.ps1 does:
//   voice-activity        the gate should open and the tone should go out
//   push-to-talk-released nothing should go out
//   push-to-talk-held     the tone should go out
const _invite = String.fromEnvironment('OPENCORD_E2E_INVITE');

/// The app's microphone: CABLE Output, which hears CABLE Input.
const _inputId = String.fromEnvironment('OPENCORD_E2E_INPUT_ID');

/// The app's speaker; deliberately *not* the cable, or what the app plays
/// would land in its own microphone.
const _outputId = String.fromEnvironment('OPENCORD_E2E_OUTPUT_ID');

const _phase = String.fromEnvironment('OPENCORD_E2E_PHASE');

/// How long to hold the app in the phase's state while the bot listens.
const _holdSeconds = int.fromEnvironment('OPENCORD_E2E_HOLD', defaultValue: 12);

/// Noise suppression for the run. `off` isolates the device path, which is
/// what step 5 is about; the check also runs it with `standard` to see what
/// the chain does to a pure tone.
const _suppression = String.fromEnvironment(
  'OPENCORD_E2E_SUPPRESSION',
  defaultValue: 'off',
);

const _profile = 'e2e_microphone';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('the microphone path, in $_phase', skip: _invite.isEmpty, (
    tester,
  ) async {
    await SecureIdentityStore(profile: _profile).clear();
    final container = await launch(tester, profile: _profile);
    try {
      await createIdentity(tester, 'Speaker');
      await startAddingServer(tester, _invite);
      await tester.tap(find.widgetWithText(OcButton, 'Join'));
      await waitFor(tester, inSidebar('General'));

      final mode = _phase == 'voice-activity'
          ? InputMode.voiceActivity
          : InputMode.pushToTalk;
      container
          .read(audioSettingsProvider.notifier)
          .update(
            (audio) => audio
                .withInput(AudioDevice(id: _inputId, name: 'Cable'))
                .withOutput(AudioDevice(id: _outputId, name: 'Speakers'))
                .copyWith(
                  inputMode: mode,
                  noiseSuppression: NoiseSuppression.values.firstWhere(
                    (value) => value.name == _suppression,
                  ),
                  // A pure tone is not speech; leave the gate where the user
                  // would have it rather than letting automatic sensitivity
                  // chase the tone.
                  automaticSensitivity: false,
                  echoCancellation: false,
                  automaticGain: false,
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
      await waitFor(tester, inSidebar('Listener'));

      if (_phase == 'push-to-talk-held') {
        container.read(repositoryProvider).setPushToTalk(true);
      }

      // Hold the state while the bot listens. Pumping keeps capture running.
      final until = DateTime.now().add(Duration(seconds: _holdSeconds));
      while (DateTime.now().isBefore(until)) {
        await tester.pump(const Duration(milliseconds: 50));
        await Future<void>.delayed(const Duration(milliseconds: 50));
      }

      if (_phase == 'push-to-talk-held') {
        container.read(repositoryProvider).setPushToTalk(false);
      }

      await container.read(voiceSessionProvider.notifier).leave();
    } finally {
      await SecureIdentityStore(profile: _profile).clear();
    }
  });
}
