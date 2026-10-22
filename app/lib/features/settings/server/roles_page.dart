import 'package:flutter/foundation.dart';
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
import 'package:opencord/ui/widgets/oc_switch.dart';
import 'package:opencord/ui/widgets/popover.dart';
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

class _ServerRolesPageState extends ConsumerState<ServerRolesPage>
    with SaveBarChanges {
  int? _selected;
  var _creating = false;

  /// The roles dragged into a new order (highest first), waiting for the
  /// save bar; null when none were.
  List<int>? _order;

  OpencordRepository get _repository => ref.read(repositoryProvider);

  /// Another role's editor, once unsaved changes may go.
  Future<void> _pick(int role) async {
    if (role == _selected) return;
    if (!await confirmLeavingUnsaved(context) || !mounted) return;
    setState(() => _selected = role);
  }

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

  /// The order of the roles the user may move as saved (highest first);
  /// kept here for the save bar (see [SaveBarChanges]).
  var _savedOrder = const <int>[];

  @override
  bool get unsavedChanges => _order != null && !listEquals(_order, _savedOrder);

  /// Moves [role] to [index] among [order] (highest first), the roles the
  /// user may move.
  void _move(List<Role> order, Role role, int index) {
    final moved = [...order]..remove(role);
    moved.insert(index.clamp(0, moved.length), role);
    setState(() => _order = [for (final role in moved) role.id]);
    changesEdited();
  }

  @override
  Future<void> saveChanges() async {
    final order = _order;
    if (order == null) return;
    await _repository.reorderRoles(widget.serverKey, order.reversed.toList());
    if (mounted) setState(() => _order = null);
  }

  @override
  void discardChanges() {
    if (mounted) setState(() => _order = null);
  }

  /// [roles] (highest first) with those the user may move in [_order].
  List<Role> _ordered(List<Role> roles, List<Role> movable) {
    final order = _order;
    if (order == null) return roles;
    final byId = {for (final role in movable) role.id: role};
    final moved = [
      for (final id in order) ?byId[id],
      for (final role in movable)
        if (!order.contains(role.id)) role,
    ].iterator;
    // The roles the user may not move keep their places.
    return [
      for (final role in roles)
        if (byId.containsKey(role.id) && moved.moveNext())
          moved.current
        else
          role,
    ];
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
    final saved = [
      for (final role in data.rolesDescending)
        if (role.id != data.info.everyoneRoleId) role,
    ];
    final savedMovable = [
      for (final role in saved)
        if (rules.canManage(role)) role,
    ];
    _savedOrder = [for (final role in savedMovable) role.id];
    final roles = _ordered(saved, savedMovable);
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
                      onTap: () => _pick(role.id),
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
                  onTap: () => _pick(everyone.id),
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

/// A role's name, options, upload limit and permissions, waiting for the
/// save bar as in Discord.
class _RoleEditorState extends ConsumerState<_RoleEditor> with SaveBarChanges {
  late final _name = TextEditingController(text: widget.role.name)
    ..addListener(_edited);
  late final _limitController = TextEditingController()..addListener(_edited);
  String _unit = 'MB';
  bool _notSet = false;
  late var _hoist = widget.role.hoist;
  late var _mentionable = widget.role.mentionable;
  late var _permissions = widget.role.permissions;
  String? _error;

  /// What the server allows; kept here for the save bar (see
  /// [SaveBarChanges]).
  late ({int ceiling, int own, bool owner}) _limits = _readLimits();

  ({int ceiling, int own, bool owner}) _readLimits() {
    final data = ref.read(serverProvider(widget.serverKey)).data;
    return (
      ceiling: data?.uploadSettings.maxFileBytes ?? 50 * 1024 * 1024,
      own: data?.myUploadLimit ?? 0,
      owner: data?.isOwner ?? false,
    );
  }

  OpencordRepository get _repository => ref.read(repositoryProvider);

  void _edited() {
    setState(() {});
    changesEdited();
  }

  void _edit(VoidCallback change) {
    setState(change);
    changesEdited();
  }

  bool get _nameChanged => _name.text.trim() != widget.role.name;

  bool get _optionsChanged =>
      _hoist != widget.role.hoist ||
      _mentionable != widget.role.mentionable ||
      _permissions != widget.role.permissions;

  int? get _parsedBytes {
    if (_notSet) return 0;
    final num = int.tryParse(_limitController.text.trim());
    if (num == null) return null;
    return num * (_unit == 'GB' ? 1024 * 1024 * 1024 : 1024 * 1024);
  }

  String? get _limitError {
    if (_notSet) {
      if (widget.everyone) return 'A limit is required for @everyone';
      return null;
    }
    final text = _limitController.text.trim();
    if (text.isEmpty) return 'Enter a number';
    final num = int.tryParse(text);
    if (num == null || num <= 0) return 'Enter a valid positive number';
    final bytes = num * (_unit == 'GB' ? 1024 * 1024 * 1024 : 1024 * 1024);
    if (bytes < 1024 * 1024) return 'Must be at least 1 MB';

    final limits = _limits;
    if (bytes > limits.ceiling) {
      return 'Cannot exceed server maximum (${formatFileSize(limits.ceiling)})';
    }

    if (!limits.owner && bytes > limits.own) {
      return "You can't set more than your own limit (${formatFileSize(limits.own)})";
    }

    return null;
  }

  bool get _limitChanged {
    final current = widget.role.maxUploadBytes;
    if (_notSet) {
      return current != null && current > 0;
    }
    final bytes = _parsedBytes;
    if (bytes == null) return false;
    if (current == null) {
      if (widget.everyone) return bytes != _limits.ceiling;
      return true;
    }
    return bytes != current;
  }

  /// A limit typed but not a number yet counts as a change, so the bar
  /// shows and saving says what is wrong.
  bool get _limitEdited => _limitChanged || (!_notSet && _parsedBytes == null);

  @override
  bool get unsavedChanges => _nameChanged || _optionsChanged || _limitEdited;

  @override
  void initState() {
    super.initState();
    _initUploadLimit();
  }

  void _initUploadLimit() {
    final current = widget.role.maxUploadBytes;
    if (current != null && current > 0) {
      _notSet = false;
      if (current >= 1024 * 1024 * 1024 &&
          current % (1024 * 1024 * 1024) == 0) {
        _unit = 'GB';
        _limitController.text = (current ~/ (1024 * 1024 * 1024)).toString();
      } else {
        _unit = 'MB';
        _limitController.text = (current ~/ (1024 * 1024)).toString();
      }
    } else {
      if (widget.everyone) {
        _notSet = false;
        _unit = 'MB';
        _limitController.text = (_limits.ceiling ~/ (1024 * 1024)).toString();
      } else {
        _notSet = true;
        _unit = 'MB';
        _limitController.text = '';
      }
    }
  }

  @override
  void didUpdateWidget(_RoleEditor oldWidget) {
    super.didUpdateWidget(oldWidget);
    final (old, role) = (oldWidget.role, widget.role);
    // Saved here or elsewhere: what is not being edited follows it.
    if (_hoist == old.hoist) _hoist = role.hoist;
    if (_mentionable == old.mentionable) _mentionable = role.mentionable;
    if (_permissions == old.permissions) _permissions = role.permissions;
    if (_name.text.trim() == old.name && old.name != role.name) {
      _name.text = role.name;
    }
    if (old.maxUploadBytes != role.maxUploadBytes) _initUploadLimit();
  }

  @override
  void dispose() {
    _name.dispose();
    _limitController.dispose();
    super.dispose();
  }

  Never _refuse(String problem) {
    if (mounted) setState(() => _error = problem);
    throw RepoException(RepoErrorKind.invalidArgument, problem);
  }

  @override
  Future<void> saveChanges() async {
    final name = _name.text.trim();
    if (name.isEmpty) _refuse('The role needs a name.');
    if (_limitEdited) {
      if (_limitError case final problem?) _refuse(problem);
    }
    if (mounted) setState(() => _error = null);
    final role = widget.role;
    try {
      await _repository.updateRole(
        widget.serverKey,
        role.id,
        name: _nameChanged ? name : null,
        permissions: _permissions != role.permissions ? _permissions : null,
        hoist: _hoist != role.hoist ? _hoist : null,
        mentionable: _mentionable != role.mentionable ? _mentionable : null,
        maxUploadBytes: _limitChanged ? _parsedBytes ?? 0 : null,
      );
    } on RepoException catch (error) {
      if (mounted) setState(() => _error = error.message);
      rethrow;
    }
  }

  @override
  void discardChanges() {
    final role = widget.role;
    _name.text = role.name;
    if (!mounted) return;
    setState(() {
      _hoist = role.hoist;
      _mentionable = role.mentionable;
      _permissions = role.permissions;
      _error = null;
      _initUploadLimit();
    });
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
    ref.watch(
      serverProvider(widget.serverKey).select(
        (s) => (
          s.data?.uploadSettings.maxFileBytes,
          s.data?.myUploadLimit,
          s.data?.isOwner,
        ),
      ),
    );
    _limits = _readLimits();
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
              child: OcTextField(
                controller: _name,
                enabled: editable && !widget.everyone,
                semanticLabel: 'Role name',
                maxLength: 100,
              ),
            ),
            if (!widget.everyone) ...[
              SettingsSwitchRow(
                title: 'Display separately',
                subtitle: 'Its members get their own section in the list',
                value: _hoist,
                onChanged: editable
                    ? (value) => _edit(() => _hoist = value)
                    : null,
              ),
              SettingsSwitchRow(
                title: 'Allow anyone to @mention it',
                value: _mentionable,
                onChanged: editable
                    ? (value) => _edit(() => _mentionable = value)
                    : null,
              ),
            ],
            Padding(
              padding: const EdgeInsets.symmetric(
                horizontal: OcSpace.s16,
                vertical: OcSpace.s12,
              ),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    'Maximum file size',
                    style: OcText.bodyStrong.copyWith(color: colors.text),
                  ),
                  const SizedBox(height: 2),
                  Text(
                    widget.everyone
                        ? 'The largest file anyone can send, unless one of '
                              'their roles allows more.'
                        : 'The largest file members with this role can '
                              'send. Members get the largest limit among '
                              'their roles.',
                    style: OcText.small.copyWith(color: colors.textSecondary),
                  ),
                  const SizedBox(height: OcSpace.s8),
                  Row(
                    children: [
                      Expanded(
                        child: OcTextField(
                          key: const Key('role-max-upload-size'),
                          controller: _limitController,
                          enabled: editable && !_notSet,
                          semanticLabel: 'Maximum file size',
                          hint: _notSet ? 'No limit of its own' : 'Size',
                          inputFormatters: [
                            FilteringTextInputFormatter.digitsOnly,
                          ],
                          suffix: _UnitPicker(
                            unit: _unit,
                            enabled: editable && !_notSet,
                            onPicked: (unit) => _edit(() => _unit = unit),
                          ),
                        ),
                      ),
                      if (!widget.everyone) ...[
                        const SizedBox(width: OcSpace.s12),
                        Row(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            OcSwitch(
                              value: _notSet,
                              semanticLabel: 'Not set',
                              onChanged: editable
                                  ? (value) => _edit(() => _notSet = value)
                                  : null,
                            ),
                            const SizedBox(width: OcSpace.s6),
                            Text(
                              'Not set',
                              style: OcText.body.copyWith(color: colors.text),
                            ),
                          ],
                        ),
                      ],
                    ],
                  ),
                  if (_limitError case final err?) ...[
                    const SizedBox(height: OcSpace.s8),
                    InlineError(err),
                  ],
                ],
              ),
            ),
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
                  value: _permissions.has(info.bit),
                  editable: editable,
                  canChange: rules.canChange(info.bit),
                  onChanged: (on) => _edit(
                    () => _permissions = on
                        ? _permissions | info.bit
                        : _permissions - info.bit,
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

/// MB or GB, picked inside the size field it belongs to.
class _UnitPicker extends StatelessWidget {
  const _UnitPicker({
    required this.unit,
    required this.enabled,
    required this.onPicked,
  });

  final String unit;
  final bool enabled;
  final ValueChanged<String> onPicked;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Padding(
      padding: const EdgeInsets.only(right: OcSpace.s4),
      child: Hoverable(
        onTap: enabled
            ? () {
                final rect = globalRectOf(context);
                showOcMenu(
                  context: context,
                  position: rect.bottomLeft.translate(0, 4),
                  entries: [
                    for (final each in const ['MB', 'GB'])
                      OcMenuItem(
                        label: each,
                        checked: unit == each,
                        onSelected: () => onPicked(each),
                      ),
                  ],
                );
              }
            : null,
        semanticLabel: 'Unit $unit',
        focusRadius: BorderRadius.circular(6),
        builder: (context, state) => Container(
          padding: const EdgeInsets.symmetric(
            horizontal: OcSpace.s6,
            vertical: 2,
          ),
          decoration: BoxDecoration(
            color: state.active ? colors.selected : null,
            borderRadius: BorderRadius.circular(6),
          ),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(
                unit,
                style: OcText.bodyStrong.copyWith(
                  color: enabled ? colors.text : colors.textMuted,
                ),
              ),
              Icon(
                OcIcons.expandMore,
                size: 16,
                color: enabled ? colors.textSecondary : colors.textMuted,
              ),
            ],
          ),
        ),
      ),
    );
  }
}
