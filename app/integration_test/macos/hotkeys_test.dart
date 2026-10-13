import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

import '../support/e2e.dart';

// Global push-to-talk on macOS (docs/plans/macos.md M2, step 8), from
// app/tool/macos/check_hotkeys.sh: afplay plays a 1 kHz tone into
// BlackHole 2ch, the app's microphone, and the app is in push-to-talk with
// F13 as its key. The app checks that the keybinds are global, then holds
// three phases while the script focuses TextEdit and posts F13 down and up
// as real key events; for each, a listening voicebot measures the tone in
// what the app sends. Needs Input Monitoring for the terminal app (the app
// under `flutter test` is its child). Without the script this test skips.
const _invite = String.fromEnvironment('OPENCORD_E2E_INVITE');

/// The cpal ids of the devices the app plays into and records from.
const _output = String.fromEnvironment('OPENCORD_E2E_OUTPUT');
const _input = String.fromEnvironment('OPENCORD_E2E_INPUT');
const _profile = 'e2e_hotkeys';

/// How long each phase lasts: the script posts its key, then its listener
/// joins and listens for 5 s.
const _phase = Duration(seconds: 12);

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    'push-to-talk on a global key, while another app is focused',
    skip: _invite.isEmpty,
    (tester) async {
      await SecureIdentityStore(profile: _profile).clear();
      final container = await launch(tester, profile: _profile);
      final failures = <AudioDeviceFailed>[];
      final subscription = container.read(repositoryProvider).events.listen((
        event,
      ) {
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
        container
            .read(keybindsProvider.notifier)
            .add(HotkeyAction.pushToTalk, keys: 'F13');
        await waitUntil(
          tester,
          () => container.read(hotkeySupportProvider) is HotkeysGlobal,
          reason:
              'global keybinds (${container.read(hotkeySupportProvider)}); '
              'allow the terminal app under Input Monitoring',
        );
        // ignore: avoid_print
        print(
          'HOTKEYS: ${switch (container.read(hotkeySupportProvider)) {
            HotkeysGlobal(:final method) => 'global, through $method',
            final other => '$other',
          }}',
        );

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
                    inputMode: InputMode.pushToTalk,
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

        await hold('not held');
        await hold('held');
        await hold('released');
        // ignore: avoid_print
        print('PHASE: done');

        expect(failures.map((failure) => failure.message), isEmpty);
        await container.read(voiceSessionProvider.notifier).leave();
      } finally {
        await subscription.cancel();
        await SecureIdentityStore(profile: _profile).clear();
      }
    },
  );
}
