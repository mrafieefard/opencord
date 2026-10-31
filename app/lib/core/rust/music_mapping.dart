import 'package:opencord/core/model/music.dart';
import 'package:opencord/core/model/music_session.dart';
import 'package:opencord/src/rust/api/types.dart' as core;

/// The core's music settings as the app's model.
MusicSettings musicSettingsFrom(core.MusicSettings settings) => MusicSettings(
  enabled: settings.enabled,
  enabledSources: settings.enabledSources,
  defaultSource: settings.defaultSource,
  allowLinks: settings.allowLinks,
  allowStreams: settings.allowStreams,
  maxTrack: Duration(milliseconds: settings.maxTrackMs.toInt()),
  maxTracksPerMember: settings.maxTracksPerMember,
  maxPlaylistTracks: settings.maxPlaylistTracks,
  queueLimit: settings.queueLimit,
  voteSkip: settings.voteSkip,
  voteSkipPercent: settings.voteSkipPercent,
  stopAfterEmpty: Duration(seconds: settings.stopAfterEmptyS),
  disconnectAfterIdle: Duration(seconds: settings.disconnectAfterIdleS),
  bridgeDomain: settings.bridgeDomain,
  availableSources: settings.availableSources,
  supported: settings.supported,
  configured: settings.configured,
  hasNodes: settings.hasNodes,
  localSongs: settings.localSongs,
);

/// Only what differs from [before]; the rest stays as the server has it.
core.MusicSettingsChanges musicSettingsChanges(
  MusicSettings before,
  MusicSettings after,
) {
  T? changed<T>(T Function(MusicSettings s) field) =>
      field(before) == field(after) ? null : field(after);
  final sourcesChanged =
      before.enabledSources.join(',') != after.enabledSources.join(',');
  return core.MusicSettingsChanges(
    enabled: changed((s) => s.enabled),
    enabledSources: sourcesChanged ? after.enabledSources : null,
    defaultSource: changed((s) => s.defaultSource),
    allowLinks: changed((s) => s.allowLinks),
    allowStreams: changed((s) => s.allowStreams),
    maxTrackMs: changed((s) => s.maxTrack) == null
        ? null
        : BigInt.from(after.maxTrack.inMilliseconds),
    maxTracksPerMember: changed((s) => s.maxTracksPerMember),
    maxPlaylistTracks: changed((s) => s.maxPlaylistTracks),
    queueLimit: changed((s) => s.queueLimit),
    voteSkip: changed((s) => s.voteSkip),
    voteSkipPercent: changed((s) => s.voteSkipPercent),
    stopAfterEmptyS: changed((s) => s.stopAfterEmpty)?.inSeconds,
    disconnectAfterIdleS: changed((s) => s.disconnectAfterIdle)?.inSeconds,
    bridgeDomain: changed((s) => s.bridgeDomain),
  );
}

MusicBlock musicBlockFrom(core.MusicBlock block) => MusicBlock(
  userId: block.userId,
  reason: block.reason,
  blockedBy: block.blockedBy,
  at: DateTime.fromMillisecondsSinceEpoch(block.createdAtMs),
);

MusicTrack musicTrackFrom(core.MusicTrack track) => MusicTrack(
  title: track.title,
  artist: track.artist,
  duration: Duration(milliseconds: track.durationMs.toInt()),
  isStream: track.isStream,
  isSeekable: track.isSeekable,
  source: track.source,
  sourceName: track.sourceDisplayName,
  uri: track.uri,
  artworkId: track.artworkId,
  localSongId: track.localSongId.toInt(),
  removable: track.removable,
);

MusicQueueItem musicItemFrom(core.MusicQueueItem item) => MusicQueueItem(
  id: item.itemId,
  track: musicTrackFrom(item.track),
  requesterId: item.requesterId,
  addedAt: DateTime.fromMillisecondsSinceEpoch(item.addedAtMs),
);

MusicSession musicSessionFrom(core.MusicSession session) => MusicSession(
  channelId: session.channelId,
  version: session.version.toInt(),
  state: switch (session.state) {
    core.MusicPlayState.idle => MusicPlayState.idle,
    core.MusicPlayState.loading => MusicPlayState.loading,
    core.MusicPlayState.playing => MusicPlayState.playing,
    core.MusicPlayState.paused => MusicPlayState.paused,
  },
  nowPlaying: session.nowPlaying == null
      ? null
      : musicItemFrom(session.nowPlaying!),
  queue: [for (final item in session.queue) musicItemFrom(item)],
  history: [for (final item in session.history) musicItemFrom(item)],
  position: Duration(milliseconds: session.positionMs.toInt()),
  anchor: DateTime.fromMillisecondsSinceEpoch(session.anchorMs),
  repeat: switch (session.repeat) {
    core.MusicRepeat.off => MusicRepeat.off,
    core.MusicRepeat.queue => MusicRepeat.queue,
    core.MusicRepeat.track => MusicRepeat.track,
  },
  shuffle: session.shuffle,
  waitingForNode: session.waitingForNode,
  pausedEmpty: session.pausedEmpty,
  skipVotes: [for (final id in session.skipVotes) id.toInt()],
  skipVotesNeeded: session.skipVotesNeeded,
);

MusicSearchResults musicSearchResultsFrom(core.MusicSearchResults found) =>
    MusicSearchResults(
      results: [
        for (final result in found.results)
          MusicSearchResult(
            id: result.resultId,
            track: musicTrackFrom(result.track),
          ),
      ],
      playlist: found.playlist == null
          ? null
          : (name: found.playlist!.name, resultIds: found.playlist!.resultIds),
    );

core.MusicRepeat musicRepeatTo(MusicRepeat repeat) => switch (repeat) {
  MusicRepeat.off => core.MusicRepeat.off,
  MusicRepeat.queue => core.MusicRepeat.queue,
  MusicRepeat.track => core.MusicRepeat.track,
};

core.MusicPlacement musicPlacementTo(MusicPlacement placement) =>
    switch (placement) {
      MusicPlacement.end => core.MusicPlacement.end,
      MusicPlacement.next => core.MusicPlacement.next,
      MusicPlacement.now => core.MusicPlacement.now,
    };

LocalSong localSongFrom(core.LocalSong song) => LocalSong(
  id: song.id.toInt(),
  title: song.title,
  artist: song.artist,
  duration: Duration(milliseconds: song.durationMs.toInt()),
  sizeBytes: song.sizeBytes.toInt(),
  filename: song.filename,
  uploaderId: song.uploaderId.toInt(),
  createdAt: DateTime.fromMillisecondsSinceEpoch(song.createdAtMs.toInt()),
);
