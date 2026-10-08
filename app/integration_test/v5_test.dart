import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/src/rust/api/client.dart' as core;
import 'package:opencord/ui/widgets/oc_button.dart';

import 'support/e2e.dart';

// Phase 2 V5 against a real server: a voicebot in General sends a camera
// of real H.264 (tool/check_v5.sh starts it); the app joins the call, the
// bot's tile shows the camera, and Flutter draws its texture from the
// frames the core decodes at the tile's size. This machine's camera stays
// off. Without the script this test skips.
const _invite = String.fromEnvironment('OPENCORD_E2E_INVITE');
const _profile = 'e2e_video';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    "another person's camera shows in their tile and is drawn",
    skip: _invite.isEmpty,
    (tester) async {
      await SecureIdentityStore(profile: _profile).clear();
      final container = await launch(tester, profile: _profile);
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
        await waitUntil(
          tester,
          () => container.read(videoFeedsProvider).cameras.isNotEmpty,
          reason: "the bot's camera",
        );
        final feed = container.read(videoFeedsProvider).cameras.values.single;
        final texture = feed.textureId;
        expect(texture, isNotNull, reason: 'the app gave the core its engine');
        await waitFor(
          tester,
          find.byWidgetPredicate(
            (widget) => widget is Texture && widget.textureId == texture,
          ),
        );
        await waitUntil(
          tester,
          () =>
              (core.videoTextureStats(textureId: texture!)?.drawn ??
                  BigInt.zero) >=
              BigInt.from(30),
          reason: 'Flutter drawing the camera',
        );

        final stats = core.videoTextureStats(textureId: texture!)!;
        expect(stats.presented, greaterThanOrEqualTo(BigInt.from(30)));
        expect(container.read(videoWantsProvider), isNotEmpty);

        await container.read(voiceSessionProvider.notifier).leave();
        await waitUntil(
          tester,
          () => container.read(voiceConnectionProvider) == null,
          reason: 'leaving voice',
        );
        await waitUntil(
          tester,
          () => core.videoTextureStats(textureId: texture) == null,
          reason: 'the texture released',
        );
      } finally {
        await SecureIdentityStore(profile: _profile).clear();
      }
    },
  );
}
