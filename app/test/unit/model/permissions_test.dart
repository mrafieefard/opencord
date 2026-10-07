import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/permissions.dart';

const everyoneId = 1;
const userId = 100;
const modRole = 10;
const memberRole = 11;

PermissionContext member({
  bool owner = false,
  Permissions everyone = Permissions.defaultEveryone,
  List<RoleGrant> roles = const [],
}) => PermissionContext(
  userId: userId,
  isOwner: owner,
  everyoneRoleId: everyoneId,
  everyone: everyone,
  roles: roles,
);

PermissionOverwrite role(int id, {Permissions? allow, Permissions? deny}) =>
    PermissionOverwrite(
      targetKind: OverwriteTargetKind.role,
      targetId: id,
      allow: allow ?? Permissions.none,
      deny: deny ?? Permissions.none,
    );

PermissionOverwrite person(int id, {Permissions? allow, Permissions? deny}) =>
    PermissionOverwrite(
      targetKind: OverwriteTargetKind.member,
      targetId: id,
      allow: allow ?? Permissions.none,
      deny: deny ?? Permissions.none,
    );

void main() {
  test('bits match opencord-common', () {
    expect(Permissions.viewChannel.bits, 1);
    expect(Permissions.manageNicknames.bits, 1 << 13);
    expect(Permissions.connect.bits, 1 << 16);
    expect(Permissions.prioritySpeaker.bits, 1 << 23);
    expect(Permissions.administrator.bits, 1 << 63);
    expect(Permissions.administrator.bits, isNegative);
  });

  test('the owner has everything', () {
    expect(member(owner: true).base, Permissions.all);
    expect(
      member(
        owner: true,
      ).inChannel([role(everyoneId, deny: Permissions.viewChannel)]),
      Permissions.all,
    );
  });

  test('administrator grants everything and ignores overwrites', () {
    final admin = member(
      roles: [
        const RoleGrant(
          id: modRole,
          position: 2,
          permissions: Permissions.administrator,
        ),
      ],
    );

    expect(admin.base, Permissions.all);
    expect(
      admin.inChannel([role(everyoneId, deny: Permissions.viewChannel)]),
      Permissions.all,
    );
  });

  test('roles add up on top of @everyone', () {
    final context = member(
      everyone: Permissions.viewChannel,
      roles: [
        const RoleGrant(
          id: modRole,
          position: 2,
          permissions: Permissions.kickMembers,
        ),
        const RoleGrant(
          id: memberRole,
          position: 1,
          permissions: Permissions.sendMessages,
        ),
      ],
    );

    expect(
      context.base,
      Permissions.viewChannel |
          Permissions.kickMembers |
          Permissions.sendMessages,
    );
  });

  test('overwrites apply @everyone, then roles, then the member', () {
    final context = member(
      roles: [
        const RoleGrant(
          id: modRole,
          position: 2,
          permissions: Permissions.none,
        ),
      ],
    );

    final resolved = context.inChannel([
      role(everyoneId, deny: Permissions.sendMessages),
      role(
        modRole,
        allow: Permissions.sendMessages | Permissions.manageMessages,
      ),
      person(userId, deny: Permissions.manageMessages),
    ]);

    expect(resolved.has(Permissions.sendMessages), isTrue);
    expect(resolved.has(Permissions.manageMessages), isFalse);
  });

  test('without VIEW_CHANNEL a channel grants nothing', () {
    final resolved = member().inChannel([
      role(everyoneId, deny: Permissions.viewChannel),
    ]);

    expect(resolved, Permissions.none);
  });

  test('rank decides who can act on whom', () {
    final moderator = member(
      roles: [
        const RoleGrant(
          id: modRole,
          position: 3,
          permissions: Permissions.manageRoles,
        ),
      ],
    );

    expect(moderator.outranksRole(2), isTrue);
    expect(moderator.outranksRole(3), isFalse);
    expect(
      moderator.outranksMember(targetIsOwner: false, targetHighest: 2),
      isTrue,
    );
    expect(
      moderator.outranksMember(targetIsOwner: true, targetHighest: 0),
      isFalse,
    );
    expect(member(owner: true).outranksRole(99), isTrue);
  });

  test('only held permissions can be granted, unless administrator', () {
    expect(canGrant(Permissions.manageRoles, Permissions.banMembers), isFalse);
    expect(
      canGrant(
        Permissions.manageRoles | Permissions.banMembers,
        Permissions.banMembers,
      ),
      isTrue,
    );
    expect(canGrant(Permissions.administrator, Permissions.all), isTrue);
  });
}
