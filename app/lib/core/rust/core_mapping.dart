import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/model/misc.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/model/snapshot.dart';
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
);

ChannelKind channelKindFrom(core.ChannelKind kind) => switch (kind) {
  core.ChannelKind.text => ChannelKind.text,
  core.ChannelKind.voice => ChannelKind.voice,
  core.ChannelKind.category => ChannelKind.category,
};

/// Announcement channels are not in Phase 1's protocol; they go as text.
core.ChannelKind channelKindTo(ChannelKind kind) => switch (kind) {
  ChannelKind.text || ChannelKind.announcement => core.ChannelKind.text,
  ChannelKind.voice => core.ChannelKind.voice,
  ChannelKind.category => core.ChannelKind.category,
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
);

ServerInfo serverInfoFrom(core.ServerInfo info) => ServerInfo(
  serverIdHex: info.serverIdHex,
  name: info.name,
  description: info.description,
  ownerId: info.ownerId,
  openJoin: info.openJoin,
  everyoneRoleId: info.everyoneRoleId,
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

/// Invisible goes out as offline: others see exactly that.
core.PresenceStatus presenceTo(SelfPresence presence) => switch (presence) {
  SelfPresence.online => core.PresenceStatus.online,
  SelfPresence.idle => core.PresenceStatus.idle,
  SelfPresence.doNotDisturb => core.PresenceStatus.dnd,
  SelfPresence.invisible => core.PresenceStatus.offline,
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
  lastMessages: lastMessages,
  readStates: readStates,
);

RepoErrorKind _kindFrom(core.ErrorCode code) => switch (code) {
  core.ErrorCode.unauthorized ||
  core.ErrorCode.invalidSession => RepoErrorKind.unauthorized,
  core.ErrorCode.forbidden => RepoErrorKind.forbidden,
  core.ErrorCode.notFound => RepoErrorKind.notFound,
  core.ErrorCode.invalidArgument => RepoErrorKind.invalidArgument,
  core.ErrorCode.rateLimited => RepoErrorKind.rateLimited,
  core.ErrorCode.conflict ||
  core.ErrorCode.voiceNotConnected ||
  core.ErrorCode.soundboardFull => RepoErrorKind.conflict,
  core.ErrorCode.qualityLimit => RepoErrorKind.qualityLimit,
  core.ErrorCode.streamViewerLimit => RepoErrorKind.streamFull,
  core.ErrorCode.voiceChannelFull => RepoErrorKind.voiceChannelFull,
  core.ErrorCode.cameraLimit => RepoErrorKind.cameraLimit,
  core.ErrorCode.soundCooldown => RepoErrorKind.rateLimited,
  core.ErrorCode.soundTooLong ||
  core.ErrorCode.soundInvalid => RepoErrorKind.invalidArgument,
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
};

HotkeyAction hotkeyActionFrom(core.HotkeyAction action) => switch (action) {
  core.HotkeyAction.pushToTalk => HotkeyAction.pushToTalk,
  core.HotkeyAction.prioritySpeaker => HotkeyAction.prioritySpeaker,
  core.HotkeyAction.toggleMute => HotkeyAction.toggleMute,
  core.HotkeyAction.toggleDeafen => HotkeyAction.toggleDeafen,
};

HotkeySupport hotkeySupportFrom(core.HotkeySupport support) =>
    switch (support) {
      core.HotkeySupport_Global(:final method) => HotkeySupport.global(method),
      core.HotkeySupport_FocusedOnly(:final reason) =>
        HotkeySupport.focusedOnly(reason),
    };
