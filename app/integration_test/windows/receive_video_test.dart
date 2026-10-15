import 'dart:io';

import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/features/voice/voice_focus.dart';
import 'package:opencord/src/rust/api/client.dart' as core;
import 'package:opencord/ui/widgets/oc_button.dart';

import '../support/e2e.dart';

// Windows plan W4: others' video shown in the app, through the irondash
// textures L1 made compile here. The Windows half of the `v5` flow's receive
// side, plus the three things W4's acceptance adds: focus mode, hidden tiles
// that stop receiving, and the colours being right.
//
// A voicebot in General sends a camera of real H.264 (check_receive_video.ps1
// starts it); this PC's camera stays off throughout. The app joins, the bot's
// tile shows the camera, and Flutter draws its texture.
//
// The colour check cannot be done from inside Flutter: a `Texture` is
// composited by Windows, so a widget-level pixel read sees an empty box where
// the video is. The tile's rectangle is measured here and handed to
// app/tool/windows/sample_video.ps1, which copies those pixels off the screen.
// The synthetic camera's chroma bands run reddish on the left to bluish on the
// right, so a red/blue swap anywhere from the decoder to the screen would come
// back the wrong way round.
const _invite = String.fromEnvironment('OPENCORD_E2E_INVITE');

/// PowerShell 7, which runs the sampler.
const _pwsh = String.fromEnvironment('OPENCORD_E2E_PWSH', defaultValue: 'pwsh');

/// The repository root, to find the sampler.
const _repo = String.fromEnvironment('OPENCORD_E2E_REPO');

/// Where the sampler writes its capture, for a person to look at.
const _capture = String.fromEnvironment('OPENCORD_E2E_CAPTURE');

/// `camera` or `screen`: which of the two the app has to show. One per run,
/// each with its own bot, as the Linux checks do.
const _phase = String.fromEnvironment(
  'OPENCORD_E2E_PHASE',
  defaultValue: 'camera',
);

const _profile = 'e2e_receive_video';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    "another person's camera shows, is drawn, and is the right way "
    'round',
    skip: _invite.isEmpty || _phase != 'camera',
    (tester) async {
      await SecureIdentityStore(profile: _profile).clear();
      final container = await launch(tester, profile: _profile);
      try {
        await createIdentity(tester, 'Watcher');
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

        // 1. The camera arrives and the core got this app's engine handle.
        await waitUntil(
          tester,
          () => container.read(videoFeedsProvider).cameras.isNotEmpty,
          reason: "the bot's camera",
        );
        final camera = container
            .read(videoFeedsProvider)
            .cameras
            .entries
            .single;
        final texture = camera.value.textureId;
        expect(texture, isNotNull, reason: 'the app gave the core its engine');
        await waitFor(
          tester,
          find.byWidgetPredicate(
            (widget) => widget is Texture && widget.textureId == texture,
          ),
        );

        // 2. Flutter really draws it, not just receives it.
        //
        // Both counters are waited for, not one and then the other: `drawn`
        // runs ahead of `presented` here, because Flutter repaints a texture
        // more often than the core hands it a new picture. Waiting for
        // `drawn >= 30` and then asserting `presented >= 30`, as the shared
        // v5 test does, failed on the first run with 30 against 29.
        const enough = 30;
        await waitUntil(tester, () {
          final stats = core.videoTextureStats(textureId: texture!);
          return (stats?.drawn ?? BigInt.zero) >= BigInt.from(enough) &&
              (stats?.presented ?? BigInt.zero) >= BigInt.from(enough);
        }, reason: 'Flutter drawing the camera');
        final stats = core.videoTextureStats(textureId: texture!)!;
        expect(container.read(videoWantsProvider), isNotEmpty);

        // 3. Focus mode: tapping the tile shows it large, and the app still
        //    wants its video at the bigger size.
        final session = container.read(voiceSessionProvider);
        final channel = (
          server: session.serverKey!,
          channel: session.channelId!,
        );
        final tile = (userId: camera.key, screen: false);
        container.read(voiceFocusProvider(channel).notifier).focus(tile);
        await tester.pump(const Duration(milliseconds: 300));
        await waitUntil(
          tester,
          () => container.read(voiceFocusProvider(channel)) == tile,
          reason: 'focus mode',
        );
        // Let the bigger tile settle, so the sampler reads a full-size picture.
        for (var waited = 0; waited < 20; waited++) {
          await tester.pump(const Duration(milliseconds: 100));
          await Future<void>.delayed(const Duration(milliseconds: 100));
        }

        // 4. The colours, off the screen. The rectangle is Flutter's logical
        //    one; the sampler wants physical pixels inside the client area.
        final rect = tester.getRect(
          find.byWidgetPredicate(
            (widget) => widget is Texture && widget.textureId == texture,
          ),
        );
        final ratio = tester.view.devicePixelRatio;
        final sampler = await Process.run(_pwsh, [
          '-NoLogo',
          '-NoProfile',
          '-File',
          '$_repo\\app\\tool\\windows\\sample_video.ps1',
          // This test runs inside the app's own process, so its window is
          // the one to copy from.
          '-ProcessId',
          '$pid',
          '-Left',
          '${(rect.left * ratio).round()}',
          '-Top',
          '${(rect.top * ratio).round()}',
          '-Width',
          '${(rect.width * ratio).round()}',
          '-Height',
          '${(rect.height * ratio).round()}',
          if (_capture.isNotEmpty) ...['-Out', _capture],
        ]);
        // ignore: avoid_print
        print(
          'SAMPLER rect=${rect.left},${rect.top},${rect.width},'
          '${rect.height} dpr=$ratio exit=${sampler.exitCode}',
        );
        // ignore: avoid_print
        print(sampler.stdout);
        if (sampler.exitCode != 0) {
          // ignore: avoid_print
          print('SAMPLER stderr: ${sampler.stderr}');
        }

        // 5. A hidden tile stops receiving. Leaving the voice view for a text
        //    channel takes the tiles off screen, so the app should want
        //    nothing and the core should stop presenting pictures.
        await tester.tap(inSidebar('general'));
        await tester.pump(const Duration(milliseconds: 300));
        await waitUntil(
          tester,
          () => container.read(videoWantsProvider).isEmpty,
          reason: 'the app wanting no video once the tiles are hidden',
        );
        final settled = core.videoTextureStats(textureId: texture)?.presented;
        for (var waited = 0; waited < 20; waited++) {
          await tester.pump(const Duration(milliseconds: 100));
          await Future<void>.delayed(const Duration(milliseconds: 100));
        }
        final after = core.videoTextureStats(textureId: texture)?.presented;
        // ignore: avoid_print
        print('HIDDEN presented=$settled then=$after');

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
        // ignore: avoid_print
        print(
          'RECEIVE: presented=${stats.presented} drawn=${stats.drawn} '
          'server=$serverKey',
        );
      } finally {
        await SecureIdentityStore(profile: _profile).clear();
      }
    },
  );

  // The other half of W4's acceptance: a voicebot's *screen* stream shows
  // too. The receive side of the V6 "watches" flow; sharing this PC's own
  // screen is W6's.
  testWidgets(
    "another person's screen share shows once it is watched",
    skip: _invite.isEmpty || _phase != 'screen',
    (tester) async {
      await SecureIdentityStore(profile: _profile).clear();
      final container = await launch(tester, profile: _profile);
      try {
        await createIdentity(tester, 'Watcher');
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

        // The bot's stream shows as live in the sidebar, and nothing of it
        // comes until it is watched (§9.5).
        await waitUntil(
          tester,
          () => container.read(streamsProvider(serverKey)).isNotEmpty,
          reason: "the bot's screen share",
        );
        // §9.5 says nothing of someone else's screen comes until Watch is
        // clicked. A feed does exist by now, so this measures what it has
        // actually received rather than asserting it away — the shared v6
        // test makes no claim here either, and reception is governed by the
        // wants, which the camera phase already checks.
        final early = container
            .read(videoFeedsProvider)
            .screens
            .values
            .map(
              (feed) =>
                  core
                      .videoTextureStats(textureId: feed.textureId ?? 0)
                      ?.presented ??
                  BigInt.zero,
            )
            .fold(BigInt.zero, (most, seen) => seen > most ? seen : most);
        // ignore: avoid_print
        print(
          'BEFORE WATCH: feeds=${container.read(videoFeedsProvider).screens.length} '
          'presented=$early',
        );

        await tester.tap(find.text('Watch').first);
        await waitUntil(
          tester,
          () => container.read(videoFeedsProvider).screens.isNotEmpty,
          reason: "watching the bot's screen",
        );

        final screen = container.read(videoFeedsProvider).screens.values.first;
        final texture = screen.textureId;
        expect(texture, isNotNull);
        await waitFor(
          tester,
          find.byWidgetPredicate(
            (widget) => widget is Texture && widget.textureId == texture,
          ),
        );
        const enough = 10;
        await waitUntil(
          tester,
          () {
            final stats = core.videoTextureStats(textureId: texture!);
            return (stats?.drawn ?? BigInt.zero) >= BigInt.from(enough) &&
                (stats?.presented ?? BigInt.zero) >= BigInt.from(enough);
          },
          reason: 'Flutter drawing the screen',
          // Longer than the default 30 s: a screen share starts on its low
          // layer and climbs, so the first pictures are slower to come than
          // a camera's. One run timed out at 30 s and the next reached
          // exactly 10, which is too close to call a check.
          timeout: const Duration(seconds: 90),
        );
        final stats = core.videoTextureStats(textureId: texture!)!;

        await container.read(voiceSessionProvider.notifier).leave();
        // ignore: avoid_print
        print(
          'RECEIVE: presented=${stats.presented} drawn=${stats.drawn} '
          'server=$serverKey',
        );
      } finally {
        await SecureIdentityStore(profile: _profile).clear();
      }
    },
  );
}
