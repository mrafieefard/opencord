import 'package:opencord/core/model/attachment.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/model/misc.dart';
import 'package:opencord/core/rust/command_mapping.dart';
import 'package:opencord/core/rust/network_mapping.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/model/snapshot.dart';
import 'package:opencord/core/model/sound.dart';
import 'package:opencord/core/model/storage.dart';
import 'package:opencord/core/model/stream.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/src/rust/api/types.dart' as core;

// The Rust core's shapes (generated bindings) to the app's, and back.
// Permission sets cross as `i64`: the same 64 bits, so ADMINISTRATOR (bit
// 63) arrives negative and stays set.

/// The furthest DateTime can go either side of the epoch, in ms.
const _furthest = 8640000000000000;

/// Servers can send any number; one no date can hold is kept to the
/// furthest one, so a bad value cannot stop that server's events.
DateTime _time(int milliseconds) => DateTime.fromMillisecondsSinceEpoch(
  milliseconds.clamp(-_furthest, _furthest),
);

Message messageFrom(core.Message message) => Message(
  id: message.id,
  channelId: message.channelId,
  authorId: message.authorId,
  content: message.content,
  createdAt: _time(message.createdAtMs),
  editedAt: switch (message.editedAtMs) {
    final at? => _time(at),
    null => null,
  },
  nonce: message.nonce,
  command: message.command,
  commandUserId: message.commandUserId,
  commandState: commandRunStateFrom(message.commandState),
  attachments: [for (final a in message.attachments) attachmentFrom(a)],
);

Attachment attachmentFrom(core.Attachment a) => Attachment(
  id: a.id,
  filename: a.filename,
  contentType: a.contentType,
  sizeBytes: a.sizeBytes.toInt(),
  kind: switch (a.kind) {
    core.AttachmentKind.file => AttachmentKind.file,
    core.AttachmentKind.image => AttachmentKind.image,
    core.AttachmentKind.video => AttachmentKind.video,
    core.AttachmentKind.audio => AttachmentKind.audio,
    core.AttachmentKind.voiceMessage => AttachmentKind.voiceMessage,
  },
  width: a.width,
  height: a.height,
  durationMs: a.durationMs,
  waveform: a.waveform,
  hasThumbnail: a.hasThumbnail,
  thumbnailWidth: a.thumbnailWidth,
  thumbnailHeight: a.thumbnailHeight,
  animated: a.animated,
);

ChannelKind channelKindFrom(core.ChannelKind kind) => switch (kind) {
  core.ChannelKind.text => ChannelKind.text,
  core.ChannelKind.voice => ChannelKind.voice,
  core.ChannelKind.category => ChannelKind.category,
  core.ChannelKind.plugin => ChannelKind.plugin,
};

PluginChannelState pluginChannelStateFrom(core.PluginChannelState state) =>
    switch (state) {
      core.PluginChannelState.available => PluginChannelState.available,
      core.PluginChannelState.pluginStopped => PluginChannelState.pluginStopped,
      core.PluginChannelState.typeGone => PluginChannelState.typeGone,
    };

/// Announcement channels are not in Phase 1's protocol; they go as text.
core.ChannelKind channelKindTo(ChannelKind kind) => switch (kind) {
  ChannelKind.text || ChannelKind.announcement => core.ChannelKind.text,
  ChannelKind.voice => core.ChannelKind.voice,
  ChannelKind.category => core.ChannelKind.category,
  ChannelKind.plugin => core.ChannelKind.plugin,
};

OverwriteTargetKind _targetFrom(core.OverwriteTargetKind kind) =>
    switch (kind) {
      core.OverwriteTargetKind.role => OverwriteTargetKind.role,
      core.OverwriteTargetKind.member => OverwriteTargetKind.member,
    };

core.OverwriteTargetKind targetTo(OverwriteTargetKind kind) => switch (kind) {
  OverwriteTargetKind.role => core.OverwriteTargetKind.role,
  OverwriteTargetKind.member => core.OverwriteTargetKind.member,
};

PermissionOverwrite overwriteFrom(core.PermissionOverwrite overwrite) =>
    PermissionOverwrite(
      targetKind: _targetFrom(overwrite.targetKind),
      targetId: overwrite.targetId,
      allow: Permissions(overwrite.allow),
      deny: Permissions(overwrite.deny),
    );

core.PermissionOverwrite overwriteTo(PermissionOverwrite overwrite) =>
    core.PermissionOverwrite(
      targetKind: targetTo(overwrite.targetKind),
      targetId: overwrite.targetId,
      allow: overwrite.allow.bits,
      deny: overwrite.deny.bits,
    );

Channel channelFrom(core.Channel channel) => Channel(
  id: channel.id,
  kind: channelKindFrom(channel.kind),
  name: channel.name,
  topic: channel.topic,
  parentId: channel.parentId,
  position: channel.position,
  overwrites: [for (final o in channel.overwrites) overwriteFrom(o)],
  bitrate: channel.kind == core.ChannelKind.voice
      ? channel.bitrate
      : Channel.defaultBitrate,
  userLimit: channel.userLimit,
  textInVoice: channel.kind != core.ChannelKind.voice || channel.textInVoice,
  pluginId: channel.pluginId,
  typeKey: channel.typeKey,
  pluginState: channel.pluginState == null
      ? null
      : pluginChannelStateFrom(channel.pluginState!),
);

VoiceParticipant participantFrom(core.VoiceState state) => VoiceParticipant(
  userId: state.userId,
  muted: state.selfMute,
  deafened: state.selfDeaf,
  camera: state.selfVideo,
  screensharing: state.selfStream,
  serverMuted: state.serverMute,
  serverDeafened: state.serverDeaf,
  suppressed: state.suppress,
);

/// Who is in each voice channel, in the order the server listed them.
Map<int, List<VoiceParticipant>> voiceFrom(Iterable<core.VoiceState> states) {
  final voice = <int, List<VoiceParticipant>>{};
  for (final state in states) {
    if (state.channelId case final channelId?) {
      (voice[channelId] ??= []).add(participantFrom(state));
    }
  }
  return voice;
}

ScreenShareResolution _resolutionFrom(core.ScreenShareResolution value) =>
    switch (value) {
      core.ScreenShareResolution.p480 => ScreenShareResolution.p480,
      core.ScreenShareResolution.p720 => ScreenShareResolution.p720,
      core.ScreenShareResolution.p1080 => ScreenShareResolution.p1080,
      core.ScreenShareResolution.p1440 => ScreenShareResolution.p1440,
      core.ScreenShareResolution.source => ScreenShareResolution.source,
    };

core.ScreenShareResolution _resolutionTo(ScreenShareResolution value) =>
    switch (value) {
      ScreenShareResolution.p480 => core.ScreenShareResolution.p480,
      ScreenShareResolution.p720 => core.ScreenShareResolution.p720,
      ScreenShareResolution.p1080 => core.ScreenShareResolution.p1080,
      ScreenShareResolution.p1440 => core.ScreenShareResolution.p1440,
      ScreenShareResolution.source => core.ScreenShareResolution.source,
    };

/// The app's screen share preset as the core's.
core.ScreenShareResolution resolutionTo(ScreenShareResolution value) =>
    _resolutionTo(value);

LiveStream streamFrom(core.ScreenStream stream) => LiveStream(
  key: stream.streamKey,
  channelId: stream.channelId,
  userId: stream.userId,
  source: streamSourceFrom(stream.sourceKind),
  resolution: _resolutionFrom(stream.resolution),
  fps: stream.fps,
  hasAudio: stream.hasAudio,
  viewerCount: stream.viewerCount,
);

StreamSource streamSourceFrom(core.StreamSourceKind kind) => switch (kind) {
  core.StreamSourceKind.screen => StreamSource.screen,
  core.StreamSourceKind.window => StreamSource.window,
};

VoiceSettings voiceSettingsFrom(core.VoiceSettings settings) => VoiceSettings(
  screenShareMaxResolution: _resolutionFrom(settings.screenShareMaxResolution),
  screenShareMaxFps: settings.screenShareMaxFps,
  maxStreamViewers: settings.maxStreamViewers,
  cameraAllowed: settings.cameraAllowed,
  maxCameraParticipants: settings.maxCameraParticipants,
  maxVoiceBitrate: settings.maxVoiceBitrate,
  afkChannelId: settings.afkChannelId,
  afkTimeout: Duration(seconds: settings.afkTimeoutS),
  soundboardEnabled: settings.soundboardEnabled,
  allowDefaultSounds: settings.allowDefaultSounds,
  allowExternalSounds: settings.allowExternalSounds,
  soundCooldown: Duration(seconds: settings.soundCooldownS),
  maxSounds: settings.maxSounds,
);

/// Every setting, so the server ends up with exactly [settings]; no AFK
/// channel goes as 0.
core.VoiceSettingsChanges voiceSettingsTo(VoiceSettings settings) =>
    core.VoiceSettingsChanges(
      screenShareMaxResolution: _resolutionTo(
        settings.screenShareMaxResolution,
      ),
      screenShareMaxFps: settings.screenShareMaxFps,
      maxStreamViewers: settings.maxStreamViewers,
      cameraAllowed: settings.cameraAllowed,
      maxCameraParticipants: settings.maxCameraParticipants,
      maxVoiceBitrate: settings.maxVoiceBitrate,
      afkChannelId: settings.afkChannelId ?? 0,
      afkTimeoutS: settings.afkTimeout.inSeconds,
      soundboardEnabled: settings.soundboardEnabled,
      allowDefaultSounds: settings.allowDefaultSounds,
      allowExternalSounds: settings.allowExternalSounds,
      soundCooldownS: settings.soundCooldown.inSeconds,
      maxSounds: settings.maxSounds,
    );

User userFrom(core.User user) => User(
  id: user.id,
  displayName: user.displayName,
  publicKeyHex: user.publicKeyHex,
  fingerprint: user.fingerprint,
  avatarHash: user.avatarHash,
);

Member memberFrom(core.Member member) => Member(
  user: userFrom(member.user),
  nickname: member.nickname,
  roleIds: [for (final id in member.roleIds) id.toInt()],
  joinedAt: _time(member.joinedAtMs),
);

Role roleFrom(core.Role role) => Role(
  id: role.id,
  name: role.name,
  color: role.color,
  position: role.position,
  permissions: Permissions(role.permissions),
  hoist: role.hoist,
  mentionable: role.mentionable,
  maxUploadBytes: role.maxUploadBytes?.toInt(),
  pluginId: role.pluginId,
);

ServerInfo serverInfoFrom(core.ServerInfo info) => ServerInfo(
  serverIdHex: info.serverIdHex,
  name: info.name,
  description: info.description,
  ownerId: info.ownerId,
  openJoin: info.openJoin,
  everyoneRoleId: info.everyoneRoleId,
  iconHash: info.iconHash,
);

ServerSummary serverFrom(core.Server server) => ServerSummary(
  key: server.key,
  name: server.name,
  fingerprint: server.fingerprint,
  userId: server.userId,
);

Invite inviteFrom(core.Invite invite) => Invite(
  code: invite.code,
  link: invite.link,
  webLink: invite.webLink,
  domainId: invite.domainId,
  createdBy: invite.createdBy,
  createdAt: _time(invite.createdAtMs),
  maxUses: invite.maxUses,
  uses: invite.uses,
  expiresAt: switch (invite.expiresAtMs) {
    final at? => _time(at),
    null => null,
  },
);

Ban banFrom(core.Ban ban) => Ban(
  user: userFrom(ban.user),
  reason: ban.reason,
  bannedBy: ban.bannedBy,
  createdAt: _time(ban.createdAtMs),
);

Presence presenceFrom(core.PresenceStatus status) => switch (status) {
  core.PresenceStatus.online => Presence.online,
  core.PresenceStatus.idle => Presence.idle,
  core.PresenceStatus.dnd => Presence.doNotDisturb,
  core.PresenceStatus.offline => Presence.offline,
};

/// Maps self presence to the core presence status.
core.PresenceStatus presenceTo(SelfPresence presence) => switch (presence) {
  SelfPresence.online => core.PresenceStatus.online,
  SelfPresence.idle => core.PresenceStatus.idle,
  SelfPresence.doNotDisturb => core.PresenceStatus.dnd,
};

FailureReason failureFrom(core.FailureReason reason) => switch (reason) {
  core.FailureReason.rejected => FailureReason.rejected,
  core.FailureReason.kicked => FailureReason.kicked,
  core.FailureReason.banned => FailureReason.banned,
  core.FailureReason.fingerprintChanged => FailureReason.fingerprintChanged,
  core.FailureReason.incompatible => FailureReason.incompatible,
};

/// [now] turns the core's "retry in" into the time it happens.
ConnectionStatus connectionFrom(
  core.ConnectionState state, {
  required DateTime now,
}) => switch (state) {
  core.ConnectionState_Connecting() => const ConnectionStatus.connecting(),
  core.ConnectionState_Connected() => const ConnectionStatus.connected(),
  core.ConnectionState_Reconnecting(:final attempt, :final retryInMs) =>
    ConnectionStatus.reconnecting(
      attempt: attempt,
      retryAt: now.add(Duration(milliseconds: retryInMs)),
    ),
  core.ConnectionState_Failed(
    :final reason,
    :final message,
    :final expectedFingerprint,
    :final presentedFingerprint,
  ) =>
    ConnectionStatus.failed(
      failureFrom(reason),
      message,
      expectedFingerprint: expectedFingerprint,
      presentedFingerprint: presentedFingerprint,
    ),
};

/// Phase 1's Ready has no last messages or read states; the repository adds
/// them from recent history and what this device remembers.
ReadySnapshot snapshotFrom(
  core.ReadySnapshot ready, {
  Map<int, Message> lastMessages = const {},
  Map<int, ReadState> readStates = const {},
}) => ReadySnapshot(
  self: userFrom(ready.selfUser),
  info: serverInfoFrom(ready.server),
  channels: [for (final channel in ready.channels) channelFrom(channel)],
  roles: [for (final role in ready.roles) roleFrom(role)],
  members: [for (final member in ready.members) memberFrom(member)],
  presences: {
    for (final presence in ready.presences)
      presence.userId: presenceFrom(presence.status),
  },
  serverPermissions: Permissions(ready.serverPermissions),
  channelPermissions: {
    for (final entry in ready.channelPermissions)
      entry.channelId: Permissions(entry.permissions),
  },
  voice: voiceFrom(ready.voiceStates),
  streams: {
    for (final stream in ready.streams) stream.streamKey: streamFrom(stream),
  },
  voiceEnabled: ready.voiceEnabled,
  voiceSettings: voiceSettingsFrom(ready.voiceSettings),
  sounds: {
    for (final sound in ready.soundboardSounds) sound.id: soundFrom(sound),
  },
  lastMessages: lastMessages,
  readStates: readStates,
  addresses: serverAddressesFrom(ready.addresses),
  commands: commandListFrom(ready.commands),
  uploadSettings: uploadSettingsFrom(ready.uploadSettings),
  privateMessages: ready.privateMessages,
  privateSettings: privateSettingsFrom(ready.privateSettings),
  pluginUsers: [for (final user in ready.pluginUsers) pluginUserFrom(user)],
  pluginChannelTypes: [
    for (final type in ready.pluginChannelTypes) pluginChannelTypeFrom(type),
  ],
  pluginActivities: [
    for (final activity in ready.pluginActivities) pluginActivityFrom(activity),
  ],
  activitySessions: {
    for (final session in ready.activitySessions)
      session.id: activitySessionFrom(session),
  },
);

PluginActivity pluginActivityFrom(core.PluginActivity activity) =>
    PluginActivity(
      pluginId: activity.pluginId,
      activityKey: activity.activityKey,
      name: activity.name,
      description: activity.description,
      icon: activity.icon,
      pluginName: activity.pluginName,
    );

ActivitySession activitySessionFrom(core.ActivitySession session) =>
    ActivitySession(
      id: session.id,
      channelId: session.channelId,
      pluginId: session.pluginId,
      activityKey: session.activityKey,
      name: session.name,
      icon: session.icon,
      pluginName: session.pluginName,
      starterId: session.starterId,
      startedAt: _time(session.startedAtMs),
      viewerIds: List.unmodifiable([
        for (final id in session.viewerIds) id.toInt(),
      ]),
    );

ActivityEndReason activityEndReasonFrom(core.ActivityEndReason reason) =>
    switch (reason) {
      core.ActivityEndReason.ended => ActivityEndReason.ended,
      core.ActivityEndReason.plugin => ActivityEndReason.plugin,
      core.ActivityEndReason.callEmpty => ActivityEndReason.callEmpty,
      core.ActivityEndReason.pluginStopped => ActivityEndReason.pluginStopped,
      core.ActivityEndReason.unavailable => ActivityEndReason.unavailable,
    };

PluginChannelType pluginChannelTypeFrom(core.PluginChannelType type) =>
    PluginChannelType(
      pluginId: type.pluginId,
      typeKey: type.typeKey,
      name: type.name,
      description: type.description,
      icon: type.icon,
      pluginName: type.pluginName,
    );

ViewOutput viewOutputFrom(core.ViewOutput output) =>
    ViewOutput(viewId: output.viewId, json: output.json);

ServerPrivateSettings privateSettingsFrom(core.PrivateSettings s) =>
    ServerPrivateSettings(
      enabled: s.enabled,
      friendsOnly: s.friendsOnly,
      allowedByConfig: s.allowedByConfig,
    );

UploadSettings uploadSettingsFrom(core.UploadSettings s) => UploadSettings(
  blockedExtensions: s.blockedExtensions,
  voiceMessageMaxSeconds: s.voiceMessageMaxSeconds,
  maxFileBytes: s.maxFileBytes.toInt(),
  maxAttachments: s.maxAttachments,
);

UploadPhase uploadPhaseFrom(core.UploadPhase phase) => switch (phase) {
  core.UploadPhase_Preparing() => UploadPhase.preparing,
  core.UploadPhase_Uploading() => UploadPhase.uploading,
  core.UploadPhase_Processing() => UploadPhase.processing,
  core.UploadPhase_Sending() => UploadPhase.sending,
  core.UploadPhase_Done() => UploadPhase.done,
  core.UploadPhase_Failed() => UploadPhase.failed,
  core.UploadPhase_Cancelled() => UploadPhase.cancelled,
};

StorageInfo storageInfoFrom(core.StorageInfo info) => StorageInfo(
  backends: [for (final b in info.backends) storageBackendFrom(b)],
  moveStatus: info.moveStatus == null
      ? null
      : storageMoveFrom(info.moveStatus!),
  tempFreeBytes: info.tempFreeBytes.toInt(),
);

StorageBackend storageBackendFrom(core.StorageBackend b) => StorageBackend(
  id: b.id,
  kind: switch (b.kind) {
    core.StorageKind.local => StorageKind.local,
    core.StorageKind.s3 => StorageKind.s3,
  },
  name: b.name,
  active: b.active,
  s3: b.s3 == null ? null : s3ConfigFrom(b.s3!),
  hasSecret: b.hasSecret,
  fileCount: b.fileCount.toInt(),
  bytes: b.bytes.toInt(),
  lastError: b.lastError,
);

S3Config s3ConfigFrom(core.S3Config c) => S3Config(
  endpoint: c.endpoint,
  region: c.region,
  bucket: c.bucket,
  keyPrefix: c.keyPrefix,
  pathStyle: c.pathStyle,
  allowHttp: c.allowHttp,
  accessKeyId: c.accessKeyId,
  secretAccessKey: c.secretAccessKey,
);

core.S3Config s3ConfigTo(S3Config c) => core.S3Config(
  endpoint: c.endpoint,
  region: c.region,
  bucket: c.bucket,
  keyPrefix: c.keyPrefix,
  pathStyle: c.pathStyle,
  allowHttp: c.allowHttp,
  accessKeyId: c.accessKeyId,
  secretAccessKey: c.secretAccessKey,
);

VoiceStopReason voiceStopReasonFrom(core.VoiceStopReason r) => switch (r) {
  core.VoiceStopReason.maxLength => VoiceStopReason.maxLength,
  core.VoiceStopReason.deviceLost => VoiceStopReason.deviceLost,
};

VoiceDraft voiceDraftFrom(core.VoiceDraft draft) => VoiceDraft(
  id: draft.draftId.toInt(),
  durationMs: draft.durationMs,
  waveform: draft.waveform,
);

PlaybackItem playbackItemFrom(core.PlaybackItem item) => switch (item) {
  core.PlaybackItem_Attachment(:final serverKey, :final attachmentId) =>
    AttachmentPlayback(serverKey, attachmentId),
  core.PlaybackItem_Draft(:final draftId) => DraftPlayback(draftId.toInt()),
};

core.PlaybackItem playbackItemTo(PlaybackItem item) => switch (item) {
  AttachmentPlayback(:final serverKey, :final attachmentId) =>
    core.PlaybackItem.attachment(
      serverKey: serverKey,
      attachmentId: attachmentId,
    ),
  DraftPlayback(:final draftId) => core.PlaybackItem.draft(
    draftId: BigInt.from(draftId),
  ),
};

StorageMove storageMoveFrom(core.StorageMove m) => StorageMove(
  toStorageId: m.toStorageId,
  running: m.running,
  filesDone: m.filesDone.toInt(),
  filesTotal: m.filesTotal.toInt(),
  bytesDone: m.bytesDone.toInt(),
  bytesTotal: m.bytesTotal.toInt(),
  failed: m.failed,
);

StorageTestResult storageTestResultFrom(core.StorageTestResult r) =>
    StorageTestResult(
      steps: [
        for (final s in r.steps)
          StorageTestStep(name: s.name, ok: s.ok, detail: s.detail),
      ],
      ok: r.ok,
    );

MediaHandle mediaHandleFrom(core.MediaHandle handle) => MediaHandle(
  textureId: handle.textureId?.toInt(),
  durationMs: handle.durationMs,
  width: handle.width,
  height: handle.height,
  hasVideo: handle.hasVideo,
);

Sound soundFrom(core.SoundboardSound sound) => Sound(
  id: sound.id,
  name: sound.name,
  emoji: sound.emoji,
  volume: sound.volume,
  sha256: sound.sha256,
  duration: Duration(milliseconds: sound.durationMs),
  uploaderId: sound.uploaderId,
  createdAt: _time(sound.createdAtMs),
);

core.SoundboardSound soundTo(Sound sound) => core.SoundboardSound(
  id: sound.id,
  name: sound.name,
  emoji: sound.emoji,
  volume: sound.volume,
  sha256: sound.sha256,
  sizeBytes: 0,
  durationMs: sound.duration.inMilliseconds,
  uploaderId: sound.uploaderId,
  createdAtMs: sound.createdAt?.millisecondsSinceEpoch ?? 0,
);

DefaultSound defaultSoundFrom(core.DefaultSound sound) => DefaultSound(
  id: sound.id,
  name: sound.name,
  emoji: sound.emoji,
  duration: Duration(milliseconds: sound.durationMs),
);

SoundPlay soundPlayFrom(core.SoundEffect effect) => SoundPlay(
  channelId: effect.channelId,
  userId: effect.userId,
  soundRef: effect.soundRef,
  name: effect.name,
  emoji: effect.emoji,
  volume: effect.volume,
  duration: Duration(milliseconds: effect.durationMs),
  startedAt: _time(effect.startedAtMs),
);

core.SoundSelection selectionTo(SoundWindow window) => core.SoundSelection(
  draftId: BigInt.from(window.draftId),
  startMs: window.start.inMilliseconds,
  lengthMs: window.length.inMilliseconds,
  normalize: window.normalize,
  volume: window.volume,
);

RepoErrorKind _kindFrom(core.ErrorCode code) => switch (code) {
  core.ErrorCode.unauthorized ||
  core.ErrorCode.invalidSession => RepoErrorKind.unauthorized,
  core.ErrorCode.forbidden => RepoErrorKind.forbidden,
  core.ErrorCode.notFound => RepoErrorKind.notFound,
  core.ErrorCode.invalidArgument => RepoErrorKind.invalidArgument,
  core.ErrorCode.rateLimited => RepoErrorKind.rateLimited,
  core.ErrorCode.conflict ||
  core.ErrorCode.voiceNotConnected => RepoErrorKind.conflict,
  core.ErrorCode.soundboardFull => RepoErrorKind.soundboardFull,
  core.ErrorCode.domainInvalid ||
  core.ErrorCode.dnsMismatch => RepoErrorKind.invalidArgument,
  core.ErrorCode.domainExists ||
  core.ErrorCode.domainLimit ||
  core.ErrorCode.domainNotVerified => RepoErrorKind.conflict,
  core.ErrorCode.acmeRateLimited => RepoErrorKind.rateLimited,
  core.ErrorCode.acmeFailed ||
  core.ErrorCode.portUnreachable => RepoErrorKind.other,
  core.ErrorCode.musicBlocked ||
  core.ErrorCode.musicNotInChannel => RepoErrorKind.forbidden,
  core.ErrorCode.musicStale => RepoErrorKind.musicStale,
  core.ErrorCode.commandUnknown => RepoErrorKind.notFound,
  core.ErrorCode.commandDisabled ||
  core.ErrorCode.commandNotAllowedHere => RepoErrorKind.forbidden,
  core.ErrorCode.commandInvalidOptions => RepoErrorKind.invalidArgument,
  core.ErrorCode.musicQueueFull ||
  core.ErrorCode.musicMemberLimit => RepoErrorKind.conflict,
  core.ErrorCode.musicTrackTooLong ||
  core.ErrorCode.musicStreamsNotAllowed ||
  core.ErrorCode.musicSourceNotAllowed ||
  core.ErrorCode.musicLinkNotAllowed => RepoErrorKind.invalidArgument,
  core.ErrorCode.musicNotConfigured ||
  core.ErrorCode.musicNodeUnavailable => RepoErrorKind.musicUnavailable,
  core.ErrorCode.qualityLimit => RepoErrorKind.qualityLimit,
  core.ErrorCode.streamViewerLimit => RepoErrorKind.streamFull,
  core.ErrorCode.voiceChannelFull => RepoErrorKind.voiceChannelFull,
  core.ErrorCode.cameraLimit => RepoErrorKind.cameraLimit,
  core.ErrorCode.soundCooldown => RepoErrorKind.soundCooldown,
  core.ErrorCode.soundTooLong => RepoErrorKind.soundTooLong,
  core.ErrorCode.soundInvalid => RepoErrorKind.soundInvalid,
  core.ErrorCode.fileTooLarge => RepoErrorKind.fileTooLarge,
  core.ErrorCode.fileTypeBlocked => RepoErrorKind.fileTypeBlocked,
  core.ErrorCode.voiceMessageInvalid => RepoErrorKind.voiceMessageInvalid,
  core.ErrorCode.voiceMessageTooLong => RepoErrorKind.voiceMessageTooLong,
  core.ErrorCode.uploadOffsetMismatch => RepoErrorKind.conflict,
  core.ErrorCode.storageUnavailable => RepoErrorKind.storageUnavailable,
  core.ErrorCode.storageFull => RepoErrorKind.storageFull,
  core.ErrorCode.friendRequestsClosed => RepoErrorKind.friendRequestsClosed,
  core.ErrorCode.messageRequestsClosed => RepoErrorKind.messageRequestsClosed,
  core.ErrorCode.undeliverable => RepoErrorKind.undeliverable,
  core.ErrorCode.privateMessagesUnavailable =>
    RepoErrorKind.privateMessagesUnavailable,
  core.ErrorCode.friendsOnly => RepoErrorKind.friendsOnly,
  core.ErrorCode.keysNotAccepted => RepoErrorKind.keysNotAccepted,
  core.ErrorCode.pluginLimit => RepoErrorKind.pluginLimit,
  core.ErrorCode.pluginUnavailable => RepoErrorKind.pluginUnavailable,
  core.ErrorCode.deviceListChanged => RepoErrorKind.other,
  core.ErrorCode.internal || core.ErrorCode.unknown => RepoErrorKind.other,
};

/// The core's errors as the app's, with messages people can read.
RepoException errorFrom(core.CoreError error) => switch (error) {
  core.CoreError_NotInitialized() => const RepoException(
    RepoErrorKind.other,
    'Opencord is still starting.',
  ),
  core.CoreError_NoIdentity() => const RepoException(
    RepoErrorKind.other,
    'Create or import an identity first.',
  ),
  core.CoreError_UnknownServer() => const RepoException(
    RepoErrorKind.notFound,
    'That server is not in your list.',
  ),
  core.CoreError_NotConnected() => const RepoException(
    RepoErrorKind.notConnected,
    'Not connected to that server right now.',
  ),
  core.CoreError_Timeout() => const RepoException(
    RepoErrorKind.timeout,
    'The server did not answer in time.',
  ),
  core.CoreError_InvalidInput(:final message) => RepoException(
    RepoErrorKind.invalidArgument,
    message,
  ),
  core.CoreError_Server(:final code, :final message, :final retryAfterMs) =>
    RepoException(
      _kindFrom(code),
      message,
      retryAfter: switch (retryAfterMs) {
        final ms? => Duration(milliseconds: ms),
        null => null,
      },
    ),
  core.CoreError_FingerprintMismatch() => const RepoException(
    RepoErrorKind.fingerprintMismatch,
    "The server's identity changed since you trusted it.",
  ),
  core.CoreError_Rejected(:final message) => RepoException(
    RepoErrorKind.rejected,
    message,
  ),
  core.CoreError_Connection(:final message) => RepoException(
    RepoErrorKind.connection,
    message,
  ),
  core.CoreError_Storage(:final message) => RepoException(
    RepoErrorKind.other,
    message,
  ),
  core.CoreError_Cancelled() => const RepoException(
    RepoErrorKind.cancelled,
    'Cancelled.',
  ),
  core.CoreError_Camera(:final problem, :final message) => RepoException(
    switch (problem) {
      core.CameraProblem.denied => RepoErrorKind.cameraDenied,
      core.CameraProblem.noCamera ||
      core.CameraProblem.noUsableMode => RepoErrorKind.cameraMissing,
      core.CameraProblem.notSupported => RepoErrorKind.cameraUnsupported,
      core.CameraProblem.notInVoice => RepoErrorKind.notConnected,
      core.CameraProblem.failed => RepoErrorKind.other,
    },
    message,
  ),
  core.CoreError_Screen(:final problem, :final message) => RepoException(
    switch (problem) {
      core.ScreenProblem.cancelled => RepoErrorKind.screenCancelled,
      core.ScreenProblem.denied => RepoErrorKind.screenDenied,
      core.ScreenProblem.notSupported => RepoErrorKind.screenUnsupported,
      core.ScreenProblem.notInVoice => RepoErrorKind.notConnected,
      core.ScreenProblem.failed => RepoErrorKind.other,
    },
    message,
  ),
};

AddServerResult addServerFrom(core.AddServerOutcome outcome) =>
    switch (outcome) {
      core.AddServerOutcome_Added(:final field0) => ServerAdded(
        serverFrom(field0),
      ),
      core.AddServerOutcome_NeedsTrust(:final address, :final fingerprint) =>
        ServerNeedsTrust(address: address, fingerprint: fingerprint),
    };

VoiceConnectionStatus voiceConnectionFrom(core.VoiceConnectionState state) =>
    switch (state) {
      core.VoiceConnectionState_AwaitingEndpoint() =>
        const VoiceConnectionStatus(VoiceConnectionPhase.awaitingEndpoint),
      core.VoiceConnectionState_Authenticating() => const VoiceConnectionStatus(
        VoiceConnectionPhase.authenticating,
      ),
      core.VoiceConnectionState_RtcConnecting() => const VoiceConnectionStatus(
        VoiceConnectionPhase.rtcConnecting,
      ),
      core.VoiceConnectionState_Connected() => const VoiceConnectionStatus(
        VoiceConnectionPhase.connected,
      ),
      core.VoiceConnectionState_Reconnecting() => const VoiceConnectionStatus(
        VoiceConnectionPhase.reconnecting,
      ),
      core.VoiceConnectionState_NoRoute() => const VoiceConnectionStatus(
        VoiceConnectionPhase.noRoute,
      ),
      core.VoiceConnectionState_Disconnected(:final reason) =>
        VoiceConnectionStatus(
          VoiceConnectionPhase.disconnected,
          reason: reason,
        ),
    };

AudioDeviceList audioDevicesFrom(core.AudioDevices devices) {
  List<AudioDevice> list(List<core.AudioDevice> devices) => [
    for (final device in devices) AudioDevice(id: device.id, name: device.name),
  ];
  return AudioDeviceList(
    inputs: list(devices.inputs),
    outputs: list(devices.outputs),
    defaultInput: devices.defaultInput,
    defaultOutput: devices.defaultOutput,
  );
}

/// Volumes in percent to the core's fractions of full volume.
core.AudioSettings audioSettingsTo(AudioConfig config) => core.AudioSettings(
  inputDevice: config.inputDevice,
  outputDevice: config.outputDevice,
  pushToTalk: config.pushToTalk,
  pushToTalkReleaseMs: config.pushToTalkRelease.inMilliseconds,
  automaticSensitivity: config.automaticSensitivity,
  sensitivityDbfs: config.sensitivityDbfs,
  echoCancellation: config.echoCancellation,
  noiseSuppression: switch (config.noiseSuppression) {
    NoiseSuppression.off => core.NoiseSuppressionMode.off,
    NoiseSuppression.standard => core.NoiseSuppressionMode.standard,
    NoiseSuppression.high => core.NoiseSuppressionMode.high,
  },
  automaticGain: config.automaticGain,
  inputVolume: config.inputVolume / 100,
  outputVolume: config.outputVolume / 100,
  attenuationPercent: config.attenuationPercent,
  attenuateWhenOthersSpeak: config.attenuateWhenOthersSpeak,
  attenuateWhenISpeak: config.attenuateWhenISpeak,
);

NoiseSuppression noiseSuppressionFrom(core.NoiseSuppressionMode mode) =>
    switch (mode) {
      core.NoiseSuppressionMode.off => NoiseSuppression.off,
      core.NoiseSuppressionMode.standard => NoiseSuppression.standard,
      core.NoiseSuppressionMode.high => NoiseSuppression.high,
    };

core.HotkeyAction hotkeyActionTo(HotkeyAction action) => switch (action) {
  HotkeyAction.pushToTalk => core.HotkeyAction.pushToTalk,
  HotkeyAction.prioritySpeaker => core.HotkeyAction.prioritySpeaker,
  HotkeyAction.toggleMute => core.HotkeyAction.toggleMute,
  HotkeyAction.toggleDeafen => core.HotkeyAction.toggleDeafen,
  HotkeyAction.toggleInputMode => core.HotkeyAction.toggleInputMode,
  HotkeyAction.toggleMusicMute => core.HotkeyAction.toggleMusicMute,
};

HotkeyAction hotkeyActionFrom(core.HotkeyAction action) => switch (action) {
  core.HotkeyAction.pushToTalk => HotkeyAction.pushToTalk,
  core.HotkeyAction.prioritySpeaker => HotkeyAction.prioritySpeaker,
  core.HotkeyAction.toggleMute => HotkeyAction.toggleMute,
  core.HotkeyAction.toggleDeafen => HotkeyAction.toggleDeafen,
  core.HotkeyAction.toggleInputMode => HotkeyAction.toggleInputMode,
  core.HotkeyAction.toggleMusicMute => HotkeyAction.toggleMusicMute,
};

HotkeySupport hotkeySupportFrom(core.HotkeySupport support) =>
    switch (support) {
      core.HotkeySupport_Global(:final method, :final mouse) =>
        HotkeySupport.global(method, mouse: mouse),
      core.HotkeySupport_FocusedOnly(:final reason, :final settingsUrl) =>
        HotkeySupport.focusedOnly(reason, settingsUrl: settingsUrl),
    };

PluginState pluginStateFrom(core.PluginState state) => switch (state) {
  core.PluginState.starting => PluginState.starting,
  core.PluginState.running => PluginState.running,
  core.PluginState.stopped => PluginState.stopped,
  core.PluginState.crashed => PluginState.crashed,
};

PluginSignature pluginSignatureFrom(core.PluginSignature sig) => switch (sig) {
  core.PluginSignature.unsigned => PluginSignature.unsigned,
  core.PluginSignature.signed => PluginSignature.signed,
  core.PluginSignature.dev => PluginSignature.dev,
};

GithubCheck githubCheckFrom(core.GithubCheck check) => switch (check) {
  core.GithubCheck.unspecified => GithubCheck.unspecified,
  core.GithubCheck.verified => GithubCheck.verified,
  core.GithubCheck.mismatch => GithubCheck.mismatch,
  core.GithubCheck.failed => GithubCheck.failed,
};

PluginConfigKind pluginConfigKindFrom(core.PluginConfigKind kind) =>
    switch (kind) {
      core.PluginConfigKind.text => PluginConfigKind.text,
      core.PluginConfigKind.secret => PluginConfigKind.secret,
      core.PluginConfigKind.number => PluginConfigKind.number,
      core.PluginConfigKind.boolean => PluginConfigKind.boolean,
      core.PluginConfigKind.choice => PluginConfigKind.choice,
      core.PluginConfigKind.role => PluginConfigKind.role,
      core.PluginConfigKind.channel => PluginConfigKind.channel,
      core.PluginConfigKind.user => PluginConfigKind.user,
    };

PluginLogLevel pluginLogLevelFrom(core.PluginLogLevel level) => switch (level) {
  core.PluginLogLevel.info => PluginLogLevel.info,
  core.PluginLogLevel.warn => PluginLogLevel.warn,
  core.PluginLogLevel.error => PluginLogLevel.error,
  core.PluginLogLevel.action => PluginLogLevel.action,
};

PluginConfigField pluginConfigFieldFrom(core.PluginConfigField f) =>
    PluginConfigField(
      key: f.key,
      label: f.label,
      kind: pluginConfigKindFrom(f.kind),
      choices: f.choices,
      required: f.required_,
    );

PluginConfigValue pluginConfigValueFrom(core.PluginConfigValue v) =>
    PluginConfigValue(key: v.key, value: v.value, hasValue: v.hasValue);

core.PluginConfigValue pluginConfigValueTo(PluginConfigValue v) =>
    core.PluginConfigValue(key: v.key, value: v.value, hasValue: v.hasValue);

PluginPermissionGrant pluginPermissionGrantFrom(core.PluginPermissionGrant g) =>
    PluginPermissionGrant(
      name: g.name,
      granted: g.granted,
      grantable: g.grantable,
    );

PluginUser pluginUserFrom(core.PluginUser u) => PluginUser(
  userId: u.userId.toInt(),
  pluginId: u.pluginId,
  name: u.name,
  avatarHash: u.avatarHash,
  installed: u.installed,
);

Plugin pluginFrom(core.Plugin p) => Plugin(
  id: p.id,
  name: p.name,
  version: p.version,
  author: p.author,
  description: p.description,
  github: p.github,
  license: p.license,
  signature: pluginSignatureFrom(p.signature),
  keyFingerprint: p.keyFingerprint,
  githubCheck: githubCheckFrom(p.githubCheck),
  state: pluginStateFrom(p.state),
  statusReason: p.statusReason,
  permissions: [for (final g in p.permissions) pluginPermissionGrantFrom(g)],
  domains: p.domains,
  configFields: [for (final f in p.configFields) pluginConfigFieldFrom(f)],
  config: [for (final v in p.config) pluginConfigValueFrom(v)],
  userId: p.userId.toInt(),
  roleId: p.roleId.toInt(),
  installedAt: _time(p.installedAtMs.toInt()),
  updatedAt: _time(p.updatedAtMs.toInt()),
  cpuPermille: p.cpuPermille,
  memoryBytes: p.memoryBytes.toInt(),
  databaseBytes: p.databaseBytes.toInt(),
  filesBytes: p.filesBytes.toInt(),
);

PluginReview pluginReviewFrom(core.PluginReview r) => PluginReview(
  reviewId: r.reviewId,
  plugin: pluginFrom(r.plugin),
  installed: r.installed != null ? pluginFrom(r.installed!) : null,
  newPermissions: r.newPermissions,
  licenseNeedsAcceptance: r.licenseNeedsAcceptance,
  expiresAt: _time(r.expiresAtMs.toInt()),
  dev: r.dev,
);

PluginLogLine pluginLogLineFrom(core.PluginLogLine l) => PluginLogLine(
  seq: l.seq.toInt(),
  at: _time(l.atMs.toInt()),
  level: pluginLogLevelFrom(l.level),
  text: l.text,
);

PluginDevToken pluginDevTokenFrom(core.PluginDevToken t) => PluginDevToken(
  id: t.id.toInt(),
  pluginId: t.pluginId,
  createdBy: t.createdBy.toInt(),
  createdAt: _time(t.createdAtMs.toInt()),
  expiresAt: _time(t.expiresAtMs.toInt()),
  lastUsedAt: t.lastUsedAtMs != null ? _time(t.lastUsedAtMs!.toInt()) : null,
);

CreatedPluginDevToken createdPluginDevTokenFrom(core.CreatedPluginDevToken c) =>
    CreatedPluginDevToken(
      token: pluginDevTokenFrom(c.token),
      secret: c.secret,
      serverFingerprint: c.serverFingerprint,
    );
