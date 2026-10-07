import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/ui/widgets/tri_state.dart';

/// What [overwrite] says about [bit] (§8.2): allowed, denied, or left to
/// the roles (inherit).
TriState overwriteState(PermissionOverwrite? overwrite, Permissions bit) {
  if (overwrite == null) return TriState.inherit;
  if (overwrite.allow.has(bit)) return TriState.allow;
  if (overwrite.deny.has(bit)) return TriState.deny;
  return TriState.inherit;
}

/// [overwrite] with [bit] set to [state].
PermissionOverwrite withState(
  PermissionOverwrite overwrite,
  Permissions bit,
  TriState state,
) => PermissionOverwrite(
  targetKind: overwrite.targetKind,
  targetId: overwrite.targetId,
  allow: state == TriState.allow
      ? overwrite.allow | bit
      : overwrite.allow - bit,
  deny: state == TriState.deny ? overwrite.deny | bit : overwrite.deny - bit,
);

/// Whether someone holding [held] in a channel may set [bit] of
/// [overwrite] to [state]. Like the server: they need Manage roles there,
/// and the whole overwrite they send may only allow or deny permissions
/// they hold (§6.3).
bool canSetState(
  Permissions held,
  PermissionOverwrite overwrite,
  Permissions bit,
  TriState state,
) {
  if (!canGrant(held, Permissions.manageRoles)) return false;
  final next = withState(overwrite, bit, state);
  return canGrant(held, next.allow | next.deny);
}
