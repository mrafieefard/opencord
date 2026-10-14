import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/features/voice/screen_source_picker.dart';
import 'package:opencord/features/voice/voice_controls.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

import '../support/e2e.dart';

// Windows plan W6, step 9: sharing a screen and a window for real, picked
// in Opencord's own dialog. `check_v6.ps1` runs an ffplay window showing a
// test pattern and a voicebot that watches what goes out.
//
// One phase per run:
//   screen  share a whole monitor at 720p30
//   window  share the ffplay window
//   closed  share the ffplay window, then have it closed under us
const _invite = String.fromEnvironment('OPENCORD_E2E_INVITE');

const _phase = String.fromEnvironment(
  'OPENCORD_E2E_PHASE',
  defaultValue: 'screen',
);

/// Part of the title of the window to share, for the window phases.
const _window = String.fromEnvironment(
  'OPENCORD_E2E_WINDOW',
  defaultValue: 'OpencordTestPattern',
);

/// How long to hold the share while the bot watches.
const _holdSeconds = int.fromEnvironment('OPENCORD_E2E_HOLD', defaultValue: 14);

const _profile = 'e2e_v6';

Future<void> _hold(WidgetTester tester, Duration total) async {
  final until = DateTime.now().add(total);
  while (DateTime.now().isBefore(until)) {
    await tester.pump(const Duration(milliseconds: 50));
    await Future<void>.delayed(const Duration(milliseconds: 50));
  }
}

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('sharing a $_phase, picked in Opencord', skip: _invite.isEmpty, (
    tester,
  ) async {
    await SecureIdentityStore(profile: _profile).clear();
    final container = await launch(tester, profile: _profile);
    try {
      await createIdentity(tester, 'Sharer');
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
      await waitFor(tester, inSidebar('Watcher'));

      // Opencord's own dialog, with its own picker in it.
      await tester.tap(
        find.descendant(
          of: find.byType(VoiceControlBar),
          matching: find.bySemanticsLabel('Share your screen'),
        ),
      );
      await _hold(tester, const Duration(milliseconds: 800));
      expect(
        find.byType(ScreenSourcePicker),
        findsOneWidget,
        reason: 'Windows picks its own source',
      );

      // Which tab, and which source in it.
      final screen = _phase == 'screen';
      if (!screen) {
        await tester.tap(find.textContaining('Windows ('));
        await _hold(tester, const Duration(milliseconds: 600));
      }
      final wanted = screen
          ? find.textContaining('Screen 1 · ')
          : find.textContaining(_window);
      await waitFor(tester, wanted);
      final title = tester.widget<Text>(wanted.first).data;
      await tester.tap(wanted.first);
      await _hold(tester, const Duration(milliseconds: 400));

      // 720p30, as the plan asks.
      await tester.tap(find.text('720p'));
      await _hold(tester, const Duration(milliseconds: 200));
      await tester.tap(find.text('30 fps'));
      await _hold(tester, const Duration(milliseconds: 200));

      final goLive = find.widgetWithText(OcButton, 'Go live');
      expect(
        tester.widget<OcButton>(goLive).onPressed,
        isNotNull,
        reason: 'a source is chosen',
      );
      await tester.tap(goLive);
      await waitUntil(
        tester,
        () => container.read(voiceSessionProvider).screensharing,
        reason: 'going live',
      );
      final share = container.read(ownScreenShareProvider);
      // ignore: avoid_print
      print(
        'SHARE: phase=$_phase title="$title" source=${share?.source} '
        'audioFrom=${share?.audioFrom}',
      );

      await _hold(tester, Duration(seconds: _holdSeconds));

      if (_phase == 'closed') {
        // The script closes the window while this waits. The share has to
        // end by itself and say why (W6 step 4).
        await waitUntil(
          tester,
          () => !container.read(voiceSessionProvider).screensharing,
          reason: 'the share ending when its window closed',
          timeout: const Duration(seconds: 60),
        );
        // ignore: avoid_print
        print('CLOSED: the share ended by itself');
      } else {
        expect(
          container.read(voiceSessionProvider).screensharing,
          isTrue,
          reason: 'still sharing',
        );
      }

      await container.read(voiceSessionProvider.notifier).leave();
    } finally {
      await SecureIdentityStore(profile: _profile).clear();
    }
  });
}
