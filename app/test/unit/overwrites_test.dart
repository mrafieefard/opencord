import 'package:flutter_test/flutter_test.dart';
import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/features/settings/server/overwrites.dart';
import 'package:opencord/ui/widgets/tri_state.dart';

const empty = PermissionOverwrite(
  targetKind: OverwriteTargetKind.role,
  targetId: 1,
  allow: Permissions.none,
  deny: Permissions.none,
);

void main() {
  test('an overwrite says allow, deny or nothing about a bit', () {
    final overwrite = PermissionOverwrite(
      targetKind: OverwriteTargetKind.role,
      targetId: 1,
      allow: Permissions.sendMessages,
      deny: Permissions.viewChannel,
    );

    expect(overwriteState(overwrite, Permissions.sendMessages), TriState.allow);
    expect(overwriteState(overwrite, Permissions.viewChannel), TriState.deny);
    expect(
      overwriteState(overwrite, Permissions.readHistory),
      TriState.inherit,
    );
    expect(overwriteState(null, Permissions.readHistory), TriState.inherit);
  });

  test('setting a state moves the bit between allow and deny', () {
    final allowed = withState(empty, Permissions.sendMessages, TriState.allow);
    expect(allowed.allow, Permissions.sendMessages);
    expect(allowed.deny, Permissions.none);

    final denied = withState(allowed, Permissions.sendMessages, TriState.deny);
    expect(denied.allow, Permissions.none);
    expect(denied.deny, Permissions.sendMessages);

    final inherited = withState(
      denied,
      Permissions.sendMessages,
      TriState.inherit,
    );
    expect(inherited.allow, Permissions.none);
    expect(inherited.deny, Permissions.none);
    expect(inherited.targetId, 1);
  });

  group('who may change an overwrite (§6.3)', () {
    final moderator =
        Permissions.manageRoles |
        Permissions.viewChannel |
        Permissions.sendMessages;

    test('needs Manage roles in the channel', () {
      final held = moderator - Permissions.manageRoles;
      for (final state in TriState.values) {
        expect(
          canSetState(held, empty, Permissions.sendMessages, state),
          isFalse,
        );
      }
    });

    test('allows or denies only permissions the editor holds', () {
      for (final state in [TriState.allow, TriState.deny]) {
        expect(
          canSetState(moderator, empty, Permissions.sendMessages, state),
          isTrue,
        );
        expect(
          canSetState(moderator, empty, Permissions.manageMessages, state),
          isFalse,
        );
      }
      expect(
        canSetState(
          Permissions.administrator,
          empty,
          Permissions.manageMessages,
          TriState.allow,
        ),
        isTrue,
      );
    });

    test('the whole overwrite is checked again on every change', () {
      final other = withState(empty, Permissions.manageMessages, TriState.deny);

      expect(
        canSetState(moderator, other, Permissions.sendMessages, TriState.allow),
        isFalse,
      );
      expect(
        canSetState(
          moderator,
          other,
          Permissions.manageMessages,
          TriState.inherit,
        ),
        isTrue,
      );
    });
  });
}
