import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/src/rust/api/client.dart' as core;
import 'package:opencord/ui/widgets/oc_button.dart';

import 'support/e2e.dart';

// Phase 2 V6 against a real server: a voicebot shares a screen (test: watches)
// or the app shares a screen (test: shares) and a bot watches it. Without the
// script this test skips.
const _invite = String.fromEnvironment('OPENCORD_E2E_INVITE');
const _test = String.fromEnvironment('OPENCORD_E2E_TEST');
const _profile = 'e2e_screen';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  group('watches', () {
    testWidgets(
      "another person's screen share shows in their tile and is drawn",
      skip: _invite.isEmpty || _test != 'watches',
      (tester) async {
        await SecureIdentityStore(profile: _profile).clear();
        final container = await launch(tester, profile: _profile);
        try {
          await createIdentity(tester, 'Watcher');
          await startAddingServer(tester, _invite);
          await tester.tap(find.widgetWithText(OcButton, 'Join'));
          await waitFor(tester, inSidebar('General'));

          // Extract server key from invite for streamsProvider
          final serverKey = _invite.split('/').first.replaceFirst('opencord://', '');

          await tester.tap(inSidebar('General'));
          await waitUntil(
            tester,
            () =>
                container.read(voiceConnectionProvider)?.phase ==
                VoiceConnectionPhase.connected,
            reason: 'voice media connecting',
          );

          // The bot's stream shows as "is live" in the sidebar.
          await waitUntil(
            tester,
            () => container.read(streamsProvider(serverKey)).isNotEmpty,
            reason: "the bot's screen share",
          );

          // Tap Watch on the stream.
          await tester.tap(find.text('Watch').first);
          await waitUntil(
            tester,
            () => container.read(videoFeedsProvider).screens.isNotEmpty,
            reason: "watching the bot's screen",
          );

          // The screen tile shows and Flutter draws it.
          final feed = container.read(videoFeedsProvider).screens.values.first;
          final texture = feed.textureId;
          expect(texture, isNotNull);
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
                BigInt.from(10),
            reason: 'Flutter drawing the screen',
          );

          await container.read(voiceSessionProvider.notifier).leave();
        } finally {
          await SecureIdentityStore(profile: _profile).clear();
        }
      },
    );
  });

  group('shares', () {
    testWidgets(
      "this device's screen share shows the bot as a viewer",
      skip: _invite.isEmpty || _test != 'shares',
      (tester) async {
        await SecureIdentityStore(profile: _profile).clear();
        final container = await launch(tester, profile: _profile);
        try {
          await createIdentity(tester, 'Owner');
          await startAddingServer(tester, _invite);
          await tester.tap(find.widgetWithText(OcButton, 'Join'));
          await waitFor(tester, inSidebar('General'));

          // Extract server key from invite
          final serverKey = _invite.split('/').first.replaceFirst('opencord://', '');

          await tester.tap(inSidebar('General'));
          await waitUntil(
            tester,
            () =>
                container.read(voiceConnectionProvider)?.phase ==
                VoiceConnectionPhase.connected,
            reason: 'voice media connecting',
          );

          // Find and tap the Screenshare button.
          await tester.tap(find.byTooltip('Screenshare'));
          await waitFor(tester, find.text('Share'));

          // Select a source (the dialog should have appeared).
          // With a virtual screen set via OPENCORD_SCREEN_NODE, it auto-selects.
          await tester.tap(find.text('Share'));

          // Wait for going live.
          await waitUntil(
            tester,
            () => container.read(streamsProvider(serverKey)).isNotEmpty,
            reason: 'going live',
          );

          // The bot should be watching.
          await waitUntil(
            tester,
            () {
              final streams = container.read(streamsProvider(serverKey));
              return streams.isNotEmpty && streams.values.first.viewerCount > 0;
            },
            reason: 'the bot watching',
          );

          await container.read(voiceSessionProvider.notifier).leave();
        } finally {
          await SecureIdentityStore(profile: _profile).clear();
        }
      },
    );
  });
}
