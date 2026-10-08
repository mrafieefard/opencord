import 'dart:async';
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/channel_kind.dart';
import 'package:opencord/core/model/voice.dart';
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
  bitrate: 0,
  userLimit: 0,
  textInVoice: false,
  id: id,
  kind: core.ChannelKind.text,
  name: name,
  position: position,
  overwrites: const [],
);

core.ReadySnapshot _ready({
  List<core.Channel> more = const [],
  List<core.VoiceState> voice = const [],
}) => core.ReadySnapshot(
  voiceEnabled: true,
  voiceStates: voice,
  voiceSettings: coreVoiceSettings,
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

  group('a new display name (Phase 1 §9.1)', () {
    const home = core.Server(
      key: _server,
      name: 'Home',
      host: 'home.example',
      port: 7710,
    );
    final saved = SavedIdentity(
      secret: Uint8List.fromList([9]),
      displayName: 'Alex',
    );

    test(
      'reaches a server that was not connected, at its next session',
      () async {
        final harness = await _Harness.start(saved: saved);
        harness.core
          ..servers = const [home]
          ..profileErrors[_server] = const core.CoreError.notConnected();

        await harness.repository.updateDisplayName('Sam');
        harness.core.profileErrors.clear();
        // The server still knows the old name (Alex, in Ready's self).
        harness.core.emit(_server, core.CoreEventPayload.ready(_ready()));
        await harness.settle();
        await harness.settle();

        expect(
          harness.core.calls.where((call) => call == 'profile:$_server:Sam'),
          hasLength(2),
        );
      },
    );

    test('still reaches the other servers when one refuses it', () async {
      final harness = await _Harness.start(saved: saved);
      harness.core
        ..servers = const [
          core.Server(key: 'a:1', name: 'A', host: 'a', port: 1),
          core.Server(key: 'b:1', name: 'B', host: 'b', port: 1),
        ]
        ..profileErrors['a:1'] = const core.CoreError.server(
          code: core.ErrorCode.forbidden,
          message: 'Not here.',
        );

      await expectLater(
        harness.repository.updateDisplayName('Sam'),
        throwsA(isA<RepoException>()),
      );
      expect(harness.core.calls, contains('profile:b:1:Sam'));
    });
  });

  group('a private channel', () {
    test('that cannot be made private is not left public', () async {
      final harness = await _Harness.start();
      harness.core.emit(_server, core.CoreEventPayload.ready(_ready()));
      await harness.settle();
      await harness.settle();
      harness.core.overwriteError = const core.CoreError.notConnected();

      await expectLater(
        harness.repository.createChannel(
          _server,
          kind: ChannelKind.text,
          name: 'secret',
          private: true,
        ),
        throwsA(isA<RepoException>()),
      );
      expect(harness.core.calls, contains('deleteChannel:77'));
    });

    test('left public after all says so', () async {
      final harness = await _Harness.start();
      harness.core.emit(_server, core.CoreEventPayload.ready(_ready()));
      await harness.settle();
      await harness.settle();
      harness.core
        ..overwriteError = const core.CoreError.notConnected()
        ..deleteError = const core.CoreError.notConnected();

      await expectLater(
        harness.repository.createChannel(
          _server,
          kind: ChannelKind.text,
          name: 'secret',
          private: true,
        ),
        throwsA(
          isA<RepoException>().having(
            (e) => e.message,
            'message',
            contains('could not be made private'),
          ),
        ),
      );
    });

    test('is not made before the server is known', () async {
      final harness = await _Harness.start();

      await expectLater(
        harness.repository.createChannel(
          _server,
          kind: ChannelKind.text,
          name: 'secret',
          private: true,
        ),
        throwsA(isA<RepoException>()),
      );
      expect(harness.core.calls, isNot(contains(startsWith('createChannel'))));
    });
  });

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

  group('voice (Phase 2 V0)', () {
    const lounge = 20, studio = 21;

    Future<_Harness> ready({List<core.VoiceState> voice = const []}) async {
      final harness = await _Harness.start();
      harness.core.emit(
        _server,
        core.CoreEventPayload.ready(_ready(voice: voice)),
      );
      await harness.settle();
      await harness.settle();
      return harness;
    }

    void update(_Harness harness, core.VoiceState state) => harness.core.emit(
      _server,
      core.CoreEventPayload.voiceStateUpdate(state),
    );

    test('Ready lists who is in each voice channel', () async {
      final harness = await ready(
        voice: [voiceState(_kai, lounge, thisDevice: false, selfMute: true)],
      );

      final snapshot = harness.events.whereType<Ready>().single.snapshot;
      expect(snapshot.voiceEnabled, isTrue);
      expect(snapshot.voice[lounge]?.single.userId, _kai);
      expect(snapshot.voice[lounge]?.single.muted, isTrue);
      expect(snapshot.voiceSettings.maxVoiceBitrate, 96000);
    });

    test('a move announces both channels', () async {
      final harness = await ready(
        voice: [voiceState(_kai, lounge, thisDevice: false)],
      );

      update(
        harness,
        voiceState(_kai, studio, thisDevice: false, serverMute: true),
      );
      await harness.settle();

      final changes = {
        for (final change in harness.events.whereType<VoiceChanged>())
          change.channelId: change.participants,
      };
      expect(changes[lounge], isEmpty);
      expect(changes[studio]?.single.serverMuted, isTrue);
      expect(harness.events.whereType<OwnVoiceChanged>(), isEmpty);
    });

    test('a moderator moving or disconnecting this device is told', () async {
      final harness = await ready();
      await harness.repository.joinVoice(_server, lounge);

      update(harness, voiceState(_self, lounge));
      update(harness, voiceState(_self, studio));
      update(harness, voiceState(_self, null));
      await harness.settle();

      expect(
        harness.events.whereType<OwnVoiceChanged>().map((e) => e.channelId),
        [studio, null],
      );
    });

    test('another device taking the call over ends it here', () async {
      final harness = await ready();
      await harness.repository.joinVoice(_server, lounge);

      update(harness, voiceState(_self, lounge, thisDevice: false));
      await harness.settle();

      expect(
        harness.events.whereType<OwnVoiceChanged>().single.channelId,
        isNull,
      );
    });

    test('joining, leaving, mute and deafen go to the core', () async {
      final harness = await ready();
      harness.core.calls.clear();

      await harness.repository.joinVoice(_server, lounge);
      await harness.repository.setVoiceSelf(muted: true, deafened: false);
      await harness.repository.leaveVoice();

      expect(harness.core.calls, [
        'voiceJoin:$_server:$lounge',
        'selfMute:true',
        'selfDeaf:false',
        'voiceLeave',
      ]);
      expect(harness.repository.capabilities.voice, isTrue);
      expect(harness.repository.capabilities.camera, isFalse);
      expect(harness.repository.capabilities.screenShare, isFalse);
    });

    test('a full channel is its own kind of refusal', () async {
      final harness = await ready();
      harness.core.joinError = const core.CoreError.server(
        code: core.ErrorCode.voiceChannelFull,
        message: 'that voice channel is full',
      );

      await expectLater(
        harness.repository.joinVoice(_server, lounge),
        throwsA(
          isA<RepoException>().having(
            (e) => e.kind,
            'kind',
            RepoErrorKind.voiceChannelFull,
          ),
        ),
      );
    });

    test('voice settings arrive, and saving sends every one', () async {
      final harness = await ready();

      harness.core.emit(
        _server,
        const core.CoreEventPayload.voiceSettingsUpdate(coreVoiceSettings),
      );
      await harness.settle();
      await harness.repository.updateVoiceSettings(
        _server,
        harness.events.whereType<VoiceSettingsChanged>().single.settings,
      );

      final sent = harness.core.voiceChanges!;
      expect(sent.afkChannelId, 0, reason: 'no AFK channel goes as 0');
      expect(sent.maxVoiceBitrate, 96000);
      expect(sent.afkTimeoutS, 300);
      expect(sent.screenShareMaxResolution, core.ScreenShareResolution.p720);
    });
  });

  group('voice media (Phase 2 V2)', () {
    const lounge = 20;

    test('connection states reach the app', () async {
      final harness = await _Harness.start();

      harness.core.media.add(
        const core.MediaEvent.connectionState(
          serverKey: _server,
          channelId: lounge,
          state: core.VoiceConnectionState.rtcConnecting(),
        ),
      );
      harness.core.media.add(
        const core.MediaEvent.connectionState(
          serverKey: _server,
          channelId: lounge,
          state: core.VoiceConnectionState.disconnected(reason: 'refused'),
        ),
      );
      await harness.settle();

      final changes = harness.events.whereType<VoiceConnectionChanged>();
      expect(changes.map((c) => (c.serverKey, c.channelId, c.status)), [
        (
          _server,
          lounge,
          const VoiceConnectionStatus(VoiceConnectionPhase.rtcConnecting),
        ),
        (
          _server,
          lounge,
          const VoiceConnectionStatus(
            VoiceConnectionPhase.disconnected,
            reason: 'refused',
          ),
        ),
      ]);
    });

    test(
      'devices, and devices standing in for missing ones, reach the app',
      () async {
        final harness = await _Harness.start();
        const mic = core.AudioDevice(id: 'pipewire:mic', name: 'Mic');

        harness.core.media.add(
          const core.MediaEvent.devicesChanged(
            core.AudioDevices(
              inputs: [mic],
              outputs: [],
              defaultInput: 'pipewire:mic',
            ),
          ),
        );
        harness.core.media.add(
          const core.MediaEvent.deviceFellBack(output: false, device: 'Mic'),
        );
        await harness.settle();

        final devices = harness.events.whereType<AudioDevicesChanged>().single;
        expect(devices.devices.inputs, [
          const AudioDevice(id: 'pipewire:mic', name: 'Mic'),
        ]);
        expect(devices.devices.defaultInput, 'pipewire:mic');
        final fellBack = harness.events.whereType<AudioDeviceFellBack>().single;
        expect((fellBack.output, fellBack.device), (false, 'Mic'));
      },
    );

    test('audio choices go to the core as fractions of full volume', () async {
      final harness = await _Harness.start();

      harness.repository.applyAudio(
        const AudioConfig(
          inputDevice: 'pipewire:mic',
          pushToTalk: true,
          inputVolume: 150,
          outputVolume: 50,
        ),
      );
      harness.repository.setPushToTalk(true);
      harness.repository.setUserVolume(_server, _kai, 200);
      harness.repository.setUserLocalMute(_server, _kai, true);

      final sent = harness.core.audioSettings!;
      expect(sent.inputDevice, 'pipewire:mic');
      expect(sent.outputDevice, isNull);
      expect(sent.pushToTalk, isTrue);
      expect(sent.inputVolume, 1.5);
      expect(sent.outputVolume, 0.5);
      expect(
        harness.core.calls,
        containsAllInOrder([
          'pushToTalk:true',
          'userVolume:$_server:$_kai:2.0',
          'localMute:$_server:$_kai:true',
        ]),
      );
    });

    test('the device list comes from the core', () async {
      final harness = await _Harness.start();
      harness.core.devices = const core.AudioDevices(
        inputs: [core.AudioDevice(id: 'alsa:hw:0', name: 'Built-in')],
        outputs: [core.AudioDevice(id: 'alsa:hw:1', name: 'Speakers')],
        defaultOutput: 'alsa:hw:1',
      );

      final devices = await harness.repository.audioDevices();

      expect(devices.inputs.single.name, 'Built-in');
      expect(devices.outputs.single.id, 'alsa:hw:1');
      expect(devices.defaultOutput, 'alsa:hw:1');
      expect(devices.defaultInput, isNull);
    });
  });
}
