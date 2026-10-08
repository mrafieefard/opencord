import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/core/settings/local_prefs.dart';
import 'package:opencord/features/voice/input_level_meter.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

import 'support/e2e.dart';

// Phase 2 V3 against a real server, with this machine's audio devices and
// CPU: the first run picks noise suppression by benchmarking High here,
// voice connects with the whole processing chain running on the real
// microphone, the level meter shows that microphone, and the system
// answers whether hotkeys work outside the app. Nothing is played.
// tool/check_v3.sh starts the server and runs this; without it this test
// skips.
const _server = String.fromEnvironment('OPENCORD_E2E_SERVER');
const _claim = String.fromEnvironment('OPENCORD_E2E_CLAIM');
const _profile = 'e2e_processing';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    "voice runs its processing on this machine's microphone",
    skip: _server.isEmpty,
    (tester) async {
      await SecureIdentityStore(profile: _profile).clear();
      final container = await launch(tester, profile: _profile);
      final failures = <AudioDeviceFailed>[];
      final levels = <double>[];
      final repository = container.read(repositoryProvider);
      final subscription = repository.events.listen((event) {
        if (event is AudioDeviceFailed) failures.add(event);
        if (event is InputLevelChanged) levels.add(event.dbfs);
      });
      try {
        await waitUntil(
          tester,
          () => container.read(audioSettingsProvider).noiseSuppression != null,
          reason: "the first run's noise suppression benchmark",
        );
        await waitUntil(
          tester,
          () => container.read(hotkeySupportProvider) != null,
          reason: 'the system answering about hotkeys',
        );

        await createIdentity(tester, 'Owner');
        await startAddingServer(tester, _server);
        await tester.tap(find.text("I'm the owner"));
        await tester.pump();
        await tester.enterText(
          find.bySemanticsLabel('Owner claim token'),
          _claim,
        );
        await tester.pump();
        await tester.tap(find.widgetWithText(OcButton, 'Join'));
        await waitFor(
          tester,
          find.widgetWithText(OcButton, 'Trust and connect'),
        );
        await tester.tap(find.widgetWithText(OcButton, 'Trust and connect'));
        await waitFor(tester, inSidebar('general'));
        await tester.tap(inSidebar('General'));
        await waitFor(tester, find.text('Voice connected'));

        await tester.tap(find.bySemanticsLabel('Audio options').first);
        await waitFor(tester, find.byType(InputLevelMeter));
        await waitUntil(
          tester,
          () => levels.length >= 10,
          reason: "the microphone's level arriving",
        );

        expect(failures.map((failure) => failure.message), isEmpty);
        // Prints what this machine chose, for the record.
        // ignore: avoid_print
        print(
          'V3 check: noise suppression '
          '${container.read(audioSettingsProvider).noiseSuppression?.name}, '
          'hotkeys ${container.read(hotkeySupportProvider)}, '
          '${levels.length} levels, loudest '
          '${levels.reduce((a, b) => a > b ? a : b).toStringAsFixed(1)} dBFS',
        );
        await container.read(voiceSessionProvider.notifier).leave();
      } finally {
        await subscription.cancel();
        await SecureIdentityStore(profile: _profile).clear();
      }
    },
  );
}
