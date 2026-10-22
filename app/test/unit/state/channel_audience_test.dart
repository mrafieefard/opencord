import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/providers/channel_audience.dart';
import 'package:opencord/core/providers/server_state.dart';

import '../../support/fixtures.dart';

void main() {
  final data = ServerData.fromSnapshot(snapshot());

  test('everyone sees an open channel', () {
    final general = data.channels[generalId]!;

    expect(channelViewers(data, general).map((m) => m.id), [
      selfId,
      kaiId,
      miraId,
    ]);
  });

  test('a private channel counts the owner and those let in', () {
    const private = Channel(
      id: 50,
      kind: ChannelKind.text,
      name: 'maintainers',
      overwrites: [
        PermissionOverwrite(
          targetKind: OverwriteTargetKind.role,
          targetId: everyoneRoleId,
          allow: Permissions.none,
          deny: Permissions.viewChannel,
        ),
        PermissionOverwrite(
          targetKind: OverwriteTargetKind.role,
          targetId: 2,
          allow: Permissions.viewChannel,
          deny: Permissions.none,
        ),
      ],
    );

    // Alex owns the server; Kai holds the Maintainer role.
    expect(channelViewers(data, private).map((m) => m.id), [selfId, kaiId]);
  });

  test('online counts everyone not shown as offline', () {
    final members = channelViewers(data, data.channels[generalId]!);

    expect(
      onlineCount(members, const {
        selfId: Presence.online,
        kaiId: Presence.idle,
        miraId: Presence.offline,
      }),
      2,
    );
  });
}
