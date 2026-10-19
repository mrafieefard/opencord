import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/commands.dart';
import 'package:opencord/core/model/lavalink.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/model/misc.dart';
import 'package:opencord/core/model/music.dart';
import 'package:opencord/core/model/music_session.dart';
import 'package:opencord/core/model/network.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/model/snapshot.dart';
import 'package:opencord/core/model/sound.dart';
import 'package:opencord/core/model/storage.dart';
import 'package:opencord/core/model/stream.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/model/video.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/src/rust/api/types.dart' as core show PlaybackItem;

/// Something that happened on one server. Both repositories (mock and Rust
/// core) emit these, so the UI cannot tell them apart.
sealed class RepoEvent {
  const RepoEvent(this.serverKey);

  final String serverKey;
}

/// The saved server list changed (added, removed, renamed or reordered).
/// The current user's identity was set or changed (onboarding, a new
/// display name, an imported backup).
final class IdentityChanged extends RepoEvent {
  const IdentityChanged() : super('');
}

final class ServersChanged extends RepoEvent {
  const ServersChanged() : super('');
}

final class ConnectionChanged extends RepoEvent {
  const ConnectionChanged(super.serverKey, this.status);

  final ConnectionStatus status;
}

final class Ready extends RepoEvent {
  const Ready(super.serverKey, this.snapshot);

  final ReadySnapshot snapshot;
}

final class MessageCreated extends RepoEvent {
  const MessageCreated(super.serverKey, this.message);

  final Message message;
}

final class MessageUpdated extends RepoEvent {
  const MessageUpdated(super.serverKey, this.message);

  final Message message;
}

final class MessageDeleted extends RepoEvent {
  const MessageDeleted(super.serverKey, this.channelId, this.messageId);

  final int channelId;
  final int messageId;
}

final class ChannelUpserted extends RepoEvent {
  const ChannelUpserted(super.serverKey, this.channel);

  final Channel channel;
}

final class ChannelDeleted extends RepoEvent {
  const ChannelDeleted(super.serverKey, this.channelId);

  final int channelId;
}

final class RoleUpserted extends RepoEvent {
  const RoleUpserted(super.serverKey, this.role);

  final Role role;
}

final class RoleDeleted extends RepoEvent {
  const RoleDeleted(super.serverKey, this.roleId);

  final int roleId;
}

/// A member joined or changed (nickname, roles, profile).
final class MemberUpserted extends RepoEvent {
  const MemberUpserted(super.serverKey, this.member, {this.joined = false});

  final Member member;

  /// True when they just joined.
  final bool joined;
}

final class MemberLeft extends RepoEvent {
  const MemberLeft(super.serverKey, this.userId);

  final int userId;
}

final class PresenceChanged extends RepoEvent {
  const PresenceChanged(
    super.serverKey,
    this.userId,
    this.presence, {
    this.activity,
  });

  final int userId;
  final Presence presence;
  final String? activity;
}

final class TypingStarted extends RepoEvent {
  const TypingStarted(super.serverKey, this.channelId, this.userId);

  final int channelId;
  final int userId;
}

final class ServerInfoChanged extends RepoEvent {
  const ServerInfoChanged(super.serverKey, this.info);

  final ServerInfo info;
}

/// The current user's own permissions changed.
final class PermissionsChanged extends RepoEvent {
  const PermissionsChanged(super.serverKey, this.server, this.channels);

  final Permissions server;
  final Map<int, Permissions> channels;
}

/// Voice participants of one channel.
final class VoiceChanged extends RepoEvent {
  const VoiceChanged(super.serverKey, this.channelId, this.participants);

  final int channelId;
  final List<VoiceParticipant> participants;
}

/// This device's voice channel on a server changed without being asked:
/// a moderator moved or disconnected it, it lost the channel, another
/// device took the call over, or getting back in after a reconnect failed.
/// [channelId] is null when this device is no longer in voice there.
final class OwnVoiceChanged extends RepoEvent {
  const OwnVoiceChanged(super.serverKey, this.channelId);

  final int? channelId;
}

/// Domains & network changed, while this device watches it.
final class NetworkStatusChanged extends RepoEvent {
  const NetworkStatusChanged(super.serverKey, this.info);

  final NetworkInfo info;
}

/// A Lavalink node changed, or was removed ([node] null), while this
/// device watches the nodes (Phase 3 plan §12.8).
final class LavalinkNodeChanged extends RepoEvent {
  const LavalinkNodeChanged(
    super.serverKey, {
    required this.nodeId,
    required this.node,
    required this.channelsPlaying,
  });

  final int nodeId;
  final LavalinkNode? node;
  final int channelsPlaying;
}

/// A line for the node events.
final class LavalinkNodeEventReceived extends RepoEvent {
  const LavalinkNodeEventReceived(super.serverKey, this.event);

  final LavalinkNodeEvent event;
}

/// A connection test this device started moved on.
final class LavalinkTestProgressed extends RepoEvent {
  const LavalinkTestProgressed(super.serverKey, this.progress);

  final LavalinkTestProgress progress;
}

/// The music settings changed (Phase 3 plan §16.2).
final class MusicSettingsChanged extends RepoEvent {
  const MusicSettingsChanged(super.serverKey, this.settings);

  final MusicSettings settings;
}

/// A voice channel's player changed (§14.3): to those connected to it and
/// those who subscribed.
final class MusicSessionChanged extends RepoEvent {
  const MusicSessionChanged(super.serverKey, this.session);

  final MusicSession session;
}

/// What plays in a channel changed, or stopped ([nowPlaying] null), for
/// everyone who can see it (§19.3).
final class MusicNowPlayingChanged extends RepoEvent {
  const MusicNowPlayingChanged(
    super.serverKey, {
    required this.channelId,
    required this.nowPlaying,
  });

  final int channelId;
  final MusicNowPlaying? nowPlaying;
}

/// A line for a channel's music panel (§14.4).
final class MusicNoticeReceived extends RepoEvent {
  const MusicNoticeReceived(super.serverKey, this.notice);

  final MusicNotice notice;
}

/// Where the server is reached changed (Phase 3 plan §7.5).
final class ServerAddressesChanged extends RepoEvent {
  const ServerAddressesChanged(super.serverKey, this.addresses);

  final ServerAddresses addresses;
}

/// Something the owner or admins should know (Phase 3).
final class ServerNoticeReceived extends RepoEvent {
  const ServerNoticeReceived(super.serverKey, this.notice);

  final ServerNotice notice;
}

final class VoiceSettingsChanged extends RepoEvent {
  const VoiceSettingsChanged(super.serverKey, this.settings);

  final VoiceSettings settings;
}

/// Who is speaking right now in this device's voice channel on the server,
/// this device's user too (Phase 2 plan §7.5).
final class SpeakingChanged extends RepoEvent {
  const SpeakingChanged(super.serverKey, this.speaking);

  final Set<int> speaking;
}

/// How this device's voice connection is doing (Phase 2 plan §7.14).
final class VoiceConnectionChanged extends RepoEvent {
  const VoiceConnectionChanged(super.serverKey, this.channelId, this.status);

  final int channelId;
  final VoiceConnectionStatus status;
}

/// The microphones and speakers plugged in changed.
final class AudioDevicesChanged extends RepoEvent {
  const AudioDevicesChanged(this.devices) : super('');

  final AudioDeviceList devices;
}

/// The chosen microphone or speaker is missing; the system's default stands
/// in (Phase 2 plan §7.6).
final class AudioDeviceFellBack extends RepoEvent {
  const AudioDeviceFellBack({required this.output, required this.device})
    : super('');

  final bool output;

  /// The device standing in.
  final String device;
}

/// No microphone or speaker could be opened.
final class AudioDeviceFailed extends RepoEvent {
  const AudioDeviceFailed({required this.output, required this.message})
    : super('');

  final bool output;
  final String message;
}

/// The microphone's level after processing, in dBFS, about 20 times a
/// second while a meter is open.
/// How this device's voice connection is doing, once a second (Phase 2
/// §12).
final class VoiceStatsChanged extends RepoEvent {
  const VoiceStatsChanged(super.serverKey, this.channelId, this.stats);

  final int channelId;
  final VoiceStats stats;
}

final class InputLevelChanged extends RepoEvent {
  const InputLevelChanged(this.dbfs) : super('');

  final double dbfs;
}

/// The microphone heard someone speak while muted (at most every 30 s), for
/// the "You're muted" reminder (Phase 2 plan §12).
final class SpokeWhileMuted extends RepoEvent {
  const SpokeWhileMuted() : super('');
}

/// High noise suppression could not keep up on this computer; Standard took
/// over (Phase 2 plan §7.3).
final class NoiseSuppressionFellBack extends RepoEvent {
  const NoiseSuppressionFellBack() : super('');
}

/// A hotkey for a toggle was pressed outside Opencord (Phase 2 plan §7.13).
final class HotkeyPressed extends RepoEvent {
  const HotkeyPressed(this.action) : super('');

  final HotkeyAction action;
}

/// Someone's camera, or the screen they share, in a voice channel this
/// device is in (Phase 2 V5, V6).
final class VideoTrackAdded extends RepoEvent {
  const VideoTrackAdded(
    super.serverKey,
    this.channelId,
    this.userId,
    this.feed, {
    this.screen = false,
  });

  final int channelId;
  final int userId;
  final VideoFeed feed;

  /// Their screen share's video, not their camera.
  final bool screen;
}

final class VideoTrackRemoved extends RepoEvent {
  const VideoTrackRemoved(
    super.serverKey,
    this.channelId,
    this.userId,
    this.trackId,
  );

  final int channelId;
  final int userId;
  final String trackId;
}

/// This device's camera: its preview while on, null once off.
final class OwnCameraChanged extends RepoEvent {
  const OwnCameraChanged(this.feed) : super('');

  final VideoFeed? feed;
}

/// This device's camera stopped by itself: unplugged, failed, or the server
/// no longer allows it.
final class CameraStopped extends RepoEvent {
  const CameraStopped(this.message) : super('');

  final String message;
}

/// Someone went live in a voice channel this user can see (Phase 2 V6).
final class StreamStarted extends RepoEvent {
  const StreamStarted(super.serverKey, this.stream);

  final LiveStream stream;
}

/// A stream's quality, sound or viewer count changed.
final class StreamChanged extends RepoEvent {
  const StreamChanged(super.serverKey, this.stream);

  final LiveStream stream;
}

final class StreamEnded extends RepoEvent {
  const StreamEnded(super.serverKey, this.key, this.channelId);

  final String key;
  final int channelId;
}

/// Who watches this user's own stream; only the streamer hears it.
final class StreamViewersChanged extends RepoEvent {
  const StreamViewersChanged(super.serverKey, this.key, this.viewerIds);

  final String key;
  final List<int> viewerIds;
}

/// This device's screen share: live with its preview, or null once over.
final class OwnScreenShareChanged extends RepoEvent {
  const OwnScreenShareChanged(this.share) : super('');

  final OwnScreenShare? share;
}

/// This device's screen share ended by itself: the window closed, the
/// screen went, or the server stopped it.
final class ScreenShareStopped extends RepoEvent {
  const ScreenShareStopped(this.message) : super('');

  final String message;
}

/// A soundboard sound was added or edited (Phase 2 V8).
final class SoundUpserted extends RepoEvent {
  const SoundUpserted(super.serverKey, this.sound);

  final Sound sound;
}

final class SoundDeleted extends RepoEvent {
  const SoundDeleted(super.serverKey, this.soundId);

  final int soundId;
}

/// Someone played a sound in the voice channel this device is in.
final class SoundPlayed extends RepoEvent {
  const SoundPlayed(super.serverKey, this.play);

  final SoundPlay play;
}

/// The chat commands members can use changed (Phase 3 plan §17.1).
final class CommandsChanged extends RepoEvent {
  const CommandsChanged(super.serverKey, this.commands);

  final CommandList commands;
}

/// A command's reply only this device sees (§17.3).
final class EphemeralMessageReceived extends RepoEvent {
  const EphemeralMessageReceived(super.serverKey, this.note);

  final EphemeralNote note;
}

/// Progress of an outgoing file upload.
final class UploadProgressEvent extends RepoEvent {
  const UploadProgressEvent(
    super.serverKey, {
    required this.channelId,
    required this.nonce,
    required this.fileIndex,
    required this.sentBytes,
    required this.totalBytes,
    required this.phase,
    this.error,
  });

  final int channelId;
  final String nonce;
  final int fileIndex;
  final int sentBytes;
  final int totalBytes;
  final UploadPhase phase;
  final String? error;
}

/// Progress of a file being downloaded.
final class DownloadProgressEvent extends RepoEvent {
  const DownloadProgressEvent(
    super.serverKey, {
    required this.attachmentId,
    required this.receivedBytes,
    required this.totalBytes,
    required this.done,
    this.error,
  });

  final int attachmentId;
  final int receivedBytes;
  final int totalBytes;
  final bool done;
  final String? error;
}

/// Upload limits or blocked extensions changed on the server.
final class UploadSettingsEvent extends RepoEvent {
  const UploadSettingsEvent(super.serverKey, this.settings);

  final UploadSettings settings;
}

/// Storage backends or a file move changed on the server (owner only).
final class StorageEvent extends RepoEvent {
  const StorageEvent(super.serverKey, this.storage);

  final StorageInfo storage;
}

/// Voice message recording events (Milestone A6).
final class VoiceRecordLevel extends RepoEvent {
  const VoiceRecordLevel(this.rms) : super('');

  final double rms;
}

final class VoiceRecordStopped extends RepoEvent {
  const VoiceRecordStopped(this.reason) : super('');

  final VoiceStopReason reason;
}

/// Voice message playback events (Milestone A6).
final class PlaybackUpdate extends RepoEvent {
  const PlaybackUpdate({
    required this.item,
    required this.positionMs,
    required this.durationMs,
    required this.playing,
    required this.buffering,
  }) : super('');

  final core.PlaybackItem item;
  final int positionMs;
  final int durationMs;
  final bool playing;
  final bool buffering;
}

final class PlaybackEnded extends RepoEvent {
  const PlaybackEnded(this.item) : super('');

  final core.PlaybackItem item;
}

final class PlaybackFailed extends RepoEvent {
  const PlaybackFailed({required this.item, required this.message}) : super('');

  final core.PlaybackItem item;
  final String message;
}
