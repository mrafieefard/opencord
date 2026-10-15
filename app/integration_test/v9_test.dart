import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/features/voice/voice_tile.dart';
import 'package:opencord/features/voice/voice_view.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

import 'support/e2e.dart';

// Phase 2 V9 against a real server (app/tool/check_v9.sh): a bot called
// Talker is in General and writes "can you read me?" in its chat every 5 s. Without
// the script this test skips.
const _invite = String.fromEnvironment('OPENCORD_E2E_INVITE');
const _profile = 'e2e_v9';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    "a voice channel's chat, video-only tiles and the connection's numbers",
    skip: _invite.isEmpty,
    (tester) async {
      await SecureIdentityStore(profile: _profile).clear();
      final container = await launch(tester, profile: _profile);
      try {
        await createIdentity(tester, 'Listener');
        await startAddingServer(tester, _invite);
        await tester.tap(find.widgetWithText(OcButton, 'Join'));
        await waitFor(tester, inSidebar('General'));

        // The bot writes every few seconds: unread on the voice row
        // before joining.
        await waitFor(
          tester,
          find.bySemanticsLabel(RegExp(r'^General, voice, .*\d+ unread$')),
        );

        await tester.tap(inSidebar('General'));
        await waitUntil(
          tester,
          () =>
              container.read(voiceConnectionProvider)?.phase ==
              VoiceConnectionPhase.connected,
          reason: 'voice media connecting',
        );
        await waitFor(
          tester,
          find.descendant(
            of: find.byType(VoiceView),
            matching: find.byType(VoiceTileView),
          ),
        );

        await tester.tap(find.byTooltip(RegExp(r'^Show chat, \d+ unread$')));
        await waitFor(tester, find.textContaining('can you read me?'));

        // A window this narrow shows the chat alone; the tiles come back
        // with it closed. Neither of us has video.
        await tester.tap(find.byTooltip('Hide chat'));
        await waitFor(
          tester,
          find.descendant(
            of: find.byType(VoiceView),
            matching: find.byType(VoiceTileView),
          ),
        );
        await tester.tap(find.byTooltip('Hide non-video participants'));
        await waitFor(tester, find.text('No one has video on'));
        await tester.tap(find.widgetWithText(OcButton, 'Show everyone'));
        await tester.pumpAndSettle();

        // The real connection's numbers.
        await waitUntil(
          tester,
          () => container.read(voiceStatsProvider)?.pingMs != null,
          reason: 'connection statistics with a round trip',
        );
        final stats = container.read(voiceStatsProvider)!;
        debugPrint(
          'STATS ping ${stats.pingMs} ms, jitter ${stats.jitterMs} ms, '
          'loss ${stats.lossInPercent}/${stats.lossOutPercent} %, '
          '${stats.bitrateIn}/${stats.bitrateOut} bit/s, ${stats.voiceNode}',
        );
        expect(stats.bitrateIn, greaterThan(10000), reason: "Talker's tone");
        await tester.tap(find.byTooltip(RegExp(r'^Connection info, ')));
        await waitFor(tester, find.text('Connection info'));
        expect(find.textContaining(' ms'), findsWidgets);
        await tester.sendKeyEvent(LogicalKeyboardKey.escape);

        await container.read(voiceSessionProvider.notifier).leave();
      } finally {
        await SecureIdentityStore(profile: _profile).clear();
      }
    },
  );
}
