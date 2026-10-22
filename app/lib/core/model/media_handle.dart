import 'package:flutter/foundation.dart';

@immutable
class MediaHandle {
  const MediaHandle({
    this.textureId,
    required this.durationMs,
    required this.width,
    required this.height,
    required this.hasVideo,
  });

  final int? textureId;
  final int durationMs;
  final int width;
  final int height;
  final bool hasVideo;
}
