import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

import '../support/e2e.dart';

// Windows plan W1, step 6 — V3's acceptance, on the PC's own speakers and
// microphone: a Talker voicebot plays 440 Hz, the app plays it out of the
// speakers, its microphone hears the room, and echo cancellation has to keep
// 440 Hz out of what the app sends. A Listener voicebot measures that;
// app/tool/windows/check_echo.ps1 reads the level from its log. This test puts
// the app in the state and holds it there.
//
// The Listener hears Talker as well as the app, and the bot's log names users
// by id, so this prints the app's own id for the script to pick the right
// line. Measuring the loudest or the quietest of the two instead would either
// read Talker's tone or hide a failure.
const _invite = String.fromEnvironment('OPENCORD_E2E_INVITE');

/// The PC's real speakers.
const _outputId = String.fromEnvironment('OPENCORD_E2E_OUTPUT_ID');

/// The PC's real microphone.
const _inputId = String.fromEnvironment('OPENCORD_E2E_INPUT_ID');

/// How long to hold voice while the Listener measures.
const _holdSeconds = int.fromEnvironment('OPENCORD_E2E_HOLD', defaultValue: 20);

/// The app's output volume, as a percentage. The level the app actually plays
/// at was measured through the cable in step 4, so this is how the "about
/// -12 dBFS" the plan asks for is reached.
const _outputVolume = int.fromEnvironment(
  'OPENCORD_E2E_OUTPUT_VOLUME',
  defaultValue: 100,
);

/// Echo cancellation. The check runs this test twice: once off, which proves
/// the microphone really does hear the speakers, and once on, which is the
/// acceptance. Without the first run a muted speaker would pass the second.
const _echoCancellation = bool.fromEnvironment(
  'OPENCORD_E2E_AEC',
  defaultValue: true,
);

/// The voice gate, in dBFS. Low enough that the app transmits continuously;
/// see the note where it is used.
const _sensitivityDbfs = int.fromEnvironment(
  'OPENCORD_E2E_SENSITIVITY',
  defaultValue: -90,
);

const _profile = 'e2e_echo';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('the echo test, on the real speakers', skip: _invite.isEmpty, (
    tester,
  ) async {
    await SecureIdentityStore(profile: _profile).clear();
    final container = await launch(tester, profile: _profile);
    var selfId = 0;
    final subscription = container.read(repositoryProvider).events.listen((
      event,
    ) {
      if (event is Ready) selfId = event.snapshot.self.id;
    });
    try {
      await createIdentity(tester, 'EchoTester');
      await startAddingServer(tester, _invite);
      await tester.tap(find.widgetWithText(OcButton, 'Join'));
      await waitFor(tester, inSidebar('General'));

      container
          .read(audioSettingsProvider.notifier)
          .update(
            (audio) => audio
                .withInput(AudioDevice(id: _inputId, name: 'Microphone'))
                .withOutput(AudioDevice(id: _outputId, name: 'Speakers'))
                .copyWith(
                  // What the test is about: the chain as a user would have it,
                  // echo cancellation included.
                  echoCancellation: _echoCancellation,
                  inputMode: InputMode.voiceActivity,
                  outputVolume: _outputVolume,
                  // A fixed, wide-open gate, not the automatic one.
                  // Automatic sensitivity decides what is speech, and a
                  // 440 Hz sine is not, so with it on the app sends nothing
                  // and there is no echo to measure either way. The default
                  // -45 dBFS gate is no better here: this laptop's speakers
                  // reach its own microphone about 38 dB down, so the echo
                  // arrives near -57 dBFS and never opens it. Dropping the
                  // gate below the room floor makes the app send
                  // continuously, which is what lets the echo be measured at
                  // all — and the measurement is frequency-selective, so the
                  // room noise that comes with it does not affect the result.
                  automaticSensitivity: false,
                  sensitivityDbfs: _sensitivityDbfs.toDouble(),
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
      await waitFor(tester, inSidebar('Talker'));
      await waitFor(tester, inSidebar('Listener'));

      // Hold while the speakers play and the microphone listens. Pumping keeps
      // capture and playback running.
      final until = DateTime.now().add(Duration(seconds: _holdSeconds));
      while (DateTime.now().isBefore(until)) {
        await tester.pump(const Duration(milliseconds: 50));
        await Future<void>.delayed(const Duration(milliseconds: 50));
      }

      await container.read(voiceSessionProvider.notifier).leave();
    } finally {
      await subscription.cancel();
      // ignore: avoid_print
      print(
        'ECHO: self=$selfId volume=$_outputVolume aec=$_echoCancellation '
        'gate=$_sensitivityDbfs',
      );
      await SecureIdentityStore(profile: _profile).clear();
    }
  });
}
