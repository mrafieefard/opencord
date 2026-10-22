import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

import 'support/e2e.dart';

// Phase 2 V0 acceptance: voice states on a real server, without media. The
// app is a member; the owner is crates/opencord-core/examples/v0_peer.rs
// on the same core. tool/check_v0.sh starts the server and the owner and
// runs this; without it this test skips.
const _server = String.fromEnvironment('OPENCORD_E2E_SERVER');
const _invite = String.fromEnvironment('OPENCORD_E2E_INVITE');
const _profile = 'e2e_voice';

// What v0_peer waits for before moving, then disconnecting, the member.
const _inGeneral = 'I am in General';
const _inLounge = 'I am in Lounge';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets(
    'a member sees voice live, joins, is moved and disconnected',
    skip: _invite.isEmpty,
    (tester) async {
      await SecureIdentityStore(profile: _profile).clear();
      final container = await launch(tester, profile: _profile);
      final handshakes = <Ready>[];
      final subscription = container.read(repositoryProvider).events.listen((
        event,
      ) {
        if (event.serverKey == _server && event is Ready) handshakes.add(event);
      });
      int voiceChannel(String name) => container
          .read(serverProvider(_server))
          .data!
          .channels
          .values
          .firstWhere((channel) => channel.name == name)
          .id;
      try {
        await createIdentity(tester, 'Member');
        await startAddingServer(tester, _invite);
        await tester.tap(find.widgetWithText(OcButton, 'Join'));
        await waitFor(tester, inSidebar('general'));

        // The owner joins General once the member is in: it shows live.
        await waitFor(tester, inSidebar('Owner'));

        await tester.tap(inSidebar('General'));
        await waitUntil(
          tester,
          () =>
              container.read(voiceSessionProvider).channelId ==
              voiceChannel('General'),
          reason: 'joining General',
        );
        await waitFor(tester, inSidebar('Member'));
        await tester.tap(inSidebar('general'));
        await send(tester, _inGeneral);

        // The owner moves the member to Lounge.
        await waitUntil(
          tester,
          () =>
              container.read(voiceSessionProvider).channelId ==
              voiceChannel('Lounge'),
          reason: 'being moved to Lounge',
        );
        expect(
          container
              .read(voiceProvider(_server))[voiceChannel('Lounge')]
              ?.map((participant) => participant.userId),
          [container.read(serverProvider(_server)).data!.self.id],
        );

        // Then disconnects them, once told they are there.
        await send(tester, _inLounge);
        await waitUntil(
          tester,
          () => !container.read(voiceSessionProvider).connected,
          reason: 'being disconnected',
        );

        expect(handshakes, hasLength(1), reason: 'it reconnected on the way');
      } finally {
        await subscription.cancel();
        await SecureIdentityStore(profile: _profile).clear();
      }
    },
  );
}
