import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

import '../support/e2e.dart';

// V3's echo acceptance on a Mac's own speakers and microphone
// (docs/plans/macos.md M1, step 7), from app/tool/macos/check_echo.sh:
// Talker, a voicebot, sends 440 Hz at about -12 dBFS; the app plays it on
// the speakers and sends what its microphone hears, push-to-talk held so it
// sends throughout. In each phase the script's Listener voicebot measures
// 440 Hz in what the app sends: first with the app's processing as chosen,
// then with echo cancellation and noise suppression off, which shows the
// echo is there to cancel. Without the script this test skips.
const _invite = String.fromEnvironment('OPENCORD_E2E_INVITE');
const _profile = 'e2e_echo';

/// How long each phase lasts: the script's listener joins, then listens
/// for 5 s.
const _phase = Duration(seconds: 10);

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    'the echo of what the app plays is cancelled',
    skip: _invite.isEmpty,
    (tester) async {
      await SecureIdentityStore(profile: _profile).clear();
      final container = await launch(tester, profile: _profile);
      final repository = container.read(repositoryProvider);
      final failures = <AudioDeviceFailed>[];
      final subscription = repository.events.listen((event) {
        if (event is AudioDeviceFailed) failures.add(event);
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
        await createIdentity(tester, 'Speaker');
        await startAddingServer(tester, _invite);
        await tester.tap(find.widgetWithText(OcButton, 'Join'));
        await waitFor(tester, inSidebar('General'));
        // The system's default devices: the script makes them the Mac's
        // own speakers and microphone.
        container
            .read(audioSettingsProvider.notifier)
            .update(
              (audio) => audio
                  .withInput(null)
                  .withOutput(null)
                  .copyWith(inputMode: InputMode.pushToTalk),
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
        repository.setPushToTalk(true);
        // The echo canceller settles on the room first.
        await hold('settling');
        final chosen = container.read(audioSettingsProvider);
        // ignore: avoid_print
        print(
          'ECHO SETTINGS: echo cancellation ${chosen.echoCancellation}, '
          'noise suppression ${chosen.noiseSuppression?.name}',
        );
        await hold('as chosen');
        container
            .read(audioSettingsProvider.notifier)
            .update(
              (audio) => audio.copyWith(
                echoCancellation: false,
                noiseSuppression: NoiseSuppression.off,
              ),
            );
        await tester.pump();
        await hold('echo cancellation off');
        // ignore: avoid_print
        print('PHASE: done');
        repository.setPushToTalk(false);

        expect(failures.map((failure) => failure.message), isEmpty);
        await container.read(voiceSessionProvider.notifier).leave();
      } finally {
        await subscription.cancel();
        await SecureIdentityStore(profile: _profile).clear();
      }
    },
  );
}
