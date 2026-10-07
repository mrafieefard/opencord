import 'package:flutter/foundation.dart';

import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/message.dart';
import 'package:opencord/core/model/misc.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/model/voice.dart';

/// Everything about a server right after connecting. Replaces all earlier
/// state for that server.
@immutable
class ReadySnapshot {
  const ReadySnapshot({
    required this.self,
    required this.info,
    required this.channels,
    required this.roles,
    required this.members,
    required this.serverPermissions,
    required this.channelPermissions,
    this.presences = const {},
    this.activities = const {},
    this.voice = const {},
    this.voiceEnabled = true,
    this.voiceSettings = const VoiceSettings(),
    this.lastMessages = const {},
    this.readStates = const {},
  });

  final User self;
  final ServerInfo info;

  /// Only the channels this user can view, categories included.
  final List<Channel> channels;
  final List<Role> roles;
  final List<Member> members;
  final Map<int, Presence> presences;

  /// Free text like "Editing main.rs", per user.
  final Map<int, String> activities;
  final Permissions serverPermissions;
  final Map<int, Permissions> channelPermissions;

  /// Voice participants per voice channel.
  final Map<int, List<VoiceParticipant>> voice;

  /// Whether the server has voice at all.
  final bool voiceEnabled;
  final VoiceSettings voiceSettings;

  /// Newest message per channel, for the channel list previews.
  final Map<int, Message> lastMessages;
  final Map<int, ReadState> readStates;
}
