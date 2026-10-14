import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/features/voice/voice_controls.dart';
import 'package:opencord/src/rust/api/client.dart' as core;
import 'package:opencord/ui/widgets/oc_button.dart';

import '../support/e2e.dart';

// The Mac's own screen share (docs/plans/macos.md M6, step 6), from
// app/tool/macos/check_v6.sh: with OPENCORD_SCREEN_DISPLAY=main the core
// shares the main display without the system picker. The app goes live at
// the quality the script names (720p30 by default), holds while the
// script's voicebot watches and decodes, then stops, which ends the stream
// for everyone. A voicebot's camera in the call keeps the screen moving.
// Needs Screen Recording for the terminal app (the app under
// `flutter test` is its child). Without the script this test skips.
const _invite = String.fromEnvironment('OPENCORD_E2E_INVITE');
const _resolution = String.fromEnvironment(
  'OPENCORD_E2E_RESOLUTION',
  defaultValue: '720p',
);
const _fps = String.fromEnvironment('OPENCORD_E2E_FPS', defaultValue: '30');
const _profile = 'e2e_screen_share';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    'the main display goes live, is watched, and stops',
    skip: _invite.isEmpty,
    (tester) async {
      await SecureIdentityStore(profile: _profile).clear();
      final container = await launch(tester, profile: _profile);

      Future<void> hold(Duration length) async {
        final end = DateTime.now().add(length);
        while (DateTime.now().isBefore(end)) {
          await tester.pump(const Duration(milliseconds: 50));
          await Future<void>.delayed(const Duration(milliseconds: 50));
        }
      }

      try {
        await createIdentity(tester, 'Sharer');
        await startAddingServer(tester, _invite);
        await tester.tap(find.widgetWithText(OcButton, 'Join'));
        await waitFor(tester, inSidebar('General'));
        final serverKey = container.read(serverListProvider).single.key;
        await tester.tap(inSidebar('General'));
        await waitUntil(
          tester,
          () =>
              container.read(voiceConnectionProvider)?.phase ==
              VoiceConnectionPhase.connected,
          reason: 'voice media connecting',
        );
        // The camera voicebot's tile keeps the shared screen moving.
        await waitUntil(
          tester,
          () => container.read(videoFeedsProvider).cameras.isNotEmpty,
          reason: "the voicebot's camera",
        );

        await tester.tap(
          find.descendant(
            of: find.byType(VoiceControlBar),
            matching: find.bySemanticsLabel('Share your screen'),
          ),
        );
        await waitFor(tester, find.widgetWithText(OcButton, 'Go live'));
        // The dialog says the system picks, as the stub's own_picker says.
        expect(find.textContaining('asks which screen or window'), findsOne);
        await tester.tap(find.text(_resolution));
        await tester.tap(find.text('$_fps fps'));
        await tester.pump();
        final asked = Stopwatch()..start();
        await tester.tap(find.widgetWithText(OcButton, 'Go live'));
        await waitUntil(
          tester,
          () => container.read(ownScreenShareProvider) != null,
          reason: 'going live',
        );
        asked.stop();
        final share = container.read(ownScreenShareProvider)!;
        // ignore: avoid_print
        print(
          'SHARE LIVE: ${asked.elapsedMilliseconds} ms, ${share.source.name}, '
          '${share.preview.width}x${share.preview.height}, '
          '${share.quality.resolution.label} ${share.quality.fps} fps',
        );
        // ignore: avoid_print
        print('PHASE: live');
        await hold(const Duration(seconds: 16));
        final preview = core.videoTextureStats(
          textureId: share.preview.textureId!,
        );
        // ignore: avoid_print
        print(
          'SHARE PREVIEW: ${preview?.presented} presented, '
          '${preview?.drawn} drawn; viewers '
          '${container.read(streamsProvider(serverKey)).values.firstOrNull?.viewerCount}',
        );

        await container.read(ownScreenShareProvider.notifier).stop();
        await waitUntil(
          tester,
          () =>
              container.read(ownScreenShareProvider) == null &&
              container.read(streamsProvider(serverKey)).isEmpty,
          reason: 'the share over for everyone',
        );
        // ignore: avoid_print
        print('PHASE: done');
        await hold(const Duration(seconds: 2));
        await container.read(voiceSessionProvider.notifier).leave();
      } finally {
        await SecureIdentityStore(profile: _profile).clear();
      }
    },
  );
}
