import 'dart:async';
import 'dart:math';

import 'package:clock/clock.dart';
import 'package:flutter/foundation.dart';

import 'package:opencord/core/mock/mock_music.dart';
import 'package:opencord/core/mock/mock_world.dart';
import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/stream.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/commands.dart';
import 'package:opencord/core/model/lavalink.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/model/misc.dart';
import 'package:opencord/core/model/music.dart';
import 'package:opencord/core/model/music_session.dart';
import 'package:opencord/core/model/network.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/model/sound.dart';
import 'package:opencord/core/model/storage.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/model/video.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/repository/repository.dart';

export 'package:opencord/core/mock/mock_world.dart' show mockMutedServers;

DateTime _systemClock() => clock.now();

const _replies = [
  'Makes sense to me.',
  'Good point, let me check.',
  'Agreed, ship it.',
  'Can you share a bit more detail?',
  'On it.',
  '👍',
  'Nice! That reads much better.',
  'I had the same thought this morning.',
];

/// The repository of plan §11: three servers with realistic content and a
/// little simulated life, plus debug triggers for edge states. Everything
/// lives in memory.
class MockRepository implements OpencordRepository {
  MockRepository({
    DateTime Function()? clock,
    Random? random,
    this.simulateLife = true,
    bool withIdentity = true,
    this.capabilities = RepoCapabilities.everything,
    RepoException? identityUnavailable,
  }) : _clock = clock ?? _systemClock,
       _random = random ?? Random(),
       _identity = withIdentity ? _seededIdentity : null {
    _identityUnavailable = identityUnavailable;
    _world = buildMockWorld(_clock());
  }

  static final _seededIdentity = LocalIdentity(
    displayName: selfName,
    fingerprint: selfFingerprint,
    publicKeyHex: fakeFingerprint('public key of $selfName'),
  );

  final DateTime Function() _clock;
  final Random _random;

  /// Typing, replies and voice activity; off in some tests.
  final bool simulateLife;

  late MockWorld _world;
  final _events = StreamController<RepoEvent>.broadcast();
  final _timers = <Timer>{};
  final _retryTimers = <String, Timer>{};
  final _trusted = <String>{};
  Timer? _speaking;
  Timer? _musicTick;
  final _music = <String, MockMusic>{};
  final _nowPlaying = <(String, int), MusicNowPlaying?>{};
  ({String server, int channel})? _voice;
  var _selfVoice = const VoiceParticipant(userId: 0);
  LocalIdentity? _identity;
  RepoException? _identityUnavailable;
  var _disposed = false;
  final _activeSends = <String, bool>{};
  final _activeDownloads = <int, bool>{};
  Uint8List? _clipboardImage;

  /// Everything by default; Phase 1's set to see the app as it will be
  /// on the Rust core.
  @override
  final RepoCapabilities capabilities;

  @override
  Stream<RepoEvent> get events => _events.stream;

  @override
  LocalIdentity? get identity => _identity;

  @override
  RepoException? get identityUnavailable => _identityUnavailable;

  /// The keyring is always back when asked again.
  @override
  Future<void> reloadIdentity() async {
    _identityUnavailable = null;
    _emit(const IdentityChanged());
  }

  @override
  List<ServerSummary> get servers => [
    for (final server in _world.servers) server.summary,
  ];

  void _emit(RepoEvent event) {
    if (!_disposed) _events.add(event);
  }

  void _later(Duration delay, void Function() action) {
    late final Timer timer;
    timer = Timer(delay, () {
      _timers.remove(timer);
      if (!_disposed) action();
    });
    _timers.add(timer);
  }

  Future<void> _latency([int base = 120]) =>
      Future<void>.delayed(Duration(milliseconds: base + _random.nextInt(180)));

  MockServer _server(String key) =>
      _world.servers.where((server) => server.key == key).firstOrNull ??
      (throw const RepoException(
        RepoErrorKind.notFound,
        'That server is not in the list.',
      ));

  @override
  void start() {
    for (final (index, server) in _world.servers.indexed) {
      _connect(server, delay: Duration(milliseconds: 150 + index * 120));
    }
  }

  @override
  void dispose() {
    _disposed = true;
    for (final timer in [..._timers, ..._retryTimers.values]) {
      timer.cancel();
    }
    _recordingTimer?.cancel();
    _playbackTimer?.cancel();
    _musicTick?.cancel();
    _speaking?.cancel();
    _levels?.cancel();
    _events.close();
  }

  void _connect(
    MockServer server, {
    Duration delay = const Duration(milliseconds: 300),
  }) {
    _retryTimers.remove(server.key)?.cancel();
    _emit(ConnectionChanged(server.key, const ConnectionStatus.connecting()));
    _later(delay, () {
      if (!_world.servers.contains(server)) return;
      _emit(Ready(server.key, server.snapshot()));
      if (server.unreachable) {
        _scheduleRetry(server, 1);
      } else {
        _emit(
          ConnectionChanged(server.key, const ConnectionStatus.connected()),
        );
      }
    });
  }

  void _scheduleRetry(MockServer server, int attempt) {
    final wait = Duration(seconds: min(30, 1 << min(attempt + 2, 5)));
    _emit(
      ConnectionChanged(
        server.key,
        ConnectionStatus.reconnecting(
          attempt: attempt,
          retryAt: _clock().add(wait),
        ),
      ),
    );
    _retryTimers.remove(server.key)?.cancel();
    _retryTimers[server.key] = Timer(wait, () => _retry(server, attempt + 1));
  }

  void _retry(MockServer server, int attempt) {
    if (_disposed || !_world.servers.contains(server)) return;
    if (server.unreachable) {
      _scheduleRetry(server, attempt);
    } else {
      _connect(server);
    }
  }

  @override
  Future<void> retryNow(String serverKey) async {
    final server = _server(serverKey);
    _emit(ConnectionChanged(serverKey, const ConnectionStatus.connecting()));
    await _latency(400);
    _retry(server, 1);
  }

  // Debug triggers (plan §11) ------------------------------------------

  /// The network as the server keeps it.
  NetworkInfo debugNetwork(String serverKey) => _server(serverKey).network;

  /// A notice from the server, as the owner would get one.
  void debugNotice(String serverKey, ServerNotice notice) =>
      _emit(ServerNoticeReceived(serverKey, notice));

  void debugDisconnect(String serverKey) {
    final server = _server(serverKey);
    _scheduleRetry(server, 1);
  }

  void debugReconnect(String serverKey) {
    final server = _server(serverKey)..unreachable = false;
    _connect(server);
  }

  void debugFingerprintMismatch(String serverKey) {
    final server = _server(serverKey);
    _retryTimers.remove(serverKey)?.cancel();
    final presented = fakeFingerprint('${server.key}-impostor');
    _emit(
      ConnectionChanged(
        serverKey,
        ConnectionStatus.failed(
          FailureReason.fingerprintChanged,
          "The server's certificate changed: expected ${server.fingerprint}, got $presented",
          expectedFingerprint: server.fingerprint,
          presentedFingerprint: presented,
        ),
      ),
    );
  }

  void debugEmptyServerList() {
    for (final timer in _retryTimers.values) {
      timer.cancel();
    }
    _retryTimers.clear();
    _leaveVoiceNow();
    _world.servers.clear();
    _emit(const ServersChanged());
  }

  void debugLongNames() {
    for (final server in _world.servers) {
      final channels = server.channels.values
          .where((c) => !c.isCategory)
          .take(2)
          .toList();
      for (final channel in channels) {
        final renamed = channel.copyWith(
          name:
              '${channel.name}-with-a-really-long-name-that-keeps-going-and-going-to-test-ellipsis',
        );
        server.channels[channel.id] = renamed;
        _emit(ChannelUpserted(server.key, renamed));
      }
    }
  }

  /// Adds `#scroll-test` with 10 000 messages to [serverKey].
  void debugStressChannel(String serverKey) {
    final server = _server(serverKey);
    final id = _world.ids.at(_clock());
    final channel = Channel(
      id: id,
      kind: ChannelKind.text,
      name: 'scroll-test',
      topic: '10 000 messages for scroll performance',
      position: server.channels.length,
    );
    server.channels[id] = channel;
    final authors = server.members.keys.toList();
    final start = _clock().subtract(const Duration(minutes: 10000));
    server.messages[id] = [
      for (var i = 0; i < 10000; i++)
        Message(
          id: _world.ids.at(start.add(Duration(minutes: i))),
          channelId: id,
          authorId: authors[i % authors.length],
          content: i % 37 == 0
              ? 'Message $i with a block:\n```\nfor i in 0..$i { tick(); }\n```'
              : i % 5 == 0
              ? 'Message $i — a longer line to vary the bubble heights, so that scrolling has to measure real text instead of a single repeated height.'
              : 'Message $i',
          createdAt: start.add(Duration(minutes: i)),
        ),
    ];
    server.readStates[id] = ReadState(lastReadId: server.messages[id]!.last.id);
    _emit(ChannelUpserted(serverKey, channel));
    _emit(
      PermissionsChanged(
        serverKey,
        server.selfPermissions,
        server.selfChannelPermissions,
      ),
    );
  }

  /// A message from someone other than the current user.
  void debugPostAs(
    String serverKey,
    int channelId,
    String content, {
    int? authorId,
  }) {
    final server = _server(serverKey);
    final history = _history(server, channelId);
    final author =
        authorId ?? server.members.keys.firstWhere((id) => id != server.selfId);
    final now = _clock();
    final message = Message(
      id: _world.ids.at(now),
      channelId: channelId,
      authorId: author,
      content: content,
      createdAt: now,
    );
    history.add(message);
    _emit(MessageCreated(serverKey, message));
  }

  DateTime? _rateLimitedUntil;

  /// Like the server, which limits every request, edits included.
  void _refuseWhileRateLimited() {
    final limit = _rateLimitedUntil;
    if (limit != null && _clock().isBefore(limit)) {
      throw RepoException(
        RepoErrorKind.rateLimited,
        'You are sending messages too quickly.',
        retryAfter: limit.difference(_clock()),
      );
    }
  }

  /// Sends fail as rate limited for [duration], to see the countdown
  /// (§4.6).
  void debugRateLimit(Duration duration) =>
      _rateLimitedUntil = _clock().add(duration);

  void debugTyping(String serverKey, int channelId, int userId) {
    _emit(TypingStarted(serverKey, channelId, userId));
  }

  /// Puts everything back the way it started.
  void debugReset() {
    for (final timer in [..._timers, ..._retryTimers.values]) {
      timer.cancel();
    }
    _timers.clear();
    _retryTimers.clear();
    _leaveVoiceNow();
    _world = buildMockWorld(_clock());
    _emit(const ServersChanged());
    start();
  }

  // Servers ---------------------------------------------------------------

  static final _invite = RegExp(
    r'^opencord://([^/:#\s]+):(\d+)/invite/([A-Za-z0-9]+)(?:#fp=([0-9a-fA-F]{64}))?$',
  );
  static final _address = RegExp(r'^([A-Za-z0-9.-]+):(\d{1,5})$');

  @override
  Future<AddServerResult> addServer(
    String linkOrAddress, {
    String? claimToken,
  }) async {
    await _latency(500);
    final input = linkOrAddress.trim();
    final invite = _invite.firstMatch(input);
    final address = _address.firstMatch(input);
    final match = invite ?? address;
    if (match == null) {
      throw const RepoException(
        RepoErrorKind.invalidArgument,
        'Enter an invite link (opencord://…) or a host:port address.',
      );
    }
    final key = '${match.group(1)!.toLowerCase()}:${match.group(2)}';
    final existing = _world.servers.where((s) => s.key == key).firstOrNull;
    if (existing != null) {
      _connect(existing);
      return ServerAdded(existing.summary);
    }
    if (match.group(1)!.startsWith('unreachable')) {
      throw const RepoException(
        RepoErrorKind.connection,
        'Could not connect: connection refused.',
      );
    }
    final linkFingerprint = invite?.group(4)?.toLowerCase();
    if (linkFingerprint == null && !_trusted.contains(key)) {
      return ServerNeedsTrust(address: key, fingerprint: fakeFingerprint(key));
    }
    final owner = claimToken != null && claimToken.trim().isNotEmpty;
    final server = _newServer(key, owner: owner);
    _world.servers.add(server);
    _emit(const ServersChanged());
    _connect(server);
    return ServerAdded(server.summary);
  }

  MockServer _newServer(String key, {required bool owner}) {
    final host = key.split(':').first.split('.').first;
    final name = host.isEmpty
        ? 'New server'
        : '${host[0].toUpperCase()}${host.substring(1)}';
    final base = _world.ids.at(_clock());
    final selfId = base + 1;
    final server = MockServer(
      key: key,
      info: ServerInfo(
        name: name,
        ownerId: owner ? selfId : null,
        everyoneRoleId: base + 2,
        openJoin: false,
      ),
      selfId: selfId,
      fingerprint: fakeFingerprint(key),
    );
    server.roles[base + 2] = Role(
      id: base + 2,
      name: '@everyone',
      position: 0,
      permissions: Permissions.defaultEveryone,
    );
    server.members[selfId] = Member(
      user: User(
        id: selfId,
        displayName: _identity?.displayName ?? selfName,
        fingerprint: _identity?.fingerprint ?? selfFingerprint,
      ),
      joinedAt: _clock(),
    );
    server.presences[selfId] = Presence.online;
    final general = base + 3;
    server.channels[general] = Channel(
      id: general,
      kind: ChannelKind.text,
      name: 'general',
    );
    server.messages[general] = [];
    server.channels[base + 4] = Channel(
      id: base + 4,
      kind: ChannelKind.voice,
      name: 'General',
      position: 1,
    );
    server.voice[base + 4] = [];
    return server;
  }

  @override
  Future<void> trustFingerprint(String address, String fingerprint) async {
    _trusted.add(address);
  }

  @override
  Future<void> removeServer(String serverKey) async {
    final server = _server(serverKey);
    _retryTimers.remove(serverKey)?.cancel();
    if (_voice?.server == serverKey) _leaveVoiceNow();
    _world.servers.remove(server);
    _emit(const ServersChanged());
  }

  // Messages ----------------------------------------------------------------

  List<Message> _history(MockServer server, int channelId) {
    if (!server.selfChannelPermissions.containsKey(channelId)) {
      throw const RepoException(
        RepoErrorKind.notFound,
        'That channel does not exist.',
      );
    }
    return server.messages[channelId] ??
        (throw const RepoException(
          RepoErrorKind.invalidArgument,
          'Only text channels have messages.',
        ));
  }

  void _require(MockServer server, Permissions needed, [int? channelId]) {
    final held = channelId == null
        ? server.selfPermissions
        : server.selfChannelPermissions[channelId] ?? Permissions.none;
    if (!held.has(needed)) {
      throw const RepoException(
        RepoErrorKind.forbidden,
        'You do not have permission to do that.',
      );
    }
  }

  /// History fails to load while set, to see a channel that could not
  /// load (§4.13).
  bool debugFailHistory = false;

  /// How long history takes to arrive, to see it loading.
  Duration debugHistoryDelay = Duration.zero;

  @override
  Future<List<Message>> fetchMessages(
    String serverKey,
    int channelId, {
    int? before,
    int limit = 50,
  }) async {
    if (debugHistoryDelay > Duration.zero) {
      await Future<void>.delayed(debugHistoryDelay);
    }
    if (debugFailHistory) {
      throw const RepoException(
        RepoErrorKind.notConnected,
        'Not connected to that server right now.',
      );
    }
    final history = _history(_server(serverKey), channelId);
    var end = history.length;
    if (before != null) {
      end = history.indexWhere((message) => message.id >= before);
      if (end == -1) end = history.length;
    }
    return history.sublist(max(0, end - limit), end);
  }

  @override
  Future<Message> sendMessage(
    String serverKey,
    int channelId,
    String content, {
    required String nonce,
    int? replyToId,
  }) async {
    final server = _server(serverKey);
    final history = _history(server, channelId);
    _require(server, Permissions.sendMessages, channelId);
    final text = content.trim();
    if (text.isEmpty) {
      throw const RepoException(
        RepoErrorKind.invalidArgument,
        'A message cannot be empty.',
      );
    }
    await _latency();
    _refuseWhileRateLimited();
    final now = _clock();
    final message = Message(
      id: _world.ids.at(now),
      channelId: channelId,
      authorId: server.selfId,
      content: text,
      createdAt: now,
      nonce: nonce,
      replyToId: replyToId,
    );
    history.add(message);
    _emit(MessageCreated(serverKey, message));
    _simulateReply(server, channelId);
    return message;
  }

  void _simulateReply(MockServer server, int channelId) {
    if (!simulateLife) return;
    final candidates = [
      for (final MapEntry(:key, :value) in server.presences.entries)
        if (key != server.selfId &&
            (value == Presence.online || value == Presence.idle))
          key,
    ];
    if (candidates.isEmpty) return;
    final responder = candidates[_random.nextInt(candidates.length)];
    final typingAt = Duration(milliseconds: 300 + _random.nextInt(900));
    final replyAt =
        typingAt + Duration(milliseconds: 1000 + _random.nextInt(2000));
    _later(
      typingAt,
      () => _emit(TypingStarted(server.key, channelId, responder)),
    );
    _later(replyAt, () {
      final history = server.messages[channelId];
      if (history == null) return;
      final now = _clock();
      final reply = Message(
        id: _world.ids.at(now),
        channelId: channelId,
        authorId: responder,
        content: _replies[_random.nextInt(_replies.length)],
        createdAt: now,
      );
      history.add(reply);
      _emit(MessageCreated(server.key, reply));
    });
  }

  Message _find(List<Message> history, int messageId) =>
      history.where((message) => message.id == messageId).firstOrNull ??
      (throw const RepoException(
        RepoErrorKind.notFound,
        'That message no longer exists.',
      ));

  void _replace(MockServer server, Message updated) {
    final history = server.messages[updated.channelId]!;
    history[history.indexWhere((message) => message.id == updated.id)] =
        updated;
    _emit(MessageUpdated(server.key, updated));
  }

  @override
  Future<Message> editMessage(
    String serverKey,
    int channelId,
    int messageId,
    String content,
  ) async {
    final server = _server(serverKey);
    final message = _find(_history(server, channelId), messageId);
    if (message.authorId != server.selfId) {
      throw const RepoException(
        RepoErrorKind.forbidden,
        'You can only edit your own messages.',
      );
    }
    await _latency();
    _refuseWhileRateLimited();
    final updated = message.copyWith(content: content.trim(), editedAt: _clock);
    _replace(server, updated);
    return updated;
  }

  @override
  Future<void> deleteMessage(
    String serverKey,
    int channelId,
    int messageId,
  ) async {
    final server = _server(serverKey);
    final history = _history(server, channelId);
    final message = _find(history, messageId);
    if (message.authorId != server.selfId) {
      _require(server, Permissions.manageMessages, channelId);
    }
    await _latency();
    history.remove(message);
    _emit(MessageDeleted(serverKey, channelId, messageId));
  }

  @override
  Future<void> startTyping(String serverKey, int channelId) async {
    typingSignals++;
  }

  /// How many typing signals the UI sent, for tests (§6 throttling).
  int typingSignals = 0;

  @override
  Future<void> toggleReaction(
    String serverKey,
    int channelId,
    int messageId,
    String emoji,
  ) async {
    final server = _server(serverKey);
    final message = _find(_history(server, channelId), messageId);
    final reactions = [...message.reactions];
    final index = reactions.indexWhere((reaction) => reaction.emoji == emoji);
    final self = server.selfId;
    if (index == -1) {
      reactions.add(Reaction(emoji: emoji, userIds: [self], me: true));
    } else {
      final current = reactions[index];
      final userIds = current.me
          ? [
              for (final id in current.userIds)
                if (id != self) id,
            ]
          : [...current.userIds, self];
      if (userIds.isEmpty) {
        reactions.removeAt(index);
      } else {
        reactions[index] = Reaction(
          emoji: emoji,
          userIds: userIds,
          me: !current.me,
        );
      }
    }
    _replace(server, message.copyWith(reactions: reactions));
  }

  @override
  Future<void> setPinned(
    String serverKey,
    int channelId,
    int messageId, {
    required bool pinned,
  }) async {
    final server = _server(serverKey);
    final history = _history(server, channelId);
    _require(server, Permissions.manageMessages, channelId);
    final message = _find(history, messageId);
    await _latency();
    _replace(server, message.copyWith(pinned: pinned));
    if (pinned) {
      final now = _clock();
      final notice = Message(
        id: _world.ids.at(now),
        channelId: channelId,
        authorId: server.selfId,
        content: '',
        createdAt: now,
        systemEvent: SystemEvent.messagePinned,
      );
      history.add(notice);
      _emit(MessageCreated(serverKey, notice));
    }
  }

  @override
  Future<List<Message>> fetchPins(String serverKey, int channelId) async => [
    for (final message in _history(_server(serverKey), channelId))
      if (message.pinned) message,
  ];

  // Channels ----------------------------------------------------------------

  /// Re-sends what the current user can see after a permission change, the
  /// way the server does.
  void _refreshSelf(
    MockServer server,
    Map<int, Permissions> before,
    Permissions beforeServer,
  ) {
    final after = server.selfChannelPermissions;
    for (final id in before.keys) {
      if (!after.containsKey(id)) _emit(ChannelDeleted(server.key, id));
    }
    for (final id in after.keys) {
      if (!before.containsKey(id)) {
        _emit(ChannelUpserted(server.key, server.channels[id]!));
      }
    }
    final changed =
        server.selfPermissions != beforeServer ||
        after.length != before.length ||
        after.entries.any((entry) => before[entry.key] != entry.value);
    if (changed) {
      _emit(PermissionsChanged(server.key, server.selfPermissions, after));
    }
  }

  @override
  Future<Channel> createChannel(
    String serverKey, {
    required ChannelKind kind,
    required String name,
    int? parentId,
    String? topic,
    bool private = false,
  }) async {
    final server = _server(serverKey);
    _require(server, Permissions.manageChannels);
    final trimmed = name.trim();
    if (trimmed.isEmpty) {
      throw const RepoException(
        RepoErrorKind.invalidArgument,
        'Give the channel a name.',
      );
    }
    await _latency();
    final before = server.selfChannelPermissions;
    final beforeServer = server.selfPermissions;
    final id = _world.ids.at(_clock());
    final channel = Channel(
      id: id,
      kind: kind,
      name: kind.isTextLike ? _kebab(trimmed) : trimmed,
      topic: topic,
      parentId: kind == ChannelKind.category ? null : parentId,
      position: server.channels.values
          .where((c) => c.parentId == parentId)
          .length,
      overwrites: [
        if (kind == ChannelKind.announcement)
          PermissionOverwrite(
            targetKind: OverwriteTargetKind.role,
            targetId: server.info.everyoneRoleId,
            allow: Permissions.none,
            deny: Permissions.sendMessages,
          ),
        if (private)
          PermissionOverwrite(
            targetKind: OverwriteTargetKind.role,
            targetId: server.info.everyoneRoleId,
            allow: Permissions.none,
            deny: Permissions.viewChannel,
          ),
        if (private)
          PermissionOverwrite(
            targetKind: OverwriteTargetKind.member,
            targetId: server.selfId,
            allow: Permissions.viewChannel,
            deny: Permissions.none,
          ),
      ],
    );
    server.channels[id] = channel;
    if (kind.isTextLike || kind == ChannelKind.voice) {
      server.messages[id] = [];
      server.readStates[id] = ReadState.empty;
    }
    if (kind == ChannelKind.voice) server.voice[id] = [];
    _emit(ChannelUpserted(serverKey, channel));
    _refreshSelf(server, before, beforeServer);
    return channel;
  }

  static String _kebab(String name) => name
      .toLowerCase()
      .replaceAll(RegExp(r'[^a-z0-9À-￿]+'), '-')
      .replaceAll(RegExp(r'^-+|-+$'), '');

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
  }) async {
    final server = _server(serverKey);
    _require(server, Permissions.manageChannels);
    final channel =
        server.channels[channelId] ??
        (throw const RepoException(
          RepoErrorKind.notFound,
          'That channel does not exist.',
        ));
    final maxBitrate = server.voiceSettings.maxVoiceBitrate;
    if (bitrate != null &&
        (bitrate < Channel.minBitrate || bitrate > maxBitrate)) {
      throw RepoException(
        RepoErrorKind.invalidArgument,
        'The bitrate must be from ${Channel.minBitrate ~/ 1000} to '
        '${maxBitrate ~/ 1000} kbps.',
      );
    }
    await _latency();
    final updated = channel.copyWith(
      name: name == null
          ? null
          : (channel.kind.isTextLike ? _kebab(name) : name.trim()),
      topic: topic == null ? null : () => topic.isEmpty ? null : topic,
      parentId: parentId == null ? null : () => parentId == 0 ? null : parentId,
      bitrate: bitrate,
      userLimit: userLimit,
      textInVoice: textInVoice,
    );
    server.channels[channelId] = updated;
    _emit(ChannelUpserted(serverKey, updated));
    return updated;
  }

  @override
  Future<void> deleteChannel(String serverKey, int channelId) async {
    final server = _server(serverKey);
    _require(server, Permissions.manageChannels);
    await _latency();
    final channel = server.channels.remove(channelId);
    if (channel == null) return;
    server.messages.remove(channelId);
    server.voice.remove(channelId);
    server.readStates.remove(channelId);
    _emit(ChannelDeleted(serverKey, channelId));
    if (channel.isCategory) {
      for (final child
          in server.channels.values
              .where((c) => c.parentId == channelId)
              .toList()) {
        final moved = child.copyWith(parentId: () => null);
        server.channels[child.id] = moved;
        _emit(ChannelUpserted(serverKey, moved));
      }
    }
  }

  @override
  Future<void> reorderChannels(
    String serverKey,
    Map<int, int> positions,
  ) async {
    final server = _server(serverKey);
    _require(server, Permissions.manageChannels);
    await _latency();
    for (final MapEntry(key: id, value: position) in positions.entries) {
      final channel = server.channels[id];
      if (channel == null || channel.position == position) continue;
      final moved = channel.copyWith(position: position);
      server.channels[id] = moved;
      _emit(ChannelUpserted(serverKey, moved));
    }
  }

  Future<void> _editOverwrites(
    String serverKey,
    int channelId,
    List<PermissionOverwrite> Function(List<PermissionOverwrite>) change,
  ) async {
    final server = _server(serverKey);
    _require(server, Permissions.manageRoles, channelId);
    final channel =
        server.channels[channelId] ??
        (throw const RepoException(
          RepoErrorKind.notFound,
          'That channel does not exist.',
        ));
    await _latency();
    final before = server.selfChannelPermissions;
    final beforeServer = server.selfPermissions;
    final updated = channel.copyWith(overwrites: change(channel.overwrites));
    server.channels[channelId] = updated;
    if (before.containsKey(channelId)) {
      _emit(ChannelUpserted(serverKey, updated));
    }
    _refreshSelf(server, before, beforeServer);
  }

  @override
  Future<void> setOverwrite(
    String serverKey,
    int channelId,
    PermissionOverwrite overwrite,
  ) => _editOverwrites(
    serverKey,
    channelId,
    (overwrites) => [
      for (final existing in overwrites)
        if (!existing.targets(overwrite.targetKind, overwrite.targetId))
          existing,
      overwrite,
    ],
  );

  @override
  Future<void> deleteOverwrite(
    String serverKey,
    int channelId,
    OverwriteTargetKind kind,
    int targetId,
  ) => _editOverwrites(
    serverKey,
    channelId,
    (overwrites) => [
      for (final existing in overwrites)
        if (!existing.targets(kind, targetId)) existing,
    ],
  );

  // Roles and members --------------------------------------------------------

  Future<T> _changeRoles<T>(
    MockServer server,
    Future<T> Function() change,
  ) async {
    final before = server.selfChannelPermissions;
    final beforeServer = server.selfPermissions;
    final result = await change();
    _refreshSelf(server, before, beforeServer);
    return result;
  }

  @override
  Future<Role> createRole(
    String serverKey, {
    required String name,
    Permissions permissions = Permissions.none,
    bool hoist = false,
    bool mentionable = false,
    int? maxUploadBytes,
  }) async {
    final server = _server(serverKey);
    _require(server, Permissions.manageRoles);
    if (!canGrant(server.selfPermissions, permissions)) {
      throw const RepoException(
        RepoErrorKind.forbidden,
        'You can only grant permissions you have.',
      );
    }
    await _latency();
    for (final role
        in server.roles.values.where((r) => r.position >= 1).toList()) {
      final moved = role.copyWith(position: role.position + 1);
      server.roles[role.id] = moved;
      _emit(RoleUpserted(serverKey, moved));
    }
    final role = Role(
      id: _world.ids.at(_clock()),
      name: name.trim(),
      position: 1,
      permissions: permissions,
      hoist: hoist,
      mentionable: mentionable,
      maxUploadBytes: maxUploadBytes == 0 ? null : maxUploadBytes,
    );
    server.roles[role.id] = role;
    _emit(RoleUpserted(serverKey, role));
    return role;
  }

  @override
  Future<Role> updateRole(
    String serverKey,
    int roleId, {
    String? name,
    Permissions? permissions,
    bool? hoist,
    bool? mentionable,
    int? maxUploadBytes,
  }) {
    final server = _server(serverKey);
    _require(server, Permissions.manageRoles);
    return _changeRoles(server, () async {
      final role =
          server.roles[roleId] ??
          (throw const RepoException(
            RepoErrorKind.notFound,
            'That role does not exist.',
          ));
      await _latency();
      final updated = role.copyWith(
        name: name,
        permissions: permissions,
        hoist: hoist,
        mentionable: mentionable,
        maxUploadBytes: maxUploadBytes == null
            ? null
            : () => (maxUploadBytes == 0 ? null : maxUploadBytes),
      );
      server.roles[roleId] = updated;
      _emit(RoleUpserted(serverKey, updated));
      return updated;
    });
  }

  @override
  Future<void> deleteRole(String serverKey, int roleId) {
    final server = _server(serverKey);
    _require(server, Permissions.manageRoles);
    if (roleId == server.info.everyoneRoleId) {
      throw const RepoException(
        RepoErrorKind.invalidArgument,
        '@everyone cannot be deleted.',
      );
    }
    return _changeRoles(server, () async {
      await _latency();
      server.roles.remove(roleId);
      for (final member
          in server.members.values
              .where((m) => m.roleIds.contains(roleId))
              .toList()) {
        server.members[member.id] = member.copyWith(
          roleIds: [
            for (final id in member.roleIds)
              if (id != roleId) id,
          ],
        );
      }
      _emit(RoleDeleted(serverKey, roleId));
    });
  }

  @override
  Future<void> reorderRoles(String serverKey, List<int> roleIds) {
    final server = _server(serverKey);
    _require(server, Permissions.manageRoles);
    return _changeRoles(server, () async {
      await _latency();
      final positions = [for (final id in roleIds) server.roles[id]!.position]
        ..sort();
      for (final (index, id) in roleIds.indexed) {
        final role = server.roles[id]!;
        if (role.position == positions[index]) continue;
        final moved = role.copyWith(position: positions[index]);
        server.roles[id] = moved;
        _emit(RoleUpserted(serverKey, moved));
      }
    });
  }

  Future<void> _changeMemberRoles(
    String serverKey,
    int userId,
    List<int> Function(List<int>) change,
  ) {
    final server = _server(serverKey);
    _require(server, Permissions.manageRoles);
    return _changeRoles(server, () async {
      final member =
          server.members[userId] ??
          (throw const RepoException(
            RepoErrorKind.notFound,
            'That member left.',
          ));
      await _latency();
      final updated = member.copyWith(roleIds: change(member.roleIds));
      server.members[userId] = updated;
      _emit(MemberUpserted(serverKey, updated));
    });
  }

  @override
  Future<void> addMemberRole(String serverKey, int userId, int roleId) =>
      _changeMemberRoles(
        serverKey,
        userId,
        (roles) => roles.contains(roleId) ? roles : [...roles, roleId],
      );

  @override
  Future<void> removeMemberRole(String serverKey, int userId, int roleId) =>
      _changeMemberRoles(
        serverKey,
        userId,
        (roles) => [
          for (final id in roles)
            if (id != roleId) id,
        ],
      );

  Future<void> _removeMember(MockServer server, int userId) async {
    await _latency();
    if (server.members.remove(userId) == null) return;
    server.presences.remove(userId);
    server.activities.remove(userId);
    _emit(MemberLeft(server.key, userId));
  }

  @override
  Future<void> kickMember(String serverKey, int userId, {String? reason}) {
    final server = _server(serverKey);
    _require(server, Permissions.kickMembers);
    return _removeMember(server, userId);
  }

  @override
  Future<void> banMember(String serverKey, int userId, {String? reason}) {
    final server = _server(serverKey);
    _require(server, Permissions.banMembers);
    final member = server.members[userId];
    if (member != null) {
      server.bans.add(
        Ban(
          user: member.user,
          reason: reason,
          bannedBy: server.selfId,
          createdAt: _clock(),
        ),
      );
    }
    return _removeMember(server, userId);
  }

  @override
  Future<void> unbanMember(String serverKey, int userId) async {
    final server = _server(serverKey);
    _require(server, Permissions.banMembers);
    await _latency();
    server.bans.removeWhere((ban) => ban.user.id == userId);
  }

  @override
  Future<List<Ban>> fetchBans(String serverKey) async {
    final server = _server(serverKey);
    _require(server, Permissions.banMembers);
    await _latency();
    return List.of(server.bans);
  }

  @override
  Future<void> updateNickname(
    String serverKey,
    int userId,
    String? nickname,
  ) async {
    final server = _server(serverKey);
    _require(
      server,
      userId == server.selfId
          ? Permissions.changeNickname
          : Permissions.manageNicknames,
    );
    final member =
        server.members[userId] ??
        (throw const RepoException(
          RepoErrorKind.notFound,
          'That member left.',
        ));
    await _latency();
    final trimmed = nickname?.trim();
    final updated = member.copyWith(
      nickname: () => trimmed == null || trimmed.isEmpty ? null : trimmed,
    );
    server.members[userId] = updated;
    _emit(MemberUpserted(serverKey, updated));
  }

  static const _backupPrefix = 'opencord-identity-v1:';

  @override
  Future<String> exportIdentityBackup() async {
    await _latency();
    return '$_backupPrefix${fakeFingerprint('secret of ${_identity?.fingerprint}')}';
  }

  /// The identity in a backup, or an error when it is not one.
  LocalIdentity _fromBackup(String backup, String displayName) {
    final text = backup.trim();
    if (!text.startsWith(_backupPrefix) ||
        !RegExp(
          r'^[0-9a-f]{64}$',
        ).hasMatch(text.substring(_backupPrefix.length))) {
      throw const RepoException(
        RepoErrorKind.invalidArgument,
        'That is not an Opencord identity backup.',
      );
    }
    final secret = text.substring(_backupPrefix.length);
    final code = secret.substring(0, 16).toUpperCase();
    return LocalIdentity(
      displayName: displayName,
      fingerprint: [
        for (var i = 0; i < 16; i += 4) code.substring(i, i + 4),
      ].join('-'),
      publicKeyHex: fakeFingerprint('public key of $secret'),
    );
  }

  @override
  Future<void> importIdentityBackup(String backup) async {
    await _latency();
    _identity = _fromBackup(backup, _identity?.displayName ?? selfName);
    _emit(const IdentityChanged());
  }

  @override
  Future<NewIdentity> generateIdentity() async {
    final secret = fakeFingerprint('new identity ${_random.nextInt(1 << 30)}');
    final backup = '$_backupPrefix$secret';
    final identity = _fromBackup(backup, '');
    return NewIdentity(
      fingerprint: identity.fingerprint,
      publicKeyHex: identity.publicKeyHex,
      backup: backup,
    );
  }

  @override
  Future<void> adoptIdentity(
    String backup, {
    required String displayName,
  }) async {
    final name = displayName.trim();
    if (name.isEmpty) {
      throw const RepoException(
        RepoErrorKind.invalidArgument,
        'Choose a display name.',
      );
    }
    await _latency();
    _identity = _fromBackup(backup, name);
    _emit(const IdentityChanged());
  }

  @override
  void markRead(String serverKey, int channelId, int messageId) {
    final server = _world.servers
        .where((server) => server.key == serverKey)
        .firstOrNull;
    if (server == null) return;
    final known = server.readStates[channelId]?.lastReadId ?? 0;
    if (messageId > known) {
      server.readStates[channelId] = ReadState(lastReadId: messageId);
    }
  }

  @override
  Future<void> updateDisplayName(String displayName) async {
    await _latency();
    _identity = LocalIdentity(
      displayName: displayName.trim(),
      fingerprint: _identity?.fingerprint ?? selfFingerprint,
      publicKeyHex: _identity?.publicKeyHex ?? '',
    );
    _emit(const IdentityChanged());
    for (final server in _world.servers) {
      final updated = server.self.copyWith(
        user: server.self.user.copyWith(displayName: displayName.trim()),
      );
      server.members[server.selfId] = updated;
      _emit(MemberUpserted(server.key, updated));
    }
  }

  @override
  Future<void> updatePresence(SelfPresence presence) async {
    for (final server in _world.servers) {
      server.presences[server.selfId] = presence.shown;
      _emit(
        PresenceChanged(
          server.key,
          server.selfId,
          presence.shown,
          activity: server.activities[server.selfId],
        ),
      );
    }
  }

  // Invites and server ----------------------------------------------------------

  @override
  Future<Invite> createInvite(
    String serverKey, {
    Duration? expiresIn,
    int? maxUses,
    int? domainId,
  }) async {
    final server = _server(serverKey);
    _require(server, Permissions.createInvite);
    await _latency();
    final domain = domainId == null
        ? server.addresses.primary
        : server.addresses.domains
              .where((address) => address.domainId == domainId)
              .firstOrNull;
    if (domainId != null && domain == null) {
      throw const RepoException(
        RepoErrorKind.conflict,
        'That domain is not active.',
      );
    }
    const alphabet = 'abcdefghjkmnpqrstuvwxyz23456789';
    final code = List.generate(
      8,
      (_) => alphabet[_random.nextInt(alphabet.length)],
    ).join();
    final now = _clock();
    final invite = Invite(
      code: code,
      link: switch (domain) {
        final domain? when domain.trustedCertificate =>
          'opencord://${domain.display}/invite/$code',
        final domain? =>
          'opencord://${domain.display}/invite/$code#fp=${server.fingerprint}',
        null =>
          'opencord://${server.key}/invite/$code#fp=${server.fingerprint}',
      },
      webLink: switch (domain) {
        final domain? when domain.port == 443 =>
          'https://${domain.host}/invite/$code',
        final domain? => 'https://${domain.display}/invite/$code',
        null => null,
      },
      domainId: domain?.domainId,
      createdBy: server.selfId,
      createdAt: now,
      maxUses: maxUses,
      expiresAt: expiresIn == null ? null : now.add(expiresIn),
    );
    server.invites.add(invite);
    return invite;
  }

  @override
  Future<List<Invite>> fetchInvites(String serverKey) async {
    final server = _server(serverKey);
    _require(server, Permissions.manageServer);
    await _latency();
    return List.of(server.invites);
  }

  @override
  Future<void> revokeInvite(String serverKey, String code) async {
    final server = _server(serverKey);
    await _latency();
    server.invites.removeWhere((invite) => invite.code == code);
  }

  @override
  Future<void> updateServer(
    String serverKey, {
    String? name,
    String? description,
    bool? openJoin,
  }) async {
    final server = _server(serverKey);
    _require(server, Permissions.manageServer);
    await _latency();
    server.info = server.info.copyWith(
      name: name?.trim(),
      description: description,
      openJoin: openJoin,
    );
    _emit(ServerInfoChanged(serverKey, server.info));
    _emit(const ServersChanged());
  }

  @override
  Future<void> updateVoiceSettings(
    String serverKey,
    VoiceSettings settings,
  ) async {
    final server = _server(serverKey);
    _require(server, Permissions.manageServer);
    await _latency();
    server.voiceSettings = settings;
    _emit(VoiceSettingsChanged(serverKey, settings));
  }

  // Lavalink nodes ----------------------------------------------------------------

  LavalinkNodes _lavalink(MockServer server, LavalinkNodes nodes) {
    server.lavalink = nodes;
    return nodes;
  }

  /// Tells whoever watches, and keeps it.
  void _lavalinkNode(MockServer server, LavalinkNode node) {
    _lavalink(server, server.lavalink.withNode(node));
    _emit(
      LavalinkNodeChanged(
        server.key,
        nodeId: node.id,
        node: node,
        channelsPlaying: server.lavalink.channelsPlaying,
      ),
    );
  }

  void _lavalinkEvent(MockServer server, LavalinkEventKind kind, String text) {
    final nodeEvent = LavalinkNodeEvent(
      nodeId: 0,
      kind: kind,
      text: text,
      at: _clock(),
    );
    _lavalink(server, server.lavalink.withEvent(nodeEvent));
    _emit(LavalinkNodeEventReceived(server.key, nodeEvent));
  }

  /// Why a mock node cannot be reached: "unreachable" in the host, or the
  /// password "wrong".
  String? _lavalinkProblem(LavalinkNodeSettings settings, String password) {
    if (settings.host.contains('unreachable')) {
      return 'Nothing is listening on ${settings.address}: check the port, '
          'and that Lavalink is running';
    }
    if (password == 'wrong') return 'Lavalink refused the password';
    return null;
  }

  /// Connects a mock node a moment later.
  void _connectLavalink(MockServer server, int id) {
    _later(const Duration(milliseconds: 600), () {
      final node = server.lavalink.node(id);
      if (node == null || !node.settings.enabled) return;
      final problem = _lavalinkProblem(
        node.settings,
        server.lavalinkPasswords[id] ?? '',
      );
      final name = node.settings.name;
      if (problem != null) {
        _lavalinkNode(
          server,
          LavalinkNode(
            id: id,
            settings: node.settings,
            draining: node.draining,
            health: LavalinkHealth.unavailable,
            reason: problem,
            lastError: problem,
          ),
        );
        _lavalinkEvent(
          server,
          LavalinkEventKind.disconnected,
          '$name is unavailable: $problem',
        );
        return;
      }
      _lavalinkNode(
        server,
        LavalinkNode(
          id: id,
          settings: node.settings,
          draining: node.draining,
          health: node.draining
              ? LavalinkHealth.draining
              : LavalinkHealth.healthy,
          version: '4.2.2',
          sources: const ['youtube', 'soundcloud', 'bandcamp'],
          plugins: const ['youtube-plugin 1.13.3'],
          cpuPercent: 6 + _random.nextInt(10),
          pingMs: 18 + _random.nextInt(30),
          connectedSince: _clock(),
        ),
      );
      _lavalinkEvent(
        server,
        LavalinkEventKind.connected,
        '$name connected (Lavalink 4.2.2)',
      );
    });
  }

  @override
  Future<LavalinkNodes> lavalinkNodes(
    String serverKey, {
    required bool watch,
  }) async {
    final server = _server(serverKey);
    _require(server, Permissions.manageServer);
    if (watch) await _latency(60);
    return server.lavalink;
  }

  @override
  Future<LavalinkNodes> createLavalinkNode(
    String serverKey,
    LavalinkNodeSettings settings,
    String password,
  ) async {
    final server = _server(serverKey);
    _require(server, Permissions.manageServer);
    await _latency();
    _checkLavalink(settings);
    if (password.isEmpty) {
      throw const RepoException(
        RepoErrorKind.invalidArgument,
        "the password is Lavalink's server.password, up to 512 characters",
      );
    }
    if (server.lavalink.nodes.length >= maxLavalinkNodes) {
      throw const RepoException(
        RepoErrorKind.invalidArgument,
        'a server has at most 20 Lavalink nodes',
      );
    }
    final id = _world.ids.at(_clock());
    server.lavalinkPasswords[id] = password;
    final node = LavalinkNode(
      id: id,
      settings: settings,
      health: settings.enabled
          ? LavalinkHealth.connecting
          : LavalinkHealth.disabled,
    );
    _lavalinkNode(server, node);
    if (settings.enabled) _connectLavalink(server, id);
    return server.lavalink;
  }

  void _checkLavalink(LavalinkNodeSettings settings) {
    final host = settings.host;
    if (settings.name.trim().isEmpty) {
      throw const RepoException(
        RepoErrorKind.invalidArgument,
        'a node needs a name of up to 64 characters',
      );
    }
    if (host.isEmpty || host.contains('/') || host.contains(' ')) {
      throw const RepoException(
        RepoErrorKind.invalidArgument,
        'the host is a name like lavalink.example.com or an IP address, '
        'without http:// or a path',
      );
    }
    if (settings.port < 1 || settings.port > 65535) {
      throw const RepoException(
        RepoErrorKind.invalidArgument,
        'the port is a number from 1 to 65535',
      );
    }
  }

  @override
  Future<LavalinkNodes> updateLavalinkNode(
    String serverKey,
    int nodeId,
    LavalinkNodeChanges changes,
  ) async {
    final server = _server(serverKey);
    _require(server, Permissions.manageServer);
    await _latency();
    final old = server.lavalink.node(nodeId);
    if (old == null) {
      throw const RepoException(RepoErrorKind.notFound, 'Unknown node.');
    }
    final settings = changes.settings ?? old.settings;
    _checkLavalink(settings);
    if (changes.password case final password?) {
      server.lavalinkPasswords[nodeId] = password;
    }
    final draining = changes.draining ?? old.draining;
    final reconnect =
        changes.password != null ||
        settings.host != old.settings.host ||
        settings.port != old.settings.port ||
        settings.secure != old.settings.secure ||
        (settings.enabled && !old.settings.enabled);
    final health = switch ((settings.enabled, draining, reconnect)) {
      (false, _, _) => LavalinkHealth.disabled,
      (true, _, true) => LavalinkHealth.connecting,
      (true, true, false) when old.health.isUp => LavalinkHealth.draining,
      (true, false, false) when old.health == LavalinkHealth.draining =>
        LavalinkHealth.healthy,
      _ => old.health,
    };
    _lavalinkNode(
      server,
      LavalinkNode(
        id: nodeId,
        settings: settings,
        draining: draining,
        health: health,
        reason: reconnect ? '' : old.reason,
        version: old.version,
        sources: old.sources,
        plugins: old.plugins,
        channels: old.channels,
        cpuPercent: old.cpuPercent,
        pingMs: old.pingMs,
        lastError: old.lastError,
        connectedSince: health.isUp ? old.connectedSince : null,
      ),
    );
    if (reconnect && settings.enabled) _connectLavalink(server, nodeId);
    return server.lavalink;
  }

  @override
  Future<LavalinkNodes> deleteLavalinkNode(String serverKey, int nodeId) async {
    final server = _server(serverKey);
    _require(server, Permissions.manageServer);
    await _latency();
    server.lavalinkPasswords.remove(nodeId);
    _lavalink(server, server.lavalink.without(nodeId));
    _emit(
      LavalinkNodeChanged(
        server.key,
        nodeId: nodeId,
        node: null,
        channelsPlaying: server.lavalink.channelsPlaying,
      ),
    );
    return server.lavalink;
  }

  @override
  Future<void> testLavalinkNode(String serverKey, int nodeId) async {
    final server = _server(serverKey);
    _require(server, Permissions.manageServer);
    final node = server.lavalink.node(nodeId);
    if (node == null) {
      throw const RepoException(RepoErrorKind.notFound, 'Unknown node.');
    }
    await _latency(40);
    final problem = _lavalinkProblem(
      node.settings,
      server.lavalinkPasswords[nodeId] ?? '',
    );
    final bridge = server.lavalink.bridge;
    final outcomes = <(LavalinkTestStepState, String)>[
      if (problem != null)
        (LavalinkTestStepState.failed, problem)
      else ...[
        (
          LavalinkTestStepState.passed,
          'Lavalink 4.2.2 · sources: youtube, soundcloud, bandcamp',
        ),
        (LavalinkTestStepState.passed, 'Lavalink is ready for players'),
        (
          LavalinkTestStepState.passed,
          'Lavalink signed in at ${bridge.url} and reached the audio port '
              'from ${node.settings.host}',
        ),
      ],
    ];
    var steps = LavalinkTestProgress.started(nodeId).steps;
    void send({bool done = false}) => _emit(
      LavalinkTestProgressed(
        server.key,
        LavalinkTestProgress(nodeId: nodeId, steps: steps, done: done),
      ),
    );
    for (final (index, (state, detail)) in outcomes.indexed) {
      _later(Duration(milliseconds: 300 + 500 * index), () {
        steps = [
          for (final (at, step) in steps.indexed)
            at == index
                ? LavalinkTestStep(
                    kind: step.kind,
                    state: state,
                    detail: detail,
                  )
                : at > index && state == LavalinkTestStepState.failed
                ? LavalinkTestStep(
                    kind: step.kind,
                    state: LavalinkTestStepState.skipped,
                  )
                : at == index + 1
                ? LavalinkTestStep(
                    kind: step.kind,
                    state: LavalinkTestStepState.running,
                  )
                : step,
        ];
        send(done: index == outcomes.length - 1);
      });
    }
  }

  @override
  Future<LavalinkNodes> moveLavalinkPlayers(
    String serverKey,
    int fromNodeId, {
    int? toNodeId,
  }) async {
    final server = _server(serverKey);
    _require(server, Permissions.manageServer);
    await _latency();
    final from = server.lavalink.node(fromNodeId);
    if (from == null) {
      throw const RepoException(RepoErrorKind.notFound, 'Unknown node.');
    }
    final targets = server.lavalink.nodes
        .where(
          (node) =>
              node.id != fromNodeId &&
              node.health == LavalinkHealth.healthy &&
              (toNodeId == null || node.id == toNodeId),
        )
        .toList();
    if (from.channels == 0 || targets.isEmpty) return server.lavalink;
    final to = targets.first;
    final count = from.channels;
    _lavalinkNode(server, from.copyWithChannels(0));
    _lavalinkNode(server, to.copyWithChannels(to.channels + count));
    _lavalinkEvent(
      server,
      LavalinkEventKind.moved,
      'Moved ${count == 1 ? '1 channel' : '$count channels'} from '
      '${from.settings.name} to ${to.settings.name}',
    );
    return server.lavalink;
  }

  /// Has a mock node play for [channels] channels, as if members played
  /// music there.
  @visibleForTesting
  void debugSetLavalinkChannels(String serverKey, int nodeId, int channels) {
    final server = _server(serverKey);
    final node = server.lavalink.node(nodeId);
    if (node != null) _lavalinkNode(server, node.copyWithChannels(channels));
  }

  // Music --------------------------------------------------------------------------

  /// Fills a channel's queue with [count] tracks at once, for load checks.
  @visibleForTesting
  void debugFillQueue(String serverKey, int channelId, int count) {
    final server = _server(serverKey);
    final music = _musicOf(server);
    _musicChanged(
      server,
      music.enqueue(channelId, server.selfId, music.debugResults(count)),
    );
  }

  /// As if the server's music were set up or not (a node added).
  @visibleForTesting
  void debugSetMusicConfigured(String serverKey, {required bool configured}) {
    final server = _server(serverKey);
    server.music = server.music.copyWith(configured: configured);
    _emit(MusicSettingsChanged(server.key, server.music));
    _emit(CommandsChanged(server.key, server.availableCommands));
  }

  @override
  Future<MusicSettings> musicSettings(String serverKey) async {
    final server = _server(serverKey);
    await _latency(40);
    return server.music;
  }

  @override
  Future<MusicSettings> updateMusicSettings(
    String serverKey,
    MusicSettings before,
    MusicSettings after,
  ) async {
    final server = _server(serverKey);
    _require(server, Permissions.musicManage);
    await _latency();
    if (!after.allows(after.defaultSource)) {
      throw const RepoException(
        RepoErrorKind.invalidArgument,
        'the default source must be one of the enabled sources',
      );
    }
    server.music = after;
    _emit(MusicSettingsChanged(server.key, after));
    return after;
  }

  List<MusicBlock> _blocks(MockServer server) =>
      server.musicBlocks.values.toList()..sort((a, b) => a.at.compareTo(b.at));

  MockMusic _musicOf(MockServer server) =>
      _music.putIfAbsent(server.key, () => MockMusic(_clock));

  /// As the server checks a member acting on a channel's music.
  void _musicAct(MockServer server, int channelId, Permissions needed) {
    if (!server.music.configured || !server.music.enabled) {
      throw const RepoException(
        RepoErrorKind.musicUnavailable,
        "Music isn't set up on this server",
      );
    }
    if (server.musicBlocks.containsKey(server.selfId)) {
      throw const RepoException(
        RepoErrorKind.forbidden,
        "You're blocked from music on this server",
      );
    }
    final held = server.selfChannelPermissions[channelId] ?? Permissions.none;
    final connected = _voice == (server: server.key, channel: channelId);
    if (!connected && !held.has(Permissions.musicManage)) {
      throw const RepoException(
        RepoErrorKind.forbidden,
        'Join the voice channel to control its music',
      );
    }
    _require(server, needed, channelId);
  }

  /// Tells the channel's listeners, and everyone the now-playing card.
  MusicSession _musicChanged(
    MockServer server,
    MusicSession session, {
    String? notice,
  }) {
    _emit(MusicSessionChanged(server.key, session));
    final now = session.nowPlaying;
    final card = now == null
        ? null
        : MusicNowPlaying(
            channelId: session.channelId,
            title: now.track.title,
            artist: now.track.artist,
            artworkId: now.track.artworkId,
            playing: session.playing,
            sourceName: now.track.sourceName,
          );
    if (_nowPlaying[(server.key, session.channelId)] != card) {
      _nowPlaying[(server.key, session.channelId)] = card;
      _emit(
        MusicNowPlayingChanged(
          server.key,
          channelId: session.channelId,
          nowPlaying: card,
        ),
      );
    }
    if (notice != null) {
      _emit(
        MusicNoticeReceived(
          server.key,
          MusicNotice(channelId: session.channelId, text: notice, at: _clock()),
        ),
      );
    }
    _musicTicking();
    return session;
  }

  /// Plays on to the next track when one ends, while something plays.
  void _musicTicking() {
    if (!simulateLife || _musicTick != null) return;
    _musicTick = Timer.periodic(const Duration(seconds: 1), (_) {
      for (final MapEntry(key: serverKey, value: music) in _music.entries) {
        final server = _world.servers.where((s) => s.key == serverKey);
        for (final session in music.finishEnded()) {
          if (server.isNotEmpty) _musicChanged(server.first, session);
        }
      }
    });
  }

  String _selfName(MockServer server) =>
      server.members[server.selfId]?.displayName ?? 'You';

  @override
  Future<MusicSearchResults> musicSearch(
    String serverKey,
    int channelId,
    String query, {
    String source = '',
  }) async {
    final server = _server(serverKey);
    _musicAct(server, channelId, Permissions.musicAdd);
    await _latency(200);
    if (query.trim().isEmpty || query.length > 200) {
      throw const RepoException(
        RepoErrorKind.invalidArgument,
        'a search is 1 to 200 characters',
      );
    }
    return _musicOf(
      server,
    ).search(query, source.isEmpty ? server.music.defaultSource : source);
  }

  @override
  Future<MusicSession> musicSubscribe(String serverKey, int channelId) async {
    final server = _server(serverKey);
    await _latency(40);
    return _musicOf(server).session(channelId);
  }

  @override
  Future<void> musicUnsubscribe(String serverKey, int channelId) async {}

  @override
  Future<MusicSession> musicEnqueue(
    String serverKey,
    int channelId,
    List<String> resultIds, {
    List<String> historyItemIds = const [],
    String? beforeItemId,
    MusicPlacement placement = MusicPlacement.end,
  }) async {
    final server = _server(serverKey);
    _musicAct(server, channelId, Permissions.musicAdd);
    if (placement != MusicPlacement.end || beforeItemId != null) {
      _require(server, Permissions.musicManageQueue, channelId);
    }
    await _latency();
    final session = _musicOf(server).enqueue(
      channelId,
      server.selfId,
      resultIds,
      historyItemIds: historyItemIds,
      beforeItemId: beforeItemId,
      placement: placement,
    );
    final count = resultIds.length + historyItemIds.length;
    return _musicChanged(
      server,
      session,
      notice: count == 1
          ? '${_selfName(server)} added '
                "'${session.queue.lastOrNull?.track.title ?? session.nowPlaying?.track.title}'"
          : '${_selfName(server)} added $count tracks',
    );
  }

  Future<MusicSession> _musicControl(
    String serverKey,
    int channelId,
    Permissions needed,
    void Function(MockPlayer player) change,
  ) async {
    final server = _server(serverKey);
    _musicAct(server, channelId, needed);
    await _latency(60);
    return _musicChanged(server, _musicOf(server).update(channelId, change));
  }

  @override
  Future<MusicSession> musicPause(
    String serverKey,
    int channelId, {
    required bool paused,
  }) => _musicControl(
    serverKey,
    channelId,
    Permissions.musicControl,
    (player) => player.setPaused(paused),
  );

  @override
  Future<MusicSession> musicSkip(
    String serverKey,
    int channelId, {
    required bool forward,
    String? expectedItemId,
  }) => _musicControl(serverKey, channelId, Permissions.musicControl, (player) {
    player.expect(expectedItemId);
    forward ? player.next() : player.previous();
  });

  @override
  Future<MusicSession> musicVoteSkip(
    String serverKey,
    int channelId, {
    required String expectedItemId,
  }) => _musicControl(serverKey, channelId, Permissions.none, (player) {
    player.voteSkip(_server(serverKey).selfId, expectedItemId);
  });

  @override
  Future<MusicSession> musicSeek(
    String serverKey,
    int channelId,
    Duration position, {
    String? expectedItemId,
  }) => _musicControl(serverKey, channelId, Permissions.musicControl, (player) {
    player.expect(expectedItemId);
    player.seek(position);
  });

  @override
  Future<MusicSession> musicStop(String serverKey, int channelId) =>
      _musicControl(
        serverKey,
        channelId,
        Permissions.musicManage,
        (player) => player.stop(),
      );

  @override
  Future<MusicSession> musicSetMode(
    String serverKey,
    int channelId, {
    MusicRepeat? repeat,
    bool? shuffle,
  }) => _musicControl(
    serverKey,
    channelId,
    Permissions.musicControl,
    (player) => player.setMode(repeat: repeat, shuffle: shuffle),
  );

  @override
  Future<MusicSession> musicMove(
    String serverKey,
    int channelId,
    String itemId, {
    String? beforeItemId,
  }) => _musicControl(
    serverKey,
    channelId,
    Permissions.musicManageQueue,
    (player) => player.move(itemId, beforeItemId),
  );

  @override
  Future<MusicSession> musicRemove(
    String serverKey,
    int channelId,
    List<String> itemIds,
  ) => _musicControl(
    serverKey,
    channelId,
    Permissions.musicAdd,
    (player) => player.remove(itemIds),
  );

  @override
  Future<MusicSession> musicClear(String serverKey, int channelId) =>
      _musicControl(
        serverKey,
        channelId,
        Permissions.musicManageQueue,
        (player) => player.clear(),
      );

  // Commands -------------------------------------------------------------------

  /// Commands the mock ran, by name, for tests.
  final List<String> commandsRun = [];

  @override
  Future<CommandOutcome> executeCommand(
    String serverKey,
    int channelId,
    String name,
    Map<String, String> options, {
    required String nonce,
  }) async {
    final server = _server(serverKey);
    _require(server, Permissions.sendMessages, channelId);
    final spec = server.availableCommands.find(name);
    if (spec == null) {
      throw RepoException(
        RepoErrorKind.notFound,
        '/$name isn\'t a command here',
      );
    }
    if (!spec.allowedIn(channelId)) {
      throw RepoException(
        RepoErrorKind.forbidden,
        '/${spec.name} can\'t be used in this channel',
      );
    }
    commandsRun.add(spec.name);
    final voice = _voice;
    final reply = voice == null || voice.server != serverKey
        ? (false, 'Join a voice channel to use music commands')
        : await _runMusicCommand(server, voice.channel, spec.name, options);
    final (public, text) = reply;
    if (public && server.commands.resultsPublic) {
      final now = _clock();
      final message = Message(
        id: _world.ids.at(now),
        channelId: channelId,
        authorId: server.selfId,
        content: text,
        createdAt: now,
        command: spec.name,
      );
      _history(server, channelId).add(message);
      _emit(MessageCreated(serverKey, message));
      return (messageId: message.id, ephemeral: false);
    }
    _emit(
      EphemeralMessageReceived(
        serverKey,
        EphemeralNote(
          channelId: channelId,
          nonce: nonce,
          content: text,
          at: _clock(),
        ),
      ),
    );
    return (messageId: null, ephemeral: true);
  }

  /// (public, what it says).
  Future<(bool, String)> _runMusicCommand(
    MockServer server,
    int channelId,
    String name,
    Map<String, String> options,
  ) async {
    final where = server.channels[channelId]?.name ?? 'the channel';
    final key = server.key;
    String line(MusicQueueItem item) =>
        '*${item.track.title}* — ${item.track.artist}';
    switch (name) {
      case 'play':
        final query = options['query'] ?? '';
        final found = await musicSearch(key, channelId, query);
        if (found.results.isEmpty) return (false, 'Nothing found for “$query”');
        final next = options['next'] == 'yes';
        final session = await musicEnqueue(key, channelId, [
          found.results.first.id,
        ], placement: next ? MusicPlacement.next : MusicPlacement.end);
        final added = [?session.nowPlaying, ...session.queue].lastWhere(
          (item) => item.track.title == found.results.first.track.title,
        );
        return (true, 'Added ${line(added)} to the queue in $where');
      case 'pause' || 'resume':
        await musicPause(key, channelId, paused: name == 'pause');
        return (
          true,
          '${name == 'pause' ? 'Paused' : 'Resumed'} the music in $where',
        );
      case 'skip':
        await musicSkip(key, channelId, forward: true);
        return (true, 'Skipped in $where');
      case 'stop':
        await musicStop(key, channelId);
        return (true, 'Stopped the music in $where');
      case 'clear':
        await musicClear(key, channelId);
        return (true, 'Cleared the queue in $where');
      case 'nowplaying' || 'queue':
        final session = _musicOf(server).session(channelId);
        final now = session.nowPlaying;
        return (
          false,
          now == null
              ? 'Nothing is playing in $where'
              : 'Now playing in $where: ${line(now)}',
        );
      default:
        return (false, '/$name did nothing here');
    }
  }

  @override
  Future<List<CommandChoice>> autocompleteCommand(
    String serverKey,
    int channelId,
    String name,
    String option,
    String value,
  ) async {
    final server = _server(serverKey);
    final voice = _voice;
    if (name != 'play' ||
        option != 'query' ||
        value.trim().length < 2 ||
        voice == null) {
      return const [];
    }
    final found = _musicOf(server).search(value, server.music.defaultSource);
    await _latency(60);
    return [
      for (final result in found.results)
        CommandChoice(
          name:
              '${result.track.title} — ${result.track.artist} · '
              '${formatTrackTime(result.track.duration)}',
          value: result.id,
          artworkId: result.track.artworkId,
        ),
    ];
  }

  @override
  Future<CommandList> commandSettings(String serverKey) async {
    final server = _server(serverKey);
    _require(server, Permissions.manageServer);
    await _latency(40);
    return server.commands;
  }

  @override
  Future<CommandList> updateCommandSettings(
    String serverKey, {
    String name = '',
    bool? enabled,
    List<int>? channelIds,
    bool? resultsPublic,
  }) async {
    final server = _server(serverKey);
    _require(server, Permissions.manageServer);
    await _latency();
    server.commands = CommandList(
      commands: [
        for (final spec in server.commands.commands)
          spec.name == name
              ? spec.copyWith(enabled: enabled, channelIds: channelIds)
              : spec,
      ],
      resultsPublic: resultsPublic ?? server.commands.resultsPublic,
    );
    _emit(CommandsChanged(serverKey, server.availableCommands));
    return server.commands;
  }

  /// What the app last set, for tests.
  MusicListening musicListening = const MusicListening();

  @override
  void setMusicListening(MusicListening listening) =>
      musicListening = listening;

  @override
  Future<String?> musicArtwork(
    String serverKey,
    String artworkId,
    int size,
  ) async => null;

  @override
  Future<List<MusicBlock>> musicBlocks(String serverKey) async {
    final server = _server(serverKey);
    _require(server, Permissions.musicManage);
    await _latency(40);
    return _blocks(server);
  }

  @override
  Future<List<MusicBlock>> blockFromMusic(
    String serverKey,
    int userId, {
    required bool blocked,
    String reason = '',
  }) async {
    final server = _server(serverKey);
    _require(server, Permissions.musicManage);
    await _latency();
    if (!server.members.containsKey(userId)) {
      throw const RepoException(RepoErrorKind.notFound, 'Unknown member.');
    }
    if (blocked) {
      server.musicBlocks[userId] = MusicBlock(
        userId: userId,
        reason: reason.trim(),
        blockedBy: server.selfId,
        at: _clock(),
      );
    } else {
      server.musicBlocks.remove(userId);
    }
    return _blocks(server);
  }

  // Domains & network -------------------------------------------------------------

  void _requireOwner(MockServer server) {
    if (server.info.ownerId != server.selfId) {
      throw const RepoException(
        RepoErrorKind.forbidden,
        "Only the server's owner manages its domains and network.",
      );
    }
  }

  /// Saves the network and tells whoever watches it.
  NetworkInfo _network(MockServer server, NetworkInfo network) {
    server.network = network;
    _emit(NetworkStatusChanged(server.key, network));
    return network;
  }

  @override
  Future<NetworkInfo> watchNetwork(
    String serverKey, {
    required bool watch,
  }) async {
    final server = _server(serverKey);
    _requireOwner(server);
    // Stopping is fire and forget: nobody waits on it.
    if (watch) await _latency(60);
    return server.network;
  }

  @override
  Future<NetworkInfo> setPublicAddress(
    String serverKey, {
    required bool autoDetect,
    String overrideIpv4 = '',
    String overrideIpv6 = '',
  }) async {
    final server = _server(serverKey);
    _requireOwner(server);
    await _latency();
    final old = server.network;
    final address = old.publicAddress;
    return _network(
      server,
      NetworkInfo(
        publicAddress: PublicAddress(
          ipv4: overrideIpv4.isNotEmpty ? overrideIpv4 : address.detectedIpv4,
          ipv6: overrideIpv6.isNotEmpty ? overrideIpv6 : address.detectedIpv6,
          detectedIpv4: address.detectedIpv4,
          detectedIpv6: address.detectedIpv6,
          overrideIpv4: overrideIpv4,
          overrideIpv6: overrideIpv6,
          autoDetect: autoDetect,
          detectedAt: address.detectedAt,
          hasIpv6: address.hasIpv6,
        ),
        ports: old.ports,
        settings: old.settings,
        listeningPort: old.listeningPort,
        nextMainPort: old.nextMainPort,
        domains: old.domains,
        dnsProviders: old.dnsProviders,
        acme: old.acme,
        tunnel: old.tunnel,
      ),
    );
  }

  @override
  Future<NetworkInfo> detectPublicAddress(String serverKey) async {
    final server = _server(serverKey);
    _requireOwner(server);
    await _latency(600);
    final old = server.network;
    final address = old.publicAddress;
    return _network(
      server,
      NetworkInfo(
        publicAddress: PublicAddress(
          ipv4: address.overrideIpv4.isNotEmpty
              ? address.overrideIpv4
              : address.detectedIpv4,
          ipv6: address.ipv6,
          detectedIpv4: address.detectedIpv4,
          detectedIpv6: address.detectedIpv6,
          overrideIpv4: address.overrideIpv4,
          overrideIpv6: address.overrideIpv6,
          autoDetect: address.autoDetect,
          detectedAt: clock.now(),
          hasIpv6: address.hasIpv6,
        ),
        ports: old.ports,
        settings: old.settings,
        listeningPort: old.listeningPort,
        nextMainPort: old.nextMainPort,
        domains: old.domains,
        dnsProviders: old.dnsProviders,
        acme: old.acme,
        tunnel: old.tunnel,
      ),
    );
  }

  /// Replaces the domains, keeps the addresses in step, and tells whom it
  /// concerns.
  NetworkInfo _domains(MockServer server, List<Domain> domains) {
    final old = server.network;
    final active = [
      for (final domain in domains)
        if (domain.primary && domain.active) domain,
      for (final domain in domains)
        if (domain.active && !domain.primary) domain,
    ];
    ConnectAddress address(Domain domain) => ConnectAddress(
      host: domain.hostname,
      port: domain.publicPort,
      trustedCertificate: domain.trustedCertificate,
      domainId: domain.id,
    );
    final addresses = ServerAddresses(
      primary: active.isNotEmpty && active.first.primary
          ? address(active.first)
          : null,
      domains: [for (final domain in active) address(domain)],
      ip: old.publicAddress.ipv4.isEmpty
          ? null
          : ConnectAddress(
              host: old.publicAddress.ipv4,
              port: old.listeningPort,
            ),
    );
    if (addresses != server.addresses) {
      server.addresses = addresses;
      _emit(ServerAddressesChanged(server.key, addresses));
    }
    return _network(
      server,
      NetworkInfo(
        publicAddress: old.publicAddress,
        ports: old.ports,
        settings: old.settings,
        listeningPort: old.listeningPort,
        nextMainPort: old.nextMainPort,
        upnpGateway: old.upnpGateway,
        upnpExternalIp: old.upnpExternalIp,
        upnpError: old.upnpError,
        domains: domains,
        dnsProviders: old.dnsProviders,
        acme: old.acme,
        tunnel: old.tunnel,
      ),
    );
  }

  Domain _domain(MockServer server, int id) =>
      server.network.domains.where((domain) => domain.id == id).firstOrNull ??
      (throw const RepoException(RepoErrorKind.notFound, 'No such domain.'));

  @override
  Future<NetworkInfo> addDomain(
    String serverKey, {
    required String hostname,
    required DomainKind kind,
    int? publicPort,
  }) async {
    final server = _server(serverKey);
    _requireOwner(server);
    await _latency();
    final name = hostname.trim().toLowerCase();
    if (!RegExp(r'^([a-z0-9-]+\.)+[a-z]{2,}$').hasMatch(name)) {
      throw RepoException(
        RepoErrorKind.invalidArgument,
        '${hostname.trim()} is not a domain name on the internet.',
      );
    }
    final domains = server.network.domains;
    if (domains.any((domain) => domain.hostname == name)) {
      throw RepoException(RepoErrorKind.conflict, '$name is already added.');
    }
    if (domains.length >= 20) {
      throw const RepoException(
        RepoErrorKind.conflict,
        'A server has at most 20 domains.',
      );
    }
    final domain = Domain(
      id: _random.nextInt(1 << 30) + 1,
      hostname: name,
      kind: kind,
      publicPort:
          publicPort ?? (kind.behindProxy ? 443 : server.network.listeningPort),
      trustedCertificate: kind.behindProxy,
    );
    return _domains(server, [...domains, domain]);
  }

  @override
  Future<NetworkInfo> updateDomain(
    String serverKey,
    int domainId,
    DomainUpdate update,
  ) async {
    final server = _server(serverKey);
    _requireOwner(server);
    await _latency();
    var domain = _domain(server, domainId);
    var domains = server.network.domains;
    if (update.confirm) {
      domain = _with(
        domain,
        status: DomainStatus.verified,
        lastCheck: DomainCheck(
          result: CheckResult.confirmedByOwner,
          message: 'Confirmed by the owner.',
          checkedAt: _clock(),
        ),
      );
    }
    final primary = update.primary ?? false;
    if ((update.active ?? false) || primary) {
      if (!domain.wasVerified) {
        throw const RepoException(
          RepoErrorKind.conflict,
          'Check the domain first: only verified domains can be used.',
        );
      }
      domain = _with(domain, active: true);
    }
    if (update.active == false) {
      if (domain.primary && domains.where((d) => d.active).length > 1) {
        throw const RepoException(
          RepoErrorKind.conflict,
          'Make another domain the primary before turning this one off.',
        );
      }
      domain = _with(
        domain,
        active: false,
        primary: false,
        deactivatedAt: _clock(),
      );
    }
    if (update.publicPort case final port?) {
      domain = _with(domain, publicPort: port);
    }
    domains = [
      for (final other in domains)
        if (other.id == domain.id)
          primary ? _with(domain, primary: true) : domain
        else if (primary)
          _with(other, primary: false)
        else
          other,
    ];
    return _domains(server, _withPrimary(domains));
  }

  /// One active domain is the primary, while any is active.
  List<Domain> _withPrimary(List<Domain> domains) {
    if (domains.any((d) => d.active && d.primary)) return domains;
    final first = domains.where((d) => d.active).firstOrNull;
    return [
      for (final domain in domains)
        domain == first ? _with(domain, primary: true) : domain,
    ];
  }

  @override
  Future<NetworkInfo> deleteDomain(String serverKey, int domainId) async {
    final server = _server(serverKey);
    _requireOwner(server);
    await _latency();
    final domain = _domain(server, domainId);
    final domains = server.network.domains;
    if (domain.primary && domains.where((d) => d.active).length > 1) {
      throw const RepoException(
        RepoErrorKind.conflict,
        'Make another domain the primary before deleting this one.',
      );
    }
    server.domainChecks.remove(domainId);
    return _domains(
      server,
      _withPrimary([
        for (final other in domains)
          if (other.id != domainId) other,
      ]),
    );
  }

  /// Names starting with "wrong" point elsewhere; the rest pass.
  @override
  Future<NetworkInfo> checkDomain(String serverKey, int domainId) async {
    final server = _server(serverKey);
    _requireOwner(server);
    await _latency(60);
    final domain = _domain(server, domainId);
    final info = _domains(server, [
      for (final other in server.network.domains)
        other.id == domainId ? _with(other, checking: true) : other,
    ]);
    _later(const Duration(milliseconds: 900), () {
      final passed = !domain.hostname.startsWith('wrong');
      final check = DomainCheck(
        result: passed ? CheckResult.verified : CheckResult.pointsElsewhere,
        message: passed
            ? 'It points to this server.'
            : 'It points to 198.51.100.1, not this server '
                  '(${server.network.publicAddress.ipv4}).',
        found: [passed ? server.network.publicAddress.ipv4 : '198.51.100.1'],
        checkedAt: _clock(),
      );
      server.domainChecks[domainId] = [
        check,
        ...?server.domainChecks[domainId],
      ].take(10).toList();
      final first = passed && !server.network.domains.any((d) => d.active);
      final current = _domain(server, domainId);
      _domains(server, [
        for (final other in server.network.domains)
          if (other.id == domainId)
            _with(
              current,
              checking: false,
              lastCheck: check,
              status: passed
                  ? DomainStatus.verified
                  : current.wasVerified
                  ? DomainStatus.dnsProblem
                  : DomainStatus.pending,
              active: first ? true : null,
              primary: first ? true : null,
            )
          else
            other,
      ]);
    });
    return info;
  }

  /// Steps through a request; names starting with "nocert" fail at
  /// validation, as when port 80 is closed.
  @override
  Future<NetworkInfo> requestCertificate(
    String serverKey,
    int domainId, {
    String? email,
    bool agreeTerms = false,
  }) async {
    final server = _server(serverKey);
    _requireOwner(server);
    await _latency(60);
    final domain = _domain(server, domainId);
    if (domain.kind.behindProxy) {
      throw const RepoException(
        RepoErrorKind.invalidArgument,
        "The tunnel or proxy provides this domain's certificate.",
      );
    }
    if (domain.status != DomainStatus.verified) {
      throw const RepoException(
        RepoErrorKind.conflict,
        'The domain must point to this server first: check it.',
      );
    }
    if (!server.network.acme.exists && !agreeTerms) {
      throw const RepoException(
        RepoErrorKind.invalidArgument,
        "Agree to Let's Encrypt's subscriber agreement first.",
      );
    }
    final old = domain.certificate;
    void at(CertificateStatus certificate) => _domains(server, [
      for (final other in server.network.domains)
        other.id == domainId ? _with(other, certificate: certificate) : other,
    ]);
    CertificateStatus stepping(CertificateStep step) => CertificateStatus(
      state: CertificateState.requesting,
      issuer: old.issuer,
      notAfter: old.notAfter,
      test: old.test,
      step: step,
    );
    at(stepping(CertificateStep.account));
    final steps = CertificateStep.values.skip(1).toList();
    for (final (index, step) in steps.indexed) {
      _later(
        Duration(milliseconds: 600 * (index + 1)),
        () => at(stepping(step)),
      );
    }
    _later(Duration(milliseconds: 600 * (steps.length + 1)), () {
      final staging = server.network.settings.acmeStaging;
      final info = server.network;
      _network(
        server,
        NetworkInfo(
          publicAddress: info.publicAddress,
          ports: info.ports,
          settings: info.settings,
          listeningPort: info.listeningPort,
          nextMainPort: info.nextMainPort,
          upnpGateway: info.upnpGateway,
          upnpExternalIp: info.upnpExternalIp,
          upnpError: info.upnpError,
          domains: info.domains,
          dnsProviders: info.dnsProviders,
          acme: AcmeAccount(
            exists: true,
            email: email ?? info.acme.email,
            termsUrl: info.acme.termsUrl,
          ),
          tunnel: info.tunnel,
        ),
      );
      at(
        domain.hostname.startsWith('nocert')
            ? CertificateStatus(
                state: old.held
                    ? CertificateState.renewalFailed
                    : CertificateState.failed,
                issuer: old.issuer,
                notAfter: old.notAfter,
                error:
                    "Port 80 isn't reachable from the internet. Open or "
                    'forward it, or connect your DNS provider to use the DNS '
                    'challenge.',
              )
            : CertificateStatus(
                state: CertificateState.valid,
                issuer: staging ? '(STAGING) Ersatz Edamame E1' : 'E6',
                notAfter: _clock().add(const Duration(days: 90)),
                test: staging,
              ),
      );
    });
    return server.network;
  }

  @override
  Future<NetworkInfo> uploadCertificate(
    String serverKey,
    int domainId, {
    required String chainPem,
    required String keyPem,
  }) async {
    final server = _server(serverKey);
    _requireOwner(server);
    await _latency();
    _domain(server, domainId);
    if (!chainPem.contains('-----BEGIN CERTIFICATE-----')) {
      throw const RepoException(
        RepoErrorKind.invalidArgument,
        'The certificate is not in PEM form.',
      );
    }
    if (!keyPem.contains('PRIVATE KEY-----')) {
      throw const RepoException(
        RepoErrorKind.invalidArgument,
        'The private key is not in PEM form.',
      );
    }
    return _domains(server, [
      for (final other in server.network.domains)
        other.id == domainId
            ? _with(
                other,
                certificate: CertificateStatus(
                  state: CertificateState.valid,
                  issuer: 'Example CA',
                  notAfter: _clock().add(const Duration(days: 365)),
                  uploaded: true,
                ),
              )
            : other,
    ]);
  }

  /// The network with the tunnel, providers or account replaced.
  NetworkInfo _replaced(
    MockServer server, {
    TunnelStatus? tunnel,
    List<DnsProvider>? dnsProviders,
  }) {
    final old = server.network;
    return _network(
      server,
      NetworkInfo(
        publicAddress: old.publicAddress,
        ports: old.ports,
        settings: old.settings,
        listeningPort: old.listeningPort,
        nextMainPort: old.nextMainPort,
        upnpGateway: old.upnpGateway,
        upnpExternalIp: old.upnpExternalIp,
        upnpError: old.upnpError,
        domains: old.domains,
        dnsProviders: dnsProviders ?? old.dnsProviders,
        acme: old.acme,
        tunnel: tunnel ?? old.tunnel,
      ),
    );
  }

  TunnelStatus _tunnel(
    TunnelStatus old, {
    TunnelState? state,
    TunnelKind? kind,
    String? hostname,
    int? domainId,
    String? error,
    int? restarts,
    String? cloudflaredVersion,
    bool? installing,
  }) => TunnelStatus(
    state: state ?? old.state,
    kind: kind ?? old.kind,
    hostname: hostname ?? old.hostname,
    domainId: domainId ?? old.domainId,
    error: error ?? old.error,
    restarts: restarts ?? old.restarts,
    cloudflaredVersion: cloudflaredVersion ?? old.cloudflaredVersion,
    installing: installing ?? old.installing,
  );

  /// Names starting with "taken" are someone else's at the provider.
  @override
  Future<NetworkInfo> addFreeSubdomain(
    String serverKey, {
    required DnsProviderKind provider,
    required String name,
    String token = '',
    String? email,
    bool agreeTerms = false,
  }) async {
    final server = _server(serverKey);
    _requireOwner(server);
    await _latency();
    final suffix = switch (provider) {
      DnsProviderKind.duckdns => '.duckdns.org',
      DnsProviderKind.desec => '.dedyn.io',
      DnsProviderKind.cloudflare => throw const RepoException(
        RepoErrorKind.invalidArgument,
        'Free subdomains come from DuckDNS or deSEC.',
      ),
    };
    final label = name.trim().toLowerCase().replaceAll(suffix, '');
    if (!RegExp(r'^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$').hasMatch(label)) {
      throw RepoException(
        RepoErrorKind.invalidArgument,
        '$label is not a ${provider.label} name: letters, digits and hyphens.',
      );
    }
    final connected = server.network.provider(provider) != null;
    if (token.trim().isEmpty && !connected) {
      throw RepoException(
        RepoErrorKind.invalidArgument,
        'Paste your ${provider.label} token.',
      );
    }
    if (!server.network.acme.exists && !agreeTerms) {
      throw const RepoException(
        RepoErrorKind.invalidArgument,
        "Agree to Let's Encrypt's subscriber agreement first: the domain gets "
        'its certificate by itself.',
      );
    }
    if (label.startsWith('taken')) {
      throw RepoException(
        RepoErrorKind.invalidArgument,
        '${provider.label}: $label$suffix is not one of your names.',
      );
    }
    if (token.trim().isNotEmpty) {
      await setDnsProvider(serverKey, provider, token);
    }
    final hostname = '$label$suffix';
    await addDomain(
      serverKey,
      hostname: hostname,
      kind: DomainKind.freeSubdomain,
    );
    final id = server.network.domains.last.id;
    await checkDomain(serverKey, id);
    _later(const Duration(milliseconds: 1000), () {
      if (_domain(server, id).status == DomainStatus.verified) {
        unawaited(
          requestCertificate(
            serverKey,
            id,
            email: email,
            agreeTerms: true,
          ).catchError((Object _) => server.network),
        );
      }
    });
    return server.network;
  }

  @override
  Future<NetworkInfo> startTunnel(
    String serverKey,
    TunnelKind kind, {
    String token = '',
    String hostname = '',
  }) async {
    final server = _server(serverKey);
    _requireOwner(server);
    await _latency();
    final old = server.network.tunnel;
    if (!old.hasCloudflared) {
      throw const RepoException(
        RepoErrorKind.invalidArgument,
        'cloudflared is not installed: install it, or let Opencord download '
        'it.',
      );
    }
    var host = hostname.trim().toLowerCase();
    if (kind == TunnelKind.named) {
      final saved = old.kind == TunnelKind.named;
      if (token.trim().isEmpty && !saved) {
        throw const RepoException(
          RepoErrorKind.invalidArgument,
          'Paste the tunnel token.',
        );
      }
      if (token.trim().isNotEmpty && token.trim().length < 32) {
        throw const RepoException(
          RepoErrorKind.invalidArgument,
          "That is not a tunnel token: copy the long one from the tunnel's "
          "install command in Cloudflare's dashboard.",
        );
      }
      if (!host.contains('.')) {
        throw const RepoException(
          RepoErrorKind.invalidArgument,
          'Enter the public hostname you set up for the tunnel.',
        );
      }
    } else {
      host = 'mock-words-${_random.nextInt(900) + 100}.trycloudflare.com';
    }
    for (final domain in server.network.domains) {
      if (domain.kind == DomainKind.tunnel &&
          domain.hostname.endsWith('.trycloudflare.com')) {
        _domains(server, [
          for (final other in server.network.domains)
            if (other.id != domain.id) other,
        ]);
      }
    }
    _replaced(
      server,
      tunnel: TunnelStatus(
        state: TunnelState.starting,
        kind: kind,
        hostname: kind == TunnelKind.named ? host : '',
        cloudflaredVersion: old.cloudflaredVersion,
      ),
    );
    _later(const Duration(milliseconds: 600), () async {
      if (server.network.tunnel.state != TunnelState.starting) return;
      var id = server.network.domains
          .where((domain) => domain.hostname == host)
          .firstOrNull
          ?.id;
      if (id == null) {
        await addDomain(serverKey, hostname: host, kind: DomainKind.tunnel);
        id = server.network.domains.last.id;
      }
      _replaced(
        server,
        tunnel: _tunnel(
          server.network.tunnel,
          state: TunnelState.running,
          hostname: host,
          domainId: id,
        ),
      );
      await checkDomain(serverKey, id);
    });
    return server.network;
  }

  @override
  Future<NetworkInfo> stopTunnel(String serverKey) async {
    final server = _server(serverKey);
    _requireOwner(server);
    await _latency();
    final old = server.network.tunnel;
    if (old.kind == TunnelKind.quick && old.domainId != 0) {
      final remaining = [
        for (final domain in server.network.domains)
          if (domain.id != old.domainId) domain,
      ];
      final hasPrimary = remaining.any((d) => d.active && d.primary);
      _domains(server, [
        for (final (index, domain) in remaining.indexed)
          !hasPrimary &&
                  domain.active &&
                  index == remaining.indexWhere((d) => d.active)
              ? _with(domain, primary: true)
              : domain,
      ]);
    }
    return _replaced(
      server,
      tunnel: TunnelStatus(cloudflaredVersion: old.cloudflaredVersion),
    );
  }

  @override
  Future<List<String>> tunnelLog(String serverKey) async {
    final server = _server(serverKey);
    _requireOwner(server);
    await _latency(60);
    final tunnel = server.network.tunnel;
    if (!tunnel.on) return const [];
    return [
      'INF Starting tunnel tunnelID=mock',
      'INF Version ${tunnel.cloudflaredVersion}',
      if (tunnel.kind == TunnelKind.quick)
        'INF |  https://${tunnel.hostname}  |',
      if (tunnel.state == TunnelState.running)
        'INF Registered tunnel connection connIndex=0 location=ams01',
    ];
  }

  @override
  Future<NetworkInfo> installCloudflared(String serverKey) async {
    final server = _server(serverKey);
    _requireOwner(server);
    await _latency();
    _later(const Duration(milliseconds: 1200), () {
      _replaced(
        server,
        tunnel: _tunnel(
          server.network.tunnel,
          installing: false,
          cloudflaredVersion: '2026.10.0',
        ),
      );
    });
    return _replaced(
      server,
      tunnel: _tunnel(server.network.tunnel, installing: true, error: ''),
    );
  }

  @override
  Future<NetworkInfo> setDnsProvider(
    String serverKey,
    DnsProviderKind kind,
    String token,
  ) async {
    final server = _server(serverKey);
    _requireOwner(server);
    await _latency();
    final trimmed = token.trim();
    if (trimmed.isNotEmpty && trimmed.length < 8) {
      throw RepoException(
        RepoErrorKind.invalidArgument,
        '${kind.label} refused the token.',
      );
    }
    final old = server.network;
    return _network(
      server,
      NetworkInfo(
        publicAddress: old.publicAddress,
        ports: old.ports,
        settings: old.settings,
        listeningPort: old.listeningPort,
        nextMainPort: old.nextMainPort,
        upnpGateway: old.upnpGateway,
        upnpExternalIp: old.upnpExternalIp,
        upnpError: old.upnpError,
        domains: old.domains,
        dnsProviders: [
          for (final provider in old.dnsProviders)
            if (provider.kind != kind) provider,
          if (trimmed.isNotEmpty)
            DnsProvider(
              kind: kind,
              masked: '••••${trimmed.substring(trimmed.length - 4)}',
            ),
        ]..sort((a, b) => a.kind.index.compareTo(b.kind.index)),
        acme: old.acme,
        tunnel: old.tunnel,
      ),
    );
  }

  @override
  Future<List<DomainCheck>> domainChecks(String serverKey, int domainId) async {
    final server = _server(serverKey);
    _requireOwner(server);
    _domain(server, domainId);
    await _latency(60);
    return server.domainChecks[domainId] ?? const [];
  }

  @override
  Future<NetworkInfo> updateNetworkSettings(
    String serverKey,
    NetworkSettingsChanges changes,
  ) async {
    final server = _server(serverKey);
    _requireOwner(server);
    await _latency();
    final old = server.network;
    final settings = old.settings;
    final next = NetworkSettings(
      mainPort: changes.mainPort ?? settings.mainPort,
      upnpEnabled: changes.upnpEnabled ?? settings.upnpEnabled,
      localListenerPort:
          changes.localListenerPort ?? settings.localListenerPort,
      trustedProxies: changes.trustedProxies ?? settings.trustedProxies,
      dnsResolvers: changes.dnsResolvers ?? settings.dnsResolvers,
      acmeStaging: changes.acmeStaging ?? settings.acmeStaging,
    );
    return _network(
      server,
      NetworkInfo(
        publicAddress: old.publicAddress,
        ports: [
          for (final port in old.ports)
            next.upnpEnabled && port.purpose != PortPurpose.acmeHttp
                ? PortStatus(
                    purpose: port.purpose,
                    protocol: port.protocol,
                    port: port.port,
                    state: port.state,
                    mapping: port.state == PortState.listening
                        ? MappingState.mapped
                        : MappingState.notUsed,
                  )
                : PortStatus(
                    purpose: port.purpose,
                    protocol: port.protocol,
                    port: port.port,
                    state: port.state,
                  ),
        ],
        settings: next,
        listeningPort: old.listeningPort,
        nextMainPort: next.mainPort == 0 || next.mainPort == old.listeningPort
            ? PortState.unknown
            : PortState.free,
        upnpGateway: next.upnpEnabled ? '192.168.1.1' : '',
        upnpExternalIp: next.upnpEnabled ? old.publicAddress.ipv4 : '',
        domains: old.domains,
        dnsProviders: old.dnsProviders,
        acme: old.acme,
        tunnel: old.tunnel,
      ),
    );
  }

  // Voice -------------------------------------------------------------------------

  @override
  Future<void> joinVoice(String serverKey, int channelId) async {
    final server = _server(serverKey);
    final participants =
        server.voice[channelId] ??
        (throw const RepoException(
          RepoErrorKind.invalidArgument,
          'That is not a voice channel.',
        ));
    final held = server.selfChannelPermissions[channelId] ?? Permissions.none;
    if (!held.has(Permissions.connect)) {
      throw const RepoException(
        RepoErrorKind.forbidden,
        'missing permission: CONNECT',
      );
    }
    final limit = server.channels[channelId]?.userLimit ?? 0;
    final others = participants.where((p) => p.userId != server.selfId);
    if (limit > 0 &&
        others.length >= limit &&
        !held.has(Permissions.moveMembers)) {
      throw const RepoException(
        RepoErrorKind.voiceChannelFull,
        'that voice channel is full',
      );
    }
    _leaveVoiceNow();
    _selfVoice = VoiceParticipant(
      userId: server.selfId,
      muted: _selfVoice.muted,
      deafened: _selfVoice.deafened,
    );
    participants.add(_selfVoice);
    _voice = (server: serverKey, channel: channelId);
    _emit(VoiceChanged(serverKey, channelId, List.of(participants)));
    _emit(VoiceConnectionChanged(serverKey, channelId, _connected));
    if (!simulateLife) return;
    _speaking = Timer.periodic(const Duration(milliseconds: 650), (_) {
      final speakers = <int>{
        for (final participant in participants)
          if (!participant.muted &&
              !participant.deafened &&
              participant.userId != server.selfId &&
              _random.nextDouble() < 0.45)
            participant.userId,
        if (!_selfVoice.muted &&
            !_selfVoice.deafened &&
            _random.nextDouble() < 0.25)
          server.selfId,
      };
      _emit(SpeakingChanged(serverKey, speakers));
    });
  }

  void _leaveVoiceNow() {
    _speaking?.cancel();
    _speaking = null;
    if (_screen case final share?) {
      _screen = null;
      _emit(const OwnScreenShareChanged(null));
      if (_voice case final session?) {
        _emit(StreamEnded(session.server, share.key, session.channel));
      }
    }
    watched.clear();
    final session = _voice;
    _voice = null;
    if (session == null) return;
    final server = _world.servers
        .where((s) => s.key == session.server)
        .firstOrNull;
    final participants = server?.voice[session.channel];
    if (server == null || participants == null) return;
    participants.removeWhere(
      (participant) => participant.userId == server.selfId,
    );
    _emit(VoiceChanged(session.server, session.channel, List.of(participants)));
    _emit(SpeakingChanged(session.server, const {}));
  }

  @override
  Future<void> leaveVoice() async => _leaveVoiceNow();

  /// The voice channel [userId] is in and their place in it.
  (int, List<VoiceParticipant>, int) _inVoice(MockServer server, int userId) {
    for (final MapEntry(key: channel, value: participants)
        in server.voice.entries) {
      final index = participants.indexWhere((p) => p.userId == userId);
      if (index != -1) return (channel, participants, index);
    }
    throw const RepoException(
      RepoErrorKind.notFound,
      'They are not in a voice channel.',
    );
  }

  void _moderate(
    String serverKey,
    int userId,
    Permissions needed,
    VoiceParticipant Function(VoiceParticipant) change,
  ) {
    final server = _server(serverKey);
    final (channel, participants, index) = _inVoice(server, userId);
    _require(server, needed, channel);
    participants[index] = change(participants[index]);
    if (userId == server.selfId) _selfVoice = participants[index];
    _emit(VoiceChanged(serverKey, channel, List.of(participants)));
  }

  @override
  Future<void> serverMuteMember(
    String serverKey,
    int userId,
    bool muted,
  ) async => _moderate(
    serverKey,
    userId,
    Permissions.muteMembers,
    (p) => p.copyWith(serverMuted: muted),
  );

  @override
  Future<void> serverDeafenMember(
    String serverKey,
    int userId,
    bool deafened,
  ) async => _moderate(
    serverKey,
    userId,
    Permissions.deafenMembers,
    (p) => p.copyWith(serverDeafened: deafened),
  );

  @override
  Future<void> moveMember(String serverKey, int userId, int channelId) async {
    final server = _server(serverKey);
    final (from, participants, index) = _inVoice(server, userId);
    _require(server, Permissions.moveMembers, from);
    final destination =
        server.voice[channelId] ??
        (throw const RepoException(
          RepoErrorKind.invalidArgument,
          'That is not a voice channel.',
        ));
    _require(server, Permissions.moveMembers, channelId);
    if (from == channelId) return;
    destination.add(participants.removeAt(index));
    if (userId == server.selfId) {
      _voice = (server: serverKey, channel: channelId);
      _emit(VoiceConnectionChanged(serverKey, channelId, _connected));
    }
    _emit(VoiceChanged(serverKey, from, List.of(participants)));
    _emit(VoiceChanged(serverKey, channelId, List.of(destination)));
  }

  @override
  Future<void> disconnectMember(String serverKey, int userId) async {
    final server = _server(serverKey);
    final (channel, participants, index) = _inVoice(server, userId);
    _require(server, Permissions.moveMembers, channel);
    if (userId == server.selfId) return _leaveVoiceNow();
    participants.removeAt(index);
    _emit(VoiceChanged(serverKey, channel, List.of(participants)));
  }

  /// What the system's picker chooses, for tests.
  StreamSource debugShareSource = StreamSource.screen;

  /// The app a shared window belongs to, where the system says (Windows),
  /// for tests; `null`: the app asks ("Sound from").
  String? debugShareSound;

  /// Whether sharing sound works here, and the apps playing, for tests.
  ScreenAudioSupport screenAudio = (supported: true, reason: null);
  List<ScreenAudioApp> appsPlaying = [
    (name: 'Firefox', icon: 'firefox'),
    (name: 'Spotify', icon: 'spotify'),
  ];

  /// Each stream's volume and mute here, by server and user, for tests.
  final streamListening = <(String, int), (int, bool)>{};

  @override
  Future<ScreenAudioSupport> screenAudioSupport() async => screenAudio;

  @override
  Future<List<ScreenAudioApp>> screenAudioApps() async => appsPlaying;

  @override
  void setScreenShareAudioFrom(String? app) {
    if (_screen case final share?) {
      _screen = share.copyWith(audioFrom: () => app);
      _emit(OwnScreenShareChanged(_screen));
    }
  }

  @override
  void setStreamVolume(String serverKey, int userId, int volume) {
    final (_, muted) = streamListening[(serverKey, userId)] ?? (100, false);
    streamListening[(serverKey, userId)] = (volume, muted);
  }

  @override
  void setStreamMute(String serverKey, int userId, bool muted) {
    final (volume, _) = streamListening[(serverKey, userId)] ?? (100, false);
    streamListening[(serverKey, userId)] = (volume, muted);
  }

  /// The mock paints its cameras; what the tiles asked for is only kept,
  /// for tests.
  @override
  void setVideoWants(List<VideoWant> wants) => videoWants = wants;

  List<VideoWant> videoWants = const [];

  /// The mock paints its own camera; the one chosen is only kept, for
  /// tests.
  @override
  Future<void> chooseCamera(String? deviceId) async => cameraDevice = deviceId;

  String? cameraDevice;

  @override
  Future<List<(String, String)>> cameras() async => const [
    ('mock-camera', 'Mock camera'),
  ];

  /// Thrown when the camera is turned on, for tests.
  RepoException? cameraError;

  /// Thrown when going live or watching, for tests.
  RepoException? screenError;
  RepoException? watchError;

  /// The streams this device watches, for tests.
  final watched = <String>{};

  OwnScreenShare? _screen;

  /// The sources the share dialog lists where Opencord picks, for tests;
  /// the id last asked to share.
  List<ScreenSource> debugScreenSources = [
    (
      id: 'screen:1',
      kind: StreamSource.screen,
      title: 'Screen 1 · 2560×1440',
      app: null,
      thumbnail: null,
    ),
    (
      id: 'window:42',
      kind: StreamSource.window,
      title: 'Notes — Opencord plan',
      app: 'Notes',
      thumbnail: null,
    ),
  ];
  String? lastScreenSourceId;

  @override
  Future<List<ScreenSource>> screenSources() async => debugScreenSources;

  @override
  Future<void> startScreenShare(
    ScreenShareQuality quality, {
    bool audio = true,
    String? sourceId,
  }) async {
    if (screenError case final error?) throw error;
    lastScreenSourceId = sourceId;
    final session = _voice;
    if (session == null) {
      throw const RepoException(
        RepoErrorKind.notConnected,
        'Join a voice channel first.',
      );
    }
    await setVoiceSelf(screensharing: true);
    final key = mockStreamKey(session.channel, _selfVoice.userId);
    final stream = LiveStream(
      key: key,
      channelId: session.channel,
      userId: _selfVoice.userId,
      resolution: quality.resolution,
      fps: quality.fps,
      hasAudio: audio,
    );
    _emit(
      _screen == null
          ? StreamStarted(session.server, stream)
          : StreamChanged(session.server, stream),
    );
    _screen = OwnScreenShare(
      key: key,
      source: debugShareSource,
      quality: quality,
      audio: audio && screenAudio.supported,
      audioFrom: debugShareSource == StreamSource.window
          ? debugShareSound
          : null,
      preview: const VideoFeed(
        trackId: 'mock-screen',
        textureId: null,
        width: 1920,
        height: 1080,
      ),
    );
    _emit(OwnScreenShareChanged(_screen));
  }

  @override
  Future<void> updateScreenShare(
    ScreenShareQuality quality, {
    bool audio = true,
  }) async {
    final (session, share) = (_voice, _screen);
    if (session == null || share == null) return;
    _screen = share.copyWith(quality: quality, audio: audio);
    _emit(
      StreamChanged(
        session.server,
        LiveStream(
          key: share.key,
          channelId: session.channel,
          userId: _selfVoice.userId,
          resolution: quality.resolution,
          fps: quality.fps,
          hasAudio: audio,
        ),
      ),
    );
    _emit(OwnScreenShareChanged(_screen));
  }

  @override
  Future<void> stopScreenShare() async {
    final (session, share) = (_voice, _screen);
    _screen = null;
    if (share == null) return;
    _emit(const OwnScreenShareChanged(null));
    if (session == null) return;
    _emit(StreamEnded(session.server, share.key, session.channel));
    await setVoiceSelf(screensharing: false);
  }

  /// The mock paints its screens; watching is only kept, for tests.
  @override
  Future<void> watchStream(String key) async {
    if (watchError case final error?) throw error;
    watched.add(key);
  }

  @override
  Future<void> unwatchStream(String key) async => watched.remove(key);

  /// As a moderator would: moves the current user to [channelId] on the
  /// server they are in voice on, or disconnects them (null).
  void debugMoveSelf(int? channelId) {
    final session = _voice;
    if (session == null) return;
    if (channelId == null) {
      _leaveVoiceNow();
      _emit(OwnVoiceChanged(session.server, null));
      return;
    }
    final server = _server(session.server);
    final from = server.voice[session.channel]!
      ..removeWhere((participant) => participant.userId == server.selfId);
    final to = server.voice[channelId]!..add(_selfVoice);
    _voice = (server: session.server, channel: channelId);
    _emit(VoiceChanged(session.server, session.channel, List.of(from)));
    _emit(VoiceChanged(session.server, channelId, List.of(to)));
    _emit(OwnVoiceChanged(session.server, channelId));
    _emit(VoiceConnectionChanged(session.server, channelId, _connected));
  }

  static const _connected = VoiceConnectionStatus(
    VoiceConnectionPhase.connected,
  );

  /// As voice media would report: the connection of the current voice
  /// session is now [status].
  void debugVoiceConnection(VoiceConnectionStatus status) {
    final session = _voice;
    if (session == null) return;
    _emit(VoiceConnectionChanged(session.server, session.channel, status));
  }

  /// As voice media would report: the chosen device is missing.
  void debugDeviceFellBack({required bool output, required String device}) =>
      _emit(AudioDeviceFellBack(output: output, device: device));

  /// Stand-ins for the system's microphones and speakers.
  static const devices = AudioDeviceList(
    inputs: [
      AudioDevice(id: 'mock:microphone', name: 'Built-in microphone'),
      AudioDevice(id: 'mock:usb-microphone', name: 'USB headset microphone'),
    ],
    outputs: [
      AudioDevice(id: 'mock:speakers', name: 'Built-in speakers'),
      AudioDevice(id: 'mock:usb-headset', name: 'USB headset'),
    ],
    defaultInput: 'mock:microphone',
    defaultOutput: 'mock:speakers',
  );

  /// What [applyAudio] was last given.
  AudioConfig? audio;
  var pushToTalkHeld = false;

  /// Per server and user: volume in percent, and local mute.
  final listening = <(String, int), (int, bool)>{};

  @override
  Future<AudioDeviceList> audioDevices() async => deviceList;

  /// What [audioDevices] answers; [devices] unless a test says otherwise.
  AudioDeviceList deviceList = devices;

  @override
  void applyAudio(AudioConfig config) => audio = config;

  @override
  void setPushToTalk(bool held) => pushToTalkHeld = held;

  var prioritySpeakerHeld = false;
  var micTest = false;

  /// While a meter shows the microphone: a made-up level now and then.
  Timer? _levels;

  @override
  void setPrioritySpeaker(bool held) => prioritySpeakerHeld = held;

  /// Whether a meter is showing the microphone.
  var levelMeterOn = false;

  @override
  void setLevelMeter(bool on) {
    levelMeterOn = on;
    _levels?.cancel();
    _levels = on && simulateLife
        ? Timer.periodic(const Duration(milliseconds: 120), (_) {
            _emit(InputLevelChanged(-50 + _random.nextDouble() * 30));
          })
        : null;
  }

  /// As voice media would report: the microphone's level.
  void debugInputLevel(double dbfs) => _emit(InputLevelChanged(dbfs));

  /// Connection statistics for the channel this device is in.
  void debugVoiceStats(String serverKey, int channelId, VoiceStats stats) =>
      _emit(VoiceStatsChanged(serverKey, channelId, stats));

  /// Someone else plays a sound in the call (Phase 2 V8).
  void debugSoundPlayed(String serverKey, SoundPlay play) =>
      _emit(SoundPlayed(serverKey, play));

  /// As voice media would report: someone spoke while muted.
  void debugSpokeWhileMuted() => _emit(const SpokeWhileMuted());

  /// As voice media would report: High gave way to Standard.
  void debugNoiseSuppressionFellBack() =>
      _emit(const NoiseSuppressionFellBack());

  @override
  Future<void> setMicTest(bool on) async => micTest = on;

  /// The mock is a fast computer.
  @override
  Future<NoiseSuppression> recommendedNoiseSuppression() async =>
      NoiseSuppression.high;

  /// What [setHotkeys] answers: the mock binds nothing system-wide.
  HotkeySupport hotkeySupport = const HotkeySupport.focusedOnly(
    'the mock has no system hotkeys',
  );

  /// How many times [setHotkeys] was called.
  var hotkeyBinds = 0;

  /// What [setHotkeys] was last given.
  List<HotkeyBinding> hotkeys = const [];

  @override
  Future<HotkeySupport> setHotkeys(List<HotkeyBinding> bindings) async {
    hotkeys = bindings;
    hotkeyBinds++;
    return hotkeySupport;
  }

  /// As the system would report: a toggle's hotkey, pressed elsewhere.
  void debugHotkeyPressed(HotkeyAction action) => _emit(HotkeyPressed(action));

  @override
  void setUserVolume(String serverKey, int userId, int volume) {
    final (_, muted) = listening[(serverKey, userId)] ?? (100, false);
    listening[(serverKey, userId)] = (volume, muted);
  }

  @override
  List<DefaultSound> get defaultSounds => mockDefaultSounds;

  /// Sound references played and previewed, for tests.
  final soundsPlayed = <String>[];
  final soundsPreviewed = <String>[];

  /// The soundboard volume and mute, and whose soundboards are muted.
  ({int volume, bool muted}) soundboardListening = (volume: 100, muted: false);
  final mutedSoundboards = <(String, int)>{};

  /// When this user last played a sound, per server: the cooldown.
  final _lastSound = <String, DateTime>{};
  final _drafts = <int, SoundDraft>{};
  var _nextDraft = 0;

  @override
  Future<void> playSound(
    String serverKey,
    int channelId,
    String soundRef, {
    ({String serverKey, Sound sound})? from,
  }) async {
    final server = _server(serverKey);
    if (_voice != (server: serverKey, channel: channelId)) {
      throw const RepoException(
        RepoErrorKind.conflict,
        'you are not in a voice channel',
      );
    }
    final settings = server.voiceSettings;
    if (!settings.soundboardEnabled) {
      throw const RepoException(
        RepoErrorKind.forbidden,
        'the soundboard is turned off on this server',
      );
    }
    if (_selfVoice.deafened) {
      throw const RepoException(
        RepoErrorKind.forbidden,
        "sounds can't be played while deafened",
      );
    }
    final now = _clock();
    if (_lastSound[serverKey] case final last?) {
      final ready = last.add(settings.soundCooldown);
      if (now.isBefore(ready)) {
        throw RepoException(
          RepoErrorKind.soundCooldown,
          'Wait for the cooldown to play another sound',
          retryAfter: ready.difference(now),
        );
      }
    }
    final (name, emoji, volume, duration) = switch (soundRef.split(':')) {
      ['server', final sha] => switch (server.sounds.values
          .where((sound) => sound.sha256 == sha)
          .firstOrNull) {
        final sound? => (sound.name, sound.emoji, sound.volume, sound.duration),
        null => throw const RepoException(
          RepoErrorKind.notFound,
          'sound not found',
        ),
      },
      ['external', _] when from != null => (
        from.sound.name,
        from.sound.emoji,
        from.sound.volume,
        from.sound.duration,
      ),
      ['default', final id] => switch (defaultSounds
          .where((sound) => sound.id == id)
          .firstOrNull) {
        final sound? => (sound.name, sound.emoji, 100, sound.duration),
        null => throw const RepoException(
          RepoErrorKind.notFound,
          'sound not found',
        ),
      },
      _ => throw const RepoException(
        RepoErrorKind.invalidArgument,
        'that is not a sound',
      ),
    };
    _lastSound[serverKey] = now;
    soundsPlayed.add(soundRef);
    _emit(
      SoundPlayed(
        serverKey,
        SoundPlay(
          channelId: channelId,
          userId: server.selfId,
          soundRef: soundRef,
          name: name,
          emoji: emoji,
          volume: volume,
          duration: duration,
          startedAt: now,
        ),
      ),
    );
  }

  @override
  Future<void> previewSound(String serverKey, String soundRef) async =>
      soundsPreviewed.add(soundRef);

  @override
  void setSoundboardListening({required int volume, required bool muted}) =>
      soundboardListening = (volume: volume, muted: muted);

  /// Every voice cue played, oldest first.
  final cuesPlayed = <VoiceCue>[];

  @override
  void playCue(VoiceCue cue) => cuesPlayed.add(cue);

  @override
  void setUserSoundboardMute(String serverKey, int userId, bool muted) {
    if (muted) {
      mutedSoundboards.add((serverKey, userId));
    } else {
      mutedSoundboards.remove((serverKey, userId));
    }
  }

  @override
  Future<Sound> updateSound(
    String serverKey,
    int soundId, {
    String? name,
    String? emoji,
    int? volume,
  }) async {
    final server = _server(serverKey);
    final sound =
        server.sounds[soundId] ??
        (throw const RepoException(RepoErrorKind.notFound, 'sound not found'));
    final updated = sound.copyWith(
      name: name,
      emoji: emoji == null || emoji.isEmpty ? null : emoji,
      clearEmoji: emoji != null && emoji.isEmpty,
      volume: volume,
    );
    server.sounds[soundId] = updated;
    _emit(SoundUpserted(serverKey, updated));
    return updated;
  }

  @override
  Future<void> deleteSound(String serverKey, int soundId) async {
    final server = _server(serverKey);
    if (server.sounds.remove(soundId) != null) {
      _emit(SoundDeleted(serverKey, soundId));
    }
  }

  /// Any file opens as eight seconds of a made-up waveform.
  @override
  Future<SoundDraft> openSoundDraft(String path) async {
    final id = ++_nextDraft;
    final draft = SoundDraft(
      id: id,
      duration: const Duration(seconds: 8),
      waveform: [
        for (var i = 0; i < 600; i++)
          (0.2 + 0.8 * ((i * 37) % 100) / 100) * (i % 150 < 120 ? 1 : 0.1),
      ],
    );
    _drafts[id] = draft;
    return draft;
  }

  @override
  Future<void> previewSoundDraft(SoundWindow window) async =>
      soundsPreviewed.add('draft:${window.draftId}');

  @override
  Future<Sound> uploadSound(
    String serverKey,
    SoundWindow window, {
    required String name,
    String? emoji,
  }) async {
    final server = _server(serverKey);
    if (!_drafts.containsKey(window.draftId)) {
      throw const RepoException(
        RepoErrorKind.invalidArgument,
        'that file is no longer open',
      );
    }
    final max = server.voiceSettings.maxSounds;
    if (server.sounds.length >= max) {
      throw RepoException(
        RepoErrorKind.soundboardFull,
        "this server's soundboard is full ($max sounds)",
      );
    }
    final id = _world.ids.at(_clock());
    final sound = Sound(
      id: id,
      name: name.trim(),
      emoji: emoji,
      volume: window.volume,
      sha256: fakeFingerprint('upload $id'),
      duration: window.length,
      uploaderId: server.selfId,
      createdAt: _clock(),
    );
    server.sounds[id] = sound;
    _drafts.remove(window.draftId);
    _emit(SoundUpserted(serverKey, sound));
    return sound;
  }

  @override
  void closeSoundDraft(int draftId) => _drafts.remove(draftId);

  @override
  void setUserLocalMute(String serverKey, int userId, bool muted) {
    final (volume, _) = listening[(serverKey, userId)] ?? (100, false);
    listening[(serverKey, userId)] = (volume, muted);
  }

  @override
  Future<void> setVoiceSelf({
    bool? muted,
    bool? deafened,
    bool? camera,
    bool? screensharing,
  }) async {
    if (cameraError case final error? when camera == true) throw error;
    _selfVoice = _selfVoice.copyWith(
      muted: muted,
      deafened: deafened,
      camera: camera,
      screensharing: screensharing,
    );
    final session = _voice;
    if (session == null) return;
    final participants = _server(session.server).voice[session.channel]!;
    final index = participants.indexWhere(
      (participant) => participant.userId == _selfVoice.userId,
    );
    if (index != -1) participants[index] = _selfVoice;
    _emit(VoiceChanged(session.server, session.channel, List.of(participants)));
  }

  @override
  Future<Message> sendMessageWithFiles(
    String serverKey,
    int channelId,
    String content, {
    required String nonce,
    required List<StagedFile> files,
  }) async {
    final server = _server(serverKey);
    _require(server, Permissions.sendMessages, channelId);
    _require(server, Permissions.attachFiles, channelId);

    _activeSends[nonce] = false;
    try {
      for (var i = 0; i < files.length; i++) {
        if (_activeSends[nonce] == true) {
          throw const RepoException(
            RepoErrorKind.cancelled,
            'Upload cancelled',
          );
        }
        final file = files[i];
        _emit(
          UploadProgressEvent(
            serverKey,
            channelId: channelId,
            nonce: nonce,
            fileIndex: i,
            sentBytes: file.size ~/ 2,
            totalBytes: file.size,
            phase: UploadPhase.uploading,
          ),
        );
        await _latency(40);
        if (_activeSends[nonce] == true) {
          throw const RepoException(
            RepoErrorKind.cancelled,
            'Upload cancelled',
          );
        }
        _emit(
          UploadProgressEvent(
            serverKey,
            channelId: channelId,
            nonce: nonce,
            fileIndex: i,
            sentBytes: file.size,
            totalBytes: file.size,
            phase: UploadPhase.done,
          ),
        );
      }

      final now = _clock();
      final messageId = _world.ids.at(now);
      final attachments = [
        for (var i = 0; i < files.length; i++)
          Attachment(
            id: messageId + i + 1,
            filename: files[i].name,
            contentType: 'application/octet-stream',
            sizeBytes: files[i].size,
            kind:
                files[i].name.endsWith('.png') || files[i].name.endsWith('.jpg')
                ? AttachmentKind.image
                : (files[i].name.endsWith('.ogg')
                      ? AttachmentKind.voiceMessage
                      : AttachmentKind.file),
            hasThumbnail:
                files[i].name.endsWith('.png') ||
                files[i].name.endsWith('.jpg'),
          ),
      ];

      final message = Message(
        id: messageId,
        channelId: channelId,
        authorId: server.selfId,
        content: content,
        createdAt: now,
        attachments: attachments,
      );
      server.messages[channelId]!.add(message);
      _emit(MessageCreated(serverKey, message));
      return message;
    } finally {
      _activeSends.remove(nonce);
    }
  }

  @override
  void cancelSend(String serverKey, String nonce) {
    if (_activeSends.containsKey(nonce)) {
      _activeSends[nonce] = true;
    }
  }

  @override
  Future<Uint8List> attachmentBytes(
    String serverKey,
    int attachmentId, {
    bool thumbnail = false,
  }) async {
    await _latency(40);
    return mockGreyPngBytes;
  }

  @override
  Future<void> downloadAttachment(
    String serverKey,
    int attachmentId,
    String path,
  ) async {
    _activeDownloads[attachmentId] = false;
    try {
      _emit(
        DownloadProgressEvent(
          serverKey,
          attachmentId: attachmentId,
          receivedBytes: 500,
          totalBytes: 1000,
          done: false,
        ),
      );
      await _latency(40);
      if (_activeDownloads[attachmentId] == true) {
        // Like the core: a download always ends with a last event.
        _emit(
          DownloadProgressEvent(
            serverKey,
            attachmentId: attachmentId,
            receivedBytes: 500,
            totalBytes: 1000,
            done: false,
            error: 'Cancelled.',
          ),
        );
        throw const RepoException(RepoErrorKind.cancelled, 'Cancelled.');
      }
      _emit(
        DownloadProgressEvent(
          serverKey,
          attachmentId: attachmentId,
          receivedBytes: 1000,
          totalBytes: 1000,
          done: true,
        ),
      );
    } finally {
      _activeDownloads.remove(attachmentId);
    }
  }

  @override
  void cancelDownload(String serverKey, int attachmentId) {
    if (_activeDownloads.containsKey(attachmentId)) {
      _activeDownloads[attachmentId] = true;
    }
  }

  @override
  Future<Uint8List?> clipboardImage() async {
    return _clipboardImage;
  }

  @override
  Future<void> copyImage(Uint8List bytes) async {
    _clipboardImage = bytes;
  }

  @override
  Future<UploadSettings> updateUploadSettings(
    String serverKey, {
    List<String>? blockedExtensions,
    int? voiceMessageMaxSeconds,
  }) async {
    final server = _server(serverKey);
    _require(server, Permissions.manageServer);
    await _latency();
    final updated = UploadSettings(
      blockedExtensions:
          blockedExtensions ?? server.uploadSettings.blockedExtensions,
      voiceMessageMaxSeconds:
          voiceMessageMaxSeconds ??
          server.uploadSettings.voiceMessageMaxSeconds,
      maxFileBytes: server.uploadSettings.maxFileBytes,
      maxAttachments: server.uploadSettings.maxAttachments,
    );
    server.uploadSettings = updated;
    _emit(UploadSettingsEvent(serverKey, updated));
    return updated;
  }

  @override
  Future<StorageInfo> storage(String serverKey) async {
    final server = _server(serverKey);
    if (server.info.ownerId != server.selfId) {
      throw const RepoException(
        RepoErrorKind.forbidden,
        'Only the owner manages storage.',
      );
    }
    await _latency();
    return server.storage;
  }

  @override
  Future<StorageInfo> addStorage(
    String serverKey,
    String name,
    S3Config s3,
  ) async {
    final server = _server(serverKey);
    if (server.info.ownerId != server.selfId) {
      throw const RepoException(
        RepoErrorKind.forbidden,
        'Only the owner manages storage.',
      );
    }
    _checkKeys(s3);
    await _latency();
    final id = server.storage.backends.length + 1;
    final backend = StorageBackend(
      id: id,
      kind: StorageKind.s3,
      name: name,
      active: false,
      s3: _answered(s3),
      hasSecret: s3.secretAccessKey.isNotEmpty,
    );
    final updated = StorageInfo(
      backends: [...server.storage.backends, backend],
      moveStatus: server.storage.moveStatus,
      tempFreeBytes: server.storage.tempFreeBytes,
    );
    server.storage = updated;
    _emit(StorageEvent(serverKey, updated));
    return updated;
  }

  @override
  Future<StorageInfo> updateStorage(
    String serverKey,
    int storageId, {
    String? name,
    S3Config? s3,
  }) async {
    final server = _server(serverKey);
    if (server.info.ownerId != server.selfId) {
      throw const RepoException(
        RepoErrorKind.forbidden,
        'Only the owner manages storage.',
      );
    }
    if (s3 != null) _checkKeys(s3);
    await _latency();
    final backends = server.storage.backends.map((b) {
      if (b.id != storageId) return b;
      return StorageBackend(
        id: b.id,
        kind: b.kind,
        name: name ?? b.name,
        active: b.active,
        s3: s3 == null ? b.s3 : _answered(s3),
        hasSecret: (s3?.secretAccessKey.isNotEmpty ?? false) || b.hasSecret,
        fileCount: b.fileCount,
        bytes: b.bytes,
      );
    }).toList();
    final updated = StorageInfo(
      backends: backends,
      moveStatus: server.storage.moveStatus,
      tempFreeBytes: server.storage.tempFreeBytes,
    );
    server.storage = updated;
    _emit(StorageEvent(serverKey, updated));
    return updated;
  }

  /// As the server answers: the access key masked, no secret.
  S3Config _answered(S3Config s3) {
    final key = s3.accessKeyId;
    return S3Config(
      endpoint: s3.endpoint,
      region: s3.region,
      bucket: s3.bucket,
      keyPrefix: s3.keyPrefix,
      pathStyle: s3.pathStyle,
      allowHttp: s3.allowHttp,
      accessKeyId: '••••${key.length > 4 ? key.substring(key.length - 4) : ''}',
      secretAccessKey: '',
    );
  }

  /// The server tests a bucket before saving it, and a masked or missing
  /// key fails that test.
  void _checkKeys(S3Config s3) {
    if (s3.accessKeyId.isEmpty || s3.accessKeyId.startsWith('•')) {
      throw const RepoException(
        RepoErrorKind.invalidArgument,
        'The bucket refused the access key.',
      );
    }
  }

  @override
  Future<StorageTestResult> testStorage(
    String serverKey, {
    int? storageId,
    S3Config? s3,
  }) async {
    final server = _server(serverKey);
    if (server.info.ownerId != server.selfId) {
      throw const RepoException(
        RepoErrorKind.forbidden,
        'Only the owner manages storage.',
      );
    }
    await _latency();
    return const StorageTestResult(
      steps: [
        StorageTestStep(name: 'List objects', ok: true, detail: 'OK'),
        StorageTestStep(name: 'Put object', ok: true, detail: 'OK'),
        StorageTestStep(name: 'Get object', ok: true, detail: 'OK'),
        StorageTestStep(name: 'Delete object', ok: true, detail: 'OK'),
      ],
      ok: true,
    );
  }

  @override
  Future<StorageInfo> activateStorage(String serverKey, int storageId) async {
    final server = _server(serverKey);
    if (server.info.ownerId != server.selfId) {
      throw const RepoException(
        RepoErrorKind.forbidden,
        'Only the owner manages storage.',
      );
    }
    await _latency();
    final backends = server.storage.backends.map((b) {
      return StorageBackend(
        id: b.id,
        kind: b.kind,
        name: b.name,
        active: b.id == storageId,
        s3: b.s3,
        hasSecret: b.hasSecret,
        fileCount: b.fileCount,
        bytes: b.bytes,
      );
    }).toList();
    final updated = StorageInfo(
      backends: backends,
      moveStatus: server.storage.moveStatus,
      tempFreeBytes: server.storage.tempFreeBytes,
    );
    server.storage = updated;
    _emit(StorageEvent(serverKey, updated));
    return updated;
  }

  @override
  Future<StorageInfo> removeStorage(String serverKey, int storageId) async {
    final server = _server(serverKey);
    if (server.info.ownerId != server.selfId) {
      throw const RepoException(
        RepoErrorKind.forbidden,
        'Only the owner manages storage.',
      );
    }
    await _latency();
    final backends = server.storage.backends
        .where((b) => b.id != storageId)
        .toList();
    final updated = StorageInfo(
      backends: backends,
      moveStatus: server.storage.moveStatus,
      tempFreeBytes: server.storage.tempFreeBytes,
    );
    server.storage = updated;
    _emit(StorageEvent(serverKey, updated));
    return updated;
  }

  @override
  Future<StorageInfo> startStorageMove(String serverKey) async {
    final server = _server(serverKey);
    if (server.info.ownerId != server.selfId) {
      throw const RepoException(
        RepoErrorKind.forbidden,
        'Only the owner manages storage.',
      );
    }
    await _latency();
    final activeId = server.storage.backends.firstWhere((b) => b.active).id;
    final updated = StorageInfo(
      backends: server.storage.backends,
      moveStatus: StorageMove(
        toStorageId: activeId,
        running: true,
        filesDone: 0,
        filesTotal: 10,
        bytesDone: 0,
        bytesTotal: 1024 * 1024,
        failed: 0,
      ),
      tempFreeBytes: server.storage.tempFreeBytes,
    );
    server.storage = updated;
    _emit(StorageEvent(serverKey, updated));
    return updated;
  }

  @override
  Future<StorageInfo> stopStorageMove(String serverKey) async {
    final server = _server(serverKey);
    if (server.info.ownerId != server.selfId) {
      throw const RepoException(
        RepoErrorKind.forbidden,
        'Only the owner manages storage.',
      );
    }
    await _latency();
    final updated = StorageInfo(
      backends: server.storage.backends,
      moveStatus: null,
      tempFreeBytes: server.storage.tempFreeBytes,
    );
    server.storage = updated;
    _emit(StorageEvent(serverKey, updated));
    return updated;
  }

  int _mediaCacheSize = 12 * 1024 * 1024;

  @override
  Future<int> mediaCacheSize() async {
    return _mediaCacheSize;
  }

  @override
  Future<void> clearMediaCache() async {
    _mediaCacheSize = 0;
  }

  // Voice messages: answers as the core does, refusing what it refuses.

  int _nextDraftId = 1;
  final _voiceDrafts = <int, VoiceDraft>{};
  Timer? _recordingTimer;
  bool _recording = false;

  void emitVoiceRecordLevel(double level) => _emit(VoiceRecordLevel(level));

  void emitVoiceRecordStopped(VoiceStopReason reason) {
    _recordingTimer?.cancel();
    _emit(VoiceRecordStopped(reason));
  }

  @override
  void voiceRecordStart(int maxSeconds) {
    if (_recording) {
      throw const RepoException(
        RepoErrorKind.invalidArgument,
        'already recording',
      );
    }
    _recording = true;
    _recordingTimer = Timer.periodic(
      const Duration(milliseconds: 100),
      (_) => _emit(const VoiceRecordLevel(0.5)),
    );
  }

  @override
  Future<VoiceDraft> voiceRecordStop() async {
    if (!_recording) {
      throw const RepoException(
        RepoErrorKind.invalidArgument,
        'Nothing was recorded.',
      );
    }
    _recording = false;
    _recordingTimer?.cancel();
    final draft = VoiceDraft(
      id: _nextDraftId++,
      durationMs: 3000,
      waveform: List.filled(128, 120),
    );
    _voiceDrafts[draft.id] = draft;
    return draft;
  }

  @override
  void voiceRecordCancel() {
    _recording = false;
    _recordingTimer?.cancel();
  }

  @override
  void voiceDraftDiscard(int draftId) => _voiceDrafts.remove(draftId);

  @override
  Future<Message> sendVoiceMessage(
    String serverKey,
    int channelId,
    String nonce,
    int draftId,
  ) async {
    final server = _server(serverKey);
    _require(server, Permissions.sendVoiceMessages, channelId);
    final draft = _voiceDrafts[draftId];
    if (draft == null) {
      throw RepoException(
        RepoErrorKind.invalidArgument,
        'voice draft $draftId not found',
      );
    }
    final history = _history(server, channelId);
    final now = _clock();
    final messageId = _world.ids.at(now);
    final message = Message(
      id: messageId,
      channelId: channelId,
      authorId: server.selfId,
      content: '',
      createdAt: now,
      nonce: nonce,
      attachments: [
        Attachment(
          id: messageId + 1,
          filename: 'voice-message.ogg',
          contentType: 'audio/ogg',
          sizeBytes: 12000,
          kind: AttachmentKind.voiceMessage,
          durationMs: draft.durationMs,
          waveform: draft.waveform,
        ),
      ],
    );
    history.add(message);
    _voiceDrafts.remove(draftId);
    _emit(MessageCreated(serverKey, message));
    return message;
  }

  PlaybackItem? _playingItem;
  int _playbackPositionMs = 0;
  int _playbackDurationMs = 0;
  bool _playbackPlaying = false;
  double _playbackSpeed = 1.0;
  Timer? _playbackTimer;

  /// The speed last asked of the player, for tests.
  double get lastVoiceSpeed => _playbackSpeed;

  void _emitPlayback() {
    if (_playingItem case final item?) {
      _emit(
        PlaybackUpdate(
          item: item,
          positionMs: _playbackPositionMs,
          durationMs: _playbackDurationMs,
          playing: _playbackPlaying,
          buffering: false,
        ),
      );
    }
  }

  int _durationOf(PlaybackItem item) => switch (item) {
    DraftPlayback(:final draftId) => _voiceDrafts[draftId]?.durationMs ?? 0,
    // The attachment's length when this mock holds its message, else 3 s.
    AttachmentPlayback(:final serverKey, :final attachmentId) =>
      _world.servers
              .where((server) => server.key == serverKey)
              .expand((server) => server.messages.values)
              .expand((messages) => messages)
              .expand((message) => message.attachments)
              .where((attachment) => attachment.id == attachmentId)
              .firstOrNull
              ?.durationMs ??
          3000,
  };

  @override
  Future<void> voicePlay(
    PlaybackItem item,
    int positionMs,
    double speed,
  ) async {
    if (item case DraftPlayback(
      :final draftId,
    ) when !_voiceDrafts.containsKey(draftId)) {
      throw RepoException(
        RepoErrorKind.invalidArgument,
        'voice draft $draftId not found',
      );
    }
    _playbackTimer?.cancel();
    // Like the core: a voice message pauses the open media, which keeps
    // its place.
    if (_playingItem == _media && _media != null) {
      _mediaPositionMs = _playbackPositionMs;
    }
    _playingItem = item;
    _playbackDurationMs = _durationOf(item);
    _playbackPositionMs = positionMs;
    _playbackSpeed = speed;
    _playbackPlaying = true;
    _runPlayback(item);
    _emitPlayback();
  }

  /// Advances the position every 100 ms while playing.
  void _runPlayback(PlaybackItem item) {
    _playbackTimer?.cancel();
    _playbackTimer = Timer.periodic(const Duration(milliseconds: 100), (timer) {
      _playbackPositionMs += (100 * _playbackSpeed).toInt();
      if (_playbackPositionMs >= _playbackDurationMs) {
        timer.cancel();
        _playbackPlaying = false;
        // Like the core: a voice message's player ends with it, so a
        // resume after the end finds nothing; the media stays open.
        if (item != _media) _playingItem = null;
        _emit(PlaybackEnded(item));
      } else {
        _emitPlayback();
      }
    });
  }

  @override
  void voicePause() {
    _playbackTimer?.cancel();
    _playbackPlaying = false;
    _emitPlayback();
  }

  @override
  void voiceResume() {
    if (_playingItem case final item?) {
      _playbackPlaying = true;
      _runPlayback(item);
      _emitPlayback();
    }
  }

  @override
  void voiceSeek(int positionMs) {
    _playbackPositionMs = positionMs;
    _emitPlayback();
  }

  @override
  void voiceSetSpeed(double speed) {
    _playbackSpeed = speed;
    _emitPlayback();
  }

  @override
  void voiceStop() {
    _playbackTimer?.cancel();
    _playbackPlaying = false;
    _playingItem = null;
  }

  bool mockMediaSupported = true;

  /// The media open, its length, and its place while a voice message
  /// plays.
  AttachmentPlayback? _media;
  int _mediaDurationMs = 0;
  int _mediaPositionMs = 0;

  @override
  bool mediaSupported() => mockMediaSupported;

  @override
  Future<MediaHandle> mediaOpen(String serverKey, int attachmentId) async {
    if (!mediaSupported()) {
      throw const RepoException(
        RepoErrorKind.invalidArgument,
        "This computer can't play media yet",
      );
    }
    final att = _world.servers
        .where((server) => server.key == serverKey)
        .expand((server) => server.messages.values)
        .expand((messages) => messages)
        .expand((message) => message.attachments)
        .where((attachment) => attachment.id == attachmentId)
        .firstOrNull;
    final hasVideo = att == null || att.kind == AttachmentKind.video;
    final handle = MediaHandle(
      textureId: hasVideo ? (attachmentId > 0 ? attachmentId : 42) : null,
      durationMs: att?.durationMs ?? 10000,
      width: att?.width ?? 320,
      height: att?.height ?? 240,
      hasVideo: hasVideo,
    );
    mediaClose();
    _media = AttachmentPlayback(serverKey, attachmentId);
    _mediaDurationMs = handle.durationMs;
    _mediaPositionMs = 0;
    return handle;
  }

  @override
  void mediaPlay() {
    final media = _media;
    if (media == null) return;
    if (_playingItem != media) {
      _playbackTimer?.cancel();
      _playingItem = media;
      _playbackDurationMs = _mediaDurationMs;
      _playbackPositionMs = _mediaPositionMs;
      _playbackSpeed = 1.0;
    }
    // After the end it starts again, as the core's player does.
    if (_playbackPositionMs >= _playbackDurationMs) _playbackPositionMs = 0;
    _playbackPlaying = true;
    _runPlayback(media);
    _emitPlayback();
  }

  @override
  void mediaPause() {
    if (_playingItem != _media) return;
    _playbackTimer?.cancel();
    _playbackPlaying = false;
    _emitPlayback();
  }

  @override
  void mediaSeek(int positionMs) {
    if (_media == null) return;
    if (_playingItem != _media) {
      _mediaPositionMs = positionMs;
      return;
    }
    _playbackPositionMs = positionMs;
    _emitPlayback();
  }

  /// The last volume asked for, as `(volume, muted)`.
  (double, bool)? lastMediaVolume;

  @override
  void mediaSetVolume(double volume, bool muted) {
    if (_media != null) lastMediaVolume = (volume, muted);
  }

  @override
  void mediaClose() {
    if (_media == null) return;
    if (_playingItem == _media) {
      _playbackTimer?.cancel();
      _playbackPlaying = false;
      _playingItem = null;
    }
    _media = null;
  }
}

/// [domain] with some fields changed.
Domain _with(
  Domain domain, {
  DomainStatus? status,
  bool? active,
  bool? primary,
  bool? checking,
  int? publicPort,
  DomainCheck? lastCheck,
  DateTime? deactivatedAt,
  CertificateStatus? certificate,
}) => Domain(
  id: domain.id,
  hostname: domain.hostname,
  kind: domain.kind,
  publicPort: publicPort ?? domain.publicPort,
  status: status ?? domain.status,
  active: active ?? domain.active,
  primary: primary ?? domain.primary,
  problemSince: domain.problemSince,
  lastCheck: lastCheck ?? domain.lastCheck,
  checking: checking ?? domain.checking,
  users24h: domain.users24h,
  deactivatedAt: active == true ? null : deactivatedAt ?? domain.deactivatedAt,
  trustedCertificate: switch (certificate) {
    final certificate? =>
      certificate.state == CertificateState.valid && !certificate.test,
    null => domain.trustedCertificate,
  },
  certificate: certificate ?? domain.certificate,
);
