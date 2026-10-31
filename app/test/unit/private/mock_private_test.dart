import 'dart:math';

import 'package:fake_async/fake_async.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/mock/mock_repository.dart';
import 'package:opencord/core/mock/mock_world.dart';
import 'package:opencord/core/repository/repository.dart';

final _base = DateTime(2026, 10, 7, 14, 30);

void withMock(
  void Function(FakeAsync async, MockRepository repo, List<RepoEvent> events)
  body, {
  bool simulateLife = false,
}) {
  fakeAsync((async) {
    final repo = MockRepository(
      clock: () => _base.add(async.elapsed),
      random: Random(7),
      simulateLife: simulateLife,
    );
    final events = <RepoEvent>[];
    final subscription = repo.events.listen(events.add);
    repo.start();
    async.elapse(const Duration(seconds: 2));
    body(async, repo, events);
    subscription.cancel();
    repo.dispose();
    async.flushTimers(flushPeriodicTimers: false);
  });
}

void main() {
  group('MockRepository private messages', () {
    test('a send turns sent', () {
      withMock((async, repo, events) {
        // Send a message to Kai
        final kaiKey = fakeFingerprint('key-1001');
        repo.sendPrivateMessage(kaiKey, 'Hello from test!');
        async.flushMicrotasks();

        final added = events.whereType<PrivateMessageAdded>().lastWhere(
          (e) => e.message.peer == kaiKey,
        );
        expect(added.message.content, 'Hello from test!');
        expect(added.message.state, PrivateSendState.sending);

        // With simulateLife: false, it transitions immediately (or next tick) to sent
        async.elapse(const Duration(milliseconds: 350));
        final changed = events.whereType<PrivateMessageChanged>().lastWhere(
          (e) => e.message.id == added.message.id,
        );
        expect(changed.message.state, PrivateSendState.sent);
      });
    });

    test('accepting a friend request makes friends', () {
      withMock((async, repo, events) {
        // Jonas Weber (1003) has an incoming friend request
        final jonasKey = fakeFingerprint('key-1003');
        repo.answerFriendRequest(jonasKey, accept: true);
        async.flushMicrotasks();

        final changed = events.whereType<PrivateContactChanged>().lastWhere(
          (e) => e.contact.peer == jonasKey,
        );
        expect(changed.contact.friend, FriendStatus.friends);
      });
    });

    test('blocking ends a friendship', () {
      withMock((async, repo, events) {
        // Kai Nakamura (1001) is currently friends
        final kaiKey = fakeFingerprint('key-1001');
        repo.blockUser(kaiKey);
        async.flushMicrotasks();

        final changed = events.whereType<PrivateContactChanged>().lastWhere(
          (e) => e.contact.peer == kaiKey,
        );
        expect(changed.contact.blocked, isTrue);
        expect(changed.contact.friend, FriendStatus.none);
      });
    });

    test('Quiet Quinn throws friendRequestsClosed', () {
      withMock((async, repo, events) {
        const serverKey = 'rust-berlin.example:7710';
        const quinnId = 2009;

        expect(
          () => repo.sendFriendRequest(serverKey, quinnId),
          throwsA(
            isA<RepoException>().having(
              (e) => e.kind,
              'kind',
              RepoErrorKind.friendRequestsClosed,
            ),
          ),
        );
      });
    });
  });
}
