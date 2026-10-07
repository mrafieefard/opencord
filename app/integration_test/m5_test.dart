import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

import 'support/e2e.dart';

// Phase 1 M5 acceptance against a real server, in two runs of the app:
// a fresh install creates an identity and joins (owner claim, TOFU), then a
// restart is still connected. tool/check_m5.sh starts the server and runs
// both; without it this test skips.
const _server = String.fromEnvironment('OPENCORD_E2E_SERVER');
const _claim = String.fromEnvironment('OPENCORD_E2E_CLAIM');
const _phase = String.fromEnvironment('OPENCORD_E2E_PHASE');
const _profile = 'e2e';
const _message = 'Hello from the M5 check';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  final skip = _server.isEmpty ? 'Run through tool/check_m5.sh' : null;

  testWidgets(
    'a fresh install creates an identity and joins a server',
    skip: skip != null || _phase != 'join',
    (tester) async {
      // A run that failed half way may have left an identity behind.
      await SecureIdentityStore(profile: _profile).clear();
      await launch(tester, profile: _profile);

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
      // A self-signed server: the fingerprint is shown and trusted (TOFU).
      await waitFor(tester, find.widgetWithText(OcButton, 'Trust and connect'));
      await tester.tap(find.widgetWithText(OcButton, 'Trust and connect'));

      await waitFor(tester, inSidebar('general'));
      await tester.tap(inSidebar('general'));
      await send(tester, _message);
    },
  );

  testWidgets(
    'after a restart it is still there and connected',
    skip: skip != null || _phase != 'restart',
    (tester) async {
      final container = await launch(tester, profile: _profile);
      try {
        // No onboarding: the identity came back from the keychain.
        await waitFor(tester, find.byType(DesktopShell));
        await waitUntil(
          tester,
          () => container.read(serverProvider(_server)).connection.isConnected,
          reason: 'not connected after the restart',
        );
        await tester.tap(inSidebar('general'));
        await waitFor(tester, messageBubble(_message));
      } finally {
        await SecureIdentityStore(profile: _profile).clear();
      }
    },
  );
}
