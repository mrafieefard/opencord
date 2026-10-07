import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/model/misc.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/model/snapshot.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/model/voice.dart';

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

final class VoiceSettingsChanged extends RepoEvent {
  const VoiceSettingsChanged(super.serverKey, this.settings);

  final VoiceSettings settings;
}

/// Who is speaking right now (mock only until voice carries audio).
final class SpeakingChanged extends RepoEvent {
  const SpeakingChanged(super.serverKey, this.speaking);

  final Set<int> speaking;
}
