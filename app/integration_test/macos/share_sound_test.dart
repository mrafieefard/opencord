import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/voice/voice_controls.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/settings.dart';

import '../support/e2e.dart';

// V7 on the Mac (docs/plans/macos.md M7, step 6), from
// app/tool/macos/check_v7.sh: plan §10.7's "no double audio" test. The app
// shares with its sound (the main display through OPENCORD_SCREEN_DISPLAY,
// or, with OPENCORD_E2E_WINDOW, a window the user picks in the system's
// picker) while another app plays; Talker talks and the app plays him into
// BlackHole; Watcher measures the stream. The app's own microphone is
// muted (it is BlackHole too). Without the script this test skips.
const _invite = String.fromEnvironment('OPENCORD_E2E_INVITE');
const _device = String.fromEnvironment('OPENCORD_E2E_DEVICE');
const _window = bool.fromEnvironment('OPENCORD_E2E_WINDOW');
const _profile = 'e2e_share_sound';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    'a share with its sound sends what it shows, never Opencord',
    skip: _invite.isEmpty,
    timeout: const Timeout(Duration(minutes: 5)),
    (tester) async {
      await SecureIdentityStore(profile: _profile).clear();
      final container = await launch(tester, profile: _profile);

      // Without frames: a window share puts the shared window in front, and
      // macOS draws nothing for a window that is covered.
      Future<void> hold(Duration length) => Future<void>.delayed(length);

      try {
        await createIdentity(tester, 'Streamer');
        await startAddingServer(tester, _invite);
        await tester.tap(find.widgetWithText(OcButton, 'Join'));
        await waitFor(tester, inSidebar('General'));
        final server = container.read(serverListProvider).single.key;
        container
            .read(audioSettingsProvider.notifier)
            .update(
              (audio) => audio
                  .withInput(const AudioDevice(id: _device, name: 'BlackHole'))
                  .withOutput(
                    const AudioDevice(id: _device, name: 'BlackHole'),
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
        if (!container.read(voiceSessionProvider).muted) {
          await container.read(voiceSessionProvider.notifier).toggleMute();
        }
        await waitFor(tester, inSidebar('Talker'));

        await tester.tap(
          find.descendant(
            of: find.byType(VoiceControlBar),
            matching: find.bySemanticsLabel('Share your screen'),
          ),
        );
        await waitFor(tester, find.widgetWithText(OcButton, 'Go live'));
        await waitUntil(
          tester,
          () => tester
              .widgetList<SettingsSwitchRow>(
                find.widgetWithText(SettingsSwitchRow, 'Share sound'),
              )
              .any((row) => row.onChanged != null),
          reason: 'the sound switch enabled (macOS 14.2)',
        );
        final row = tester.widget<SettingsSwitchRow>(
          find.widgetWithText(SettingsSwitchRow, 'Share sound'),
        );
        if (!row.value) {
          await tester.tap(
            find.widgetWithText(SettingsSwitchRow, 'Share sound'),
          );
          await tester.pump();
        }
        await tester.ensureVisible(find.widgetWithText(OcButton, 'Go live'));
        await tester.pump();
        // ignore: avoid_print
        if (_window) print('PICK: the window now');
        await tester.tap(find.widgetWithText(OcButton, 'Go live'));
        await waitUntil(
          tester,
          () => container.read(ownScreenShareProvider)?.audio ?? false,
          reason: 'going live with sound',
          timeout: const Duration(minutes: 3),
        );
        final share = container.read(ownScreenShareProvider)!;
        // ignore: avoid_print
        print(
          'SHARE SOUND: ${share.source.name}, sound from '
          '${share.audioFrom ?? 'everything but Opencord'}',
        );
        await waitUntil(
          tester,
          () =>
              container
                  .read(streamsProvider(server))
                  .values
                  .firstOrNull
                  ?.viewerCount ==
              1,
          reason: 'Watcher watching',
        );
        // ignore: avoid_print
        print('PHASE: live');
        // Long enough for Watcher to hear several seconds of each.
        await hold(const Duration(seconds: 12));
        await container.read(ownScreenShareProvider.notifier).stop();
        await hold(const Duration(seconds: 1));
        // ignore: avoid_print
        print('PHASE: done');
        await container.read(voiceSessionProvider.notifier).leave();
      } finally {
        await SecureIdentityStore(profile: _profile).clear();
      }
    },
  );
}
