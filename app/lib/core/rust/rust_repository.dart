import 'dart:async';
import 'dart:developer' as developer;
import 'dart:typed_data';

import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    as frb_types
    show Int64List;

import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/commands.dart';
import 'package:opencord/core/model/lavalink.dart';
import 'package:opencord/core/model/music.dart';
import 'package:opencord/core/model/music_session.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/model/misc.dart';
import 'package:opencord/core/model/network.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/model/snapshot.dart';
import 'package:opencord/core/model/sound.dart';
import 'package:opencord/core/model/storage.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/model/stream.dart';
import 'package:opencord/core/model/video.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/providers/activity_state.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/core/rust/command_mapping.dart';
import 'package:opencord/core/rust/core_api.dart';
import 'package:opencord/core/rust/core_mapping.dart';
import 'package:opencord/core/rust/identity_store.dart';
import 'package:opencord/core/rust/lavalink_mapping.dart';
import 'package:opencord/core/rust/music_mapping.dart';
import 'package:opencord/core/rust/network_mapping.dart';
import 'package:opencord/core/rust/read_positions.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/src/rust/api/types.dart' as core;

/// The app's repository over the Rust core (desktop UI plan §12 step 15,
/// Phase 1 M5/M6, Phase 2). Servers keep no read states, pins, reactions or
/// replies; read positions live on this device, and after each Ready the
/// unread counts are rebuilt from recent history. Voice has its states but
/// no media yet, so no camera or screen share.
class RustRepository implements OpencordRepository {
  RustRepository._(
    this._core,
    this._identities,
    this._positions,
    this._clock,
    this._saved,
    this._identityUnavailable,
  ) {
    // Listed now: the tray and the server rail read them before start, and
    // they show while offline too.
    _servers = _listServers();
    final video = _core.videoSupport();
    _capabilities = RepoCapabilities(
      voice: true,
      camera: video.camera,
      screenShare: video.screen,
      soundboard: true,
      ownScreenPicker: video.ownScreenPicker,
      lowerOtherApps: _core.audioAttenuationSupported(),
    );
  }

  /// Reads the saved identity first, so the app knows straight away
  /// whether it needs onboarding, or a keyring it cannot reach.
  static Future<RustRepository> open({
    required CoreApi core,
    required IdentityStore identities,
    required KeyValueStore store,
    DateTime Function()? clock,
  }) async {
    SavedIdentity? saved;
    RepoException? unavailable;
    try {
      saved = await identities.read();
    } on RepoException catch (error) {
      unavailable = error;
    }
    return RustRepository._(
      core,
      identities,
      ReadPositions(store),
      clock ?? DateTime.now,
      saved,
      unavailable,
    );
  }

  /// Messages read per channel after a Ready to count what is unread.
  static const _recent = 50;
  static const _fetchesAtOnce = 4;
  static const _fetchTimeout = Duration(seconds: 10);

  final CoreApi _core;
  final IdentityStore _identities;
  final ReadPositions _positions;
  final DateTime Function() _clock;
  final SavedIdentity? _saved;
  final _events = StreamController<RepoEvent>.broadcast();
  StreamSubscription<core.CoreEvent>? _coreEvents;
  StreamSubscription<core.MediaEvent>? _mediaEvents;

  LocalIdentity? _identity;
  Uint8List? _secret;
  RepoException? _identityUnavailable;
  var _servers = const <ServerSummary>[];
  var _presence = SelfPresence.online;

  /// The latest snapshot per server, for its self and @everyone ids.
  final _snapshots = <String, ReadySnapshot>{};

  /// Events held while a Ready is being prepared, and which Ready that is.
  final _held = <String, List<core.CoreEventPayload>>{};
  final _readyCount = <String, int>{};

  /// Everyone in voice per server, by user id.
  final _voice = <String, Map<int, core.VoiceState>>{};

  /// The voice channel this device is in, as far as the app knows.
  ({String server, int channel})? _ownVoice;

  late final RepoCapabilities _capabilities;

  @override
  RepoCapabilities get capabilities => _capabilities;

  /// Whether this device's camera is on.
  var _cameraOn = false;

  /// The camera to turn on; `null` for the system's first.
  String? _cameraDevice;

  @override
  Stream<RepoEvent> get events => _events.stream;

  @override
  LocalIdentity? get identity => _identity;

  @override
  RepoException? get identityUnavailable => _identityUnavailable;

  @override
  Future<void> reloadIdentity() async {
    final SavedIdentity? saved;
    try {
      saved = await _identities.read();
    } on RepoException catch (error) {
      _identityUnavailable = error;
      _emit(const IdentityChanged());
      return;
    }
    _identityUnavailable = null;
    if (saved != null) _useSaved(saved);
    _emit(const IdentityChanged());
  }

  @override
  List<ServerSummary> get servers => _servers;

  @override
  void start() {
    _coreEvents = _core.eventStream().listen(_onCoreEvent);
    _mediaEvents = _core.mediaEventStream().listen(_onMediaEvent);
    if (_saved case final saved?) _useSaved(saved);
  }

  void _useSaved(SavedIdentity saved) {
    try {
      _use(saved.secret, saved.displayName);
    } on RepoException catch (error) {
      developer.log('The saved identity did not load: $error', name: 'core');
    }
  }

  @override
  void dispose() {
    _coreEvents?.cancel();
    _mediaEvents?.cancel();
    _events.close();
  }

  void _emit(RepoEvent event) {
    if (!_events.isClosed) _events.add(event);
  }

  // Calls -------------------------------------------------------------------

  /// Runs a core call, its errors turned into the app's.
  Future<T> _call<T>(Future<T> Function() call) async {
    try {
      return await call();
    } on core.CoreError catch (error) {
      throw errorFrom(error);
    }
  }

  T _now<T>(T Function() call) {
    try {
      return call();
    } on core.CoreError catch (error) {
      throw errorFrom(error);
    }
  }

  List<ServerSummary> _listServers() {
    try {
      return [for (final server in _core.serversList()) serverFrom(server)];
    } on core.CoreError catch (error) {
      developer.log('The server list did not load: $error', name: 'core');
      return const [];
    }
  }

  void _refreshServers() {
    _servers = _listServers();
    _emit(const ServersChanged());
  }

  // Identity ----------------------------------------------------------------

  /// Uses [secret] under [displayName] from now on; the core connects to the
  /// saved servers.
  void _use(List<int> secret, String displayName) {
    final info = _now(() => _core.identityLoad(secret, displayName));
    _secret = Uint8List.fromList(secret);
    _identity = LocalIdentity(
      displayName: displayName,
      fingerprint: info.fingerprint,
      publicKeyHex: info.publicKeyHex,
    );
    _emit(const IdentityChanged());
  }

  Uint8List get _currentSecret =>
      _secret ??
      (throw const RepoException(
        RepoErrorKind.other,
        'Create or import an identity first.',
      ));

  @override
  Future<NewIdentity> generateIdentity() async {
    final generated = _core.identityGenerate();
    return NewIdentity(
      fingerprint: generated.info.fingerprint,
      publicKeyHex: generated.info.publicKeyHex,
      backup: _now(() => _core.identityBackupEncode(generated.secret)),
    );
  }

  @override
  Future<void> adoptIdentity(
    String backup, {
    required String displayName,
  }) async {
    final secret = _now(() => _core.identityBackupDecode(backup.trim()));
    await _saveAndUse(secret, displayName.trim());
  }

  /// Checks [secret] and [displayName], saves them in the keychain, and
  /// only then uses them: in use means the next start finds them too.
  Future<void> _saveAndUse(Uint8List secret, String displayName) async {
    _now(() => _core.identityCheck(secret, displayName));
    await _identities.write(
      SavedIdentity(secret: secret, displayName: displayName),
    );
    _use(secret, displayName);
  }

  @override
  Future<String> exportIdentityBackup() async =>
      _now(() => _core.identityBackupEncode(_currentSecret));

  /// The servers this device knows were joined with the old identity; they
  /// meet the new one at the next connection.
  @override
  Future<void> importIdentityBackup(String backup) =>
      adoptIdentity(backup, displayName: _identity?.displayName ?? '');

  @override
  Future<void> updateDisplayName(String displayName) async {
    final name = displayName.trim();
    await _saveAndUse(_currentSecret, name);
    RepoException? refused;
    for (final server in _listServers()) {
      try {
        await _core.updateProfile(server.key, name);
      } on core.CoreError catch (error) {
        // Servers not connected now get it after their next Ready.
        if (error is! core.CoreError_NotConnected) refused ??= errorFrom(error);
      }
    }
    if (refused != null) throw refused;
  }

  /// A server that missed a new name (it changed while that server was
  /// not connected) gets it now.
  Future<void> _catchUpName(String server, core.ReadySnapshot ready) async {
    final name = _identity?.displayName;
    if (name == null || ready.selfUser.displayName == name) return;
    try {
      await _core.updateProfile(server, name);
    } on core.CoreError catch (error) {
      developer.log('$server kept the old name: $error', name: 'core');
    }
  }

  // Events ------------------------------------------------------------------

  void _onCoreEvent(core.CoreEvent event) {
    final server = event.serverKey;
    if (event.payload case core.CoreEventPayload_Ready(:final field0)) {
      final count = (_readyCount[server] ?? 0) + 1;
      _readyCount[server] = count;
      _held[server] = [];
      unawaited(_prepareReady(server, field0, count));
      return;
    }
    if (_held[server] case final held?) {
      held.add(event.payload);
      return;
    }
    _forward(server, event.payload);
  }

  Future<void> _prepareReady(
    String server,
    core.ReadySnapshot ready,
    int count,
  ) async {
    var activity = (last: <int, Message>{}, read: <int, ReadState>{});
    try {
      activity = await _recentActivity(server, ready);
    } on Exception catch (error, stack) {
      // The counts are a nicety: without them the server still goes on.
      developer.log(
        'Unread counts were not rebuilt',
        name: 'core',
        error: error,
        stackTrace: stack,
      );
    } finally {
      // Unless a newer Ready replaced this one meanwhile.
      if (_readyCount[server] == count) _finishReady(server, ready, activity);
    }
  }

  void _finishReady(
    String server,
    core.ReadySnapshot ready,
    ({Map<int, Message> last, Map<int, ReadState> read}) activity,
  ) {
    final snapshot = snapshotFrom(
      ready,
      lastMessages: activity.last,
      readStates: activity.read,
    );
    _snapshots[server] = snapshot;
    _voice[server] = {
      for (final state in ready.voiceStates)
        if (state.channelId != null) state.userId: state,
    };
    _emit(Ready(server, snapshot));
    for (final payload in _held.remove(server) ?? const []) {
      _forward(server, payload);
    }
    _refreshServers();
    if (_presence != SelfPresence.online) unawaited(_sendPresence(server));
    unawaited(_catchUpName(server, ready));
  }

  /// The newest message and unread counts per readable text channel, from
  /// recent history and the positions remembered here. Channels never
  /// read before start out read.
  Future<({Map<int, Message> last, Map<int, ReadState> read})> _recentActivity(
    String server,
    core.ReadySnapshot ready,
  ) async {
    final selfId = ready.selfUser.id;
    const readable = Permissions(
      1 | 4, // view channel, read history
    );
    final allowed = {
      for (final entry in ready.channelPermissions)
        if (Permissions(entry.permissions).has(readable)) entry.channelId,
    };
    final pending = [
      for (final channel in ready.channels)
        // Voice channels hold a chat too (Phase 2 §12).
        if ((channel.kind == core.ChannelKind.text ||
                channel.kind == core.ChannelKind.voice) &&
            allowed.contains(channel.id))
          channel.id,
    ];
    final last = <int, Message>{};
    final read = <int, ReadState>{};

    Future<void> seed(int channel) async {
      final List<core.Message> recent;
      try {
        recent = await _core
            .fetchMessages(server, channel, null, _recent)
            .timeout(_fetchTimeout);
      } on core.CoreError catch (error) {
        developer.log('No history for $channel: $error', name: 'core');
        return;
      } on TimeoutException {
        developer.log('History for $channel timed out', name: 'core');
        return;
      }
      if (recent.isEmpty) {
        _positions.start(server, channel);
        return;
      }
      final messages = [for (final message in recent) messageFrom(message)];
      final newest = messages.first;
      last[channel] = newest;
      var position = _positions.of(server, channel);
      if (position == null) {
        position = newest.id;
        _positions.save(server, channel, position);
      }
      final unseen = [
        for (final message in messages)
          if (message.id > position && message.authorId != selfId) message,
      ];
      read[channel] = ReadState(
        lastReadId: position,
        unread: unseen.length,
        mentions: unseen.where((m) => mentionsUser(m.content, selfId)).length,
      );
    }

    await Future.wait([
      for (var worker = 0; worker < _fetchesAtOnce; worker++)
        () async {
          while (pending.isNotEmpty) {
            await seed(pending.removeLast());
          }
        }(),
    ]);
    return (last: last, read: read);
  }

  void _forward(String server, core.CoreEventPayload payload) {
    if (payload case core.CoreEventPayload_ChannelCreate(:final field0)
        when field0.kind == core.ChannelKind.text ||
            field0.kind == core.ChannelKind.voice) {
      _positions.start(server, field0.id);
    }
    final RepoEvent? event = switch (payload) {
      core.CoreEventPayload_ConnectionState(:final field0) => ConnectionChanged(
        server,
        connectionFrom(field0, now: _clock()),
      ),
      core.CoreEventPayload_Ready() => null,
      core.CoreEventPayload_MessageCreate(:final field0) => MessageCreated(
        server,
        messageFrom(field0),
      ),
      core.CoreEventPayload_MessageUpdate(:final field0) => MessageUpdated(
        server,
        messageFrom(field0),
      ),
      core.CoreEventPayload_MessageDelete(:final channelId, :final messageId) =>
        MessageDeleted(server, channelId, messageId),
      core.CoreEventPayload_ChannelCreate(:final field0) ||
      core.CoreEventPayload_ChannelUpdate(
        :final field0,
      ) => ChannelUpserted(server, channelFrom(field0)),
      core.CoreEventPayload_ChannelDelete(:final channelId) => ChannelDeleted(
        server,
        channelId,
      ),
      core.CoreEventPayload_RoleCreate(:final field0) ||
      core.CoreEventPayload_RoleUpdate(
        :final field0,
      ) => RoleUpserted(server, roleFrom(field0)),
      core.CoreEventPayload_RoleDelete(:final roleId) => RoleDeleted(
        server,
        roleId,
      ),
      core.CoreEventPayload_MemberJoin(:final field0) => MemberUpserted(
        server,
        memberFrom(field0),
        joined: true,
      ),
      core.CoreEventPayload_MemberUpdate(:final field0) => MemberUpserted(
        server,
        memberFrom(field0),
      ),
      core.CoreEventPayload_MemberLeave(:final userId) => MemberLeft(
        server,
        userId,
      ),
      core.CoreEventPayload_PresenceUpdate(:final field0) => PresenceChanged(
        server,
        field0.userId,
        presenceFrom(field0.status),
      ),
      core.CoreEventPayload_TypingStart(:final channelId, :final userId) =>
        TypingStarted(server, channelId, userId),
      core.CoreEventPayload_ServerUpdate(:final field0) => ServerInfoChanged(
        server,
        serverInfoFrom(field0),
      ),
      core.CoreEventPayload_PermissionsUpdate(
        :final serverPermissions,
        :final channelPermissions,
      ) =>
        PermissionsChanged(server, Permissions(serverPermissions), {
          for (final entry in channelPermissions)
            entry.channelId: Permissions(entry.permissions),
        }),
      core.CoreEventPayload_VoiceStateUpdate(:final field0) => _voiceState(
        server,
        field0,
      ),
      core.CoreEventPayload_VoiceSettingsUpdate(:final field0) =>
        VoiceSettingsChanged(server, voiceSettingsFrom(field0)),
      core.CoreEventPayload_StreamCreate(:final field0) => StreamStarted(
        server,
        streamFrom(field0),
      ),
      core.CoreEventPayload_StreamUpdate(:final field0) => StreamChanged(
        server,
        streamFrom(field0),
      ),
      core.CoreEventPayload_StreamDelete(:final streamKey, :final channelId) =>
        StreamEnded(server, streamKey, channelId),
      core.CoreEventPayload_StreamViewersUpdate(
        :final streamKey,
        :final viewerIds,
      ) =>
        StreamViewersChanged(server, streamKey, [
          for (final id in viewerIds) id.toInt(),
        ]),
      core.CoreEventPayload_SoundboardSoundCreate(:final field0) ||
      core.CoreEventPayload_SoundboardSoundUpdate(
        :final field0,
      ) => SoundUpserted(server, soundFrom(field0)),
      core.CoreEventPayload_SoundboardSoundDelete(:final soundId) =>
        SoundDeleted(server, soundId),
      core.CoreEventPayload_VoiceChannelEffect(:final field0) => SoundPlayed(
        server,
        soundPlayFrom(field0),
      ),
      core.CoreEventPayload_NetworkStatus(:final field0) =>
        NetworkStatusChanged(server, networkInfoFrom(field0)),
      core.CoreEventPayload_ServerAddressesUpdate(:final field0) =>
        ServerAddressesChanged(server, serverAddressesFrom(field0)),
      core.CoreEventPayload_ServerNotice(:final field0) => ServerNoticeReceived(
        server,
        serverNoticeFrom(field0),
      ),
      core.CoreEventPayload_LavalinkNodeStatus(
        :final node,
        :final channelsPlaying,
      ) =>
        LavalinkNodeChanged(
          server,
          nodeId: node.id,
          node: lavalinkNodeFrom(node),
          channelsPlaying: channelsPlaying,
        ),
      core.CoreEventPayload_LavalinkNodeRemoved(
        :final nodeId,
        :final channelsPlaying,
      ) =>
        LavalinkNodeChanged(
          server,
          nodeId: nodeId,
          node: null,
          channelsPlaying: channelsPlaying,
        ),
      core.CoreEventPayload_LavalinkNodeEvent(:final field0) =>
        LavalinkNodeEventReceived(server, lavalinkEventFrom(field0)),
      core.CoreEventPayload_LavalinkTestProgress(:final field0) =>
        LavalinkTestProgressed(server, lavalinkTestFrom(field0)),
      core.CoreEventPayload_MusicSettingsUpdate(:final field0) =>
        MusicSettingsChanged(server, musicSettingsFrom(field0)),
      core.CoreEventPayload_MusicSession(:final field0) => MusicSessionChanged(
        server,
        musicSessionFrom(field0),
      ),
      core.CoreEventPayload_MusicNowPlaying(:final field0) =>
        MusicNowPlayingChanged(
          server,
          channelId: field0.channelId,
          nowPlaying: field0.ended
              ? null
              : MusicNowPlaying(
                  channelId: field0.channelId,
                  title: field0.title,
                  artist: field0.artist,
                  artworkId: field0.artworkId,
                  playing: field0.playing,
                  sourceName: field0.sourceDisplayName,
                ),
        ),
      core.CoreEventPayload_CommandsUpdate(:final field0) => CommandsChanged(
        server,
        commandListFrom(field0),
      ),
      core.CoreEventPayload_EphemeralMessage(:final field0) =>
        EphemeralMessageReceived(
          server,
          EphemeralNote(
            channelId: field0.channelId,
            nonce: field0.nonce,
            content: field0.content,
            at: DateTime.now(),
          ),
        ),
      core.CoreEventPayload_MusicNotice(:final field0) => MusicNoticeReceived(
        server,
        MusicNotice(
          channelId: field0.channelId,
          text: field0.text,
          at: DateTime.fromMillisecondsSinceEpoch(field0.atMs),
        ),
      ),
      core.CoreEventPayload_UploadSettingsUpdate(:final field0) =>
        UploadSettingsEvent(server, uploadSettingsFrom(field0)),
      core.CoreEventPayload_StorageUpdate(:final field0) => StorageEvent(
        server,
        storageInfoFrom(field0),
      ),
      core.CoreEventPayload_UploadProgress(
        :final channelId,
        :final nonce,
        :final fileIndex,
        :final sentBytes,
        :final totalBytes,
        :final phase,
      ) =>
        UploadProgressEvent(
          server,
          channelId: channelId.toInt(),
          nonce: nonce,
          fileIndex: fileIndex,
          sentBytes: sentBytes.toInt(),
          totalBytes: totalBytes.toInt(),
          phase: uploadPhaseFrom(phase),
          error: switch (phase) {
            core.UploadPhase_Failed(:final message) => message,
            _ => null,
          },
        ),
      core.CoreEventPayload_DownloadProgress(
        :final attachmentId,
        :final receivedBytes,
        :final totalBytes,
        :final done,
        :final error,
      ) =>
        DownloadProgressEvent(
          server,
          attachmentId: attachmentId.toInt(),
          receivedBytes: receivedBytes.toInt(),
          totalBytes: totalBytes.toInt(),
          done: done,
          error: error,
        ),
    };
    if (event != null) _emit(event);
  }

  /// Updates who is in voice, announces the channels that changed, and
  /// tells the app when this device's own channel changed by itself.
  RepoEvent? _voiceState(String server, core.VoiceState state) {
    final voice = _voice[server] ??= {};
    final before = voice[state.userId]?.channelId;
    if (state.channelId == null) {
      voice.remove(state.userId);
    } else {
      voice[state.userId] = state;
    }
    for (final channel in {before, state.channelId}.nonNulls) {
      _emit(
        VoiceChanged(server, channel, [
          for (final other in voice.values)
            if (other.channelId == channel) participantFrom(other),
        ]),
      );
    }
    if (state.userId != _snapshots[server]?.self.id) return null;
    final own = _ownVoice;
    if (state.thisDevice) {
      final channel = state.channelId;
      if (channel == null && own?.server != server) return null;
      if (own?.server == server && own?.channel == channel) return null;
      _ownVoice = channel == null ? null : (server: server, channel: channel);
      return OwnVoiceChanged(server, channel);
    }
    if (own?.server != server) return null;
    _ownVoice = null;
    return OwnVoiceChanged(server, null);
  }

  // Servers -----------------------------------------------------------------

  @override
  Future<AddServerResult> addServer(
    String linkOrAddress, {
    String? claimToken,
  }) async {
    final result = addServerFrom(
      await _call(() => _core.serverAdd(linkOrAddress, claimToken)),
    );
    if (result is ServerAdded) _refreshServers();
    return result;
  }

  @override
  Future<void> trustFingerprint(String address, String fingerprint) async =>
      _now(() => _core.serverTrustFingerprint(address, fingerprint));

  @override
  Future<void> removeServer(String serverKey) async {
    await _call(() => _core.serverRemove(serverKey));
    _positions.forget(serverKey);
    _snapshots.remove(serverKey);
    _refreshServers();
  }

  @override
  Future<void> retryNow(String serverKey) =>
      _call(() => _core.serverRetryNow(serverKey));

  @override
  Future<void> updateServer(
    String serverKey, {
    String? name,
    String? description,
    bool? openJoin,
  }) => _call(
    () => _core.updateServer(
      serverKey,
      core.ServerChanges(
        name: name,
        description: description,
        openJoin: openJoin,
      ),
    ),
  );

  // Messages ----------------------------------------------------------------

  @override
  Future<List<Message>> fetchMessages(
    String serverKey,
    int channelId, {
    int? before,
    int limit = 50,
  }) async {
    final newestFirst = await _call(
      () => _core.fetchMessages(serverKey, channelId, before, limit),
    );
    return [for (final message in newestFirst.reversed) messageFrom(message)];
  }

  /// Phase 1 has no replies; the UI does not offer them ([capabilities]).
  @override
  Future<Message> sendMessage(
    String serverKey,
    int channelId,
    String content, {
    required String nonce,
    int? replyToId,
  }) async => messageFrom(
    await _call(() => _core.sendMessage(serverKey, channelId, content, nonce)),
  );

  @override
  Future<Message> editMessage(
    String serverKey,
    int channelId,
    int messageId,
    String content,
  ) async => messageFrom(
    await _call(() => _core.editMessage(serverKey, messageId, content)),
  );

  @override
  Future<void> deleteMessage(String serverKey, int channelId, int messageId) =>
      _call(() => _core.deleteMessage(serverKey, messageId));

  @override
  Future<void> startTyping(String serverKey, int channelId) =>
      _call(() => _core.startTyping(serverKey, channelId));

  @override
  void markRead(String serverKey, int channelId, int messageId) =>
      _positions.save(serverKey, channelId, messageId);

  static const _later = RepoException(
    RepoErrorKind.other,
    'This comes in a later version of Opencord.',
  );

  @override
  Future<void> toggleReaction(
    String serverKey,
    int channelId,
    int messageId,
    String emoji,
  ) => Future.error(_later);

  @override
  Future<void> setPinned(
    String serverKey,
    int channelId,
    int messageId, {
    required bool pinned,
  }) => Future.error(_later);

  @override
  Future<List<Message>> fetchPins(String serverKey, int channelId) async =>
      const [];

  // Channels ----------------------------------------------------------------

  /// A private channel stays visible to its maker: they are allowed in
  /// before @everyone is denied.
  @override
  Future<Channel> createChannel(
    String serverKey, {
    required ChannelKind kind,
    required String name,
    int? parentId,
    String? topic,
    bool private = false,
  }) async {
    final snapshot = _snapshots[serverKey];
    if (private && snapshot == null) {
      throw const RepoException(
        RepoErrorKind.notConnected,
        'Not connected to that server right now.',
      );
    }
    final created = await _call(
      () => _core.createChannel(
        serverKey,
        channelKindTo(kind),
        name,
        topic,
        parentId,
      ),
    );
    if (!private || snapshot == null) return channelFrom(created);
    try {
      await setOverwrite(
        serverKey,
        created.id,
        PermissionOverwrite(
          targetKind: OverwriteTargetKind.member,
          targetId: snapshot.self.id,
          allow: Permissions.viewChannel,
          deny: Permissions.none,
        ),
      );
      final hidden = await _call(
        () => _core.setChannelOverwrite(
          serverKey,
          created.id,
          overwriteTo(
            PermissionOverwrite(
              targetKind: OverwriteTargetKind.role,
              targetId: snapshot.info.everyoneRoleId,
              allow: Permissions.none,
              deny: Permissions.viewChannel,
            ),
          ),
        ),
      );
      return channelFrom(hidden);
    } on RepoException {
      // Never left public: it goes again, or the person is told to.
      try {
        await _call(() => _core.deleteChannel(serverKey, created.id));
      } on RepoException {
        throw RepoException(
          RepoErrorKind.other,
          '#$name was made but could not be made private, or removed. '
          'Delete it, or make it private in its settings.',
        );
      }
      rethrow;
    }
  }

  @override
  Future<Channel> updateChannel(
    String serverKey,
    int channelId, {
    String? name,
    String? topic,
    int? parentId,
    int? bitrate,
    int? userLimit,
    bool? textInVoice,
  }) async => channelFrom(
    await _call(
      () => _core.updateChannel(
        serverKey,
        channelId,
        core.ChannelChanges(
          name: name,
          topic: topic,
          parentId: parentId,
          bitrate: bitrate,
          userLimit: userLimit,
          textInVoice: textInVoice,
        ),
      ),
    ),
  );

  @override
  Future<void> deleteChannel(String serverKey, int channelId) =>
      _call(() => _core.deleteChannel(serverKey, channelId));

  @override
  Future<void> reorderChannels(String serverKey, Map<int, int> positions) =>
      _call(
        () => _core.reorderChannels(serverKey, [
          for (final MapEntry(key: id, value: position) in positions.entries)
            core.ChannelPosition(channelId: id, position: position),
        ]),
      );

  @override
  Future<void> setOverwrite(
    String serverKey,
    int channelId,
    PermissionOverwrite overwrite,
  ) => _call(
    () =>
        _core.setChannelOverwrite(serverKey, channelId, overwriteTo(overwrite)),
  );

  @override
  Future<void> deleteOverwrite(
    String serverKey,
    int channelId,
    OverwriteTargetKind kind,
    int targetId,
  ) => _call(
    () => _core.deleteChannelOverwrite(
      serverKey,
      channelId,
      targetTo(kind),
      targetId,
    ),
  );

  // Roles and members -------------------------------------------------------

  @override
  Future<Role> createRole(
    String serverKey, {
    required String name,
    Permissions permissions = Permissions.none,
    bool hoist = false,
    bool mentionable = false,
    int? maxUploadBytes,
  }) async => roleFrom(
    await _call(
      () => _core.createRole(
        serverKey,
        name: name,
        color: 0,
        permissions: permissions.bits,
        hoist: hoist,
        mentionable: mentionable,
        maxUploadBytes: maxUploadBytes,
      ),
    ),
  );

  @override
  Future<Role> updateRole(
    String serverKey,
    int roleId, {
    String? name,
    Permissions? permissions,
    bool? hoist,
    bool? mentionable,
    int? maxUploadBytes,
  }) async => roleFrom(
    await _call(
      () => _core.updateRole(
        serverKey,
        roleId,
        core.RoleChanges(
          name: name,
          permissions: permissions?.bits,
          hoist: hoist,
          mentionable: mentionable,
          maxUploadBytes: maxUploadBytes != null
              ? BigInt.from(maxUploadBytes)
              : null,
        ),
      ),
    ),
  );

  @override
  Future<void> deleteRole(String serverKey, int roleId) =>
      _call(() => _core.deleteRole(serverKey, roleId));

  @override
  Future<void> reorderRoles(String serverKey, List<int> roleIds) =>
      _call(() => _core.reorderRoles(serverKey, roleIds));

  @override
  Future<void> addMemberRole(String serverKey, int userId, int roleId) =>
      _call(() => _core.addMemberRole(serverKey, userId, roleId));

  @override
  Future<void> removeMemberRole(String serverKey, int userId, int roleId) =>
      _call(() => _core.removeMemberRole(serverKey, userId, roleId));

  @override
  Future<void> kickMember(String serverKey, int userId, {String? reason}) =>
      _call(() => _core.kickMember(serverKey, userId, reason));

  @override
  Future<void> banMember(String serverKey, int userId, {String? reason}) =>
      _call(() => _core.banMember(serverKey, userId, reason));

  @override
  Future<void> unbanMember(String serverKey, int userId) =>
      _call(() => _core.unbanMember(serverKey, userId));

  @override
  Future<List<Ban>> fetchBans(String serverKey) async => [
    for (final ban in await _call(() => _core.fetchBans(serverKey)))
      banFrom(ban),
  ];

  @override
  Future<void> updateNickname(String serverKey, int userId, String? nickname) =>
      _call(() => _core.updateNickname(serverKey, userId, nickname));

  // Presence ----------------------------------------------------------------

  Future<void> _sendPresence(String server) async {
    try {
      await _core.updatePresence(server, presenceTo(_presence));
    } on core.CoreError catch (error) {
      // Sent again after the next Ready.
      developer.log('Presence not sent to $server: $error', name: 'core');
    }
  }

  @override
  Future<void> updatePresence(SelfPresence presence) async {
    _presence = presence;
    await Future.wait([
      for (final server in _listServers()) _sendPresence(server.key),
    ]);
  }

  // Invites -----------------------------------------------------------------

  @override
  Future<Invite> createInvite(
    String serverKey, {
    Duration? expiresIn,
    int? maxUses,
    int? domainId,
  }) async => inviteFrom(
    await _call(
      () => _core.createInvite(
        serverKey,
        maxUses: maxUses,
        expiresInS: expiresIn?.inSeconds,
        domainId: domainId,
      ),
    ),
  );

  @override
  Future<List<Invite>> fetchInvites(String serverKey) async => [
    for (final invite in await _call(() => _core.fetchInvites(serverKey)))
      inviteFrom(invite),
  ];

  @override
  Future<void> revokeInvite(String serverKey, String code) =>
      _call(() => _core.revokeInvite(serverKey, code));

  @override
  Future<void> updateVoiceSettings(String serverKey, VoiceSettings settings) =>
      _call(
        () => _core.updateVoiceSettings(serverKey, voiceSettingsTo(settings)),
      );

  // Domains & network ---------------------------------------------------------

  @override
  Future<NetworkInfo> watchNetwork(
    String serverKey, {
    required bool watch,
  }) async =>
      networkInfoFrom(await _call(() => _core.watchNetwork(serverKey, watch)));

  @override
  Future<NetworkInfo> setPublicAddress(
    String serverKey, {
    required bool autoDetect,
    String overrideIpv4 = '',
    String overrideIpv6 = '',
  }) async => networkInfoFrom(
    await _call(
      () => _core.setPublicAddress(
        serverKey,
        autoDetect: autoDetect,
        overrideIpv4: overrideIpv4,
        overrideIpv6: overrideIpv6,
      ),
    ),
  );

  @override
  Future<NetworkInfo> detectPublicAddress(String serverKey) async =>
      networkInfoFrom(await _call(() => _core.detectPublicAddress(serverKey)));

  @override
  Future<NetworkInfo> updateNetworkSettings(
    String serverKey,
    NetworkSettingsChanges changes,
  ) async => networkInfoFrom(
    await _call(
      () => _core.updateNetworkSettings(serverKey, networkChangesTo(changes)),
    ),
  );

  @override
  Future<NetworkInfo> addDomain(
    String serverKey, {
    required String hostname,
    required DomainKind kind,
    int? publicPort,
  }) async => networkInfoFrom(
    await _call(
      () => _core.addDomain(
        serverKey,
        hostname: hostname,
        kind: domainKindTo(kind),
        publicPort: publicPort,
      ),
    ),
  );

  @override
  Future<NetworkInfo> updateDomain(
    String serverKey,
    int domainId,
    DomainUpdate update,
  ) async => networkInfoFrom(
    await _call(
      () => _core.updateDomain(serverKey, domainId, domainUpdateTo(update)),
    ),
  );

  @override
  Future<NetworkInfo> deleteDomain(String serverKey, int domainId) async =>
      networkInfoFrom(
        await _call(() => _core.deleteDomain(serverKey, domainId)),
      );

  @override
  Future<NetworkInfo> checkDomain(String serverKey, int domainId) async =>
      networkInfoFrom(
        await _call(() => _core.checkDomain(serverKey, domainId)),
      );

  @override
  Future<NetworkInfo> requestCertificate(
    String serverKey,
    int domainId, {
    String? email,
    bool agreeTerms = false,
  }) async => networkInfoFrom(
    await _call(
      () => _core.requestCertificate(serverKey, domainId, email, agreeTerms),
    ),
  );

  @override
  Future<NetworkInfo> uploadCertificate(
    String serverKey,
    int domainId, {
    required String chainPem,
    required String keyPem,
  }) async => networkInfoFrom(
    await _call(
      () => _core.uploadCertificate(serverKey, domainId, chainPem, keyPem),
    ),
  );

  @override
  Future<NetworkInfo> addFreeSubdomain(
    String serverKey, {
    required DnsProviderKind provider,
    required String name,
    String token = '',
    String? email,
    bool agreeTerms = false,
  }) async => networkInfoFrom(
    await _call(
      () => _core.addFreeSubdomain(
        serverKey,
        provider.wire,
        name,
        token,
        email,
        agreeTerms,
      ),
    ),
  );

  @override
  Future<NetworkInfo> startTunnel(
    String serverKey,
    TunnelKind kind, {
    String token = '',
    String hostname = '',
  }) async => networkInfoFrom(
    await _call(
      () => _core.startTunnel(
        serverKey,
        switch (kind) {
          TunnelKind.quick => core.TunnelKind.quick,
          TunnelKind.named => core.TunnelKind.named,
        },
        token,
        hostname,
      ),
    ),
  );

  @override
  Future<NetworkInfo> stopTunnel(String serverKey) async =>
      networkInfoFrom(await _call(() => _core.stopTunnel(serverKey)));

  @override
  Future<List<String>> tunnelLog(String serverKey) =>
      _call(() => _core.tunnelLog(serverKey));

  @override
  Future<NetworkInfo> installCloudflared(String serverKey) async =>
      networkInfoFrom(await _call(() => _core.installCloudflared(serverKey)));

  @override
  Future<LavalinkNodes> lavalinkNodes(
    String serverKey, {
    required bool watch,
  }) async => lavalinkNodesFrom(
    await _call(() => _core.lavalinkNodes(serverKey, watch: watch)),
  );

  @override
  Future<LavalinkNodes> createLavalinkNode(
    String serverKey,
    LavalinkNodeSettings settings,
    String password,
  ) async => lavalinkNodesFrom(
    await _call(
      () => _core.createLavalinkNode(
        serverKey,
        lavalinkSettingsToCore(settings),
        password,
      ),
    ),
  );

  @override
  Future<LavalinkNodes> updateLavalinkNode(
    String serverKey,
    int nodeId,
    LavalinkNodeChanges changes,
  ) async => lavalinkNodesFrom(
    await _call(
      () => _core.updateLavalinkNode(
        serverKey,
        nodeId,
        core.LavalinkNodeChanges(
          settings: switch (changes.settings) {
            final settings? => lavalinkSettingsToCore(settings),
            null => null,
          },
          password: changes.password,
          draining: changes.draining,
        ),
      ),
    ),
  );

  @override
  Future<LavalinkNodes> deleteLavalinkNode(
    String serverKey,
    int nodeId,
  ) async => lavalinkNodesFrom(
    await _call(() => _core.deleteLavalinkNode(serverKey, nodeId)),
  );

  @override
  Future<void> testLavalinkNode(String serverKey, int nodeId) =>
      _call(() => _core.testLavalinkNode(serverKey, nodeId));

  @override
  Future<LavalinkNodes> moveLavalinkPlayers(
    String serverKey,
    int fromNodeId, {
    int? toNodeId,
  }) async => lavalinkNodesFrom(
    await _call(
      () => _core.moveLavalinkPlayers(serverKey, fromNodeId, toNodeId),
    ),
  );

  @override
  Future<MusicSettings> musicSettings(String serverKey) async =>
      musicSettingsFrom(await _call(() => _core.musicSettings(serverKey)));

  @override
  Future<MusicSettings> updateMusicSettings(
    String serverKey,
    MusicSettings before,
    MusicSettings after,
  ) async => musicSettingsFrom(
    await _call(
      () => _core.updateMusicSettings(
        serverKey,
        musicSettingsChanges(before, after),
      ),
    ),
  );

  @override
  Future<List<MusicBlock>> musicBlocks(String serverKey) async => [
    for (final block in await _call(() => _core.musicBlocks(serverKey)))
      musicBlockFrom(block),
  ];

  @override
  Future<List<MusicBlock>> blockFromMusic(
    String serverKey,
    int userId, {
    required bool blocked,
    String reason = '',
  }) async => [
    for (final block in await _call(
      () => _core.blockFromMusic(serverKey, userId, blocked, reason),
    ))
      musicBlockFrom(block),
  ];

  @override
  Future<CommandOutcome> executeCommand(
    String serverKey,
    int channelId,
    String name,
    Map<String, String> options, {
    required String nonce,
  }) async {
    final outcome = await _call(
      () => _core.commandExecute(serverKey, channelId, name, [
        for (final MapEntry(:key, :value) in options.entries)
          core.CommandOptionValue(name: key, value: value),
      ], nonce),
    );
    return (messageId: outcome.messageId, ephemeral: outcome.ephemeral);
  }

  @override
  Future<List<CommandChoice>> autocompleteCommand(
    String serverKey,
    int channelId,
    String name,
    String option,
    String value,
  ) async => [
    for (final choice in await _call(
      () =>
          _core.commandAutocomplete(serverKey, channelId, name, option, value),
    ))
      commandChoiceFrom(choice),
  ];

  @override
  Future<CommandList> commandSettings(String serverKey) async =>
      commandListFrom(await _call(() => _core.commandSettings(serverKey)));

  @override
  Future<CommandList> updateCommandSettings(
    String serverKey, {
    String name = '',
    bool? enabled,
    List<int>? channelIds,
    bool? resultsPublic,
  }) async => commandListFrom(
    await _call(
      () => _core.updateCommandSettings(
        serverKey,
        core.CommandSettingsChanges(
          name: name,
          enabled: enabled,
          channelIds: channelIds == null
              ? null
              : frb_types.Int64List.fromList(channelIds),
          resultsPublic: resultsPublic,
        ),
      ),
    ),
  );

  @override
  Future<MusicSearchResults> musicSearch(
    String serverKey,
    int channelId,
    String query, {
    String source = '',
  }) async => musicSearchResultsFrom(
    await _call(() => _core.musicSearch(serverKey, channelId, query, source)),
  );

  @override
  Future<MusicSession> musicSubscribe(String serverKey, int channelId) async =>
      musicSessionFrom(
        await _call(() => _core.musicSubscribe(serverKey, channelId)),
      );

  @override
  Future<void> musicUnsubscribe(String serverKey, int channelId) =>
      _call(() => _core.musicUnsubscribe(serverKey, channelId));

  @override
  Future<MusicSession> musicEnqueue(
    String serverKey,
    int channelId,
    List<String> resultIds, {
    List<String> historyItemIds = const [],
    String? beforeItemId,
    MusicPlacement placement = MusicPlacement.end,
  }) async => musicSessionFrom(
    await _call(
      () => _core.musicEnqueue(
        serverKey,
        channelId,
        resultIds,
        historyItemIds,
        beforeItemId,
        musicPlacementTo(placement),
      ),
    ),
  );

  Future<MusicSession> _playback(
    String serverKey,
    int channelId,
    core.MusicAction action, {
    int? positionMs,
    String? expectedItemId,
  }) async => musicSessionFrom(
    await _call(
      () => _core.musicPlayback(
        serverKey,
        channelId,
        action,
        positionMs: positionMs,
        expectedItemId: expectedItemId,
      ),
    ),
  );

  @override
  Future<MusicSession> musicPause(
    String serverKey,
    int channelId, {
    required bool paused,
  }) => _playback(
    serverKey,
    channelId,
    paused ? core.MusicAction.pause : core.MusicAction.resume,
  );

  @override
  Future<MusicSession> musicSkip(
    String serverKey,
    int channelId, {
    required bool forward,
    String? expectedItemId,
  }) => _playback(
    serverKey,
    channelId,
    forward ? core.MusicAction.next : core.MusicAction.previous,
    expectedItemId: expectedItemId,
  );

  @override
  Future<MusicSession> musicVoteSkip(
    String serverKey,
    int channelId, {
    required String expectedItemId,
  }) async => musicSessionFrom(
    await _call(
      () => _core.musicVoteSkip(serverKey, channelId, expectedItemId),
    ),
  );

  @override
  Future<MusicSession> musicSeek(
    String serverKey,
    int channelId,
    Duration position, {
    String? expectedItemId,
  }) => _playback(
    serverKey,
    channelId,
    core.MusicAction.seek,
    positionMs: position.inMilliseconds,
    expectedItemId: expectedItemId,
  );

  @override
  Future<MusicSession> musicStop(String serverKey, int channelId) =>
      _playback(serverKey, channelId, core.MusicAction.stop);

  @override
  Future<MusicSession> musicSetMode(
    String serverKey,
    int channelId, {
    MusicRepeat? repeat,
    bool? shuffle,
  }) async => musicSessionFrom(
    await _call(
      () => _core.musicSetMode(
        serverKey,
        channelId,
        repeat: repeat == null ? null : musicRepeatTo(repeat),
        shuffle: shuffle,
      ),
    ),
  );

  @override
  Future<MusicSession> musicMove(
    String serverKey,
    int channelId,
    String itemId, {
    String? beforeItemId,
  }) async => musicSessionFrom(
    await _call(
      () => _core.musicQueueMove(serverKey, channelId, itemId, beforeItemId),
    ),
  );

  @override
  Future<MusicSession> musicRemove(
    String serverKey,
    int channelId,
    List<String> itemIds,
  ) async => musicSessionFrom(
    await _call(() => _core.musicQueueRemove(serverKey, channelId, itemIds)),
  );

  @override
  Future<MusicSession> musicClear(String serverKey, int channelId) async =>
      musicSessionFrom(
        await _call(() => _core.musicQueueClear(serverKey, channelId)),
      );

  @override
  void setMusicListening(MusicListening listening) => _now(
    () => _core.musicSetListening(
      MusicListening.volumeToGain(listening.volume),
      listening.muted,
      listening.duckWhileTalking,
    ),
  );

  @override
  Future<String?> musicArtwork(
    String serverKey,
    String artworkId,
    int size,
  ) async {
    if (artworkId.isEmpty) return null;
    try {
      return await _call(() => _core.musicArtwork(serverKey, artworkId, size));
    } on RepoException {
      return null;
    }
  }

  @override
  Future<NetworkInfo> setDnsProvider(
    String serverKey,
    DnsProviderKind kind,
    String token,
  ) async => networkInfoFrom(
    await _call(() => _core.setDnsProvider(serverKey, kind.wire, token)),
  );

  @override
  Future<List<DomainCheck>> domainChecks(
    String serverKey,
    int domainId,
  ) async => [
    for (final check in await _call(
      () => _core.domainChecks(serverKey, domainId),
    ))
      domainCheckFrom(check),
  ];

  // Voice ---------------------------------------------------------------------

  @override
  Future<void> joinVoice(String serverKey, int channelId) async {
    await _call(() => _core.voiceJoin(serverKey, channelId));
    _ownVoice = (server: serverKey, channel: channelId);
  }

  @override
  Future<void> leaveVoice() async {
    _ownVoice = null;
    // Leaving voice turns the camera off and ends the screen share.
    _cameraOff();
    _screenOff();
    await _call(_core.voiceLeave);
    for (final server in _speaking.keys.toList()) {
      _speaking.remove(server);
      _emit(SpeakingChanged(server, const {}));
    }
  }

  /// Who is speaking, per server, in the voice channel media reported.
  final _speaking = <String, ({int channel, Set<int> users})>{};

  /// The core sends who changed; the app keeps who is speaking.
  SpeakingChanged _speakingChanged(
    String server,
    int channel,
    List<core.SpeakingChange> changes,
  ) {
    final previous = _speaking[server];
    final users = {
      if (previous != null && previous.channel == channel) ...previous.users,
    };
    for (final change in changes) {
      if (change.speaking) {
        users.add(change.userId);
      } else {
        users.remove(change.userId);
      }
    }
    _speaking[server] = (channel: channel, users: users);
    return SpeakingChanged(server, users);
  }

  void _onMediaEvent(core.MediaEvent event) {
    if (event is core.MediaEvent_CameraStopped) {
      _cameraOff();
      _emit(CameraStopped(event.message));
      return;
    }
    if (event is core.MediaEvent_ScreenShareStopped) {
      _screenOff();
      _emit(ScreenShareStopped(event.message));
      return;
    }
    if (_mediaEvent(event) case final repoEvent?) _emit(repoEvent);
  }

  void _cameraOff() {
    if (!_cameraOn) return;
    _cameraOn = false;
    _emit(const OwnCameraChanged(null));
  }

  /// This device's screen share, while live.
  OwnScreenShare? _screen;

  void _screenOff() {
    if (_screen == null) return;
    _screen = null;
    _emit(const OwnScreenShareChanged(null));
  }

  core.ScreenShareRequest _screenRequest(
    ScreenShareQuality quality,
    bool audio,
  ) => core.ScreenShareRequest(
    resolution: resolutionTo(quality.resolution),
    fps: quality.fps,
    hasAudio: audio,
  );

  @override
  Future<void> startScreenShare(
    ScreenShareQuality quality, {
    bool audio = true,
    String? sourceId,
  }) async {
    final started = await _call(
      () => _core.screenShareStart(_screenRequest(quality, audio), sourceId),
    );
    _screen = OwnScreenShare(
      key: started.streamKey,
      source: streamSourceFrom(started.sourceKind),
      quality: quality,
      audio: audio,
      // Known when the system says which app the window is.
      audioFrom: started.audioApp,
      preview: VideoFeed(
        trackId: started.trackId,
        textureId: started.textureId,
        width: started.width,
        height: started.height,
      ),
    );
    _emit(OwnScreenShareChanged(_screen));
  }

  @override
  Future<void> updateScreenShare(
    ScreenShareQuality quality, {
    bool audio = true,
  }) async {
    await _call(() => _core.screenShareUpdate(_screenRequest(quality, audio)));
    if (_screen case final share?) {
      _screen = share.copyWith(quality: quality, audio: audio);
      _emit(OwnScreenShareChanged(_screen));
    }
  }

  @override
  Future<ScreenAudioSupport> screenAudioSupport() async {
    try {
      final support = await _core.screenAudioSupport();
      return (supported: support.supported, reason: support.reason);
    } on Object catch (error) {
      return (supported: false, reason: error.toString());
    }
  }

  @override
  Future<List<ScreenSource>> screenSources() async => [
    for (final source in await _call(_core.screenSources))
      (
        id: source.id,
        kind: streamSourceFrom(source.kind),
        title: source.title,
        app: source.app,
        thumbnail: switch (source.thumbnail) {
          final thumbnail? => (
            width: thumbnail.width,
            height: thumbnail.height,
            rgba: thumbnail.rgba,
          ),
          null => null,
        },
      ),
  ];

  @override
  Future<List<ScreenAudioApp>> screenAudioApps() async => [
    for (final app in await _call(_core.screenAudioApps))
      (name: app.name, icon: app.icon),
  ];

  @override
  void setScreenShareAudioFrom(String? app) {
    _now(() => _core.screenShareAudioFrom(app));
    if (_screen case final share?) {
      _screen = share.copyWith(audioFrom: () => app);
      _emit(OwnScreenShareChanged(_screen));
    }
  }

  @override
  void setStreamVolume(String serverKey, int userId, int volume) =>
      _now(() => _core.voiceSetStreamVolume(serverKey, userId, volume / 100));

  @override
  void setStreamMute(String serverKey, int userId, bool muted) =>
      _now(() => _core.voiceSetStreamMute(serverKey, userId, muted));

  @override
  Future<void> stopScreenShare() async {
    await _call(_core.screenShareStop);
    _screenOff();
  }

  @override
  Future<void> watchStream(String key) => _call(() => _core.streamWatch(key));

  @override
  late final List<DefaultSound> defaultSounds = [
    for (final sound in _core.soundboardDefaultSounds())
      defaultSoundFrom(sound),
  ];

  @override
  Future<void> playSound(
    String serverKey,
    int channelId,
    String soundRef, {
    ({String serverKey, Sound sound})? from,
  }) => _call(
    () => _core.soundboardPlay(
      serverKey,
      channelId,
      soundRef,
      externalServerKey: from?.serverKey,
      externalSound: switch (from) {
        (:final sound, serverKey: _) => soundTo(sound),
        null => null,
      },
    ),
  );

  @override
  Future<void> previewSound(String serverKey, String soundRef) =>
      _call(() => _core.soundboardPreview(serverKey, soundRef));

  @override
  void setSoundboardListening({required int volume, required bool muted}) =>
      _now(
        () => _core.soundboardSetListening(volume.clamp(0, 100) / 100, muted),
      );

  @override
  void playCue(VoiceCue cue) => _now(
    () => _core.playCue(switch (cue) {
      VoiceCue.join => core.VoiceCue.join,
      VoiceCue.leave => core.VoiceCue.leave,
      VoiceCue.userJoin => core.VoiceCue.userJoin,
      VoiceCue.userLeave => core.VoiceCue.userLeave,
      VoiceCue.mute => core.VoiceCue.mute,
      VoiceCue.unmute => core.VoiceCue.unmute,
      VoiceCue.deafen => core.VoiceCue.deafen,
      VoiceCue.undeafen => core.VoiceCue.undeafen,
      VoiceCue.streamStart => core.VoiceCue.streamStart,
      VoiceCue.streamStop => core.VoiceCue.streamStop,
    }, 1),
  );

  @override
  void setUserSoundboardMute(String serverKey, int userId, bool muted) =>
      _now(() => _core.soundboardMutePlayer(serverKey, userId, muted));

  @override
  Future<Sound> updateSound(
    String serverKey,
    int soundId, {
    String? name,
    String? emoji,
    int? volume,
  }) async => soundFrom(
    await _call(
      () => _core.soundboardUpdateSound(
        serverKey,
        soundId,
        core.SoundboardSoundChanges(name: name, emoji: emoji, volume: volume),
      ),
    ),
  );

  @override
  Future<void> deleteSound(String serverKey, int soundId) =>
      _call(() => _core.soundboardDeleteSound(serverKey, soundId));

  @override
  Future<SoundDraft> openSoundDraft(String path) async {
    final draft = await _call(() => _core.soundboardOpenDraft(path));
    return SoundDraft(
      id: draft.draftId.toInt(),
      duration: Duration(milliseconds: draft.durationMs),
      waveform: [for (final peak in draft.waveform) peak.toDouble()],
    );
  }

  @override
  Future<void> previewSoundDraft(SoundWindow window) =>
      _call(() => _core.soundboardPreviewDraft(selectionTo(window)));

  @override
  Future<Sound> uploadSound(
    String serverKey,
    SoundWindow window, {
    required String name,
    String? emoji,
  }) async => soundFrom(
    await _call(
      () => _core.soundboardUpload(serverKey, selectionTo(window), name, emoji),
    ),
  );

  @override
  void closeSoundDraft(int draftId) =>
      _now(() => _core.soundboardCloseDraft(draftId));

  @override
  Future<void> unwatchStream(String key) =>
      _call(() => _core.streamUnwatch(key));

  /// Media's news as the app's; screens wait for V6.
  RepoEvent? _mediaEvent(core.MediaEvent event) => switch (event) {
    core.MediaEvent_ConnectionState(
      :final serverKey,
      :final channelId,
      :final state,
    ) =>
      VoiceConnectionChanged(serverKey, channelId, voiceConnectionFrom(state)),
    core.MediaEvent_DevicesChanged(:final field0) => AudioDevicesChanged(
      audioDevicesFrom(field0),
    ),
    core.MediaEvent_DeviceFellBack(:final output, :final device) =>
      AudioDeviceFellBack(output: output, device: device),
    core.MediaEvent_DeviceFailed(:final output, :final message) =>
      AudioDeviceFailed(output: output, message: message),
    core.MediaEvent_Speaking(
      :final serverKey,
      :final channelId,
      :final changes,
    ) =>
      _speakingChanged(serverKey, channelId, changes),
    core.MediaEvent_InputLevel(:final dbfs) => InputLevelChanged(dbfs),
    core.MediaEvent_SpeakingWhileMuted() => const SpokeWhileMuted(),
    core.MediaEvent_NoiseSuppressionFellBack() =>
      const NoiseSuppressionFellBack(),
    core.MediaEvent_HotkeyPressed(:final action) => HotkeyPressed(
      hotkeyActionFrom(action),
    ),
    core.MediaEvent_VideoTrackAdded(
      kind: core.VideoTrackKind.camera,
      :final serverKey,
      :final channelId,
      :final userId,
      :final trackId,
      :final textureId,
      :final width,
      :final height,
    ) =>
      VideoTrackAdded(
        serverKey,
        channelId,
        userId,
        VideoFeed(
          trackId: trackId,
          textureId: textureId,
          width: width,
          height: height,
        ),
      ),
    core.MediaEvent_VideoTrackAdded(
      kind: core.VideoTrackKind.screen,
      :final serverKey,
      :final channelId,
      :final userId,
      :final trackId,
      :final textureId,
      :final width,
      :final height,
    ) =>
      VideoTrackAdded(
        serverKey,
        channelId,
        userId,
        VideoFeed(
          trackId: trackId,
          textureId: textureId,
          width: width,
          height: height,
        ),
        screen: true,
      ),
    core.MediaEvent_VideoTrackRemoved(
      :final serverKey,
      :final channelId,
      :final userId,
      :final trackId,
    ) =>
      VideoTrackRemoved(serverKey, channelId, userId, trackId),
    core.MediaEvent_CameraStopped() => null,
    core.MediaEvent_ScreenShareStopped() => null,
    core.MediaEvent_ConnectionStats(
      :final serverKey,
      :final channelId,
      :final stats,
    ) =>
      VoiceStatsChanged(
        serverKey,
        channelId,
        VoiceStats(
          pingMs: stats.pingMs,
          jitterMs: stats.jitterMs,
          lossInPercent: stats.lossInPercent,
          lossOutPercent: stats.lossOutPercent,
          bitrateIn: stats.bitrateIn.toInt(),
          bitrateOut: stats.bitrateOut.toInt(),
          voiceNode: stats.voiceNode,
          cameraEncoder: stats.cameraEncoder,
          screenEncoder: stats.screenEncoder,
        ),
      ),
    core.MediaEvent_VoiceRecordLevel(:final level) => VoiceRecordLevel(level),
    core.MediaEvent_VoiceRecordStopped(:final reason) => VoiceRecordStopped(
      voiceStopReasonFrom(reason),
    ),
    core.MediaEvent_PlaybackUpdate(
      :final item,
      :final positionMs,
      :final durationMs,
      :final playing,
      :final buffering,
    ) =>
      PlaybackUpdate(
        item: playbackItemFrom(item),
        positionMs: positionMs,
        durationMs: durationMs,
        playing: playing,
        buffering: buffering,
      ),
    core.MediaEvent_PlaybackEnded(:final item) => PlaybackEnded(
      playbackItemFrom(item),
    ),
    core.MediaEvent_PlaybackFailed(:final item, :final message) =>
      PlaybackFailed(item: playbackItemFrom(item), message: message),
  };

  @override
  Future<AudioDeviceList> audioDevices() async =>
      audioDevicesFrom(await _call(_core.audioDevices));

  @override
  void applyAudio(AudioConfig config) =>
      _now(() => _core.audioApplySettings(audioSettingsTo(config)));

  @override
  void setPushToTalk(bool held) => _now(() => _core.voiceSetPushToTalk(held));

  @override
  void setPrioritySpeaker(bool held) =>
      _now(() => _core.voiceSetPrioritySpeaker(held));

  @override
  void setLevelMeter(bool on) => _now(() => _core.audioSetLevelMeter(on));

  @override
  Future<void> setMicTest(bool on) => _call(() => _core.audioMicTest(on));

  @override
  Future<NoiseSuppression> recommendedNoiseSuppression() async =>
      noiseSuppressionFrom(await _call(_core.audioRecommendedNoiseSuppression));

  @override
  Future<HotkeySupport> setHotkeys(List<HotkeyBinding> bindings) async =>
      hotkeySupportFrom(
        await _call(
          () => _core.hotkeysSet([
            for (final (:action, :keys) in bindings)
              core.HotkeyBinding(
                action: hotkeyActionTo(action),
                accelerator: keys,
              ),
          ]),
        ),
      );

  @override
  void setUserVolume(String serverKey, int userId, int volume) =>
      _now(() => _core.voiceSetUserVolume(serverKey, userId, volume / 100));

  @override
  void setUserLocalMute(String serverKey, int userId, bool muted) =>
      _now(() => _core.voiceSetUserLocalMute(serverKey, userId, muted));

  @override
  Future<void> serverMuteMember(String serverKey, int userId, bool muted) =>
      _call(() => _core.voiceServerMute(serverKey, userId, muted));

  @override
  Future<void> serverDeafenMember(
    String serverKey,
    int userId,
    bool deafened,
  ) => _call(() => _core.voiceServerDeafen(serverKey, userId, deafened));

  @override
  Future<void> moveMember(String serverKey, int userId, int channelId) =>
      _call(() => _core.voiceMoveMember(serverKey, userId, channelId));

  @override
  Future<void> disconnectMember(String serverKey, int userId) =>
      _call(() => _core.voiceDisconnectMember(serverKey, userId));

  /// Screen sharing has its own calls; `screensharing` is ignored here.
  @override
  Future<void> setVoiceSelf({
    bool? muted,
    bool? deafened,
    bool? camera,
    bool? screensharing,
  }) async {
    if (muted != null) _now(() => _core.voiceSetSelfMute(muted));
    if (deafened != null) _now(() => _core.voiceSetSelfDeaf(deafened));
    if (camera == null || camera == _cameraOn) return;
    if (!camera) {
      _now(_core.cameraStop);
      _cameraOff();
      return;
    }
    final started = await _call(() => _core.cameraStart(_cameraDevice));
    _cameraOn = true;
    _cameraStarted(started);
  }

  void _cameraStarted(core.CameraStarted started) => _emit(
    OwnCameraChanged(
      VideoFeed(
        trackId: started.trackId,
        textureId: started.textureId,
        width: started.width,
        height: started.height,
        mirrored: true,
      ),
    ),
  );

  @override
  Future<void> chooseCamera(String? deviceId) async {
    // The camera that's on already: reopening it could find it still busy
    // (V4L2, Media Foundation) while the old capture closes.
    if (deviceId == _cameraDevice) return;
    _cameraDevice = deviceId;
    if (!_cameraOn) return;
    // Starting again replaces the camera that's on (a new track).
    try {
      _cameraStarted(await _call(() => _core.cameraStart(deviceId)));
    } on RepoException {
      _now(_core.cameraStop);
      _cameraOff();
      rethrow;
    }
  }

  @override
  Future<List<(String, String)>> cameras() async => [
    for (final camera in await _call(_core.cameraDevices))
      (camera.id, camera.name),
  ];

  @override
  void setVideoWants(List<VideoWant> wants) => _now(
    () => _core.videoSetWants([
      for (final want in wants)
        core.VideoWant(
          trackId: want.trackId,
          width: want.width,
          height: want.height,
        ),
    ]),
  );

  // Media uploads and storage ------------------------------------------------

  @override
  Future<Message> sendMessageWithFiles(
    String serverKey,
    int channelId,
    String content, {
    required String nonce,
    required List<StagedFile> files,
  }) async => messageFrom(
    await _call(
      () => _core.sendMessageWithFiles(
        serverKey: serverKey,
        channelId: channelId,
        content: content,
        nonce: nonce,
        files: [
          for (final f in files)
            core.OutgoingFile(filename: f.name, path: f.path, bytes: f.bytes),
        ],
      ),
    ),
  );

  @override
  void cancelSend(String serverKey, String nonce) =>
      _now(() => _core.cancelSend(serverKey: serverKey, nonce: nonce));

  @override
  Future<Uint8List> attachmentBytes(
    String serverKey,
    int attachmentId, {
    bool thumbnail = false,
  }) => _call(
    () => _core.attachmentBytes(
      serverKey: serverKey,
      attachmentId: attachmentId,
      variant: thumbnail
          ? core.AttachmentVariant.thumbnail
          : core.AttachmentVariant.original,
    ),
  );

  @override
  Future<void> downloadAttachment(
    String serverKey,
    int attachmentId,
    String path,
  ) => _call(
    () => _core.downloadAttachment(
      serverKey: serverKey,
      attachmentId: attachmentId,
      path: path,
    ),
  );

  @override
  void cancelDownload(String serverKey, int attachmentId) => _now(
    () =>
        _core.cancelDownload(serverKey: serverKey, attachmentId: attachmentId),
  );

  @override
  Future<Uint8List?> clipboardImage() => _call(() => _core.clipboardImage());

  @override
  Future<void> copyImage(Uint8List bytes) =>
      _call(() => _core.clipboardSetImage(bytes));

  @override
  Future<UploadSettings> updateUploadSettings(
    String serverKey, {
    List<String>? blockedExtensions,
    int? voiceMessageMaxSeconds,
  }) async => uploadSettingsFrom(
    await _call(
      () => _core.updateUploadSettings(
        serverKey: serverKey,
        blockedExtensions: blockedExtensions,
        voiceMessageMaxSeconds: voiceMessageMaxSeconds,
      ),
    ),
  );

  @override
  Future<StorageInfo> storage(String serverKey) async => storageInfoFrom(
    await _call(() => _core.storageGet(serverKey: serverKey)),
  );

  @override
  Future<StorageInfo> addStorage(
    String serverKey,
    String name,
    S3Config s3,
  ) async => storageInfoFrom(
    await _call(
      () => _core.storageAdd(
        serverKey: serverKey,
        name: name,
        s3: s3ConfigTo(s3),
      ),
    ),
  );

  @override
  Future<StorageInfo> updateStorage(
    String serverKey,
    int storageId, {
    String? name,
    S3Config? s3,
  }) async => storageInfoFrom(
    await _call(
      () => _core.storageUpdate(
        serverKey: serverKey,
        storageId: storageId,
        name: name,
        s3: s3 != null ? s3ConfigTo(s3) : null,
      ),
    ),
  );

  @override
  Future<StorageTestResult> testStorage(
    String serverKey, {
    int? storageId,
    S3Config? s3,
    int? keysFrom,
  }) async {
    final core.StorageTestTarget target;
    if (storageId != null) {
      target = core.StorageTestTarget.existing(storageId: storageId);
    } else if (s3 != null && keysFrom != null) {
      target = core.StorageTestTarget.edited(
        storageId: keysFrom,
        s3: s3ConfigTo(s3),
      );
    } else if (s3 != null) {
      target = core.StorageTestTarget.config(s3: s3ConfigTo(s3));
    } else {
      throw ArgumentError(
        'Either storageId or s3 must be provided to testStorage',
      );
    }
    return storageTestResultFrom(
      await _call(
        () => _core.storageTest(serverKey: serverKey, target: target),
      ),
    );
  }

  @override
  Future<StorageInfo> activateStorage(String serverKey, int storageId) async =>
      storageInfoFrom(
        await _call(
          () =>
              _core.storageActivate(serverKey: serverKey, storageId: storageId),
        ),
      );

  @override
  Future<StorageInfo> removeStorage(String serverKey, int storageId) async =>
      storageInfoFrom(
        await _call(
          () => _core.storageRemove(serverKey: serverKey, storageId: storageId),
        ),
      );

  @override
  Future<StorageInfo> startStorageMove(String serverKey) async =>
      storageInfoFrom(
        await _call(() => _core.storageMoveStart(serverKey: serverKey)),
      );

  @override
  Future<StorageInfo> stopStorageMove(String serverKey) async =>
      storageInfoFrom(
        await _call(() => _core.storageMoveStop(serverKey: serverKey)),
      );

  @override
  Future<int> mediaCacheSize() async {
    final size = await _call(() => _core.mediaCacheSize());
    return size.toInt();
  }

  @override
  Future<void> clearMediaCache() => _call(() => _core.mediaCacheClear());

  // Voice messages (Phase 4 A6) ---------------------------------------------

  @override
  void voiceRecordStart(int maxSeconds) =>
      _now(() => _core.voiceRecordStart(maxSeconds));

  @override
  Future<VoiceDraft> voiceRecordStop() async =>
      voiceDraftFrom(_now(() => _core.voiceRecordStop()));

  @override
  void voiceRecordCancel() => _now(() => _core.voiceRecordCancel());

  @override
  void voiceDraftDiscard(int draftId) =>
      _now(() => _core.voiceDraftDiscard(BigInt.from(draftId)));

  @override
  Future<Message> sendVoiceMessage(
    String serverKey,
    int channelId,
    String nonce,
    int draftId,
  ) async => messageFrom(
    await _call(
      () => _core.sendVoiceMessage(
        serverKey,
        channelId,
        nonce,
        BigInt.from(draftId),
      ),
    ),
  );

  @override
  Future<void> voicePlay(PlaybackItem item, int positionMs, double speed) =>
      _call(() => _core.voicePlay(playbackItemTo(item), positionMs, speed));

  @override
  void voicePause() => _now(_core.voicePause);

  @override
  void voiceResume() => _now(_core.voiceResume);

  @override
  void voiceSeek(int positionMs) => _now(() => _core.voiceSeek(positionMs));

  @override
  void voiceSetSpeed(double speed) => _now(() => _core.voiceSetSpeed(speed));

  @override
  void voiceStop() => _now(() => _core.voiceStop());

  @override
  Future<MediaHandle> mediaOpen(String serverKey, int attachmentId) => _call(
    () async => mediaHandleFrom(await _core.mediaOpen(serverKey, attachmentId)),
  );

  @override
  void mediaPlay() => _now(_core.mediaPlay);

  @override
  void mediaPause() => _now(_core.mediaPause);

  @override
  void mediaSeek(int positionMs) => _now(() => _core.mediaSeek(positionMs));

  @override
  void mediaSetVolume(double volume, bool muted) =>
      _now(() => _core.mediaSetVolume(volume, muted));

  @override
  void mediaClose() => _now(_core.mediaClose);

  @override
  bool mediaSupported() => _now(_core.mediaSupported);
}
