import 'dart:async';

import 'package:fake_async/fake_async.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/core/rust/rust_repository.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/src/rust/api/types.dart' as core;

import '../../support/fake_core.dart';

// A channel's history over the Rust core's repository (§6), where pages
// can fail, wait, or change between sessions.

const _server = 'home.example:7710';
const _general = 10, _random = 11;
const ChannelRef _open = (server: _server, channel: _general);
const ChannelRef _other = (server: _server, channel: _random);

core.Message _message(int id, {int channel = _general}) => core.Message(
  id: id,
  channelId: channel,
  authorId: 2,
  content: 'm$id',
  createdAtMs: 1760000000000 + id,
);

/// Newest first, as the core answers.
List<core.Message> _ids(Iterable<int> ids, {int channel = _general}) => [
  for (final id in ids.toList().reversed) _message(id, channel: channel),
];

core.Channel _text(int id, String name) => core.Channel(
  id: id,
  kind: core.ChannelKind.text,
  name: name,
  position: 0,
  overwrites: const [],
);

final _readyPayload = core.CoreEventPayload.ready(
  core.ReadySnapshot(
    selfUser: const core.User(
      id: 1,
      publicKeyHex: 'abcd',
      fingerprint: 'ABCD-EFGH-IJKL-MNOP',
      displayName: 'Alex',
    ),
    server: const core.ServerInfo(
      serverIdHex: 'ff',
      name: 'Home',
      description: '',
      ownerId: 1,
      openJoin: false,
      everyoneRoleId: 5,
    ),
    channels: [_text(_general, 'general'), _text(_random, 'random')],
    roles: const [],
    members: const [],
    presences: const [],
    serverPermissions: 0x7,
    channelPermissions: const [
      core.ChannelPermissions(channelId: _general, permissions: 0x7),
      core.ChannelPermissions(channelId: _random, permissions: 0x7),
    ],
  ),
);

void _withCore(
  void Function(
    FakeAsync async,
    ProviderContainer container,
    FakeCoreApi core,
    void Function() newSession,
  )
  body,
) {
  fakeAsync((async) {
    final fake = FakeCoreApi();
    late RustRepository repository;
    RustRepository.open(
      core: fake,
      identities: MemoryIdentityStore(),
      store: MemoryKeyValueStore(),
    ).then((opened) => repository = opened);
    async.flushMicrotasks();
    final container = ProviderContainer(
      overrides: [
        repositoryProvider.overrideWithValue(repository),
        keyValueStoreProvider.overrideWithValue(MemoryKeyValueStore()),
      ],
    );
    container.read(eventPumpProvider);
    repository.start();
    void newSession() {
      fake
        ..emit(
          _server,
          const core.CoreEventPayload.connectionState(
            core.ConnectionState.connected(),
          ),
        )
        ..emit(_server, _readyPayload);
      async.flushMicrotasks();
    }

    newSession();
    body(async, container, fake, newSession);
    container.dispose();
    repository.dispose();
    async.flushTimers(flushPeriodicTimers: false);
  });
}

void main() {
  test(
    'a first page that fails says so, and loads once the server is back',
    () {
      _withCore((async, container, fake, newSession) {
        fake.fetchError = const core.CoreError.notConnected();
        final open = container.listen(
          channelMessagesProvider(_open),
          (_, _) {},
        );
        container.read(channelMessagesProvider(_open).notifier).show();
        async.flushMicrotasks();

        expect(open.read().loaded, isFalse);
        expect(open.read().loadError, isA<RepoException>());

        fake
          ..fetchError = null
          ..history[_general] = _ids([1, 2])
          ..emit(
            _server,
            const core.CoreEventPayload.connectionState(
              core.ConnectionState.reconnecting(attempt: 1, retryInMs: 1000),
            ),
          )
          ..emit(
            _server,
            const core.CoreEventPayload.connectionState(
              core.ConnectionState.connected(),
            ),
          );
        async.flushMicrotasks();

        expect(open.read().loaded, isTrue);
        expect(open.read().loadError, isNull);
        expect(open.read().messages.map((m) => m.id), [1, 2]);
      });
    },
  );

  test('a rate-limited first page is tried again when the server says', () {
    _withCore((async, container, fake, newSession) {
      fake.fetchError = const core.CoreError.server(
        code: core.ErrorCode.rateLimited,
        message: 'Slow down.',
        retryAfterMs: 3000,
      );
      final open = container.listen(channelMessagesProvider(_open), (_, _) {});
      async.flushMicrotasks();
      fake
        ..fetchError = null
        ..history[_general] = _ids([1]);

      async.elapse(const Duration(seconds: 2));
      expect(open.read().loaded, isFalse);
      async.elapse(const Duration(seconds: 2));

      expect(open.read().messages.map((m) => m.id), [1]);
    });
  });

  test('a new session keeps the messages up while the newest page reloads', () {
    _withCore((async, container, fake, newSession) {
      fake.history[_general] = _ids([1, 2, 3]);
      final seen = <List<int>>[];
      final open = container.listen(
        channelMessagesProvider(_open),
        (_, next) => seen.add([for (final m in next.messages) m.id]),
      );
      container.read(channelMessagesProvider(_open).notifier).show();
      async.flushMicrotasks();
      seen.clear();

      fake.history[_general] = _ids([1, 3, 4]);
      newSession();

      // Never an empty, loading list on the way: 2 was deleted and 4 sent
      // meanwhile.
      expect(seen, isNot(contains(isEmpty)));
      expect(open.read().loaded, isTrue);
      expect(open.read().messages.map((m) => m.id), [1, 3, 4]);
    });
  });

  test('older history asked for before a new session stays out of it', () {
    _withCore((async, container, fake, newSession) {
      fake.history[_general] = _ids([for (var id = 1; id <= 120; id++) id]);
      final open = container.listen(channelMessagesProvider(_open), (_, _) {});
      container.read(channelMessagesProvider(_open).notifier).show();
      async.flushMicrotasks();
      expect(open.read().messages.first.id, 71);

      fake.olderGate = Completer();
      unawaited(
        container.read(channelMessagesProvider(_open).notifier).loadOlder(),
      );
      async.flushMicrotasks();
      // 80 messages while away: the newest page no longer meets the old.
      fake.history[_general] = _ids([for (var id = 1; id <= 200; id++) id]);
      newSession();
      fake.olderGate!.complete();
      async.flushMicrotasks();

      final ids = open.read().messages.map((m) => m.id).toList();
      expect(ids, [for (var id = 151; id <= 200; id++) id]);
      expect(open.read().hasOlder, isTrue);
      expect(open.read().loadingOlder, isFalse);
    });
  });

  test('a channel not shown reloads when shown again, not at once', () {
    _withCore((async, container, fake, newSession) {
      fake
        ..history[_general] = _ids([1])
        ..history[_random] = _ids([5], channel: _random);
      final open = container.listen(channelMessagesProvider(_open), (_, _) {});
      container.listen(channelMessagesProvider(_other), (_, _) {});
      final general = container.read(channelMessagesProvider(_open).notifier)
        ..show();
      container.read(channelMessagesProvider(_other).notifier)
        ..show()
        ..hide();
      async.flushMicrotasks();

      fake.calls.clear();
      newSession();
      final fetched = fake.calls.where((call) => call.startsWith('fetch:'));
      // The Ready rebuild reads both; only the shown one reloads its page.
      expect(fetched.where((call) => call == 'fetch:$_random'), hasLength(1));
      expect(fetched.where((call) => call == 'fetch:$_general'), hasLength(2));

      fake.calls.clear();
      container.read(channelMessagesProvider(_other).notifier).show();
      async.flushMicrotasks();
      expect(fake.calls, ['fetch:$_random']);
      expect(open.read().loaded, isTrue);
      general.hide();
    });
  });
}
