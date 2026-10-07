import 'dart:math';

import 'package:fake_async/fake_async.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/mock/mock_repository.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/repository/repository.dart';

final base = DateTime(2026, 10, 7, 14, 30);

/// Runs [body] with a started mock on fake time.
void withMock(
  void Function(FakeAsync async, MockRepository repo, List<RepoEvent> events)
  body,
) {
  fakeAsync((async) {
    final repo = MockRepository(
      clock: () => base.add(async.elapsed),
      random: Random(7),
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

Ready readyOf(List<RepoEvent> events, String key) =>
    events.whereType<Ready>().lastWhere((event) => event.serverKey == key);

ConnectionStatus statusOf(List<RepoEvent> events, String key) => events
    .whereType<ConnectionChanged>()
    .lastWhere((event) => event.serverKey == key)
    .status;

void main() {
  group('content (§11)', () {
    test('three servers: one owned, one joined, one reconnecting', () {
      withMock((async, repo, events) {
        final [owned, joined, homelab] = repo.servers;
        final ownedReady = readyOf(events, owned.key).snapshot;
        final joinedReady = readyOf(events, joined.key).snapshot;

        expect(repo.servers, hasLength(3));
        expect(ownedReady.info.ownerId, ownedReady.self.id);
        expect(joinedReady.info.ownerId, isNot(joinedReady.self.id));
        expect(statusOf(events, owned.key).isConnected, isTrue);
        expect(readyOf(events, homelab.key).snapshot.channels, isNotEmpty);
        expect(
          statusOf(events, homelab.key).phase,
          ConnectionPhase.reconnecting,
        );
      });
    });

    test('categories hold text, announcement and voice channels', () {
      withMock((async, repo, events) {
        final channels = readyOf(
          events,
          repo.servers.first.key,
        ).snapshot.channels;
        final kinds = channels.map((channel) => channel.kind).toSet();
        final categorized = channels.where(
          (channel) => !channel.isCategory && channel.parentId != null,
        );

        expect(kinds, containsAll(ChannelKind.values));
        expect(categorized.length, greaterThan(5));
      });
    });

    test(
      'about a dozen people with every presence, activities and hoisted roles',
      () {
        withMock((async, repo, events) {
          final snapshot = readyOf(events, repo.servers.first.key).snapshot;

          expect(snapshot.members.length, inInclusiveRange(12, 16));
          expect(
            snapshot.presences.values.toSet(),
            containsAll(Presence.values),
          );
          expect(snapshot.activities, isNotEmpty);
          expect(
            snapshot.roles.where((role) => role.hoist).length,
            greaterThanOrEqualTo(2),
          );
        });
      },
    );

    test('#general spans yesterday and today with every kind of message', () {
      withMock((async, repo, events) {
        final key = repo.servers.first.key;
        final snapshot = readyOf(events, key).snapshot;
        final general = snapshot.channels.firstWhere(
          (c) => c.name == 'general',
        );
        late List<Message> messages;
        repo
            .fetchMessages(key, general.id, limit: 500)
            .then((m) => messages = m);
        async.flushMicrotasks();
        final today = DateTime(base.year, base.month, base.day);
        final read = snapshot.readStates[general.id]!;

        expect(messages.any((m) => m.createdAt.isBefore(today)), isTrue);
        expect(messages.any((m) => !m.createdAt.isBefore(today)), isTrue);
        expect(messages.any((m) => m.replyToId != null), isTrue);
        expect(messages.any((m) => m.reactions.isNotEmpty), isTrue);
        expect(messages.any((m) => m.reactions.any((r) => r.me)), isTrue);
        expect(messages.any((m) => m.content.contains('```')), isTrue);
        expect(messages.any((m) => m.edited), isTrue);
        expect(
          messages.any((m) => m.content.contains('<@${snapshot.self.id}>')),
          isTrue,
        );
        expect(messages.any((m) => m.pinned), isTrue);
        expect(messages.any((m) => m.isSystem), isTrue);
        expect(messages.any((m) => m.authorId == snapshot.self.id), isTrue);
        expect(read.lastReadId, greaterThan(messages.first.id));
        expect(read.lastReadId, lessThan(messages.last.id));
        expect(read.unread, greaterThan(0));
        expect(read.mentions, greaterThan(0));
        expect(messages.every((m) => !m.createdAt.isAfter(base)), isTrue);
      });
    });
  });

  group('simulated life', () {
    test('someone types and replies after you send a message', () {
      withMock((async, repo, events) {
        final key = repo.servers.first.key;
        final snapshot = readyOf(events, key).snapshot;
        final general = snapshot.channels.firstWhere(
          (c) => c.name == 'general',
        );
        events.clear();

        repo.sendMessage(key, general.id, 'Anyone around?', nonce: 'n1');
        async.elapse(const Duration(seconds: 6));

        final typing = events.whereType<TypingStarted>().first;
        final created = events.whereType<MessageCreated>().toList();
        expect(created.first.message.nonce, 'n1');
        expect(typing.userId, isNot(snapshot.self.id));
        expect(created.last.message.authorId, typing.userId);
        expect(created.last.message.channelId, general.id);
      });
    });

    test('voice participants speak every ~650 ms while you are in voice', () {
      withMock((async, repo, events) {
        final key = repo.servers.first.key;
        final snapshot = readyOf(events, key).snapshot;
        final voice = snapshot.channels.firstWhere(
          (c) => c.kind == ChannelKind.voice,
        );
        final participants = snapshot.voice[voice.id]!;
        events.clear();

        async.elapse(const Duration(seconds: 2));
        final quietBeforeJoining = events.whereType<SpeakingChanged>().isEmpty;
        repo.joinVoice(key, voice.id);
        async.elapse(const Duration(seconds: 2));

        expect(quietBeforeJoining, isTrue);
        expect(
          events.whereType<SpeakingChanged>().length,
          greaterThanOrEqualTo(2),
        );
        expect(participants.where((p) => p.screensharing), hasLength(1));
        expect(participants.where((p) => p.camera), hasLength(1));
        expect(participants.where((p) => p.muted), hasLength(1));
      });
    });
  });

  group('requests', () {
    test('history pages backwards to the start', () {
      withMock((async, repo, events) {
        final key = repo.servers.first.key;
        final general = readyOf(
          events,
          key,
        ).snapshot.channels.firstWhere((c) => c.name == 'general');
        final pages = <List<Message>>[];
        int? before;
        do {
          repo
              .fetchMessages(key, general.id, before: before, limit: 5)
              .then(pages.add);
          async.flushMicrotasks();
          before = pages.last.isEmpty ? null : pages.last.first.id;
        } while (pages.last.length == 5);
        final all = pages.reversed.expand((page) => page).toList();

        expect(all.map((m) => m.id).toSet(), hasLength(all.length));
        expect(
          all.map((m) => m.id).toList(),
          orderedEquals([...all.map((m) => m.id)]..sort()),
        );
      });
    });

    test('a plain address needs trust before it is added', () {
      withMock((async, repo, events) {
        AddServerResult? first;
        AddServerResult? second;
        repo.addServer('new.example.org:7710').then((r) => first = r);
        async.elapse(const Duration(seconds: 2));
        final trust = first! as ServerNeedsTrust;
        repo.trustFingerprint(trust.address, trust.fingerprint);
        repo.addServer('new.example.org:7710').then((r) => second = r);
        async.elapse(const Duration(seconds: 2));

        expect(second, isA<ServerAdded>());
        expect(
          repo.servers.map((s) => s.key),
          contains('new.example.org:7710'),
        );
        expect(
          readyOf(events, 'new.example.org:7710').snapshot.info.ownerId,
          isNull,
        );
      });
    });
  });

  group('debug menu', () {
    test('drops and restores a connection', () {
      withMock((async, repo, events) {
        final key = repo.servers.first.key;

        repo.debugDisconnect(key);
        async.flushMicrotasks();
        final dropped = statusOf(events, key).phase;
        repo.debugReconnect(key);
        async.elapse(const Duration(seconds: 1));

        expect(dropped, ConnectionPhase.reconnecting);
        expect(statusOf(events, key).isConnected, isTrue);
      });
    });

    test('reports a changed certificate', () {
      withMock((async, repo, events) {
        final key = repo.servers.first.key;

        repo.debugFingerprintMismatch(key);
        async.flushMicrotasks();

        expect(statusOf(events, key).failure, FailureReason.fingerprintChanged);
      });
    });

    test('empties the server list', () {
      withMock((async, repo, events) {
        repo.debugEmptyServerList();
        async.flushMicrotasks();

        expect(repo.servers, isEmpty);
        expect(events.last, isA<ServersChanged>());
      });
    });

    test('gives channels very long names', () {
      withMock((async, repo, events) {
        repo.debugLongNames();
        async.flushMicrotasks();

        final renamed = events.whereType<ChannelUpserted>();
        expect(renamed, isNotEmpty);
        expect(renamed.first.channel.name.length, greaterThan(60));
      });
    });

    test('adds a 10 000-message channel', () {
      withMock((async, repo, events) {
        final key = repo.servers.first.key;

        repo.debugStressChannel(key);
        async.flushMicrotasks();
        final channel = events.whereType<ChannelUpserted>().last.channel;
        late List<Message> newest;
        late List<Message> oldest;
        repo.fetchMessages(key, channel.id, limit: 50).then((m) => newest = m);
        async.flushMicrotasks();
        repo
            .fetchMessages(
              key,
              channel.id,
              before: newest.first.id,
              limit: 10000,
            )
            .then((m) => oldest = m);
        async.flushMicrotasks();

        expect(newest, hasLength(50));
        expect(oldest.length + newest.length, 10000);
      });
    });
  });
}
