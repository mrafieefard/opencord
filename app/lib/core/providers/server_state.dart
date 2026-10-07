import 'package:flutter/foundation.dart';

import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/model/snapshot.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/model/voice.dart';
import 'package:opencord/core/repository/events.dart';

/// What the current user knows about one server since its last Ready.
@immutable
class ServerData {
  const ServerData({
    required this.self,
    required this.info,
    required this.channels,
    required this.roles,
    required this.members,
    required this.serverPermissions,
    required this.channelPermissions,
    this.voiceEnabled = true,
    this.voiceSettings = const VoiceSettings(),
  });

  factory ServerData.fromSnapshot(ReadySnapshot snapshot) => ServerData(
    self: snapshot.self,
    info: snapshot.info,
    channels: {for (final channel in snapshot.channels) channel.id: channel},
    roles: {for (final role in snapshot.roles) role.id: role},
    members: {for (final member in snapshot.members) member.id: member},
    serverPermissions: snapshot.serverPermissions,
    channelPermissions: snapshot.channelPermissions,
    voiceEnabled: snapshot.voiceEnabled,
    voiceSettings: snapshot.voiceSettings,
  );

  final User self;
  final ServerInfo info;
  final Map<int, Channel> channels;
  final Map<int, Role> roles;
  final Map<int, Member> members;
  final Permissions serverPermissions;
  final Map<int, Permissions> channelPermissions;

  /// Whether the server has voice at all.
  final bool voiceEnabled;
  final VoiceSettings voiceSettings;

  bool get isOwner => info.ownerId == self.id;

  bool can(Permissions permissions) => serverPermissions.has(permissions);

  Permissions permissionsIn(int channelId) =>
      channelPermissions[channelId] ?? Permissions.none;

  Member? get selfMember => members[self.id];

  /// Highest first; @everyone last.
  List<Role> get rolesDescending =>
      roles.values.toList()..sort((a, b) => b.position.compareTo(a.position));

  Role? highestRole(Member member) => _highest(member, hoistedOnly: false);

  Role? highestHoistedRole(Member member) =>
      _highest(member, hoistedOnly: true);

  Role? _highest(Member member, {required bool hoistedOnly}) {
    Role? best;
    for (final id in member.roleIds) {
      final role = roles[id];
      if (role == null || (hoistedOnly && !role.hoist)) continue;
      if (best == null || role.position > best.position) best = role;
    }
    return best;
  }

  /// The server-side permission rules, applied to [member].
  PermissionContext contextFor(Member member) => PermissionContext(
    userId: member.id,
    isOwner: info.ownerId == member.id,
    everyoneRoleId: info.everyoneRoleId,
    everyone:
        roles[info.everyoneRoleId]?.permissions ?? Permissions.defaultEveryone,
    roles: [
      for (final id in member.roleIds)
        if (roles[id] case final role?)
          RoleGrant(
            id: role.id,
            position: role.position,
            permissions: role.permissions,
          ),
    ],
  );

  ServerData copyWith({
    ServerInfo? info,
    Map<int, Channel>? channels,
    Map<int, Role>? roles,
    Map<int, Member>? members,
    Permissions? serverPermissions,
    Map<int, Permissions>? channelPermissions,
    VoiceSettings? voiceSettings,
  }) => ServerData(
    self: self,
    info: info ?? this.info,
    channels: channels ?? this.channels,
    roles: roles ?? this.roles,
    members: members ?? this.members,
    serverPermissions: serverPermissions ?? this.serverPermissions,
    channelPermissions: channelPermissions ?? this.channelPermissions,
    voiceEnabled: voiceEnabled,
    voiceSettings: voiceSettings ?? this.voiceSettings,
  );
}

@immutable
class ServerState {
  const ServerState({
    required this.key,
    this.connection = const ConnectionStatus.connecting(),
    this.data,
    this.epoch = 0,
  });

  final String key;
  final ConnectionStatus connection;

  /// Counts fresh sessions (Ready events). Loaded histories reload when it
  /// changes, since messages may have been missed in between.
  final int epoch;

  /// Null until the first Ready; kept (and shown greyed out) while
  /// reconnecting.
  final ServerData? data;
}

Map<int, T> _put<T>(Map<int, T> map, int id, T value) => {...map, id: value};

Map<int, T> _drop<T>(Map<int, T> map, int id) =>
    map.containsKey(id) ? ({...map}..remove(id)) : map;

/// Applies one event to a server's state. Returns the same object when the
/// event does not concern it, so watchers do not rebuild.
ServerState reduceServer(ServerState state, RepoEvent event) {
  if (event is ConnectionChanged) {
    return ServerState(
      key: state.key,
      connection: event.status,
      data: state.data,
      epoch: state.epoch,
    );
  }
  if (event is Ready) {
    return ServerState(
      key: state.key,
      connection: const ConnectionStatus.connected(),
      data: ServerData.fromSnapshot(event.snapshot),
      epoch: state.epoch + 1,
    );
  }
  final data = state.data;
  if (data == null) return state;
  final changed = switch (event) {
    ChannelUpserted(:final channel) => data.copyWith(
      channels: _put(data.channels, channel.id, channel),
    ),
    ChannelDeleted(:final channelId) => data.copyWith(
      channels: _drop(data.channels, channelId),
      channelPermissions: _drop(data.channelPermissions, channelId),
    ),
    RoleUpserted(:final role) => data.copyWith(
      roles: _put(data.roles, role.id, role),
    ),
    RoleDeleted(:final roleId) => data.copyWith(
      roles: _drop(data.roles, roleId),
      members: {
        for (final MapEntry(:key, :value) in data.members.entries)
          key: value.roleIds.contains(roleId)
              ? value.copyWith(
                  roleIds: [
                    for (final id in value.roleIds)
                      if (id != roleId) id,
                  ],
                )
              : value,
      },
    ),
    MemberUpserted(:final member) => data.copyWith(
      members: _put(data.members, member.id, member),
    ),
    MemberLeft(:final userId) => data.copyWith(
      members: _drop(data.members, userId),
    ),
    ServerInfoChanged(:final info) => data.copyWith(info: info),
    VoiceSettingsChanged(:final settings) => data.copyWith(
      voiceSettings: settings,
    ),
    PermissionsChanged(:final server, :final channels) => data.copyWith(
      serverPermissions: server,
      channelPermissions: channels,
    ),
    _ => null,
  };
  if (changed == null) return state;
  return ServerState(
    key: state.key,
    connection: state.connection,
    data: changed,
    epoch: state.epoch,
  );
}
