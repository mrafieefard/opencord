import 'package:flutter/foundation.dart';

/// A recorded voice message not sent yet. The core keeps its sound until
/// it is sent or discarded.
@immutable
class VoiceDraft {
  const VoiceDraft({
    required this.id,
    required this.durationMs,
    required this.waveform,
  });

  final int id;
  final int durationMs;

  /// 128 values, 0–255.
  final List<int> waveform;
}

/// What the voice player plays: a message's voice attachment, or a draft.
@immutable
sealed class PlaybackItem {
  const PlaybackItem();
}

final class AttachmentPlayback extends PlaybackItem {
  const AttachmentPlayback(this.serverKey, this.attachmentId);

  final String serverKey;
  final int attachmentId;

  @override
  bool operator ==(Object other) =>
      other is AttachmentPlayback &&
      other.serverKey == serverKey &&
      other.attachmentId == attachmentId;

  @override
  int get hashCode => Object.hash(serverKey, attachmentId);
}

final class DraftPlayback extends PlaybackItem {
  const DraftPlayback(this.draftId);

  final int draftId;

  @override
  bool operator ==(Object other) =>
      other is DraftPlayback && other.draftId == draftId;

  @override
  int get hashCode => draftId.hashCode;
}
