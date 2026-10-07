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

/// Someone in a voice channel (§4.2, §4.10). Voice is mock-only in
/// Phase 1.
@immutable
class VoiceParticipant {
  const VoiceParticipant({
    required this.userId,
    this.muted = false,
    this.deafened = false,
    this.camera = false,
    this.screensharing = false,
  });

  final int userId;
  final bool muted;
  final bool deafened;
  final bool camera;
  final bool screensharing;

  VoiceParticipant copyWith({
    bool? muted,
    bool? deafened,
    bool? camera,
    bool? screensharing,
  }) => VoiceParticipant(
    userId: userId,
    muted: muted ?? this.muted,
    deafened: deafened ?? this.deafened,
    camera: camera ?? this.camera,
    screensharing: screensharing ?? this.screensharing,
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
