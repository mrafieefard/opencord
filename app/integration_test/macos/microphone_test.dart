import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

import '../support/e2e.dart';

// The microphone path on macOS (docs/plans/macos.md M1, step 6), from
// app/tool/macos/check_microphone.sh: afplay plays a 1 kHz tone into
// BlackHole 16ch, which the app uses as its microphone. The app holds each
// phase (voice activity, push-to-talk held, push-to-talk released) for a
// while and prints its name; the script then starts a listening voicebot
// that measures the tone in what the app sends. The app also checks that
// its device lists name this Mac's devices and that its level meter moves.
// Without the script this test skips.
const _invite = String.fromEnvironment('OPENCORD_E2E_INVITE');

/// The cpal ids of the devices the app plays into and records from.
const _output = String.fromEnvironment('OPENCORD_E2E_OUTPUT');
const _input = String.fromEnvironment('OPENCORD_E2E_INPUT');
const _profile = 'e2e_microphone';

/// How long each phase lasts: the script's listener joins, then listens
/// for 5 s.
const _phase = Duration(seconds: 10);

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    'the microphone reaches the call, by voice activity and push-to-talk',
    skip: _invite.isEmpty,
    (tester) async {
      await SecureIdentityStore(profile: _profile).clear();
      final container = await launch(tester, profile: _profile);
      final repository = container.read(repositoryProvider);
      final failures = <AudioDeviceFailed>[];
      final levels = <double>[];
      final subscription = repository.events.listen((event) {
        if (event is AudioDeviceFailed) failures.add(event);
        if (event is InputLevelChanged) levels.add(event.dbfs);
      });
      Future<void> hold(String phase) async {
        // ignore: avoid_print
        print('PHASE: $phase');
        final end = DateTime.now().add(_phase);
        while (DateTime.now().isBefore(end)) {
          await tester.pump(const Duration(milliseconds: 100));
          await Future<void>.delayed(const Duration(milliseconds: 100));
        }
      }

      try {
        final devices = await repository.audioDevices();
        // ignore: avoid_print
        print(
          'MIC DEVICES: inputs ${devices.inputs.map((d) => d.name)}; '
          'outputs ${devices.outputs.map((d) => d.name)}',
        );
        expect(devices.inputs.map((d) => d.id), contains(_input));
        expect(devices.outputs.map((d) => d.id), contains(_output));

        await createIdentity(tester, 'Speaker');
        await startAddingServer(tester, _invite);
        await tester.tap(find.widgetWithText(OcButton, 'Join'));
        await waitFor(tester, inSidebar('General'));
        // A steady tone is what noise suppression is for: off, so the tone
        // measures the path, not the suppressor.
        container
            .read(audioSettingsProvider.notifier)
            .update(
              (audio) => audio
                  .withInput(const AudioDevice(id: _input, name: 'Tone'))
                  .withOutput(const AudioDevice(id: _output, name: 'Measure'))
                  .copyWith(
                    noiseSuppression: NoiseSuppression.off,
                    inputMode: InputMode.voiceActivity,
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

        await hold('voice activity');
        container
            .read(audioSettingsProvider.notifier)
            .update(
              (audio) => audio.copyWith(inputMode: InputMode.pushToTalk),
            );
        await tester.pump();
        repository.setPushToTalk(true);
        await hold('push-to-talk held');
        repository.setPushToTalk(false);
        await hold('push-to-talk released');
        // ignore: avoid_print
        print('PHASE: done');

        expect(failures.map((failure) => failure.message), isEmpty);
        expect(levels, isNotEmpty, reason: 'the level meter never moved');
        // ignore: avoid_print
        print(
          'MIC LEVELS: ${levels.length}, loudest '
          '${levels.reduce((a, b) => a > b ? a : b).toStringAsFixed(1)} dBFS',
        );
        await container.read(voiceSessionProvider.notifier).leave();
      } finally {
        await subscription.cancel();
        await SecureIdentityStore(profile: _profile).clear();
      }
    },
  );
}
