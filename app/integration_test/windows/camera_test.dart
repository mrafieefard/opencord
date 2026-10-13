import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/src/rust/api/client.dart' as core;
import 'package:opencord/ui/widgets/oc_button.dart';

import '../support/e2e.dart';

// Windows plan W5: this PC's camera, through Media Foundation. The app turns
// it on in a real call and a watching voicebot decodes what arrives
// (app/tool/windows/check_v5.ps1 runs the bot and reads its log). There is no
// Linux twin to copy: check_v5.sh's test watches someone *else's* camera,
// which is W4's half.
//
// What this measures, from §8 and V5's acceptance:
//   - how long turning the camera on and off takes, from the app's side;
//   - that the preview is mirrored and what is sent is not;
//   - that the camera's own feed appears and Flutter draws it.
// How many layers went out, and every picture arriving intact, are the
// voicebot's to report.
//
// **This turns the camera on**, so its lamp comes on for a few seconds.
const _invite = String.fromEnvironment('OPENCORD_E2E_INVITE');

/// How long to hold the camera on while the bot watches.
const _holdSeconds = int.fromEnvironment('OPENCORD_E2E_HOLD', defaultValue: 12);

const _profile = 'e2e_camera';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    'the camera goes on, is sent, and goes off',
    skip: _invite.isEmpty,
    (tester) async {
      await SecureIdentityStore(profile: _profile).clear();
      final container = await launch(tester, profile: _profile);
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
        await waitFor(tester, inSidebar('Watch720'));

        // The cameras this PC has, as the app lists them.
        final listed = await core.cameraDevices();
        // ignore: avoid_print
        print('CAMERAS: ${listed.map((camera) => camera.name).join(' | ')}');

        // On. §8 wants this under a second; the clock stops when the app's
        // own feed is there, which is when the user would see it.
        final turningOn = DateTime.now();
        await container.read(voiceSessionProvider.notifier).toggleCamera();
        await waitUntil(
          tester,
          () => container.read(videoFeedsProvider).own != null,
          reason: 'this device\'s camera feed',
        );
        final onMs = DateTime.now().difference(turningOn).inMilliseconds;

        final own = container.read(videoFeedsProvider).own!;
        final texture = own.textureId;
        expect(texture, isNotNull, reason: 'the preview has a texture');
        // §8: the preview is a mirror, what goes out is not.
        expect(own.mirrored, isTrue, reason: 'the preview is mirrored');
        await waitFor(
          tester,
          find.byWidgetPredicate(
            (widget) => widget is Texture && widget.textureId == texture,
          ),
        );
        await waitUntil(tester, () {
          final stats = core.videoTextureStats(textureId: texture!);
          return (stats?.drawn ?? BigInt.zero) >= BigInt.from(10) &&
              (stats?.presented ?? BigInt.zero) >= BigInt.from(10);
        }, reason: 'Flutter drawing the preview');

        // Hold it on while the bot watches and the layers settle.
        final until = DateTime.now().add(Duration(seconds: _holdSeconds));
        while (DateTime.now().isBefore(until)) {
          await tester.pump(const Duration(milliseconds: 50));
          await Future<void>.delayed(const Duration(milliseconds: 50));
        }
        final stats = core.videoTextureStats(textureId: texture!)!;

        // Off, and that has to be quick too: the button waits for it.
        final turningOff = DateTime.now();
        await container.read(voiceSessionProvider.notifier).toggleCamera();
        await waitUntil(
          tester,
          () => container.read(videoFeedsProvider).own == null,
          reason: 'the camera feed going away',
        );
        final offMs = DateTime.now().difference(turningOff).inMilliseconds;

        // ignore: avoid_print
        print(
          'CAMERA: on=${onMs}ms off=${offMs}ms mirrored=${own.mirrored} '
          'size=${own.width}x${own.height} presented=${stats.presented} '
          'drawn=${stats.drawn}',
        );

        await container.read(voiceSessionProvider.notifier).leave();
      } finally {
        await SecureIdentityStore(profile: _profile).clear();
      }
    },
  );
}
