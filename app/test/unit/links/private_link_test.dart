import 'package:flutter/widgets.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/mock/mock_repository.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/features/links/app_links.dart';
import 'package:opencord/features/shell/navigation.dart';

void main() {
  group('private_and_friends_links_parse_and_open', () {
    final peerLower = 'a' * 64;
    final peerUpper = 'A' * 64;

    test('parseAppLink parses valid private and friends links', () {
      expect(
        parseAppLink('opencord://private/$peerLower'),
        PrivateAppLink(peerLower),
      );
      expect(
        parseAppLink('opencord://private/$peerUpper'),
        PrivateAppLink(peerLower),
      );
      expect(parseAppLink(' opencord://friends '), const FriendsAppLink());
    });

    test('parseAppLink rejects invalid private and friends links', () {
      // Invalid hex
      expect(parseAppLink('opencord://private/123'), isNull);
      expect(parseAppLink('opencord://private/${'g' * 64}'), isNull);
      expect(parseAppLink('opencord://private/${'a' * 63}'), isNull);
      expect(parseAppLink('opencord://private/${'a' * 65}'), isNull);
      expect(parseAppLink('opencord://private'), isNull);
      expect(parseAppLink('opencord://private/'), isNull);

      // Friends with extra path
      expect(parseAppLink('opencord://friends/requests'), isNull);
      expect(parseAppLink('opencord://friends/123'), isNull);
    });

    test('helpers create the right links', () {
      expect(privateLink(peerLower), 'opencord://private/$peerLower');
      expect(friendsLink(), 'opencord://friends');
    });

    testWidgets('openAppLink dispatches to openConversation and openFriends', (
      tester,
    ) async {
      late WidgetRef capturedRef;
      late BuildContext capturedContext;

      await tester.pumpWidget(
        ProviderScope(
          overrides: [
            keyValueStoreProvider.overrideWithValue(MemoryKeyValueStore()),
            repositoryProvider.overrideWithValue(
              MockRepository(simulateLife: false),
            ),
          ],
          child: Consumer(
            builder: (context, ref, _) {
              capturedRef = ref;
              capturedContext = context;
              return const SizedBox();
            },
          ),
        ),
      );

      // 1. Open conversation
      await openAppLink(
        capturedContext,
        capturedRef,
        PrivateAppLink(peerLower),
      );
      expect(capturedRef.read(homeShownProvider), isTrue);
      expect(capturedRef.read(currentPeerProvider), peerLower);

      // 2. Open friends
      await openAppLink(capturedContext, capturedRef, const FriendsAppLink());
      expect(capturedRef.read(homeShownProvider), isTrue);
      expect(capturedRef.read(currentPeerProvider), isNull);
    });
  });
}
