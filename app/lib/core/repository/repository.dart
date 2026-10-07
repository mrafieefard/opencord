import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/model/misc.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/repository/events.dart';

export 'package:opencord/core/repository/events.dart';

/// UI features that need protocol support Phase 1 does not have yet. The
/// mock has them all; the Rust core reports what it can do, and the UI
/// hides the rest (docs/decisions.md D13).
@immutable
class RepoCapabilities {
  const RepoCapabilities({
    this.reactions = false,
    this.pins = false,
    this.voice = false,
    this.announcementChannels = false,
    this.readStates = false,
  });

  static const everything = RepoCapabilities(
    reactions: true,
    pins: true,
    voice: true,
    announcementChannels: true,
    readStates: true,
  );

  final bool reactions;
  final bool pins;
  final bool voice;
  final bool announcementChannels;

  /// Unread counts that survive restarts.
  final bool readStates;
}

/// The current user as far as the UI is concerned.
@immutable
class LocalIdentity {
  const LocalIdentity({required this.displayName, required this.fingerprint});

  final String displayName;

  /// Like `ABCD-EFGH-IJKL-MNOP`.
  final String fingerprint;
}

sealed class AddServerResult {
  const AddServerResult();
}

final class ServerAdded extends AddServerResult {
  const ServerAdded(this.server);

  final ServerSummary server;
}

/// The certificate is not pinned and not from a public authority: ask the
/// user, then call [OpencordRepository.trustFingerprint] and add again.
final class ServerNeedsTrust extends AddServerResult {
  const ServerNeedsTrust({required this.address, required this.fingerprint});

  final String address;

  /// Lowercase hex SHA-256 of the certificate.
  final String fingerprint;
}

enum RepoErrorKind {
  unauthorized,
  forbidden,
  notFound,
  invalidArgument,
  rateLimited,
  conflict,
  notConnected,
  timeout,
  connection,
  fingerprintMismatch,
  rejected,
  other,
}

/// A failed request, with a message fit for showing to the user.
class RepoException implements Exception {
  const RepoException(this.kind, this.message, {this.retryAfter});

  final RepoErrorKind kind;
  final String message;

  /// Set when rate limited.
  final Duration? retryAfter;

  @override
  String toString() => message;
}

/// Self presence choices; [invisible] shows as offline to everyone else.
enum SelfPresence { online, idle, doNotDisturb, invisible }

extension SelfPresenceShape on SelfPresence {
  Presence get shown => switch (this) {
    SelfPresence.online => Presence.online,
    SelfPresence.idle => Presence.idle,
    SelfPresence.doNotDisturb => Presence.doNotDisturb,
    SelfPresence.invisible => Presence.offline,
  };

  String get label => switch (this) {
    SelfPresence.online => 'Online',
    SelfPresence.idle => 'Idle',
    SelfPresence.doNotDisturb => 'Do not disturb',
    SelfPresence.invisible => 'Invisible',
  };
}

/// Everything the UI asks of the outside world. Implemented by the mock
/// (plan §11) and by the Rust core.
abstract interface class OpencordRepository {
  RepoCapabilities get capabilities;

  /// Every event from every server, in order.
  Stream<RepoEvent> get events;

  /// Connects to the saved servers; events start flowing.
  void start();

  void dispose();

  LocalIdentity? get identity;

  List<ServerSummary> get servers;

  Future<AddServerResult> addServer(String linkOrAddress, {String? claimToken});

  Future<void> trustFingerprint(String address, String fingerprint);

  Future<void> removeServer(String serverKey);

  /// Skips the backoff wait and tries to connect right away.
  Future<void> retryNow(String serverKey);

  Future<List<Message>> fetchMessages(
    String serverKey,
    int channelId, {
    int? before,
    int limit = 50,
  });

  Future<Message> sendMessage(
    String serverKey,
    int channelId,
    String content, {
    required String nonce,
    int? replyToId,
  });

  Future<Message> editMessage(
    String serverKey,
    int channelId,
    int messageId,
    String content,
  );

  Future<void> deleteMessage(String serverKey, int channelId, int messageId);

  Future<void> startTyping(String serverKey, int channelId);

  Future<void> toggleReaction(
    String serverKey,
    int channelId,
    int messageId,
    String emoji,
  );

  Future<void> setPinned(
    String serverKey,
    int channelId,
    int messageId, {
    required bool pinned,
  });

  Future<Channel> createChannel(
    String serverKey, {
    required ChannelKind kind,
    required String name,
    int? parentId,
    String? topic,
    bool private = false,
  });

  Future<Channel> updateChannel(
    String serverKey,
    int channelId, {
    String? name,
    String? topic,
    int? parentId,
  });

  Future<void> deleteChannel(String serverKey, int channelId);

  /// New positions, as `channelId → position`.
  Future<void> reorderChannels(String serverKey, Map<int, int> positions);

  Future<void> setOverwrite(
    String serverKey,
    int channelId,
    PermissionOverwrite overwrite,
  );

  Future<void> deleteOverwrite(
    String serverKey,
    int channelId,
    OverwriteTargetKind kind,
    int targetId,
  );

  Future<Role> createRole(
    String serverKey, {
    required String name,
    Permissions permissions = Permissions.none,
    bool hoist = false,
    bool mentionable = false,
  });

  Future<Role> updateRole(
    String serverKey,
    int roleId, {
    String? name,
    Permissions? permissions,
    bool? hoist,
    bool? mentionable,
  });

  Future<void> deleteRole(String serverKey, int roleId);

  /// The listed roles take the positions they already hold, from lowest to
  /// highest in list order.
  Future<void> reorderRoles(String serverKey, List<int> roleIds);

  Future<void> addMemberRole(String serverKey, int userId, int roleId);

  Future<void> removeMemberRole(String serverKey, int userId, int roleId);

  Future<void> kickMember(String serverKey, int userId, {String? reason});

  Future<void> banMember(String serverKey, int userId, {String? reason});

  Future<void> unbanMember(String serverKey, int userId);

  Future<List<Ban>> fetchBans(String serverKey);

  Future<void> updateNickname(String serverKey, int userId, String? nickname);

  Future<void> updateDisplayName(String displayName);

  Future<void> updatePresence(SelfPresence presence);

  Future<Invite> createInvite(
    String serverKey, {
    Duration? expiresIn,
    int? maxUses,
  });

  Future<List<Invite>> fetchInvites(String serverKey);

  Future<void> revokeInvite(String serverKey, String code);

  Future<void> updateServer(
    String serverKey, {
    String? name,
    String? description,
    bool? openJoin,
  });

  Future<void> joinVoice(String serverKey, int channelId);

  Future<void> leaveVoice();

  Future<void> setVoiceSelf({
    bool? muted,
    bool? deafened,
    bool? camera,
    bool? screensharing,
  });
}

/// Overridden at startup with the mock or the Rust-core repository.
final repositoryProvider = Provider<OpencordRepository>(
  (ref) => throw StateError('repositoryProvider must be overridden'),
);
