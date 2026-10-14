import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/features/voice/voice_tile.dart';
import 'package:opencord/src/rust/api/client.dart' as core;
import 'package:opencord/ui/widgets/oc_button.dart';

import '../support/e2e.dart';

// Others' video in the app on macOS (docs/plans/macos.md M4), from
// app/tool/macos/check_receive_video.sh, after v5_test.dart's receive half:
// a voicebot's camera (real H.264, the moving test scene) shows in its tile
// and Flutter draws it; watching a voicebot's screen stream
// opens it large (focus mode) and draws it; and once the voice view is
// gone, nothing is wanted and no more pictures come. Without the script
// this test skips.
const _invite = String.fromEnvironment('OPENCORD_E2E_INVITE');
const _profile = 'e2e_receive_video';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    "others' camera and screen show, and stop when hidden",
    skip: _invite.isEmpty,
    (tester) async {
      await SecureIdentityStore(profile: _profile).clear();
      final container = await launch(tester, profile: _profile);

      BigInt drawn(int texture) =>
          core.videoTextureStats(textureId: texture)?.drawn ?? BigInt.zero;
      BigInt presented(int texture) =>
          core.videoTextureStats(textureId: texture)?.presented ?? BigInt.zero;

      try {
        await createIdentity(tester, 'Watcher');
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

        // The camera.
        await waitUntil(
          tester,
          () => container.read(videoFeedsProvider).cameras.isNotEmpty,
          reason: "the bot's camera",
        );
        final camera = container
            .read(videoFeedsProvider)
            .cameras
            .values
            .single
            .textureId!;
        await waitUntil(
          tester,
          () => drawn(camera) >= BigInt.from(30),
          reason: 'Flutter drawing the camera',
        );
        // The screen stream: watching opens it large.
        final watch = find.widgetWithText(OcButton, 'Watch');
        await waitFor(tester, watch);
        await tester.tap(watch);
        await waitUntil(
          tester,
          () => container.read(videoFeedsProvider).screens.isNotEmpty,
          reason: "the bot's screen",
        );
        final screen = container
            .read(videoFeedsProvider)
            .screens
            .values
            .single
            .textureId!;
        await waitFor(
          tester,
          find.byWidgetPredicate(
            (widget) =>
                widget is VoiceTileView && widget.screen && widget.focused,
          ),
        );
        await waitUntil(
          tester,
          () => drawn(screen) >= BigInt.from(30),
          reason: 'Flutter drawing the screen stream',
        );
        final screenStats = core.videoTextureStats(textureId: screen)!;
        final cameraStats = core.videoTextureStats(textureId: camera)!;
        // ignore: avoid_print
        print(
          'RECEIVE STATS: camera ${cameraStats.presented} presented, '
          '${cameraStats.drawn} drawn; screen ${screenStats.presented} '
          'presented, ${screenStats.drawn} drawn; '
          'wants ${container.read(videoWantsProvider).length}',
        );
        expect(container.read(videoWantsProvider), hasLength(2));

        // Hidden: a text channel instead of the voice view.
        await tester.tap(inSidebar('general'));
        await waitUntil(
          tester,
          () => container.read(videoWantsProvider).isEmpty,
          reason: 'the hidden tiles wanting nothing',
        );
        await _settle(tester, const Duration(seconds: 1));
        final before = (presented(camera), presented(screen));
        await _settle(tester, const Duration(seconds: 3));
        final after = (presented(camera), presented(screen));
        // ignore: avoid_print
        print(
          'RECEIVE HIDDEN: in 3 s, camera +${after.$1 - before.$1}, '
          'screen +${after.$2 - before.$2} pictures',
        );
        expect(after.$1 - before.$1, lessThanOrEqualTo(BigInt.from(1)));
        expect(after.$2 - before.$2, lessThanOrEqualTo(BigInt.from(1)));

        await container.read(voiceSessionProvider.notifier).leave();
      } finally {
        await SecureIdentityStore(profile: _profile).clear();
      }
    },
  );
}

/// Lets `length` pass with frames drawn.
Future<void> _settle(WidgetTester tester, Duration length) async {
  final end = DateTime.now().add(length);
  while (DateTime.now().isBefore(end)) {
    await tester.pump(const Duration(milliseconds: 50));
    await Future<void>.delayed(const Duration(milliseconds: 50));
  }
}
