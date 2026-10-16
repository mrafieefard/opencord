/// Music in the mock repository (Phase 3 plan §14–15): a search that
/// makes up tracks, and each voice channel's player with the server's
/// queue rules, as far as the UI needs them.
library;

import 'package:opencord/core/model/music_session.dart';
import 'package:opencord/core/repository/repository.dart';

/// History kept, as on the server.
const _historyMax = 50;

/// Previous restarts the track when it is further in than this.
const _restartAfter = Duration(seconds: 5);

/// Made-up tracks, by result id, and each channel's player.
class MockMusic {
  MockMusic(this._clock);

  final DateTime Function() _clock;
  final _results = <String, MusicTrack>{};
  final _players = <int, MockPlayer>{};
  var _nextId = 0;

  String _id(String prefix) => '$prefix-${_nextId++}';

  /// Three tracks named after the query, or a playlist of 12 for a query
  /// with "playlist" in it; "live" finds a stream, "nothing" nothing.
  MusicSearchResults search(String query, String source) {
    final words = query.trim();
    if (words.contains('nothing')) return const MusicSearchResults();
    final sourceName = switch (source) {
      'soundcloud' => 'SoundCloud',
      'bandcamp' => 'Bandcamp',
      _ => 'YouTube',
    };
    MusicSearchResult result(String title, {bool stream = false}) {
      final id = _id('result');
      final track = MusicTrack(
        title: title,
        artist: 'Mock Artist',
        duration: stream
            ? Duration.zero
            : Duration(seconds: 150 + title.length),
        isStream: stream,
        isSeekable: !stream,
        source: source.isEmpty ? 'youtube' : source,
        sourceName: sourceName,
        uri: 'https://example.com/${Uri.encodeComponent(title)}',
      );
      _results[id] = track;
      return MusicSearchResult(id: id, track: track);
    }

    if (words.contains('playlist')) {
      final tracks = [for (var n = 1; n <= 12; n++) result('$words $n')];
      return MusicSearchResults(
        results: tracks,
        playlist: (
          name: words,
          resultIds: [for (final track in tracks) track.id],
        ),
      );
    }
    final stream = words.contains('live');
    return MusicSearchResults(
      results: [
        for (var n = 1; n <= 3; n++) result('$words $n', stream: stream),
      ],
    );
  }

  /// [count] made-up tracks' result ids, for load checks.
  List<String> debugResults(int count) => [
    for (var n = 1; n <= count; n++)
      () {
        final id = _id('result');
        _results[id] = MusicTrack(
          title: 'Track $n',
          artist: 'Mock Artist',
          duration: Duration(seconds: 120 + n % 180),
          source: 'youtube',
          sourceName: 'YouTube',
        );
        return id;
      }(),
  ];

  MockPlayer _player(int channelId) =>
      _players.putIfAbsent(channelId, () => MockPlayer(channelId, _clock));

  MusicSession session(int channelId) => _player(channelId).session;

  MusicSession enqueue(
    int channelId,
    int userId,
    List<String> resultIds, {
    List<String> historyItemIds = const [],
    String? beforeItemId,
    MusicPlacement placement = MusicPlacement.end,
  }) {
    final player = _player(channelId);
    final again = [
      for (final id in historyItemIds)
        player.history(id) ??
            (throw const RepoException(
              RepoErrorKind.notFound,
              "That track is no longer in what's recently played.",
            )),
    ];
    final items = [
      for (final id in resultIds)
        MusicQueueItem(
          id: _id('item'),
          track:
              _results[id] ??
              (throw const RepoException(
                RepoErrorKind.notFound,
                'That search result expired: search again.',
              )),
          requesterId: userId,
          addedAt: _clock(),
        ),
      for (final track in again)
        MusicQueueItem(
          id: _id('item'),
          track: track,
          requesterId: userId,
          addedAt: _clock(),
        ),
    ];
    player.add(items, beforeItemId: beforeItemId, placement: placement);
    return player.session;
  }

  MusicSession update(int channelId, void Function(MockPlayer player) change) {
    final player = _player(channelId);
    change(player);
    return player.session;
  }

  /// Plays the next track of every channel whose track reached its end.
  List<MusicSession> finishEnded() => [
    for (final player in _players.values)
      if (player.ended) (player..finish()).session,
  ];
}

/// One channel's player.
class MockPlayer {
  MockPlayer(this.channelId, this._clock);

  final int channelId;
  final DateTime Function() _clock;
  MusicQueueItem? _now;
  final _queue = <MusicQueueItem>[];
  final _history = <MusicQueueItem>[];
  var _paused = false;
  var _position = Duration.zero;
  DateTime? _anchor;
  var _repeat = MusicRepeat.off;
  var _shuffle = false;
  var _version = 0;

  Duration get _at {
    final anchor = _anchor;
    if (_paused || _now == null || anchor == null) return _position;
    return _position + _clock().difference(anchor);
  }

  bool get ended {
    final now = _now;
    return now != null &&
        !now.track.isStream &&
        !_paused &&
        _at >= now.track.duration;
  }

  MusicSession get session => MusicSession(
    channelId: channelId,
    version: _version,
    state: _now == null
        ? MusicPlayState.idle
        : _paused
        ? MusicPlayState.paused
        : MusicPlayState.playing,
    nowPlaying: _now,
    queue: List.unmodifiable(_queue),
    history: List.unmodifiable(_history),
    position: _at,
    anchor: _clock(),
    repeat: _repeat,
    shuffle: _shuffle,
  );

  void _changed() => _version++;

  void _play(MusicQueueItem? item, {Duration from = Duration.zero}) {
    _now = item;
    _position = from;
    _anchor = _clock();
    if (item == null) _paused = false;
  }

  void _toHistory(MusicQueueItem item) {
    _history.insert(0, item);
    if (_history.length > _historyMax) _history.removeLast();
  }

  void add(
    List<MusicQueueItem> items, {
    String? beforeItemId,
    MusicPlacement placement = MusicPlacement.end,
  }) {
    if (items.isEmpty) return;
    switch (placement) {
      case MusicPlacement.now:
        final current = _now;
        if (current != null) _toHistory(current);
        _play(items.first);
        _queue.insertAll(0, items.skip(1));
      case MusicPlacement.next:
        _queue.insertAll(0, items);
      case MusicPlacement.end:
        final at = beforeItemId == null
            ? -1
            : _queue.indexWhere((item) => item.id == beforeItemId);
        _queue.insertAll(at < 0 ? _queue.length : at, items);
    }
    if (_now == null) _play(_queue.removeAt(0));
    _changed();
  }

  void setPaused(bool paused) {
    if (_now == null || paused == _paused) return;
    _position = _at;
    _anchor = _clock();
    _paused = paused;
    _changed();
  }

  void next() {
    final current = _now;
    if (current == null) return;
    _toHistory(current);
    if (_repeat == MusicRepeat.queue) _queue.add(current);
    _play(_queue.isEmpty ? null : _queue.removeAt(0));
    _changed();
  }

  /// The track ended by itself: repeat applies.
  void finish() {
    final current = _now;
    if (current != null && _repeat == MusicRepeat.track) {
      _play(current);
      _changed();
      return;
    }
    next();
  }

  void previous() {
    final current = _now;
    if (current != null && _at > _restartAfter || _history.isEmpty) {
      _play(current);
    } else {
      if (current != null) _queue.insert(0, current);
      _play(_history.removeAt(0));
    }
    _changed();
  }

  void seek(Duration position) {
    final current = _now;
    if (current == null || !current.track.isSeekable) return;
    _position = position < Duration.zero
        ? Duration.zero
        : position > current.track.duration
        ? current.track.duration
        : position;
    _anchor = _clock();
    _changed();
  }

  void stop() {
    _queue.clear();
    final current = _now;
    if (current != null) _toHistory(current);
    _play(null);
    _changed();
  }

  void setMode({MusicRepeat? repeat, bool? shuffle}) {
    if (repeat != null) _repeat = repeat;
    if (shuffle != null && shuffle != _shuffle) {
      _shuffle = shuffle;
      if (shuffle) _queue.shuffle();
    }
    _changed();
  }

  void move(String itemId, String? beforeItemId) {
    final from = _queue.indexWhere((item) => item.id == itemId);
    if (from < 0 || itemId == beforeItemId) return;
    final item = _queue.removeAt(from);
    final to = beforeItemId == null
        ? -1
        : _queue.indexWhere((other) => other.id == beforeItemId);
    _queue.insert(to < 0 ? _queue.length : to, item);
    _changed();
  }

  void remove(List<String> itemIds) {
    _queue.removeWhere((item) => itemIds.contains(item.id));
    if (_now != null && itemIds.contains(_now!.id)) next();
    _changed();
  }

  void clear() {
    _queue.clear();
    _changed();
  }

  /// A recently played track, by its item id.
  MusicTrack? history(String itemId) =>
      _history.where((item) => item.id == itemId).firstOrNull?.track;

  /// With [expected], nothing happens when another track plays.
  void expect(String? expected) {
    if (expected != null && _now?.id != expected) {
      throw const RepoException(
        RepoErrorKind.musicStale,
        'The track changed meanwhile.',
      );
    }
  }
}
