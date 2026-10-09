import 'package:flutter/foundation.dart';

import 'package:opencord/core/model/video.dart';
import 'package:opencord/core/model/voice.dart';

/// What a screen share shows.
enum StreamSource { screen, window }

/// A screen share in progress (Phase 2 plan §9): someone live in a voice
/// channel, at a quality within the server's maximum.
@immutable
class LiveStream {
  const LiveStream({
    required this.key,
    required this.channelId,
    required this.userId,
    this.source = StreamSource.screen,
    this.resolution = ScreenShareResolution.p720,
    this.fps = 30,
    this.hasAudio = false,
    this.viewerCount = 0,
  });

  /// `stream:<channel id>:<user id>`.
  final String key;
  final int channelId;
  final int userId;
  final StreamSource source;
  final ScreenShareResolution resolution;
  final int fps;
  final bool hasAudio;
  final int viewerCount;

  @override
  bool operator ==(Object other) =>
      other is LiveStream &&
      other.key == key &&
      other.channelId == channelId &&
      other.userId == userId &&
      other.source == source &&
      other.resolution == resolution &&
      other.fps == fps &&
      other.hasAudio == hasAudio &&
      other.viewerCount == viewerCount;

  @override
  int get hashCode => Object.hash(
    key,
    channelId,
    userId,
    source,
    resolution,
    fps,
    hasAudio,
    viewerCount,
  );

  @override
  String toString() =>
      'LiveStream($key, ${resolution.label} $fps fps, $viewerCount watching)';
}

/// The quality a screen is shared at: a preset and a frame rate, both
/// maxima (§9.2).
@immutable
class ScreenShareQuality {
  const ScreenShareQuality(this.resolution, this.fps);

  /// The frame rates a share can have (§9.2).
  static const frameRates = [15, 30, 60];

  final ScreenShareResolution resolution;
  final int fps;

  /// Whether a server allowing [maxResolution] at [maxFps] takes it.
  bool within(ScreenShareResolution maxResolution, int maxFps) =>
      resolution.index <= maxResolution.index && fps <= maxFps;

  /// How it is saved and shown: "1080p · 60 fps".
  String get label => '${resolution.label} · $fps fps';

  @override
  bool operator ==(Object other) =>
      other is ScreenShareQuality &&
      other.resolution == resolution &&
      other.fps == fps;

  @override
  int get hashCode => Object.hash(resolution, fps);

  @override
  String toString() => 'ScreenShareQuality($label)';
}

/// This device's screen share while live: its stream, what it shows, and
/// its small preview.
@immutable
class OwnScreenShare {
  const OwnScreenShare({
    required this.key,
    required this.source,
    required this.quality,
    required this.preview,
  });

  final String key;
  final StreamSource source;
  final ScreenShareQuality quality;

  /// The streamer's own, slow preview (§9.4).
  final VideoFeed preview;

  OwnScreenShare copyWith({ScreenShareQuality? quality}) => OwnScreenShare(
    key: key,
    source: source,
    quality: quality ?? this.quality,
    preview: preview,
  );

  @override
  bool operator ==(Object other) =>
      other is OwnScreenShare &&
      other.key == key &&
      other.source == source &&
      other.quality == quality &&
      other.preview == preview;

  @override
  int get hashCode => Object.hash(key, source, quality, preview);
}
