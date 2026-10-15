import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter/painting.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/features/voice/voice_focus.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

import 'support/e2e.dart';
import 'support/measure.dart';

// Phase 2 V10's client numbers against a real server (plan §16.1), from
// tool/check_v10_load.sh, in a profile build. Two modes:
// - "call": ten people, four cameras and a stream. Three voicebots send
//   cameras, one shares a 1080p30 screen, five sit there, one of them
//   talking; the app turns its camera on (a virtual camera) and watches the
//   stream large. Reports its memory and CPU, and each tile's pictures.
// - "watch": one voicebot shares a 1080p60 screen; the app watches it
//   large and reports its CPU.
// Without the script this test skips.
const _invite = String.fromEnvironment('OPENCORD_E2E_INVITE');
const _camera = String.fromEnvironment('OPENCORD_E2E_CAMERA');
const _mode = String.fromEnvironment('OPENCORD_E2E_MODE');
const _profile = 'e2e_v10_load';
const _measured = Duration(
  seconds: int.fromEnvironment('OPENCORD_E2E_SECONDS', defaultValue: 20),
);

void main() {
  final binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    "the client's memory and CPU in a call (plan §16.1)",
    skip: _invite.isEmpty || _mode.isEmpty,
    (tester) async {
      binding.testTextInput.register();
      addTearDown(binding.testTextInput.unregister);
      // A maximized window on a 1080p laptop screen.
      await binding.setSurfaceSize(const Size(1920, 1080));
      addTearDown(() => binding.setSurfaceSize(null));
      await SecureIdentityStore(profile: _profile).clear();
      final container = await launch(tester, profile: _profile);
      // Memory at each step, to see where it goes.
      final memory = <String, int>{'launched': rssMegabytes()};
      final atLaunch = largestMappings();
      try {
        await createIdentity(tester, 'Watcher');
        await startAddingServer(tester, _invite);
        await tester.tap(find.widgetWithText(OcButton, 'Join'));
        await waitFor(tester, inSidebar('General'));
        final server = container.read(serverListProvider).single.key;
        await tester.tap(inSidebar('General'));
        await waitUntil(
          tester,
          () =>
              container.read(voiceConnectionProvider)?.phase ==
              VoiceConnectionPhase.connected,
          reason: 'voice media connecting',
        );
        final channel = container.read(voiceSessionProvider).channelId!;
        await Future<void>.delayed(const Duration(seconds: 3));
        memory['in_voice'] = rssMegabytes();

        // Watch the stream, large, as the app opens one.
        await waitUntil(
          tester,
          () => container.read(streamsProvider(server)).isNotEmpty,
          reason: 'the bot going live',
        );
        final stream = container.read(streamsProvider(server)).values.single;
        await container.read(watchingProvider.notifier).watch(stream.key);
        container
            .read(
              voiceFocusProvider((server: server, channel: channel)).notifier,
            )
            .toggle((userId: stream.userId, screen: true));
        await waitUntil(
          tester,
          () => container.read(videoFeedsProvider).screens.isNotEmpty,
          reason: "the stream's video",
        );
        await Future<void>.delayed(const Duration(seconds: 3));
        memory['watching'] = rssMegabytes();

        if (_mode == 'call') {
          await waitUntil(
            tester,
            () => container.read(videoFeedsProvider).cameras.length >= 3,
            reason: "the bots' cameras",
          );
          container.read(repositoryProvider).chooseCamera(_camera);
          await container
              .read(voiceSessionProvider.notifier)
              .toggleCamera()
              .timeout(const Duration(seconds: 10));
          await waitUntil(
            tester,
            () => container.read(videoFeedsProvider).own?.textureId != null,
            reason: 'our camera',
          );
          await Future<void>.delayed(const Duration(seconds: 3));
          memory['camera_on'] = rssMegabytes();
        }
        final feeds = container.read(videoFeedsProvider);
        final textures = {
          for (final MapEntry(:key, :value) in feeds.cameras.entries)
            'camera $key': value.textureId!,
          for (final MapEntry(:key, :value) in feeds.screens.entries)
            'screen $key': value.textureId!,
          'own': ?feeds.own?.textureId,
        };
        // Layers and the bandwidth estimates settle.
        await Future<void>.delayed(const Duration(seconds: 8));

        final report = {
          'mode': _mode,
          'memory_mb': memory,
          'largest_at_launch_mb': atLaunch,
          'largest_at_end_mb': largestMappings(),
          ...await measure(tester, textures, _measured),
        };
        debugPrint('V10 load: ${jsonEncode(report)}');
        binding.reportData = report;

        await container.read(voiceSessionProvider.notifier).leave();
      } finally {
        await SecureIdentityStore(profile: _profile).clear();
      }
    },
    timeout: const Timeout(Duration(minutes: 5)),
  );
}
