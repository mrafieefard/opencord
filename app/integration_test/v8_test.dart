import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/features/soundboard/soundboard_picker.dart';
import 'package:opencord/features/soundboard/soundboard_ring.dart';
import 'package:opencord/features/voice/voice_controls.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

import 'support/e2e.dart';

// Phase 2 V8 against a real server (app/tool/check_v8.sh): the owner added a
// sound called Beep; a bot in General plays a default sound every few
// seconds. The app gets Beep with the server, shows the bot's soundboard
// ring, and plays Beep from the picker. Without the script this test skips.
const _invite = String.fromEnvironment('OPENCORD_E2E_INVITE');
const _profile = 'e2e_soundboard';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    "the server's sound plays from the picker, and others' sounds show",
    skip: _invite.isEmpty,
    (tester) async {
      await SecureIdentityStore(profile: _profile).clear();
      final container = await launch(tester, profile: _profile);
      try {
        await createIdentity(tester, 'Listener');
        await startAddingServer(tester, _invite);
        await tester.tap(find.widgetWithText(OcButton, 'Join'));
        await waitFor(tester, inSidebar('General'));
        final server = container.read(serverListProvider).single.key;
        await waitUntil(
          tester,
          () => container
              .read(soundsProvider(server))
              .values
              .any((sound) => sound.name == 'Beep'),
          reason: 'the uploaded sound',
        );

        await tester.tap(inSidebar('General'));
        await waitUntil(
          tester,
          () =>
              container.read(voiceConnectionProvider)?.phase ==
              VoiceConnectionPhase.connected,
          reason: 'voice media connecting',
        );

        // The bot's sounds show its dashed ring while they play.
        await waitUntil(
          tester,
          () => container.read(soundPlaysProvider(server)).isNotEmpty,
          reason: "the bot's sound",
        );
        await waitFor(
          tester,
          find.byWidgetPredicate(
            (widget) => widget is SoundboardRing && widget.playing,
          ),
        );

        await tester.tap(
          find.descendant(
            of: find.byType(VoiceControlBar),
            matching: find.bySemanticsLabel('Soundboard'),
          ),
        );
        await waitFor(tester, find.byType(SoundboardPicker));
        await tester.tap(
          find.descendant(
            of: find.byType(SoundboardPicker),
            matching: find.text('Beep'),
          ),
        );
        final self = container.read(serverProvider(server)).data!.self.id;
        await waitUntil(
          tester,
          () =>
              container.read(soundPlaysProvider(server))[self]?.name == 'Beep',
          reason: 'the server announcing our sound to the channel',
        );

        await container.read(voiceSessionProvider.notifier).leave();
      } finally {
        await SecureIdentityStore(profile: _profile).clear();
      }
    },
  );
}
