/// A voice channel's music as the app shows it (Phase 3 plan §14, §18.3):
/// tracks, the queue, and where playback is.
library;

import 'package:flutter/foundation.dart';

/// A track as the server shows it; its encoded form never leaves the
/// server.
@immutable
class MusicTrack {
  const MusicTrack({
    required this.title,
    required this.artist,
    this.duration = Duration.zero,
    this.isStream = false,
    this.isSeekable = true,
    this.source = '',
    this.sourceName = '',
    this.uri = '',
    this.artworkId = '',
  });

  final String title;
  final String artist;

  /// Zero for streams.
  final Duration duration;
  final bool isStream;
  final bool isSeekable;

  /// Lavalink's source, like "youtube".
  final String source;

  /// The source as a pill shows it: "YouTube", "SoundCloud"…
  final String sourceName;
  final String uri;

  /// Empty without artwork.
  final String artworkId;

  @override
  bool operator ==(Object other) =>
      other is MusicTrack &&
      other.title == title &&
      other.artist == artist &&
      other.duration == duration &&
      other.isStream == isStream &&
      other.isSeekable == isSeekable &&
      other.source == source &&
      other.sourceName == sourceName &&
      other.uri == uri &&
      other.artworkId == artworkId;

  @override
  int get hashCode => Object.hash(
    title,
    artist,
    duration,
    isStream,
    isSeekable,
    source,
    sourceName,
    uri,
    artworkId,
  );
}

/// A track in a channel's queue.
@immutable
class MusicQueueItem {
  const MusicQueueItem({
    required this.id,
    required this.track,
    required this.requesterId,
    required this.addedAt,
  });

  final String id;
  final MusicTrack track;
  final int requesterId;
  final DateTime addedAt;

  @override
  bool operator ==(Object other) =>
      other is MusicQueueItem &&
      other.id == id &&
      other.track == track &&
      other.requesterId == requesterId &&
      other.addedAt == addedAt;

  @override
  int get hashCode => Object.hash(id, track, requesterId, addedAt);
}

enum MusicPlayState { idle, loading, playing, paused }

/// Repeat cycles Off → Queue → Track → Off (§19.4).
enum MusicRepeat {
  off,
  queue,
  track;

  MusicRepeat get next => values[(index + 1) % values.length];
}

/// Where added tracks go.
enum MusicPlacement { end, next, now }

/// A channel's player (§14.1).
@immutable
class MusicSession {
  const MusicSession({
    required this.channelId,
    this.version = 0,
    this.state = MusicPlayState.idle,
    this.nowPlaying,
    this.queue = const [],
    this.history = const [],
    this.position = Duration.zero,
    this.anchor,
    this.repeat = MusicRepeat.off,
    this.shuffle = false,
    this.waitingForNode = false,
    this.pausedEmpty = false,
    this.skipVotes = const [],
    this.skipVotesNeeded = 1,
  });

  final int channelId;
  final int version;
  final MusicPlayState state;
  final MusicQueueItem? nowPlaying;

  /// Upcoming, in play order.
  final List<MusicQueueItem> queue;

  /// Most recent first.
  final List<MusicQueueItem> history;

  /// What is heard was [position] at [anchor] on this device's clock; it
  /// moves on while playing.
  final Duration position;
  final DateTime? anchor;
  final MusicRepeat repeat;
  final bool shuffle;

  /// Every node is unavailable: paused, the queue kept, retrying.
  final bool waitingForNode;

  /// Everyone left: paused until someone comes back.
  final bool pausedEmpty;

  /// Users who voted to skip the current track (§14.1).
  final List<int> skipVotes;
  final int skipVotesNeeded;

  bool hasVotedToSkip(int userId) => skipVotes.contains(userId);

  bool get playing => state == MusicPlayState.playing;

  /// Where playback is at [now]: moving on while playing, never past the
  /// track's end.
  Duration positionAt(DateTime now) {
    final anchor = this.anchor;
    var at = position;
    if (playing && anchor != null && now.isAfter(anchor)) {
      at += now.difference(anchor);
    }
    final length = nowPlaying?.track.duration ?? Duration.zero;
    return length > Duration.zero && at > length ? length : at;
  }

  /// The upcoming tracks' total length; streams count as nothing.
  Duration get queueLength =>
      queue.fold(Duration.zero, (sum, item) => sum + item.track.duration);

  @override
  bool operator ==(Object other) =>
      other is MusicSession &&
      other.channelId == channelId &&
      other.version == version &&
      other.state == state &&
      other.nowPlaying == nowPlaying &&
      listEquals(other.queue, queue) &&
      listEquals(other.history, history) &&
      other.position == position &&
      other.anchor == anchor &&
      other.repeat == repeat &&
      other.shuffle == shuffle &&
      other.waitingForNode == waitingForNode &&
      other.pausedEmpty == pausedEmpty &&
      listEquals(other.skipVotes, skipVotes) &&
      other.skipVotesNeeded == skipVotesNeeded;

  @override
  int get hashCode => Object.hash(
    channelId,
    version,
    state,
    nowPlaying,
    Object.hashAll(queue),
    Object.hashAll(history),
    position,
    anchor,
    repeat,
    shuffle,
    waitingForNode,
    pausedEmpty,
    Object.hashAll(skipVotes),
    skipVotesNeeded,
  );
}

/// What plays in a channel, for the now-playing card (§19.3).
@immutable
class MusicNowPlaying {
  const MusicNowPlaying({
    required this.channelId,
    required this.title,
    required this.artist,
    this.artworkId = '',
    this.playing = true,
    this.sourceName = '',
  });

  final int channelId;
  final String title;
  final String artist;
  final String artworkId;
  final bool playing;

  /// "YouTube", "SoundCloud"…
  final String sourceName;

  @override
  bool operator ==(Object other) =>
      other is MusicNowPlaying &&
      other.channelId == channelId &&
      other.title == title &&
      other.artist == artist &&
      other.artworkId == artworkId &&
      other.playing == playing &&
      other.sourceName == sourceName;

  @override
  int get hashCode =>
      Object.hash(channelId, title, artist, artworkId, playing, sourceName);
}

/// A short activity line in the music panel (§14.4).
@immutable
class MusicNotice {
  const MusicNotice({
    required this.channelId,
    required this.text,
    required this.at,
  });

  final int channelId;
  final String text;
  final DateTime at;
}

@immutable
class MusicSearchResult {
  const MusicSearchResult({required this.id, required this.track});

  /// What to add; good for 15 minutes, for this user only.
  final String id;
  final MusicTrack track;
}

/// A playlist a link loaded: one card with "Add all".
typedef MusicPlaylist = ({String name, List<String> resultIds});

@immutable
class MusicSearchResults {
  const MusicSearchResults({this.results = const [], this.playlist});

  final List<MusicSearchResult> results;
  final MusicPlaylist? playlist;
}

/// How this person hears music, on every server (§19.8); kept on this
/// device and never sent.
@immutable
class MusicListening {
  const MusicListening({
    this.volume = 100,
    this.muted = false,
    this.duckWhileTalking = false,
  });

  /// 0–200 %.
  final int volume;
  final bool muted;

  /// −10 dB while someone talks.
  final bool duckWhileTalking;

  /// The linear gain for playback, tuned with a perceptual audio curve
  /// (quadratic taper) and Spotify-like loudness normalization (-14 LUFS target):
  /// - 100 % volume maps to -6 dB (gain 0.50), matching Spotify's normalization
  ///   for standard commercial masters so voice chat is not drowned out.
  /// - 1 % volume maps to -86 dB (gain ~0.00005), giving a whisper-quiet minimum.
  /// - 200 % volume maps to +6 dB (gain 2.0), providing max boost when needed.
  static double volumeToGain(int volume) {
    if (volume <= 0) return 0.0;
    final clamped = volume.clamp(0, 200);
    final normalized = clamped / 200.0;
    return 2.0 * normalized * normalized;
  }

  MusicListening copyWith({int? volume, bool? muted, bool? duckWhileTalking}) =>
      MusicListening(
        volume: volume ?? this.volume,
        muted: muted ?? this.muted,
        duckWhileTalking: duckWhileTalking ?? this.duckWhileTalking,
      );

  @override
  bool operator ==(Object other) =>
      other is MusicListening &&
      other.volume == volume &&
      other.muted == muted &&
      other.duckWhileTalking == duckWhileTalking;

  @override
  int get hashCode => Object.hash(volume, muted, duckWhileTalking);
}

/// "3:07", or "1:02:07" from an hour.
String formatTrackTime(Duration duration) {
  final seconds = duration.inSeconds;
  final s = (seconds % 60).toString().padLeft(2, '0');
  if (seconds >= 3600) {
    final m = (seconds ~/ 60 % 60).toString().padLeft(2, '0');
    return '${seconds ~/ 3600}:$m:$s';
  }
  return '${seconds ~/ 60}:$s';
}
