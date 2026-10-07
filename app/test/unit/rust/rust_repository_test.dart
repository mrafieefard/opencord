import 'dart:async';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/channel_kind.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/core/rust/read_positions.dart';
import 'package:opencord/core/rust/rust_repository.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/src/rust/api/types.dart' as core;

import '../../support/fake_core.dart';

const _server = 'home.example:7710';
const _self = 1, _kai = 2;
const _general = 10;
final _now = DateTime(2026, 10, 7, 12);

core.Message _message(
  int id, {
  int author = _kai,
  String content = 'hi',
  int channel = _general,
}) => core.Message(
  id: id,
  channelId: channel,
  authorId: author,
  content: content,
  createdAtMs: 1760000000000 + id,
);

core.Channel _text(int id, String name, {int position = 0}) => core.Channel(
  id: id,
  kind: core.ChannelKind.text,
  name: name,
  position: position,
  overwrites: const [],
);

core.ReadySnapshot _ready({List<core.Channel> more = const []}) =>
    core.ReadySnapshot(
      selfUser: const core.User(
        id: _self,
        publicKeyHex: 'abcd',
        fingerprint: 'ABCD-EFGH-IJKL-MNOP',
        displayName: 'Alex',
      ),
      server: const core.ServerInfo(
        serverIdHex: 'ff',
        name: 'Home',
        description: '',
        ownerId: _self,
        openJoin: false,
        everyoneRoleId: 5,
      ),
      channels: [_text(_general, 'general'), ...more],
      roles: const [],
      members: const [],
      presences: const [],
      serverPermissions: 0x7,
      // View and read history.
      channelPermissions: [
        for (final channel in [_general, ...more.map((c) => c.id)])
          core.ChannelPermissions(channelId: channel, permissions: 0x5),
      ],
    );

/// A keychain that can be locked, refusing to save, or out of reach.
class _Keyring extends MemoryIdentityStore {
  var locked = false;
  var unreadable = false;

  @override
  Future<SavedIdentity?> read() async {
    if (unreadable) {
      throw const RepoException(RepoErrorKind.other, 'No Secret Service.');
    }
    return super.read();
  }

  @override
  Future<void> write(SavedIdentity identity) async {
    if (locked) {
      throw const RepoException(RepoErrorKind.other, 'The keyring is locked.');
    }
    await super.write(identity);
  }
}

class _Harness {
  _Harness._(this.core, this.store, this.identities, this.repository);

  static Future<_Harness> start({
    SavedIdentity? saved,
    MemoryIdentityStore? keyring,
  }) async {
    final fake = FakeCoreApi();
    final store = MemoryKeyValueStore();
    final identities = keyring ?? MemoryIdentityStore(saved);
    final repository = await RustRepository.open(
      core: fake,
      identities: identities,
      store: store,
      clock: () => _now,
    );
    final harness = _Harness._(fake, store, identities, repository);
    repository.events.listen(harness.events.add);
    repository.start();
    return harness;
  }

  final FakeCoreApi core;
  final MemoryKeyValueStore store;
  final MemoryIdentityStore identities;
  final RustRepository repository;
  final events = <RepoEvent>[];

  Future<void> settle() => Future<void>.delayed(Duration.zero);
}

void main() {
  group('identity (Phase 1 §9.1)', () {
    test('with none saved, the app needs onboarding', () async {
      final harness = await _Harness.start();

      expect(harness.repository.identity, isNull);
      expect(harness.core.calls, isNot(contains(startsWith('identityLoad'))));
    });

    test('a saved identity is loaded at once', () async {
      final harness = await _Harness.start(
        saved: SavedIdentity(
          secret: Uint8List.fromList([9]),
          displayName: 'Alex',
        ),
      );

      expect(harness.core.calls, contains('identityLoad:Alex'));
      expect(harness.repository.identity?.fingerprint, 'ABCD-EFGH-IJKL-MNOP');
    });

    test('a generated identity is saved once adopted', () async {
      final harness = await _Harness.start();
      final draft = await harness.repository.generateIdentity();

      await harness.repository.adoptIdentity(
        draft.backup,
        displayName: ' Alex ',
      );
      await harness.settle();

      expect(draft.fingerprint, 'ABCD-EFGH-IJKL-MNOP');
      expect(harness.identities.saved?.displayName, 'Alex');
      expect(harness.identities.saved?.secret, [1, 2, 3]);
      expect(harness.repository.identity?.displayName, 'Alex');
      expect(harness.events.whereType<IdentityChanged>(), isNotEmpty);
    });

    test('an identity the keyring could not save is not used', () async {
      final harness = await _Harness.start(keyring: _Keyring()..locked = true);
      final draft = await harness.repository.generateIdentity();

      await expectLater(
        harness.repository.adoptIdentity(draft.backup, displayName: 'Alex'),
        throwsA(isA<RepoException>()),
      );
      await harness.settle();

      expect(harness.repository.identity, isNull);
      expect(harness.events.whereType<IdentityChanged>(), isEmpty);
      expect(harness.core.calls, isNot(contains(startsWith('identityLoad'))));
    });

    test('a new name the keyring could not save is not used', () async {
      final keyring = _Keyring();
      final harness = await _Harness.start(keyring: keyring);
      final draft = await harness.repository.generateIdentity();
      await harness.repository.adoptIdentity(draft.backup, displayName: 'Alex');
      keyring.locked = true;

      await expectLater(
        harness.repository.updateDisplayName('Sam'),
        throwsA(isA<RepoException>()),
      );

      expect(harness.repository.identity?.displayName, 'Alex');
      expect(harness.core.calls, isNot(contains('identityLoad:Sam')));
    });

    test('a keyring that cannot be read is reported until it can', () async {
      final keyring = _Keyring()
        ..saved = SavedIdentity(
          secret: Uint8List.fromList([9]),
          displayName: 'Alex',
        )
        ..unreadable = true;
      final harness = await _Harness.start(keyring: keyring);

      expect(harness.repository.identityUnavailable?.message, contains('No'));
      expect(harness.repository.identity, isNull);

      keyring.unreadable = false;
      await harness.repository.reloadIdentity();
      await harness.settle();

      expect(harness.repository.identityUnavailable, isNull);
      expect(harness.repository.identity?.displayName, 'Alex');
      expect(harness.events.whereType<IdentityChanged>(), isNotEmpty);
    });

    test('a bad backup or name is refused and nothing is saved', () async {
      final harness = await _Harness.start();

      await expectLater(
        harness.repository.adoptIdentity('nope', displayName: 'Alex'),
        throwsA(
          isA<RepoException>().having(
            (e) => e.kind,
            'kind',
            RepoErrorKind.invalidArgument,
          ),
        ),
      );
      await expectLater(
        harness.repository.adoptIdentity('backup:1', displayName: ''),
        throwsA(isA<RepoException>()),
      );
      expect(harness.identities.saved, isNull);
    });
  });

  test(
    'saved servers are listed as soon as it opens, before any connects',
    () async {
      final fake = FakeCoreApi()
        ..servers = const [
          core.Server(
            key: _server,
            name: 'Home',
            host: 'home.example',
            port: 7710,
          ),
        ];

      final repository = await RustRepository.open(
        core: fake,
        identities: MemoryIdentityStore(),
        store: MemoryKeyValueStore(),
      );

      // The tray and the server list read it before start (offline too).
      expect(repository.servers.map((s) => s.key), [_server]);
    },
  );

  test('connection states come through with the retry time', () async {
    final harness = await _Harness.start();

    harness.core.emit(
      _server,
      const core.CoreEventPayload.connectionState(
        core.ConnectionState.reconnecting(attempt: 1, retryInMs: 8000),
      ),
    );
    await harness.settle();

    final change = harness.events.whereType<ConnectionChanged>().single;
    expect(change.status.retryAt, _now.add(const Duration(seconds: 8)));
  });

  group('Ready rebuilds what the server does not keep (§6)', () {
    test('unread counts and mentions since the remembered position', () async {
      final harness = await _Harness.start();
      ReadPositions(harness.store).save(_server, _general, 100);
      harness.core.history[_general] = [
        _message(103, content: 'look <@$_self>'),
        _message(102),
        _message(101, author: _self),
        _message(100),
      ];

      harness.core.emit(_server, core.CoreEventPayload.ready(_ready()));
      await harness.settle();
      await harness.settle();

      final ready = harness.events.whereType<Ready>().single.snapshot;
      expect(ready.lastMessages[_general]?.id, 103);
      expect(ready.readStates[_general]?.lastReadId, 100);
      expect(ready.readStates[_general]?.unread, 2);
      expect(ready.readStates[_general]?.mentions, 1);
    });

    test('a channel never read before starts out read', () async {
      final harness = await _Harness.start();
      harness.core.history[_general] = [_message(102), _message(101)];

      harness.core.emit(_server, core.CoreEventPayload.ready(_ready()));
      await harness.settle();
      await harness.settle();

      final ready = harness.events.whereType<Ready>().single.snapshot;
      expect(ready.readStates[_general]?.unread, 0);
      expect(ReadPositions(harness.store).of(_server, _general), 102);
    });

    test('a channel empty when first seen counts what arrives later', () async {
      final harness = await _Harness.start();
      harness.core.emit(_server, core.CoreEventPayload.ready(_ready()));
      await harness.settle();
      await harness.settle();

      // Next session: two messages came while away.
      harness.core.history[_general] = [_message(102), _message(101)];
      harness.core.emit(_server, core.CoreEventPayload.ready(_ready()));
      await harness.settle();
      await harness.settle();

      final ready = harness.events.whereType<Ready>().last.snapshot;
      expect(ready.readStates[_general]?.unread, 2);
    });

    test('a channel made during a session counts what arrives later', () async {
      final harness = await _Harness.start();
      harness.core.emit(_server, core.CoreEventPayload.ready(_ready()));
      await harness.settle();
      await harness.settle();
      const release = 11;
      final channel = _text(release, 'release', position: 1);
      harness.core.emit(_server, core.CoreEventPayload.channelCreate(channel));
      await harness.settle();

      harness.core.history[release] = [_message(201, channel: release)];
      harness.core.emit(
        _server,
        core.CoreEventPayload.ready(_ready(more: [channel])),
      );
      await harness.settle();
      await harness.settle();

      final ready = harness.events.whereType<Ready>().last.snapshot;
      expect(ready.readStates[release]?.unread, 1);
    });

    test('a rebuild that fails still lets the server go on', () async {
      final harness = await _Harness.start();
      harness.core.fetchFailure = const FormatException('unexpected');

      harness.core.emit(_server, core.CoreEventPayload.ready(_ready()));
      harness.core.emit(
        _server,
        core.CoreEventPayload.messageCreate(_message(104)),
      );
      await harness.settle();
      await harness.settle();

      expect(harness.events.whereType<Ready>(), hasLength(1));
      expect(harness.events.whereType<MessageCreated>(), hasLength(1));
    });

    test('events meanwhile wait until Ready is out', () async {
      final harness = await _Harness.start();
      harness.core.fetchGate = Completer();

      harness.core.emit(_server, core.CoreEventPayload.ready(_ready()));
      harness.core.emit(
        _server,
        core.CoreEventPayload.messageCreate(_message(104)),
      );
      await harness.settle();
      expect(harness.events.whereType<MessageCreated>(), isEmpty);

      harness.core.fetchGate!.complete();
      await harness.settle();
      await harness.settle();

      final order = [
        for (final event in harness.events)
          if (event is Ready || event is MessageCreated) event.runtimeType,
      ];
      expect(order, [Ready, MessageCreated]);
    });
  });

  test('history comes oldest first', () async {
    final harness = await _Harness.start();
    harness.core.history[_general] = [_message(3), _message(2), _message(1)];

    final messages = await harness.repository.fetchMessages(_server, _general);

    expect(messages.map((m) => m.id), [1, 2, 3]);
  });

  test(
    'a private channel is shown to its maker, then hidden from everyone',
    () async {
      final harness = await _Harness.start();
      harness.core.emit(_server, core.CoreEventPayload.ready(_ready()));
      await harness.settle();
      await harness.settle();

      await harness.repository.createChannel(
        _server,
        kind: ChannelKind.text,
        name: 'secret',
        private: true,
      );

      expect(
        harness.core.calls.where(
          (call) =>
              call.startsWith('createChannel') || call.startsWith('overwrite'),
        ),
        [
          'createChannel:secret',
          'overwrite:77:member:$_self:allow=1:deny=0',
          'overwrite:77:role:5:allow=0:deny=1',
        ],
      );
    },
  );

  test('presence goes to every server, invisible as offline', () async {
    final harness = await _Harness.start();
    harness.core.servers = const [
      core.Server(key: 'a:1', name: 'A', host: 'a', port: 1),
      core.Server(key: 'b:1', name: 'B', host: 'b', port: 1),
    ];

    await harness.repository.updatePresence(SelfPresence.invisible);

    expect(
      harness.core.calls,
      containsAll(['presence:a:1:offline', 'presence:b:1:offline']),
    );
  });

  test('core errors arrive as repository errors', () async {
    final harness = await _Harness.start();
    harness.core.sendError = const core.CoreError.server(
      code: core.ErrorCode.rateLimited,
      message: 'Slow down.',
      retryAfterMs: 2000,
    );

    await expectLater(
      harness.repository.sendMessage(_server, _general, 'hi', nonce: 'n'),
      throwsA(
        isA<RepoException>()
            .having((e) => e.kind, 'kind', RepoErrorKind.rateLimited)
            .having(
              (e) => e.retryAfter,
              'retryAfter',
              const Duration(seconds: 2),
            ),
      ),
    );
  });

  test('Retry now asks the core', () async {
    final harness = await _Harness.start();

    await harness.repository.retryNow(_server);

    expect(harness.core.calls, contains('retry:$_server'));
  });

  test('adding a server lists it and says the list changed', () async {
    final harness = await _Harness.start();
    harness.core.addOutcome = const core.AddServerOutcome.added(
      core.Server(key: _server, name: 'Home', host: 'home.example', port: 7710),
    );

    final result = await harness.repository.addServer(
      'opencord://$_server/invite/x',
    );
    await harness.settle();

    expect(result, isA<ServerAdded>());
    expect(harness.repository.servers.map((s) => s.key), [_server]);
    expect(harness.events.whereType<ServersChanged>(), isNotEmpty);
  });

  test('reading a channel is remembered on this device', () async {
    final harness = await _Harness.start();

    harness.repository.markRead(_server, _general, 120);

    expect(ReadPositions(harness.store).of(_server, _general), 120);
  });
}
