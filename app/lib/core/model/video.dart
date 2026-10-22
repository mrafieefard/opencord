import 'package:flutter/foundation.dart';

/// Video to draw in a tile (Phase 2 plan §7.11): a camera's texture, which
/// the core fills at the tile's size while the tile asks for it.
@immutable
class VideoFeed {
  const VideoFeed({
    required this.trackId,
    required this.textureId,
    required this.width,
    required this.height,
    this.mirrored = false,
  });

  final String trackId;

  /// Null when the app gave the core no engine for textures (tests).
  final int? textureId;

  /// The video's own shape, from its largest layer.
  final int width;
  final int height;

  /// This device's own camera: shown as in a mirror, sent as it is (§8).
  final bool mirrored;

  double get aspectRatio => width > 0 && height > 0 ? width / height : 16 / 9;

  @override
  bool operator ==(Object other) =>
      other is VideoFeed &&
      other.trackId == trackId &&
      other.textureId == textureId &&
      other.width == width &&
      other.height == height &&
      other.mirrored == mirrored;

  @override
  int get hashCode => Object.hash(trackId, textureId, width, height, mirrored);

  @override
  String toString() =>
      'VideoFeed($trackId, texture $textureId, ${width}x$height'
      '${mirrored ? ', mirrored' : ''})';
}

/// A tile showing a track, in physical pixels.
@immutable
class VideoWant {
  const VideoWant({
    required this.trackId,
    required this.width,
    required this.height,
  });

  final String trackId;
  final int width;
  final int height;

  @override
  bool operator ==(Object other) =>
      other is VideoWant &&
      other.trackId == trackId &&
      other.width == width &&
      other.height == height;

  @override
  int get hashCode => Object.hash(trackId, width, height);
}
