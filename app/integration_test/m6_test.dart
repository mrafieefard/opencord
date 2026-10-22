import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/features/shell/desktop_shell.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

import 'support/e2e.dart';

// Phase 1 M6 acceptance: two identities chat in real time, and when the
// owner gives the other a role and restricts a channel, their UI follows
// without reconnecting. The app is the member; the owner is
// crates/opencord-core/examples/m6_peer.rs on the same core.
// tool/check_m6.sh starts the server and the owner and runs this; without
// it this test skips.
const _server = String.fromEnvironment('OPENCORD_E2E_SERVER');
const _invite = String.fromEnvironment('OPENCORD_E2E_INVITE');
const _profile = 'e2e_member';
const _hello = 'Hello from the member';

// What m6_peer answers, and the role it gives.
const _answer = 'Hello from the owner';
const _role = 'Readers';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    'a member chats live and follows a new restriction without reconnecting',
    skip: _invite.isEmpty,
    (tester) async {
      // A run that failed half way may have left an identity behind.
      await SecureIdentityStore(profile: _profile).clear();
      final container = await launch(tester, profile: _profile);
      // Every handshake ends in a Ready, and a dropped connection (even
      // one that resumes) leaves connected: either is a reconnect.
      final handshakes = <Ready>[];
      final drops = <ConnectionChanged>[];
      var connected = false;
      final subscription = container.read(repositoryProvider).events.listen((
        event,
      ) {
        if (event.serverKey != _server) return;
        switch (event) {
          case Ready():
            handshakes.add(event);
          case ConnectionChanged(:final status) when status.isConnected:
            connected = true;
          case ConnectionChanged() when connected:
            drops.add(event);
          default:
            break;
        }
      });
      try {
        await createIdentity(tester, 'Member');
        // The invite link carries the server's fingerprint: no TOFU prompt.
        await startAddingServer(tester, _invite);
        await tester.tap(find.widgetWithText(OcButton, 'Join'));
        await waitFor(tester, inSidebar('general'));
        await tester.tap(inSidebar('general'));

        // The owner answers while #general is open: it can only arrive live.
        await send(tester, _hello);
        await waitFor(tester, messageBubble(_answer));

        // Then a role that may not send here: the composer says so, and the
        // member list has the role's section.
        await waitFor(
          tester,
          find.text(
            'You do not have permission to send messages in this channel.',
          ),
        );
        if (find.byKey(DesktopShell.membersKey).evaluate().isEmpty) {
          await tester.tap(find.byTooltip('Member list'));
        }
        await waitFor(
          tester,
          find.descendant(
            of: find.byKey(DesktopShell.membersKey),
            matching: find.text('${_role.toUpperCase()} — 1'),
          ),
        );

        expect(handshakes, hasLength(1), reason: 'it reconnected on the way');
        expect(drops, isEmpty, reason: 'the connection dropped on the way');
      } finally {
        await subscription.cancel();
        await SecureIdentityStore(profile: _profile).clear();
      }
    },
  );
}
