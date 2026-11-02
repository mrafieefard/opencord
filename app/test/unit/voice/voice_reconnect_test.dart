// The reconnect bar's offer (D88): after the app closed in a call, another
// device took it, or getting back in was refused; never after leaving or a
// moderator's disconnect.

import 'dart:convert';
import 'dart:math';

import 'package:fake_async/fake_async.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/mock/mock_repository.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/providers/voice_reconnect.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/settings/key_value_store.dart';

final _base = DateTime(2026, 11, 2, 18);

void _withApp(
  void Function(
    FakeAsync async,
    ProviderContainer container,
    MockRepository repo,
    MemoryKeyValueStore store,
  )
  body, {
  MemoryKeyValueStore? store,
}) {
  fakeAsync((async) {
    final kept = store ?? MemoryKeyValueStore();
    final repo = MockRepository(
      clock: () => _base.add(async.elapsed),
      random: Random(3),
      simulateLife: false,
    );
    final container = ProviderContainer(
      overrides: [
        repositoryProvider.overrideWithValue(repo),
        keyValueStoreProvider.overrideWithValue(kept),
        clockProvider.overrideWithValue(() => _base.add(async.elapsed)),
      ],
    );
    container
      ..read(eventPumpProvider)
      ..listen(voiceReconnectProvider, (_, _) {});
    repo.start();
    async.elapse(const Duration(seconds: 2));
    body(async, container, repo, kept);
    container.dispose();
    repo.dispose();
    async.flushTimers(flushPeriodicTimers: false);
  });
}

/// The first server and its voice channels.
(String, List<int>) _voice(ProviderContainer container, MockRepository repo) {
  final server = repo.servers.first.key;
  final channels = container
      .read(serverProvider(server))
      .data!
      .channels
      .values
      .where((channel) => channel.kind == ChannelKind.voice)
      .map((channel) => channel.id)
      .toList();
  return (server, channels);
}

void _join(FakeAsync async, ProviderContainer container, String s, int c) {
  container.read(voiceSessionProvider.notifier).join(s, c);
  async.elapse(const Duration(seconds: 1));
}

void main() {
  test('a call the app closed in is offered at the next start', () {
    final store = MemoryKeyValueStore()
      ..write(lastCallKey, jsonEncode({'server': 'a.example', 'channel': 7}));
    _withApp((async, container, repo, store) {
      expect(
        container.read(voiceReconnectProvider),
        const VoiceDrop(
          serverKey: 'a.example',
          channelId: 7,
          reason: VoiceDropReason.closed,
        ),
      );
      expect(store.read(lastCallKey), isNull, reason: 'offered once');
    }, store: store);
  });

  test('nothing is offered at a start after a clean leave', () {
    _withApp((async, container, repo, store) {
      expect(container.read(voiceReconnectProvider), isNull);
    });
  });

  test('a call is remembered while in it and forgotten on leaving', () {
    _withApp((async, container, repo, store) {
      final (server, channels) = _voice(container, repo);
      _join(async, container, server, channels.first);
      expect(jsonDecode(store.read(lastCallKey)!), {
        'server': server,
        'channel': channels.first,
      });

      container.read(voiceSessionProvider.notifier).leave();
      async.elapse(const Duration(seconds: 1));
      expect(store.read(lastCallKey), isNull);
      expect(container.read(voiceReconnectProvider), isNull);
    });
  });

  test('a move by a moderator is remembered as the call', () {
    _withApp((async, container, repo, store) {
      final (server, channels) = _voice(container, repo);
      _join(async, container, server, channels.first);
      repo.debugMoveSelf(channels.last);
      async.flushMicrotasks();
      expect(
        (jsonDecode(store.read(lastCallKey)!) as Map)['channel'],
        channels.last,
      );
    });
  });

  test('another device taking the call offers to take it back', () {
    _withApp((async, container, repo, store) {
      final (server, channels) = _voice(container, repo);
      _join(async, container, server, channels.first);

      repo.debugVoiceElsewhere();
      async.flushMicrotasks();

      expect(
        container.read(voiceReconnectProvider),
        VoiceDrop(
          serverKey: server,
          channelId: channels.first,
          reason: VoiceDropReason.elsewhere,
        ),
      );
      expect(store.read(lastCallKey), isNull);
    });
  });

  test('a refused rejoin offers to try again, with the reason', () {
    _withApp((async, container, repo, store) {
      final (server, channels) = _voice(container, repo);
      _join(async, container, server, channels.first);

      repo.debugRejoinRefused('missing permission: connect');
      async.flushMicrotasks();

      expect(
        container.read(voiceReconnectProvider),
        VoiceDrop(
          serverKey: server,
          channelId: channels.first,
          reason: VoiceDropReason.refused,
          message: 'missing permission: connect',
        ),
      );
    });
  });

  test("a moderator's disconnect offers nothing", () {
    _withApp((async, container, repo, store) {
      final (server, channels) = _voice(container, repo);
      _join(async, container, server, channels.first);

      repo.debugMoveSelf(null);
      async.flushMicrotasks();

      expect(container.read(voiceReconnectProvider), isNull);
      expect(store.read(lastCallKey), isNull);
    });
  });

  test('reconnect joins the call again and the offer goes', () {
    _withApp((async, container, repo, store) {
      final (server, channels) = _voice(container, repo);
      _join(async, container, server, channels.first);
      repo.debugVoiceElsewhere();
      async.flushMicrotasks();

      container.read(voiceReconnectProvider.notifier).reconnect();
      async.elapse(const Duration(seconds: 1));

      final session = container.read(voiceSessionProvider);
      expect((session.serverKey, session.channelId), (server, channels.first));
      expect(container.read(voiceReconnectProvider), isNull);
    });
  });

  test('joining any call takes the offer away', () {
    _withApp((async, container, repo, store) {
      final (server, channels) = _voice(container, repo);
      _join(async, container, server, channels.first);
      repo.debugVoiceElsewhere();
      async.flushMicrotasks();

      _join(async, container, server, channels.last);

      expect(container.read(voiceReconnectProvider), isNull);
    });
  });

  test('dismissing takes the offer away', () {
    _withApp((async, container, repo, store) {
      final (server, channels) = _voice(container, repo);
      _join(async, container, server, channels.first);
      repo.debugVoiceElsewhere();
      async.flushMicrotasks();

      container.read(voiceReconnectProvider.notifier).dismiss();

      expect(container.read(voiceReconnectProvider), isNull);
    });
  });

  test('quitting forgets the call, so the next start offers nothing', () {
    _withApp((async, container, repo, store) {
      final (server, channels) = _voice(container, repo);
      _join(async, container, server, channels.first);

      forgetLastCall(store);

      expect(store.read(lastCallKey), isNull);
    });
  });

  test('a remembered call that is not readable is ignored', () {
    final store = MemoryKeyValueStore()..write(lastCallKey, '{"server": 3}');
    _withApp((async, container, repo, store) {
      expect(container.read(voiceReconnectProvider), isNull);
      expect(store.read(lastCallKey), isNull);
    }, store: store);
  });
}
