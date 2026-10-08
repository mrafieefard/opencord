import 'dart:async';
import 'dart:math';

import 'package:clock/clock.dart';

import 'package:opencord/core/mock/mock_world.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/model/misc.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/model/user.dart';
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
  ({String server, int channel})? _voice;
  var _selfVoice = const VoiceParticipant(userId: 0);
  LocalIdentity? _identity;
  RepoException? _identityUnavailable;
  var _disposed = false;

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
    if (kind.isTextLike) {
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
  }) async {
    final server = _server(serverKey);
    _require(server, Permissions.createInvite);
    await _latency();
    const alphabet = 'abcdefghjkmnpqrstuvwxyz23456789';
    final code = List.generate(
      8,
      (_) => alphabet[_random.nextInt(alphabet.length)],
    ).join();
    final now = _clock();
    final invite = Invite(
      code: code,
      link: 'opencord://${server.key}/invite/$code#fp=${server.fingerprint}',
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
  Future<AudioDeviceList> audioDevices() async => devices;

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

  @override
  void setUserVolume(String serverKey, int userId, int volume) {
    final (_, muted) = listening[(serverKey, userId)] ?? (100, false);
    listening[(serverKey, userId)] = (volume, muted);
  }

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
}
