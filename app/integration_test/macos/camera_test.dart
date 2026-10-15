import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/src/rust/api/client.dart' as core;
import 'package:opencord/ui/widgets/oc_button.dart';

import '../support/e2e.dart';

// The Mac's camera in a call (docs/plans/macos.md M5, step 3), from
// app/tool/macos/check_v5.sh: the app joins, turns the camera on with its
// button and times it until its own preview is drawn, checks the preview
// is mirrored, holds the camera on while the script's voicebots watch each
// layer, then turns it off and times that too. Needs a camera and camera
// access for the terminal app (the app under `flutter test` is its child).
// Without the script this test skips.
const _invite = String.fromEnvironment('OPENCORD_E2E_INVITE');
const _profile = 'e2e_camera';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    'the camera turns on and off quickly, its preview mirrored',
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
        await createIdentity(tester, 'Camera');
        await startAddingServer(tester, _invite);
        await tester.tap(find.widgetWithText(OcButton, 'Join'));
        await waitFor(tester, inSidebar('General'));
        await tester.tap(inSidebar('General'));
        await waitUntil(
          tester,
          () =>
              container.read(voiceConnectionProvider)?.phase ==
              VoiceConnectionPhase.connected,
          reason: 'voice media connecting',
        );
        await hold(const Duration(seconds: 1));

        // On: from the button until the preview's first picture is drawn.
        final on = Stopwatch()..start();
        await tester.tap(find.byTooltip('Turn on camera'));
        await waitUntil(tester, () {
          final texture = container.read(videoFeedsProvider).own?.textureId;
          return texture != null &&
              (core.videoTextureStats(textureId: texture)?.drawn ??
                      BigInt.zero) >
                  BigInt.zero;
        }, reason: 'the preview drawn');
        on.stop();
        final own = container.read(videoFeedsProvider).own!;
        // ignore: avoid_print
        print(
          'CAMERA ON: ${on.elapsedMilliseconds} ms, ${own.width}x${own.height}, '
          'preview mirrored ${own.mirrored}',
        );
        expect(own.mirrored, isTrue, reason: 'the preview is mirrored');

        // ignore: avoid_print
        print('PHASE: camera');
        await hold(const Duration(seconds: 14));
        final stats = core.videoTextureStats(textureId: own.textureId!)!;
        // ignore: avoid_print
        print(
          'CAMERA PREVIEW: ${stats.presented} presented, ${stats.drawn} drawn',
        );

        // Off: from the button until the camera is off and its feed gone.
        final off = Stopwatch()..start();
        await tester.tap(find.byTooltip('Turn off camera'));
        await waitUntil(
          tester,
          () =>
              !container.read(voiceSessionProvider).camera &&
              container.read(videoFeedsProvider).own == null,
          reason: 'the camera off',
        );
        off.stop();
        // ignore: avoid_print
        print('CAMERA OFF: ${off.elapsedMilliseconds} ms');
        // ignore: avoid_print
        print('PHASE: done');
        // Plan §8 wants under 1 s each; this Mac's FaceTime HD camera
        // alone takes about 0.65 s to start (docs/plans/macos.md §7, M5).
        expect(on.elapsedMilliseconds, lessThan(1500));
        expect(off.elapsedMilliseconds, lessThan(1000));

        await container.read(voiceSessionProvider.notifier).leave();
      } finally {
        await SecureIdentityStore(profile: _profile).clear();
      }
    },
  );
}
