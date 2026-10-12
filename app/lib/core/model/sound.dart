import 'package:flutter/foundation.dart';

/// The longest sound, and the window the trim editor takes (Phase 2 plan
/// §11.2).
const maxSoundDuration = Duration(seconds: 5);

/// One of a server's soundboard sounds (Phase 2 plan §11).
@immutable
class Sound {
  const Sound({
    required this.id,
    required this.name,
    required this.sha256,
    required this.duration,
    this.emoji,
    this.volume = 100,
    this.uploaderId,
    this.createdAt,
  });

  final int id;
  final String name;
  final String? emoji;

  /// 0–100 %.
  final int volume;

  /// The file's SHA-256 in lowercase hex.
  final String sha256;
  final Duration duration;
  final int? uploaderId;
  final DateTime? createdAt;

  /// How it is played on its own server.
  String get ref => 'server:$sha256';

  /// How it is played on another server.
  String get externalRef => 'external:$sha256';

  Sound copyWith({
    String? name,
    String? emoji,
    int? volume,
    bool clearEmoji = false,
  }) => Sound(
    id: id,
    name: name ?? this.name,
    emoji: clearEmoji ? null : emoji ?? this.emoji,
    volume: volume ?? this.volume,
    sha256: sha256,
    duration: duration,
    uploaderId: uploaderId,
    createdAt: createdAt,
  );

  @override
  bool operator ==(Object other) =>
      other is Sound &&
      other.id == id &&
      other.name == name &&
      other.emoji == emoji &&
      other.volume == volume &&
      other.sha256 == sha256 &&
      other.duration == duration &&
      other.uploaderId == uploaderId &&
      other.createdAt == createdAt;

  @override
  int get hashCode => Object.hash(
    id,
    name,
    emoji,
    volume,
    sha256,
    duration,
    uploaderId,
    createdAt,
  );
}

/// A sound every client has (§11.8), unless the server turned them off.
@immutable
class DefaultSound {
  const DefaultSound({
    required this.id,
    required this.name,
    required this.emoji,
    required this.duration,
  });

  final String id;
  final String name;
  final String emoji;
  final Duration duration;

  String get ref => 'default:$id';

  @override
  bool operator ==(Object other) =>
      other is DefaultSound &&
      other.id == id &&
      other.name == name &&
      other.emoji == emoji &&
      other.duration == duration;

  @override
  int get hashCode => Object.hash(id, name, emoji, duration);
}

/// Someone played a sound in a voice channel (§11.5): everyone there hears
/// it, and their soundboard ring shows for its length (§11.6).
@immutable
class SoundPlay {
  const SoundPlay({
    required this.channelId,
    required this.userId,
    required this.soundRef,
    required this.name,
    required this.duration,
    required this.startedAt,
    this.emoji,
    this.volume = 100,
  });

  final int channelId;
  final int userId;
  final String soundRef;
  final String name;
  final String? emoji;
  final int volume;
  final Duration duration;
  final DateTime startedAt;

  @override
  bool operator ==(Object other) =>
      other is SoundPlay &&
      other.channelId == channelId &&
      other.userId == userId &&
      other.soundRef == soundRef &&
      other.name == name &&
      other.emoji == emoji &&
      other.volume == volume &&
      other.duration == duration &&
      other.startedAt == startedAt;

  @override
  int get hashCode => Object.hash(
    channelId,
    userId,
    soundRef,
    name,
    emoji,
    volume,
    duration,
    startedAt,
  );
}

/// A file opened in the trim editor (§11.3): its length and a waveform of
/// peaks, 0–1.
@immutable
class SoundDraft {
  const SoundDraft({
    required this.id,
    required this.duration,
    required this.waveform,
  });

  final int id;
  final Duration duration;
  final List<double> waveform;
}

/// The window of a draft to preview or upload.
@immutable
class SoundWindow {
  const SoundWindow({
    required this.draftId,
    required this.start,
    required this.length,
    this.normalize = true,
    this.volume = 100,
  });

  final int draftId;
  final Duration start;
  final Duration length;

  /// Bring it to −18 LUFS (§11.3).
  final bool normalize;

  /// 0–100 %.
  final int volume;
}
