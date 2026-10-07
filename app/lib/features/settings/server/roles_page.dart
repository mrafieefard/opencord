import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/settings/server/permission_info.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/confirm_dialog.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_text_field.dart';
import 'package:opencord/ui/widgets/settings.dart';
import 'package:opencord/ui/widgets/toast.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/ellipsis_text.dart';

/// What the current user may do with roles: whom they outrank and which
/// permissions they hold to give (Phase 1 plan §6.3).
class RoleRules {
  RoleRules(this.data) : _self = data.selfMember;

  final ServerData data;
  final Member? _self;

  PermissionContext? get _context =>
      _self == null ? null : data.contextFor(_self);

  /// @everyone included: the server wants a role above it.
  bool canManage(Role role) {
    final context = _context;
    if (context == null || !data.can(Permissions.manageRoles)) return false;
    return context.outranksRole(role.position);
  }

  /// Turning [bit] on or off both need holding it: the server checks every
  /// bit that changes.
  bool canChange(Permissions bit) {
    final context = _context;
    return context != null && canGrant(context.base, bit);
  }
}

/// Roles (§8.2): the list on the left, highest first with @everyone pinned
/// at the bottom; the chosen role's name, display options and permissions
/// on the right.
class ServerRolesPage extends ConsumerStatefulWidget {
  const ServerRolesPage({super.key, required this.serverKey});

  final String serverKey;

  @override
  ConsumerState<ServerRolesPage> createState() => _ServerRolesPageState();
}

class _ServerRolesPageState extends ConsumerState<ServerRolesPage> {
  int? _selected;
  var _creating = false;

  OpencordRepository get _repository => ref.read(repositoryProvider);

  Future<void> _create() async {
    setState(() => _creating = true);
    try {
      final role = await _repository.createRole(
        widget.serverKey,
        name: 'New role',
      );
      if (mounted) setState(() => _selected = role.id);
    } on RepoException catch (error) {
      if (mounted) showOcToast(context, error.message);
    }
    if (mounted) setState(() => _creating = false);
  }

  /// Moves [role] to [index] among [order] (highest first) and saves the
  /// new order of the roles the user may move.
  Future<void> _move(List<Role> order, Role role, int index) async {
    final moved = [...order]..remove(role);
    moved.insert(index.clamp(0, moved.length), role);
    try {
      await _repository.reorderRoles(widget.serverKey, [
        for (final role in moved.reversed) role.id,
      ]);
    } on RepoException catch (error) {
      if (mounted) showOcToast(context, error.message);
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final data = ref.watch(
      serverProvider(widget.serverKey).select((state) => state.data),
    );
    if (data == null) return const SizedBox.shrink();
    final rules = RoleRules(data);
    final everyone = data.roles[data.info.everyoneRoleId];
    final roles = [
      for (final role in data.rolesDescending)
        if (role.id != data.info.everyoneRoleId) role,
    ];
    final movable = [
      for (final role in roles)
        if (rules.canManage(role)) role,
    ];
    final selected = data.roles[_selected] ?? roles.firstOrNull ?? everyone;
    int count(Role role) => role.id == data.info.everyoneRoleId
        ? data.members.length
        : data.members.values
              .where((member) => member.roleIds.contains(role.id))
              .length;
    return Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SizedBox(
          width: 220,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: [
              OcButton(
                label: 'Create role',
                icon: OcIcons.add,
                busy: _creating,
                onPressed: data.can(Permissions.manageRoles) ? _create : null,
              ),
              const SizedBox(height: OcSpace.s12),
              ReorderableListView(
                shrinkWrap: true,
                physics: const NeverScrollableScrollPhysics(),
                buildDefaultDragHandles: false,
                proxyDecorator: (child, index, animation) =>
                    Material(type: MaterialType.transparency, child: child),
                onReorderItem: (from, to) {
                  final role = roles[from];
                  if (!movable.contains(role)) return;
                  // Only among the roles the user outranks.
                  final target = roles.take(to).where(movable.contains).length;
                  _move(movable, role, target);
                },
                children: [
                  for (final (index, role) in roles.indexed)
                    _RoleTile(
                      key: ValueKey(role.id),
                      index: index,
                      role: role,
                      members: count(role),
                      selected: role.id == selected?.id,
                      draggable: movable.contains(role),
                      onTap: () => setState(() => _selected = role.id),
                    ),
                ],
              ),
              if (everyone != null) ...[
                Divider(height: OcSpace.s16, color: colors.border),
                _RoleTile(
                  role: everyone,
                  members: count(everyone),
                  selected: everyone.id == selected?.id,
                  draggable: false,
                  onTap: () => setState(() => _selected = everyone.id),
                ),
              ],
            ],
          ),
        ),
        const SizedBox(width: OcSpace.s24),
        Expanded(
          child: selected == null
              ? const SizedBox.shrink()
              : _RoleEditor(
                  key: ValueKey(selected.id),
                  serverKey: widget.serverKey,
                  role: selected,
                  everyone: selected.id == data.info.everyoneRoleId,
                  rules: rules,
                  onDeleted: () => setState(() => _selected = null),
                ),
        ),
      ],
    );
  }
}

class _RoleTile extends StatelessWidget {
  const _RoleTile({
    super.key,
    required this.role,
    required this.members,
    required this.selected,
    required this.draggable,
    required this.onTap,
    this.index,
  });

  final Role role;
  final int members;
  final bool selected;
  final bool draggable;
  final VoidCallback onTap;

  /// Its place in the reorderable list, when it is in one.
  final int? index;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final tile = Hoverable(
      onTap: onTap,
      onSecondaryTap: (position) => showOcMenu(
        context: context,
        position: position,
        entries: [
          OcMenuItem(
            label: 'Copy role ID',
            icon: OcIcons.badge,
            onSelected: () {
              Clipboard.setData(ClipboardData(text: '${role.id}'));
              showOcToast(context, 'Role ID copied');
            },
          ),
        ],
      ),
      semanticLabel: '${role.name}, ${countLabel(members)} members',
      selected: selected,
      builder: (context, state) => AnimatedContainer(
        duration: OcMotion.of(context).hover,
        height: 36,
        margin: const EdgeInsets.only(bottom: 2),
        padding: const EdgeInsets.only(left: OcSpace.s4, right: OcSpace.s8),
        decoration: BoxDecoration(
          color: selected
              ? colors.selected
              : state.active
              ? colors.hover
              : null,
          borderRadius: BorderRadius.circular(OcRadius.row),
        ),
        child: Row(
          children: [
            SizedBox(
              width: 22,
              child: draggable
                  ? Icon(
                      OcIcons.dragIndicator,
                      size: 16,
                      color: colors.textMuted,
                    )
                  : null,
            ),
            Expanded(
              child: EllipsisText(
                role.name,
                style: OcText.body.copyWith(
                  fontWeight: selected ? FontWeight.w600 : FontWeight.w400,
                  color: colors.text,
                ),
              ),
            ),
            Text(
              countLabel(members),
              style: OcText.small.copyWith(color: colors.textMuted),
            ),
          ],
        ),
      ),
    );
    if (!draggable || index == null) return tile;
    return ReorderableDragStartListener(index: index!, child: tile);
  }
}

class _RoleEditor extends ConsumerStatefulWidget {
  const _RoleEditor({
    super.key,
    required this.serverKey,
    required this.role,
    required this.everyone,
    required this.rules,
    required this.onDeleted,
  });

  final String serverKey;
  final Role role;
  final bool everyone;
  final RoleRules rules;
  final VoidCallback onDeleted;

  @override
  ConsumerState<_RoleEditor> createState() => _RoleEditorState();
}

class _RoleEditorState extends ConsumerState<_RoleEditor> {
  late final _name = TextEditingController(text: widget.role.name)
    ..addListener(() => setState(() {}));
  Map<Object, bool Function()>? _unsaved;
  var _saving = false;
  String? _error;

  OpencordRepository get _repository => ref.read(repositoryProvider);

  bool get _nameChanged =>
      _name.text.trim().isNotEmpty && _name.text.trim() != widget.role.name;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _unsaved = SettingsScope.of(context)?..[this] = () => _nameChanged;
  }

  @override
  void dispose() {
    _unsaved?.remove(this);
    _name.dispose();
    super.dispose();
  }

  Future<void> _update({
    String? name,
    Permissions? permissions,
    bool? hoist,
    bool? mentionable,
  }) async {
    setState(() {
      _saving = name != null;
      _error = null;
    });
    try {
      await _repository.updateRole(
        widget.serverKey,
        widget.role.id,
        name: name,
        permissions: permissions,
        hoist: hoist,
        mentionable: mentionable,
      );
    } on RepoException catch (error) {
      if (mounted) setState(() => _error = error.message);
    }
    if (mounted) setState(() => _saving = false);
  }

  Future<void> _delete() async {
    final delete = await confirmAction(
      context,
      title: 'Delete ${widget.role.name}?',
      message: 'Everyone who has this role loses it.',
      action: 'Delete role',
    );
    if (!delete) return;
    try {
      await _repository.deleteRole(widget.serverKey, widget.role.id);
      widget.onDeleted();
    } on RepoException catch (error) {
      if (mounted) showOcToast(context, error.message);
    }
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final role = widget.role;
    final rules = widget.rules;
    final editable = rules.canManage(role);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        if (!editable)
          Padding(
            padding: const EdgeInsets.only(bottom: OcSpace.s16),
            child: Text(
              'This role is at or above your own, so you can look but not '
              'change it.',
              style: OcText.small.copyWith(color: colors.textSecondary),
            ),
          ),
        SettingsSection(
          title: 'Role',
          children: [
            Padding(
              padding: const EdgeInsets.all(OcSpace.s16),
              child: Row(
                children: [
                  Expanded(
                    child: OcTextField(
                      controller: _name,
                      enabled: editable && !widget.everyone,
                      semanticLabel: 'Role name',
                      maxLength: 100,
                      onSubmitted: (_) {
                        if (_nameChanged) _update(name: _name.text.trim());
                      },
                    ),
                  ),
                  if (!widget.everyone) ...[
                    const SizedBox(width: OcSpace.s8),
                    OcButton.primary(
                      label: 'Save',
                      busy: _saving,
                      onPressed: editable && _nameChanged
                          ? () => _update(name: _name.text.trim())
                          : null,
                    ),
                  ],
                ],
              ),
            ),
            if (!widget.everyone) ...[
              SettingsSwitchRow(
                title: 'Display separately',
                subtitle: 'Its members get their own section in the list',
                value: role.hoist,
                onChanged: editable ? (value) => _update(hoist: value) : null,
              ),
              SettingsSwitchRow(
                title: 'Allow anyone to @mention it',
                value: role.mentionable,
                onChanged: editable
                    ? (value) => _update(mentionable: value)
                    : null,
              ),
            ],
          ],
        ),
        if (_error case final error?) ...[
          const SizedBox(height: OcSpace.s12),
          InlineError(error),
        ],
        for (final (group, infos) in permissionGroups) ...[
          const SizedBox(height: OcSpace.s24),
          SettingsSection(
            title: group,
            footer: group == 'Advanced'
                ? 'Administrators have every permission in every channel, '
                      'whatever the channels say, and can manage everything '
                      'below their own role. Give it only to people you would '
                      'trust with the whole server.'
                : group == 'Voice & video'
                ? 'Voice arrives in Phase 2; these are kept for it.'
                : null,
            children: [
              for (final info in infos)
                _PermissionRow(
                  info: info,
                  value: role.permissions.has(info.bit),
                  editable: editable,
                  canChange: rules.canChange(info.bit),
                  onChanged: (on) => _update(
                    permissions: on
                        ? role.permissions | info.bit
                        : role.permissions - info.bit,
                  ),
                ),
            ],
          ),
        ],
        if (!widget.everyone && editable) ...[
          const SizedBox(height: OcSpace.s24),
          Align(
            alignment: Alignment.centerLeft,
            child: OcButton(
              label: 'Delete role',
              icon: OcIcons.delete,
              onPressed: _delete,
            ),
          ),
        ],
      ],
    );
  }
}

class _PermissionRow extends StatelessWidget {
  const _PermissionRow({
    required this.info,
    required this.value,
    required this.editable,
    required this.canChange,
    required this.onChanged,
  });

  final PermissionInfo info;
  final bool value;
  final bool editable;
  final bool canChange;
  final ValueChanged<bool> onChanged;

  @override
  Widget build(BuildContext context) {
    // On or off, changing it needs holding the permission.
    final enabled = editable && canChange;
    final row = SettingsSwitchRow(
      title: info.later ? '${info.label} (later)' : info.label,
      subtitle: info.description,
      value: value,
      onChanged: enabled ? onChanged : null,
    );
    if (enabled || !editable) return row;
    return Tooltip(
      message: "You don't have this permission, so you can't change it",
      child: row,
    );
  }
}
