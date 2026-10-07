import 'package:opencord/core/model/channel.dart';

/// The 64 permission bits from `opencord-common`. ADMINISTRATOR is bit 63,
/// so its value is negative: test bits with [has], never with `<` or `>`.
extension type const Permissions(int bits) {
  static const none = Permissions(0);
  static const viewChannel = Permissions(1 << 0);
  static const sendMessages = Permissions(1 << 1);
  static const readHistory = Permissions(1 << 2);
  static const manageMessages = Permissions(1 << 3);
  static const manageChannels = Permissions(1 << 4);
  static const manageRoles = Permissions(1 << 5);
  static const kickMembers = Permissions(1 << 6);
  static const banMembers = Permissions(1 << 7);
  static const createInvite = Permissions(1 << 8);
  static const manageServer = Permissions(1 << 9);
  static const attachFiles = Permissions(1 << 10);
  static const mentionEveryone = Permissions(1 << 11);
  static const changeNickname = Permissions(1 << 12);
  static const manageNicknames = Permissions(1 << 13);
  static const connect = Permissions(1 << 16);
  static const speak = Permissions(1 << 17);
  static const video = Permissions(1 << 18);
  static const screenshare = Permissions(1 << 19);
  static const muteMembers = Permissions(1 << 20);
  static const deafenMembers = Permissions(1 << 21);
  static const moveMembers = Permissions(1 << 22);
  static const prioritySpeaker = Permissions(1 << 23);

  /// Without it, push-to-talk is forced; only clients can enforce it.
  static const useVoiceActivity = Permissions(1 << 24);
  static const useSoundboard = Permissions(1 << 25);

  /// Play sounds from other servers.
  static const useExternalSounds = Permissions(1 << 26);
  static const manageSoundboard = Permissions(1 << 27);
  static const administrator = Permissions(1 << 63);

  /// Every defined bit.
  static const all = Permissions(
    (1 << 0) |
        (1 << 1) |
        (1 << 2) |
        (1 << 3) |
        (1 << 4) |
        (1 << 5) |
        (1 << 6) |
        (1 << 7) |
        (1 << 8) |
        (1 << 9) |
        (1 << 10) |
        (1 << 11) |
        (1 << 12) |
        (1 << 13) |
        (1 << 16) |
        (1 << 17) |
        (1 << 18) |
        (1 << 19) |
        (1 << 20) |
        (1 << 21) |
        (1 << 22) |
        (1 << 23) |
        (1 << 24) |
        (1 << 25) |
        (1 << 26) |
        (1 << 27) |
        (1 << 63),
  );

  /// @everyone on a new server.
  static const defaultEveryone = Permissions(
    (1 << 0) |
        (1 << 1) |
        (1 << 2) |
        (1 << 8) |
        (1 << 12) |
        (1 << 16) |
        (1 << 17) |
        (1 << 18) |
        (1 << 19) |
        (1 << 24) |
        (1 << 25) |
        (1 << 26),
  );

  bool has(Permissions other) => bits & other.bits == other.bits;

  bool get isEmpty => bits == 0;

  Permissions operator |(Permissions other) => Permissions(bits | other.bits);

  Permissions operator &(Permissions other) => Permissions(bits & other.bits);

  /// These permissions without [other].
  Permissions operator -(Permissions other) => Permissions(bits & ~other.bits);
}

/// Whether someone holding [held] may grant or deny [requested].
bool canGrant(Permissions held, Permissions requested) =>
    held.has(Permissions.administrator) || held.has(requested);

/// A role as far as permissions go.
class RoleGrant {
  const RoleGrant({
    required this.id,
    required this.position,
    required this.permissions,
  });

  final int id;
  final int position;
  final Permissions permissions;
}

/// Resolves a member's permissions the way the server does
/// (`opencord_common::permissions`).
class PermissionContext {
  const PermissionContext({
    required this.userId,
    required this.isOwner,
    required this.everyoneRoleId,
    required this.everyone,
    required this.roles,
  });

  final int userId;
  final bool isOwner;
  final int everyoneRoleId;

  /// Permissions of the @everyone role.
  final Permissions everyone;

  /// The member's roles, without @everyone.
  final List<RoleGrant> roles;

  /// Permissions before channel overwrites.
  Permissions get base {
    if (isOwner) return Permissions.all;
    final combined = roles.fold(
      everyone,
      (permissions, role) => permissions | role.permissions,
    );
    return combined.has(Permissions.administrator) ? Permissions.all : combined;
  }

  /// Permissions in a channel with [overwrites].
  Permissions inChannel(List<PermissionOverwrite> overwrites) {
    final start = base;
    if (start.has(Permissions.administrator)) return Permissions.all;
    Permissions apply(Permissions value, PermissionOverwrite? overwrite) =>
        overwrite == null ? value : (value - overwrite.deny) | overwrite.allow;

    final afterEveryone = apply(
      start,
      overwrites
          .where(
            (o) =>
                o.targetKind == OverwriteTargetKind.role &&
                o.targetId == everyoneRoleId,
          )
          .firstOrNull,
    );
    var roleAllow = Permissions.none;
    var roleDeny = Permissions.none;
    for (final overwrite in overwrites) {
      final held = roles.any((role) => role.id == overwrite.targetId);
      if (overwrite.targetKind == OverwriteTargetKind.role &&
          overwrite.targetId != everyoneRoleId &&
          held) {
        roleAllow = roleAllow | overwrite.allow;
        roleDeny = roleDeny | overwrite.deny;
      }
    }
    final afterRoles = (afterEveryone - roleDeny) | roleAllow;
    final resolved = apply(
      afterRoles,
      overwrites
          .where(
            (o) =>
                o.targetKind == OverwriteTargetKind.member &&
                o.targetId == userId,
          )
          .firstOrNull,
    );
    return resolved.has(Permissions.viewChannel) ? resolved : Permissions.none;
  }

  /// Position of the member's highest role; 0 (@everyone) without roles.
  int get highestPosition => roles.fold(
    0,
    (highest, role) => role.position > highest ? role.position : highest,
  );

  bool outranksMember({
    required bool targetIsOwner,
    required int targetHighest,
  }) => !targetIsOwner && (isOwner || highestPosition > targetHighest);

  bool outranksRole(int position) => isOwner || position < highestPosition;
}
