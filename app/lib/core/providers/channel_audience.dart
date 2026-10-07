import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/providers/server_state.dart';

/// The members who can see [channel], resolved the way the server does
/// (§4.3 header counts, §4.7 member panel).
List<Member> channelViewers(ServerData data, Channel channel) => [
  for (final member in data.members.values)
    if (data
        .contextFor(member)
        .inChannel(channel.overwrites)
        .has(Permissions.viewChannel))
      member,
];

/// How many of [members] are not shown as offline.
int onlineCount(List<Member> members, Map<int, Presence> presences) =>
    members.where((member) {
      final presence = presences[member.id] ?? Presence.offline;
      return presence != Presence.offline;
    }).length;
