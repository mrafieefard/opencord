import 'package:flutter/foundation.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/model/misc.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/model/video.dart';
import 'package:opencord/core/model/voice.dart';
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
    this.camera = false,
    this.screenShare = false,
    this.announcementChannels = false,
    this.readStates = false,
    this.replies = false,
  });

  static const everything = RepoCapabilities(
    reactions: true,
    pins: true,
    voice: true,
    camera: true,
    screenShare: true,
    announcementChannels: true,
    readStates: true,
    replies: true,
  );

  final bool reactions;
  final bool pins;
  final bool voice;
  final bool camera;
  final bool screenShare;
  final bool announcementChannels;

  /// Unread counts that survive restarts.
  final bool readStates;

  /// Messages can answer an earlier one.
  final bool replies;
}

/// The current user as far as the UI is concerned.
@immutable
class LocalIdentity {
  const LocalIdentity({
    required this.displayName,
    required this.fingerprint,
    this.publicKeyHex = '',
  });

  final String displayName;

  /// Like `ABCD-EFGH-IJKL-MNOP`.
  final String fingerprint;

  /// The Ed25519 public key, lowercase hex.
  final String publicKeyHex;
}

/// A freshly made identity, shown before it is used (Phase 1 §9.1): its
/// fingerprint and the backup text that holds its secret.
@immutable
class NewIdentity {
  const NewIdentity({
    required this.fingerprint,
    required this.publicKeyHex,
    required this.backup,
  });

  final String fingerprint;
  final String publicKeyHex;
  final String backup;
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
  /// The camera: refused by the user or the system, missing, unusable, not
  /// supported here yet, or full in this channel (Phase 2 V5).
  cameraDenied,
  cameraMissing,
  cameraUnsupported,
  cameraLimit,

  /// Screen sharing: the picker closed without a choice, refused, not
  /// supported here yet; above the server's maximum; a full stream (Phase 2
  /// V6).
  screenCancelled,
  screenDenied,
  screenUnsupported,
  qualityLimit,
  streamFull,
  unauthorized,
  forbidden,
  notFound,
  invalidArgument,
  rateLimited,
  conflict,
  voiceChannelFull,
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

  /// Why the saved identity could not be read (the system keyring is
  /// missing or locked), or null. Nothing can go on until it can.
  RepoException? get identityUnavailable;

  /// Reads the saved identity again, after [identityUnavailable].
  Future<void> reloadIdentity();

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

  /// A channel's pinned messages; empty without [RepoCapabilities.pins].
  Future<List<Message>> fetchPins(String serverKey, int channelId);

  Future<Channel> createChannel(
    String serverKey, {
    required ChannelKind kind,
    required String name,
    int? parentId,
    String? topic,
    bool private = false,
  });

  /// Changes what is given; a [parentId] of 0 moves the channel out of
  /// its category, an empty [topic] clears it. [bitrate] (bits per
  /// second), [userLimit] and [textInVoice] are for voice channels.
  Future<Channel> updateChannel(
    String serverKey,
    int channelId, {
    String? name,
    String? topic,
    int? parentId,
    int? bitrate,
    int? userLimit,
    bool? textInVoice,
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

  /// The identity's secret key as a backup text to keep safe (§8.1).
  Future<String> exportIdentityBackup();

  /// Replaces this device's identity with one from a backup.
  Future<void> importIdentityBackup(String backup);

  /// A new identity for onboarding; nothing is saved until
  /// [adoptIdentity].
  Future<NewIdentity> generateIdentity();

  /// Makes the identity in [backup] this device's, under [displayName],
  /// and connects to the saved servers (Phase 1 §9.1).
  Future<void> adoptIdentity(String backup, {required String displayName});

  /// The newest message read in a channel, for servers that do not keep
  /// read states themselves (§6).
  void markRead(String serverKey, int channelId, int messageId);

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

  /// Saves the server's voice, video and soundboard settings (Phase 2
  /// plan §5.2); needs Manage server.
  Future<void> updateVoiceSettings(String serverKey, VoiceSettings settings);

  Future<void> joinVoice(String serverKey, int channelId);

  Future<void> leaveVoice();

  /// The camera that turning the camera on starts from now on, by its
  /// device id; `null` for the system's first.
  void chooseCamera(String? deviceId);

  /// Turning the camera on may fail (a [RepoException] says why); it then
  /// stays off.
  Future<void> setVoiceSelf({
    bool? muted,
    bool? deafened,
    bool? camera,
    bool? screensharing,
  });

  /// The tiles showing video now and their sizes in physical pixels; video
  /// not named is neither received nor decoded (Phase 2 plan §6, §7.11).
  void setVideoWants(List<VideoWant> wants);

  /// Microphones and speakers the system offers (Phase 2 plan §7.6).
  Future<AudioDeviceList> audioDevices();

  /// Devices, input mode and volumes for voice; at start and on every
  /// change.
  void applyAudio(AudioConfig config);

  /// The push-to-talk key went down or up.
  void setPushToTalk(bool held);

  /// The priority speaker key went down or up.
  void setPrioritySpeaker(bool held);

  /// Report the microphone's level ([InputLevelChanged]) while a meter
  /// shows it.
  void setLevelMeter(bool on);

  /// Hear yourself through the whole voice chain, in voice or not.
  Future<void> setMicTest(bool on);

  /// The noise suppression to start with on a first run: High when this
  /// computer runs it easily, Standard otherwise (Phase 2 plan §7.3).
  Future<NoiseSuppression> recommendedNoiseSuppression();

  /// Binds the hotkeys system-wide where the system allows (Phase 2 plan
  /// §7.13), replacing the ones bound before; says whether they work while
  /// Opencord is in the background. Toggles pressed there arrive as
  /// [HotkeyPressed].
  Future<HotkeySupport> setHotkeys(Map<HotkeyAction, String> bindings);

  /// How loud someone sounds on this device, 0–200 %.
  void setUserVolume(String serverKey, int userId, int volume);

  /// Silences someone on this device only.
  void setUserLocalMute(String serverKey, int userId, bool muted);
}

/// Overridden at startup with the mock or the Rust-core repository.
final repositoryProvider = Provider<OpencordRepository>(
  (ref) => throw StateError('repositoryProvider must be overridden'),
);
