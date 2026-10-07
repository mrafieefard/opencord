import 'package:flutter/foundation.dart';

import 'package:opencord/core/model/user.dart';

@immutable
class Invite {
  const Invite({
    required this.code,
    required this.link,
    required this.createdAt,
    this.createdBy,
    this.maxUses,
    this.uses = 0,
    this.expiresAt,
  });

  final String code;

  /// `opencord://host:port/invite/CODE#fp=...`, ready to share.
  final String link;
  final int? createdBy;
  final DateTime createdAt;
  final int? maxUses;
  final int uses;
  final DateTime? expiresAt;
}

@immutable
class Ban {
  const Ban({
    required this.user,
    required this.createdAt,
    this.reason,
    this.bannedBy,
  });

  final User user;
  final String? reason;
  final int? bannedBy;
  final DateTime createdAt;
}

/// Someone in a voice channel (§4.2, §4.10).
@immutable
class VoiceParticipant {
  const VoiceParticipant({
    required this.userId,
    this.muted = false,
    this.deafened = false,
    this.camera = false,
    this.screensharing = false,
    this.serverMuted = false,
    this.serverDeafened = false,
    this.suppressed = false,
  });

  final int userId;

  /// Self mute and deafen.
  final bool muted;
  final bool deafened;
  final bool camera;
  final bool screensharing;

  /// By a moderator, for everyone.
  final bool serverMuted;
  final bool serverDeafened;

  /// Cannot speak here: no Speak permission, or the AFK channel.
  final bool suppressed;

  /// Whether nobody hears them, for any reason.
  bool get silenced => muted || deafened || serverMuted || suppressed;

  VoiceParticipant copyWith({
    bool? muted,
    bool? deafened,
    bool? camera,
    bool? screensharing,
    bool? serverMuted,
    bool? serverDeafened,
    bool? suppressed,
  }) => VoiceParticipant(
    userId: userId,
    muted: muted ?? this.muted,
    deafened: deafened ?? this.deafened,
    camera: camera ?? this.camera,
    screensharing: screensharing ?? this.screensharing,
    serverMuted: serverMuted ?? this.serverMuted,
    serverDeafened: serverDeafened ?? this.serverDeafened,
    suppressed: suppressed ?? this.suppressed,
  );

  @override
  bool operator ==(Object other) =>
      other is VoiceParticipant &&
      other.userId == userId &&
      other.muted == muted &&
      other.deafened == deafened &&
      other.camera == camera &&
      other.screensharing == screensharing &&
      other.serverMuted == serverMuted &&
      other.serverDeafened == serverDeafened &&
      other.suppressed == suppressed;

  @override
  int get hashCode => Object.hash(
    userId,
    muted,
    deafened,
    camera,
    screensharing,
    serverMuted,
    serverDeafened,
    suppressed,
  );
}

/// Where the current user stopped reading a channel, and what has arrived
/// since.
@immutable
class ReadState {
  const ReadState({this.lastReadId = 0, this.unread = 0, this.mentions = 0});

  /// The newest message seen; 0 when nothing was read.
  final int lastReadId;
  final int unread;
  final int mentions;

  static const empty = ReadState();

  @override
  bool operator ==(Object other) =>
      other is ReadState &&
      other.lastReadId == lastReadId &&
      other.unread == unread &&
      other.mentions == mentions;

  @override
  int get hashCode => Object.hash(lastReadId, unread, mentions);
}
