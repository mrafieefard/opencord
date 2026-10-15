import 'package:opencord/core/model/music.dart';
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
