import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/providers/server_state.dart';
import 'package:opencord/core/repository/events.dart';

import '../../support/fixtures.dart';

ServerState ready() => reduceServer(
  const ServerState(key: serverKey),
  Ready(serverKey, snapshot()),
);

void main() {
  test('starts connecting, with nothing known', () {
    const state = ServerState(key: serverKey);

    expect(state.connection.phase, ConnectionPhase.connecting);
    expect(state.data, isNull);
  });

  test('Ready replaces everything and means connected', () {
    final state = ready();

    expect(state.connection.isConnected, isTrue);
    expect(state.data!.self.id, selfId);
    expect(state.data!.channels.keys, [generalId, randomId]);
    expect(state.data!.members[kaiId]!.displayName, 'Kai');
    expect(state.data!.isOwner, isTrue);
  });

  test('losing the connection keeps the last state to show greyed out', () {
    final reconnecting = reduceServer(
      ready(),
      ConnectionChanged(
        serverKey,
        ConnectionStatus.reconnecting(attempt: 2, retryAt: t0),
      ),
    );

    expect(reconnecting.connection.phase, ConnectionPhase.reconnecting);
    expect(reconnecting.data, isNotNull);
  });

  test('channels are added, replaced and removed', () {
    var state = ready();

    state = reduceServer(
      state,
      const ChannelUpserted(
        serverKey,
        Channel(id: 12, kind: ChannelKind.voice, name: 'Lounge'),
      ),
    );
    state = reduceServer(
      state,
      const ChannelUpserted(
        serverKey,
        Channel(id: generalId, kind: ChannelKind.text, name: 'chat'),
      ),
    );
    state = reduceServer(state, const ChannelDeleted(serverKey, randomId));

    expect(state.data!.channels[12]!.name, 'Lounge');
    expect(state.data!.channels[generalId]!.name, 'chat');
    expect(state.data!.channels.containsKey(randomId), isFalse);
  });

  test('deleting a role also removes it from its members', () {
    final state = reduceServer(ready(), const RoleDeleted(serverKey, 2));

    expect(state.data!.roles.containsKey(2), isFalse);
    expect(state.data!.members[kaiId]!.roleIds, isEmpty);
  });

  test('members join, change and leave', () {
    var state = ready();

    state = reduceServer(
      state,
      MemberUpserted(serverKey, member(1003, 'Jonas'), joined: true),
    );
    state = reduceServer(
      state,
      MemberUpserted(
        serverKey,
        state.data!.members[kaiId]!.copyWith(nickname: () => 'kai'),
      ),
    );
    state = reduceServer(state, const MemberLeft(serverKey, miraId));

    expect(state.data!.members[1003]!.displayName, 'Jonas');
    expect(state.data!.members[kaiId]!.displayName, 'kai');
    expect(state.data!.members.containsKey(miraId), isFalse);
  });

  test('server info and own permissions are replaced', () {
    var state = ready();

    state = reduceServer(
      state,
      ServerInfoChanged(serverKey, state.data!.info.copyWith(name: 'Renamed')),
    );
    state = reduceServer(
      state,
      const PermissionsChanged(serverKey, Permissions.viewChannel, {
        generalId: Permissions.viewChannel,
      }),
    );

    expect(state.data!.info.name, 'Renamed');
    expect(state.data!.can(Permissions.manageChannels), isFalse);
    expect(state.data!.permissionsIn(generalId), Permissions.viewChannel);
    expect(state.data!.permissionsIn(randomId), Permissions.none);
  });

  test('events that change nothing keep the same state object', () {
    final state = ready();

    final after = reduceServer(
      state,
      const PresenceChanged(serverKey, kaiId, Presence.online),
    );

    expect(identical(after, state), isTrue);
  });

  test('roles rank from highest to lowest', () {
    final data = ready().data!;

    expect(data.rolesDescending.map((role) => role.name), [
      'Maintainer',
      '@everyone',
    ]);
    expect(data.highestRole(data.members[kaiId]!)?.name, 'Maintainer');
    expect(data.highestHoistedRole(data.members[miraId]!), isNull);
  });

  test('a member of nothing gets no role', () {
    final data = ready().data!;
    final stranger = Member(user: user(5, 'X'), joinedAt: t0);

    expect(data.highestRole(stranger), isNull);
  });
}
