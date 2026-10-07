import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/server.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/providers/server_state.dart';
import 'package:opencord/features/settings/server/roles_page.dart';

import '../support/fixtures.dart';

void main() {
  // Mira (not the owner) holds Moderator (position 2) with Manage roles and
  // Kick members; Helper sits below at 1, Admin above at 3.
  const moderator = Role(
    id: 10,
    name: 'Moderator',
    position: 2,
    permissions: Permissions(
      1 << 5 | 1 << 6, // manageRoles | kickMembers
    ),
  );
  const helper = Role(
    id: 11,
    name: 'Helper',
    position: 1,
    permissions: Permissions.none,
  );
  const admin = Role(
    id: 12,
    name: 'Admin',
    position: 3,
    permissions: Permissions.administrator,
  );
  final base = ServerData.fromSnapshot(snapshot());
  final data = base.copyWith(
    info: const ServerInfo(
      name: 'Test',
      everyoneRoleId: everyoneRoleId,
      ownerId: kaiId,
    ),
    roles: {...base.roles, 10: moderator, 11: helper, 12: admin},
    members: {
      ...base.members,
      selfId: member(selfId, 'Alex', roles: [10]),
    },
    serverPermissions: Permissions.manageRoles | Permissions.kickMembers,
  );
  final rules = RoleRules(data);

  test('roles below your own can be managed, not those at or above', () {
    expect(rules.canManage(helper), isTrue);
    expect(rules.canManage(moderator), isFalse);
    expect(rules.canManage(admin), isFalse);
  });

  test('only permissions you hold can be turned on', () {
    expect(rules.canTurnOn(Permissions.kickMembers), isTrue);
    expect(rules.canTurnOn(Permissions.banMembers), isFalse);
  });

  test('without Manage roles nothing can be managed', () {
    final plain = RoleRules(
      data.copyWith(serverPermissions: Permissions.kickMembers),
    );
    expect(plain.canManage(helper), isFalse);
  });
}
