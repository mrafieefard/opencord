import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

import 'support/e2e.dart';

// Phase 2 V2 against a real server, with this machine's audio devices: the
// app claims the server, joins voice, and its media connection comes up
// through the server's voice node (ICE, DTLS) without a device failing.
// Nothing is played, and while the room is quiet nothing is sent. Hearing
// each other takes two machines and people (plan §16 V2). tool/check_v2.sh
// starts the server and runs this; without it this test skips.
const _server = String.fromEnvironment('OPENCORD_E2E_SERVER');
const _claim = String.fromEnvironment('OPENCORD_E2E_CLAIM');
const _profile = 'e2e_media';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    "joining voice connects media with this machine's devices",
    skip: _server.isEmpty,
    (tester) async {
      await SecureIdentityStore(profile: _profile).clear();
      final container = await launch(tester, profile: _profile);
      final failures = <AudioDeviceFailed>[];
      final subscription = container.read(repositoryProvider).events.listen((
        event,
      ) {
        if (event is AudioDeviceFailed) failures.add(event);
      });
      try {
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
        await waitUntil(
          tester,
          () =>
              container.read(voiceConnectionProvider)?.phase ==
              VoiceConnectionPhase.connected,
          reason: 'voice media connecting',
        );
        await waitFor(tester, find.text('Voice connected'));
        expect(failures.map((failure) => failure.message), isEmpty);

        await container.read(voiceSessionProvider.notifier).leave();
        await waitUntil(
          tester,
          () => container.read(voiceConnectionProvider) == null,
          reason: 'leaving voice',
        );
      } finally {
        await subscription.cancel();
        await SecureIdentityStore(profile: _profile).clear();
      }
    },
  );
}
