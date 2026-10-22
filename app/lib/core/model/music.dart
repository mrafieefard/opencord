/// Music settings and members blocked from music (Phase 3 plan §16.2).
library;

import 'package:flutter/foundation.dart';

/// A search source: its key, as the server knows it, and its name.
typedef MusicSource = ({String key, String label});

/// Every search source the server knows (plan §15.1), in the order shown.
const List<MusicSource> musicSources = [
  (key: 'youtube_music', label: 'YouTube Music'),
  (key: 'youtube', label: 'YouTube'),
  (key: 'soundcloud', label: 'SoundCloud'),
  (key: 'bandcamp', label: 'Bandcamp'),
  (key: 'spotify', label: 'Spotify'),
  (key: 'applemusic', label: 'Apple Music'),
  (key: 'deezer', label: 'Deezer'),
];

String musicSourceLabel(String key) => musicSources
    .firstWhere(
      (source) => source.key == key,
      orElse: () => (key: key, label: key),
    )
    .label;

@immutable
class MusicSettings {
  const MusicSettings({
    this.enabled = true,
    this.enabledSources = const [],
    this.defaultSource = 'youtube_music',
    this.allowLinks = true,
    this.allowStreams = true,
    this.maxTrack = Duration.zero,
    this.maxTracksPerMember = 20,
    this.maxPlaylistTracks = 100,
    this.queueLimit = 500,
    this.voteSkip = false,
    this.voteSkipPercent = 50,
    this.stopAfterEmpty = const Duration(minutes: 2),
    this.disconnectAfterIdle = const Duration(minutes: 5),
    this.bridgeDomain = 'primary',
    this.availableSources = const [],
    this.supported = true,
    this.configured = true,
  });

  final bool enabled;

  /// Empty: every available source.
  final List<String> enabledSources;
  final String defaultSource;
  final bool allowLinks;
  final bool allowStreams;

  /// Zero for no limit.
  final Duration maxTrack;
  final int maxTracksPerMember;
  final int maxPlaylistTracks;
  final int queueLimit;
  final bool voteSkip;
  final int voteSkipPercent;
  final Duration stopAfterEmpty;
  final Duration disconnectAfterIdle;

  /// "primary", or an active domain with a trusted certificate.
  final String bridgeDomain;

  /// Sources at least one healthy node offers now; the server sets these.
  final List<String> availableSources;

  /// Music is on in the server's configuration file and runs there.
  final bool supported;

  /// Supported, enabled, and with a Lavalink node: members see music
  /// (Phase 3 plan §19.1); the server sets it.
  final bool configured;

  /// The longest track choices (plan §16.2); zero for no limit.
  static const maxTrackChoices = [
    Duration.zero,
    Duration(minutes: 10),
    Duration(minutes: 30),
    Duration(hours: 1),
    Duration(hours: 3),
  ];

  static const stopAfterEmptyChoices = [
    Duration(seconds: 30),
    Duration(minutes: 2),
    Duration(minutes: 5),
    Duration(minutes: 15),
  ];

  static const disconnectAfterIdleChoices = [
    Duration(minutes: 1),
    Duration(minutes: 5),
    Duration(minutes: 15),
    Duration(minutes: 30),
  ];

  static const queueLimitMax = 500;

  bool allows(String source) =>
      enabledSources.isEmpty || enabledSources.contains(source);

  MusicSettings copyWith({
    bool? enabled,
    List<String>? enabledSources,
    String? defaultSource,
    bool? allowLinks,
    bool? allowStreams,
    Duration? maxTrack,
    int? maxTracksPerMember,
    int? maxPlaylistTracks,
    int? queueLimit,
    bool? voteSkip,
    int? voteSkipPercent,
    Duration? stopAfterEmpty,
    Duration? disconnectAfterIdle,
    String? bridgeDomain,
    bool? configured,
  }) => MusicSettings(
    enabled: enabled ?? this.enabled,
    enabledSources: enabledSources ?? this.enabledSources,
    defaultSource: defaultSource ?? this.defaultSource,
    allowLinks: allowLinks ?? this.allowLinks,
    allowStreams: allowStreams ?? this.allowStreams,
    maxTrack: maxTrack ?? this.maxTrack,
    maxTracksPerMember: maxTracksPerMember ?? this.maxTracksPerMember,
    maxPlaylistTracks: maxPlaylistTracks ?? this.maxPlaylistTracks,
    queueLimit: queueLimit ?? this.queueLimit,
    voteSkip: voteSkip ?? this.voteSkip,
    voteSkipPercent: voteSkipPercent ?? this.voteSkipPercent,
    stopAfterEmpty: stopAfterEmpty ?? this.stopAfterEmpty,
    disconnectAfterIdle: disconnectAfterIdle ?? this.disconnectAfterIdle,
    bridgeDomain: bridgeDomain ?? this.bridgeDomain,
    availableSources: availableSources,
    supported: supported,
    configured: configured ?? this.configured,
  );

  @override
  bool operator ==(Object other) =>
      other is MusicSettings &&
      other.enabled == enabled &&
      listEquals(other.enabledSources, enabledSources) &&
      other.defaultSource == defaultSource &&
      other.allowLinks == allowLinks &&
      other.allowStreams == allowStreams &&
      other.maxTrack == maxTrack &&
      other.maxTracksPerMember == maxTracksPerMember &&
      other.maxPlaylistTracks == maxPlaylistTracks &&
      other.queueLimit == queueLimit &&
      other.voteSkip == voteSkip &&
      other.voteSkipPercent == voteSkipPercent &&
      other.stopAfterEmpty == stopAfterEmpty &&
      other.disconnectAfterIdle == disconnectAfterIdle &&
      other.bridgeDomain == bridgeDomain &&
      listEquals(other.availableSources, availableSources) &&
      other.supported == supported &&
      other.configured == configured;

  @override
  int get hashCode => Object.hash(
    enabled,
    Object.hashAll(enabledSources),
    defaultSource,
    allowLinks,
    allowStreams,
    maxTrack,
    maxTracksPerMember,
    maxPlaylistTracks,
    queueLimit,
    voteSkip,
    voteSkipPercent,
    stopAfterEmpty,
    disconnectAfterIdle,
    bridgeDomain,
    Object.hashAll(availableSources),
    supported,
    configured,
  );
}

/// A member blocked from music, and why.
@immutable
class MusicBlock {
  const MusicBlock({
    required this.userId,
    required this.reason,
    required this.blockedBy,
    required this.at,
  });

  final int userId;
  final String reason;
  final int blockedBy;
  final DateTime at;

  @override
  bool operator ==(Object other) =>
      other is MusicBlock &&
      other.userId == userId &&
      other.reason == reason &&
      other.blockedBy == blockedBy &&
      other.at == at;

  @override
  int get hashCode => Object.hash(userId, reason, blockedBy, at);
}
