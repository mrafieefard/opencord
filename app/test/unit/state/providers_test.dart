import 'dart:math';

import 'package:fake_async/fake_async.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/mock/mock_repository.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/key_value_store.dart';

final base = DateTime(2026, 10, 7, 14, 30);

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
      clock: () => base.add(async.elapsed),
      random: Random(3),
      simulateLife: simulateLife,
    );
    final container = ProviderContainer(
      overrides: [
        repositoryProvider.overrideWithValue(repo),
        keyValueStoreProvider.overrideWithValue(store ?? MemoryKeyValueStore()),
        clockProvider.overrideWithValue(() => base.add(async.elapsed)),
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

int channelNamed(ProviderContainer container, String server, String name) =>
    container
        .read(serverProvider(server))
        .data!
        .channels
        .values
        .firstWhere((channel) => channel.name == name)
        .id;

void main() {
  test('servers connect and their state fills in', () {
    withApp((async, container, repo) {
      final servers = container.read(serverListProvider);
      final first = container.read(serverProvider(servers.first.key));

      expect(servers, hasLength(3));
      expect(first.connection.isConnected, isTrue);
      expect(first.data!.info.name, 'Opencord Dev');
      expect(first.epoch, 1);
    });
  });

  test('a sent message shows at once and is confirmed by the server', () {
    withApp((async, container, repo) {
      final server = repo.servers.first.key;
      final channel = (
        server: server,
        channel: channelNamed(container, server, 'general'),
      );
      container.listen(channelMessagesProvider(channel), (_, _) {});
      async.elapse(const Duration(milliseconds: 100));

      container
          .read(channelMessagesProvider(channel).notifier)
          .send('Hello there');
      final optimistic = container.read(channelMessagesProvider(channel));
      async.elapse(const Duration(seconds: 1));
      final confirmed = container.read(channelMessagesProvider(channel));

      expect(optimistic.pending.single.content, 'Hello there');
      expect(optimistic.pending.single.sendState, SendState.pending);
      expect(confirmed.pending, isEmpty);
      expect(confirmed.messages.last.content, 'Hello there');
    });
  });

  test('older history loads when asked', () {
    withApp((async, container, repo) {
      final server = repo.servers.first.key;
      repo.debugStressChannel(server);
      async.flushMicrotasks();
      final channel = (
        server: server,
        channel: channelNamed(container, server, 'scroll-test'),
      );
      container.listen(channelMessagesProvider(channel), (_, _) {});
      async.elapse(const Duration(milliseconds: 100));
      final first = container
          .read(channelMessagesProvider(channel))
          .messages
          .length;

      container.read(channelMessagesProvider(channel).notifier).loadOlder();
      async.elapse(const Duration(milliseconds: 100));

      expect(first, 50);
      expect(
        container.read(channelMessagesProvider(channel)).messages,
        hasLength(100),
      );
    });
  });

  test('unread counts follow messages unless the channel is being read', () {
    withApp((async, container, repo) {
      final server = repo.servers.first.key;
      final general = channelNamed(container, server, 'general');
      final help = channelNamed(container, server, 'help');
      final activity = container.read(activityProvider(server).notifier);

      activity.focus(help);
      repo.sendMessage(server, general, 'mine', nonce: 'n1');
      async.elapse(const Duration(seconds: 1));
      final afterOwn = container
          .read(activityProvider(server))
          .of(general)
          .read
          .unread;
      repo.debugPostAs(server, help, 'from someone else');
      repo.debugPostAs(server, general, 'also from someone else');
      async.flushMicrotasks();
      final before = container
          .read(activityProvider(server))
          .of(general)
          .read
          .unread;
      activity.markRead(general);

      expect(afterOwn, 0);
      expect(container.read(activityProvider(server)).of(help).read.unread, 0);
      expect(before, 1);
      expect(
        container.read(activityProvider(server)).of(general).read.unread,
        0,
      );
    });
  });

  test('typing shows for 10 seconds', () {
    withApp((async, container, repo) {
      final server = repo.servers.first.key;
      final general = channelNamed(container, server, 'general');
      const kai = 1001;

      repo.debugTyping(server, general, kai);
      async.elapse(const Duration(seconds: 1));
      final during = container.read(typingProvider(server)).users(general);
      async.elapse(const Duration(seconds: 10));

      expect(during, [kai]);
      expect(container.read(typingProvider(server)).users(general), isEmpty);
    });
  });

  test('the server order is remembered', () {
    final store = MemoryKeyValueStore();
    late List<String> reordered;
    withApp((async, container, repo) {
      container
          .read(serverListProvider.notifier)
          .move(repo.servers.last.key, 0);
    }, store: store);
    withApp((async, container, repo) {
      reordered = [
        for (final server in container.read(serverListProvider)) server.key,
      ];
    }, store: store);

    expect(reordered.first, 'homelab.local:7710');
  });

  test('a rate-limited send fails and pauses sending until told', () {
    withApp((async, container, repo) {
      final server = repo.servers.first.key;
      final general = channelNamed(container, server, 'general');
      final channel = (server: server, channel: general);
      container.read(channelMessagesProvider(channel));
      async.elapse(const Duration(milliseconds: 10));
      repo.debugRateLimit(const Duration(seconds: 5));

      container.read(channelMessagesProvider(channel).notifier).send('hi');
      async.elapse(const Duration(seconds: 1));

      expect(
        container
            .read(channelMessagesProvider(channel))
            .pending
            .single
            .sendState,
        SendState.failed,
      );
      final until = container.read(sendCooldownProvider(channel));
      expect(until, isNotNull);

      async.elapse(const Duration(seconds: 5));
      expect(container.read(sendCooldownProvider(channel)), isNull);
    });
  });

  test('voice participants and speakers update', () {
    withApp((async, container, repo) {
      final server = repo.servers.first.key;
      final voice = container
          .read(serverProvider(server))
          .data!
          .channels
          .values
          .firstWhere((channel) => channel.kind == ChannelKind.voice);

      container.read(voiceSessionProvider.notifier).join(server, voice.id);
      async.elapse(const Duration(seconds: 2));

      final participants = container.read(voiceProvider(server))[voice.id]!;
      expect(participants.map((p) => p.userId), contains(1000));
      expect(container.read(voiceSessionProvider).channelId, voice.id);
    }, simulateLife: true);
  });

  test('a moderator moving or disconnecting you moves the call', () {
    withApp((async, container, repo) {
      final server = repo.servers.first.key;
      final voiceChannels = container
          .read(serverProvider(server))
          .data!
          .channels
          .values
          .where((channel) => channel.kind == ChannelKind.voice)
          .map((channel) => channel.id)
          .toList();
      container
          .read(voiceSessionProvider.notifier)
          .join(server, voiceChannels.first);
      async.elapse(const Duration(seconds: 1));

      repo.debugMoveSelf(voiceChannels.last);
      async.flushMicrotasks();
      final moved = container.read(voiceSessionProvider);
      repo.debugMoveSelf(null);
      async.flushMicrotasks();

      expect((moved.serverKey, moved.channelId), (server, voiceChannels.last));
      expect(container.read(voiceSessionProvider).connected, isFalse);
      expect(
        container.read(voiceProvider(server))[voiceChannels.last],
        isEmpty,
      );
    });
  });
}
