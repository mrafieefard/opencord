import 'package:flutter/foundation.dart';

enum AttachmentKind { file, image, video, audio, voiceMessage }

/// An uploaded file attached to a message.
@immutable
class Attachment {
  const Attachment({
    required this.id,
    required this.filename,
    required this.contentType,
    required this.sizeBytes,
    required this.kind,
    this.width,
    this.height,
    this.durationMs,
    this.waveform = const [],
    this.hasThumbnail = false,
    this.thumbnailWidth,
    this.thumbnailHeight,
    this.animated = false,
  });

  final int id;
  final String filename;
  final String contentType;
  final int sizeBytes;
  final AttachmentKind kind;
  final int? width;
  final int? height;
  final int? durationMs;
  final List<int> waveform;
  final bool hasThumbnail;
  final int? thumbnailWidth;
  final int? thumbnailHeight;
  final bool animated;

  int get size => sizeBytes;
}

/// What may be uploaded to a server.
@immutable
class UploadSettings {
  const UploadSettings({
    required this.blockedExtensions,
    required this.voiceMessageMaxSeconds,
    required this.maxFileBytes,
    required this.maxAttachments,
  });

  final List<String> blockedExtensions;
  final int voiceMessageMaxSeconds;
  final int maxFileBytes;
  final int maxAttachments;

  static const fallback = UploadSettings(
    blockedExtensions: [],
    voiceMessageMaxSeconds: 300,
    maxFileBytes: 4294967296,
    maxAttachments: 10,
  );
}

enum UploadPhase {
  preparing,
  uploading,
  processing,
  sending,
  done,
  failed,
  cancelled,
}

/// An upload in flight for a pending message.
@immutable
class PendingUpload {
  const PendingUpload({
    required this.name,
    required this.size,
    this.sent = 0,
    this.phase = UploadPhase.preparing,
    this.error,
    this.preview,
  });

  final String name;
  final int size;
  final int sent;
  final UploadPhase phase;
  final String? error;
  final Uint8List? preview;

  PendingUpload copyWith({
    String? name,
    int? size,
    int? sent,
    UploadPhase? phase,
    String? Function()? error,
    Uint8List? Function()? preview,
  }) => PendingUpload(
    name: name ?? this.name,
    size: size ?? this.size,
    sent: sent ?? this.sent,
    phase: phase ?? this.phase,
    error: error == null ? this.error : error(),
    preview: preview == null ? this.preview : preview(),
  );
}

enum StageProblem { tooLarge, blocked, tooMany }

/// A file staged in the composer before sending.
@immutable
class StagedFile {
  const StagedFile({
    required this.id,
    required this.name,
    required this.size,
    this.path,
    this.bytes,
    this.preview,
    this.problem,
  });

  final String id;
  final String name;
  final int size;
  final String? path;
  final Uint8List? bytes;
  final Uint8List? preview;
  final StageProblem? problem;

  StagedFile copyWith({
    String? id,
    String? name,
    int? size,
    String? Function()? path,
    Uint8List? Function()? bytes,
    Uint8List? Function()? preview,
    StageProblem? Function()? problem,
  }) => StagedFile(
    id: id ?? this.id,
    name: name ?? this.name,
    size: size ?? this.size,
    path: path == null ? this.path : path(),
    bytes: bytes == null ? this.bytes : bytes(),
    preview: preview == null ? this.preview : preview(),
    problem: problem == null ? this.problem : problem(),
  );
}
