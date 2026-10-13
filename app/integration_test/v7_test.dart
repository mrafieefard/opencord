import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/voice/voice_controls.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/settings.dart';

import 'support/e2e.dart';

// Phase 2 V7 against a real server (app/tool/check_v7.sh), the "no double
// audio" test of plan §10.7: the app shares a screen (a virtual one) with
// its sound while another app plays music; Talker talks; Watcher watches
// the stream and measures what it hears. The app plays Talker's voice into
// a null sink of its own (Opencord's own sound, which must not be shared)
// and records from a silent one. Without the script this test skips.
const _invite = String.fromEnvironment('OPENCORD_E2E_INVITE');
const _sink = String.fromEnvironment('OPENCORD_E2E_SINK');
const _silence = String.fromEnvironment('OPENCORD_E2E_SILENCE');
const _profile = 'e2e_share_sound';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    'a screen shared with its sound sends every app but Opencord',
    skip: _invite.isEmpty,
    (tester) async {
      await SecureIdentityStore(profile: _profile).clear();
      final container = await launch(tester, profile: _profile);
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
                  .withInput(
                    AudioDevice(id: 'pipewire:$_silence', name: 'Silence'),
                  )
                  .withOutput(
                    AudioDevice(id: 'pipewire:$_sink', name: 'Speaker'),
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

        await tester.tap(
          find.descendant(
            of: find.byType(VoiceControlBar),
            matching: find.bySemanticsLabel('Share your screen'),
          ),
        );
        await waitFor(tester, find.widgetWithText(OcButton, 'Go live'));
        // PipeWire is here: the sound switch is on.
        await waitUntil(
          tester,
          () => tester
              .widgetList<SettingsSwitchRow>(
                find.widgetWithText(SettingsSwitchRow, 'Share sound'),
              )
              .any((row) => row.value && row.onChanged != null),
          reason: 'the sound switch',
        );
        await tester.ensureVisible(find.widgetWithText(OcButton, 'Go live'));
        await tester.pump();
        await tester.tap(find.widgetWithText(OcButton, 'Go live'));
        await waitUntil(
          tester,
          () => container.read(ownScreenShareProvider)?.audio ?? false,
          reason:
              'going live with sound (share: '
              '${container.read(ownScreenShareProvider)}, screensharing: '
              '${container.read(voiceSessionProvider).screensharing}, on '
              'screen: ${find.byType(Text).evaluate().map((e) => (e.widget as Text).data).nonNulls.take(60).join(' | ')})',
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
        // Long enough for the Watcher to hear several seconds of both.
        final end = DateTime.now().add(const Duration(seconds: 8));
        while (DateTime.now().isBefore(end)) {
          await tester.pump(const Duration(milliseconds: 100));
          await Future<void>.delayed(const Duration(milliseconds: 100));
        }
        await container.read(ownScreenShareProvider.notifier).stop();
        await tester.pump(const Duration(seconds: 1));
        await container.read(voiceSessionProvider.notifier).leave();
      } finally {
        await SecureIdentityStore(profile: _profile).clear();
      }
    },
  );
}
