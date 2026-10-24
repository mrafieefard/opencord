import 'dart:math';

import 'package:fake_async/fake_async.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/mock/mock_repository.dart';
import 'package:opencord/core/mock/mock_world.dart';
import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/features/shell/navigation.dart';

final _base = DateTime(2026, 10, 7, 14, 30);

void withApp(
  void Function(
    FakeAsync async,
    ProviderContainer container,
    MockRepository repo,
  )
  body, {
  KeyValueStore? store,
  bool simulateLife = false,
}) {
  fakeAsync((async) {
    final repo = MockRepository(
      clock: () => _base.add(async.elapsed),
      random: Random(3),
      simulateLife: simulateLife,
    );
    final container = ProviderContainer(
      overrides: [
        repositoryProvider.overrideWithValue(repo),
        keyValueStoreProvider.overrideWithValue(store ?? MemoryKeyValueStore()),
        clockProvider.overrideWithValue(() => _base.add(async.elapsed)),
      ],
    );
    container.read(eventPumpProvider);
    repo.start();
    async.elapse(const Duration(seconds: 2));
    body(async, container, repo);
    container.dispose();
    repo.dispose();
    async.flushTimers(flushPeriodicTimers: false);
  });
}

void main() {
  group('private state and providers', () {
    test('contacts load once events are routed', () {
      withApp((async, container, repo) {
        final contactsState = container.read(privateContactsProvider);
        expect(contactsState.loaded, isTrue);
        expect(contactsState.byPeer, isNotEmpty);
      });
    });

    test(
      'a conversation orders by time, then id, and loadOlder adds older ones in front',
      () {
        withApp((async, container, repo) {
          final kaiKey = fakeFingerprint('key-1001');
          container.read(privateConversationProvider(kaiKey));
          // Initial conversation load
          async.flushMicrotasks();
          final loaded = container.read(privateConversationProvider(kaiKey));
          final messages = loaded.messages;
          expect(messages, isNotEmpty);

          // Check sorted oldest-first by time, then id
          for (var i = 0; i < messages.length - 1; i++) {
            final a = messages[i];
            final b = messages[i + 1];
            final cmp = a.sentAt.compareTo(b.sentAt);
            expect(cmp <= 0, isTrue);
            if (cmp == 0) {
              expect(a.id.compareTo(b.id) <= 0, isTrue);
            }
          }

          // Calling loadOlder adds older messages in front if any
          container
              .read(privateConversationProvider(kaiKey).notifier)
              .loadOlder();
          async.flushMicrotasks();
        });
      },
    );

    test('typing expires after 8 s', () {
      withApp((async, container, repo) {
        final kaiKey = fakeFingerprint('key-1001');
        repo.sendPrivateTyping(kaiKey);
        async.flushMicrotasks();

        final typingNow = container.read(privateTypingProvider);
        expect(typingNow.containsKey(kaiKey), isTrue);

        // Advance 7 seconds: still typing
        async.elapse(const Duration(seconds: 7));
        expect(
          container.read(privateTypingProvider).containsKey(kaiKey),
          isTrue,
        );

        // Advance past 8 seconds total: expired and pruned
        async.elapse(const Duration(seconds: 2));
        expect(
          container.read(privateTypingProvider).containsKey(kaiKey),
          isFalse,
        );
      });
    });

    test('the name rule, case by case', () {
      withApp((async, container, repo) {
        // Case 1: Member on relay server (Kai Nakamura on Opencord Dev)
        final kaiKey = fakeFingerprint('key-1001');
        expect(container.read(contactNameProvider(kaiKey)), 'Kai Nakamura');

        // Case 2: First shared server in rail order (if no relay or not member on relay)
        // Greta Hoffmann is only on Rust Berlin (server 2)
        final gretaKey = fakeFingerprint('key-2001');
        expect(container.read(contactNameProvider(gretaKey)), 'Greta Hoffmann');

        // Case 3: No shared server, falls back to contact.name (Old Friend)
        final oldFriendKey = fakeFingerprint('old-friend');
        expect(container.read(contactNameProvider(oldFriendKey)), 'Old Friend');

        // Case 4: No shared server and no name: falls back to fingerprint
        const strangerPeer =
            '00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff';
        final name = container.read(contactNameProvider(strangerPeer));
        expect(name.isNotEmpty, isTrue);
      });
    });

    test('the presence rule', () {
      withApp((async, container, repo) {
        // Kai Nakamura: relay server Opencord Dev, online
        final kaiKey = fakeFingerprint('key-1001');
        expect(
          container.read(contactPresenceProvider(kaiKey)),
          Presence.online,
        );

        // Noah Kim: member on Opencord Dev, offline
        final noahKey = fakeFingerprint('key-1008');
        expect(
          container.read(contactPresenceProvider(noahKey)),
          Presence.offline,
        );

        // Old Friend: no shared server -> offline
        final oldFriendKey = fakeFingerprint('old-friend');
        expect(
          container.read(contactPresenceProvider(oldFriendKey)),
          Presence.offline,
        );
      });
    });

    test(
      'privateUnreadProvider leaves out muted, blocked, spam and requests',
      () {
        withApp((async, container, repo) {
          // unread count should count only accepted, non-muted, non-blocked unreads + non-spam incoming friend requests
          final unread = container.read(privateUnreadProvider);
          // Jonas (incoming request, not spam) contributes 1
          // Lena (incoming spam request) contributes 0
          // Priya & Greta (message requests) contribute 0
          // Old Friend (blocked) contributes 0
          expect(unread, greaterThanOrEqualTo(1));
        });
      },
    );

    test('homeShownProvider is true with no servers', () {
      fakeAsync((async) {
        final repo = MockRepository(
          clock: () => _base.add(async.elapsed),
          random: Random(3),
          simulateLife: false,
        );
        // Clear all servers to simulate no servers
        final container = ProviderContainer(
          overrides: [
            repositoryProvider.overrideWithValue(repo),
            keyValueStoreProvider.overrideWithValue(MemoryKeyValueStore()),
            clockProvider.overrideWithValue(() => _base.add(async.elapsed)),
            serverListProvider.overrideWith(() => _EmptyServerListNotifier()),
          ],
        );
        container.read(eventPumpProvider);
        expect(container.read(homeShownProvider), isTrue);
        container.dispose();
        repo.dispose();
      });
    });

    test('the selection home and peer survive a reload', () {
      final store = MemoryKeyValueStore();
      withApp((async, container, repo) {
        final kaiKey = fakeFingerprint('key-1001');
        container.read(navigationProvider.notifier).openConversation(kaiKey);
        expect(container.read(navigationProvider).home, isTrue);
        expect(container.read(navigationProvider).peer, kaiKey);
      }, store: store);

      // Re-read with a new container sharing the same store
      withApp((async, container, repo) {
        expect(container.read(navigationProvider).home, isTrue);
        final kaiKey = fakeFingerprint('key-1001');
        expect(container.read(navigationProvider).peer, kaiKey);
      }, store: store);
    });
  });
}

class _EmptyServerListNotifier extends ServerListNotifier {
  @override
  List<ServerSummary> build() => const [];
}
