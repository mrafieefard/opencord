import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/channel.dart';
import 'package:opencord/core/model/permissions.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/channels/channel_row.dart';
import 'package:opencord/features/dialogs/create_channel_dialog.dart';
import 'package:opencord/features/settings/server/overwrites.dart';
import 'package:opencord/features/settings/server/permission_info.dart';
import 'package:opencord/features/settings/settings_dialog.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/choice_chips.dart';
import 'package:opencord/ui/widgets/confirm_dialog.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/inline_error.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/oc_text_field.dart';
import 'package:opencord/ui/widgets/popover.dart';
import 'package:opencord/ui/widgets/settings.dart';
import 'package:opencord/ui/widgets/toast.dart';
import 'package:opencord/ui/widgets/tri_state.dart';

enum _Tab { overview, permissions }

/// One channel or category in server settings (§8.2): its Overview and
/// Permissions tabs.
class ChannelEditor extends ConsumerStatefulWidget {
  const ChannelEditor({
    super.key,
    required this.serverKey,
    required this.channel,
    required this.onDeleted,
  });

  final String serverKey;
  final Channel channel;
  final VoidCallback onDeleted;

  @override
  ConsumerState<ChannelEditor> createState() => _ChannelEditorState();
}

class _ChannelEditorState extends ConsumerState<ChannelEditor> {
  var _tab = _Tab.overview;

  @override
  Widget build(BuildContext context) {
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Align(
          alignment: Alignment.centerLeft,
          child: ChoiceChips<_Tab>(
            options: const [
              (_Tab.overview, 'Overview'),
              (_Tab.permissions, 'Permissions'),
            ],
            value: _tab,
            onChanged: (tab) => setState(() => _tab = tab),
          ),
        ),
        const SizedBox(height: OcSpace.s16),
        // Both tabs stay alive so unsaved Overview edits survive a look at
        // the permissions.
        Visibility(
          visible: _tab == _Tab.overview,
          maintainState: true,
          child: _Overview(
            serverKey: widget.serverKey,
            channel: widget.channel,
            onDeleted: widget.onDeleted,
          ),
        ),
        Visibility(
          visible: _tab == _Tab.permissions,
          maintainState: true,
          child: _Permissions(
            serverKey: widget.serverKey,
            channel: widget.channel,
          ),
        ),
      ],
    );
  }
}

class _Overview extends ConsumerStatefulWidget {
  const _Overview({
    required this.serverKey,
    required this.channel,
    required this.onDeleted,
  });

  final String serverKey;
  final Channel channel;
  final VoidCallback onDeleted;

  @override
  ConsumerState<_Overview> createState() => _OverviewState();
}

class _OverviewState extends ConsumerState<_Overview> {
  late final _name = TextEditingController(text: widget.channel.name)
    ..addListener(_changedText);
  late final _topic = TextEditingController(text: widget.channel.topic ?? '')
    ..addListener(_changedText);
  late var _bitrate = widget.channel.bitrate;
  late var _userLimit = widget.channel.userLimit;
  late var _textInVoice = widget.channel.textInVoice;
  late var _pushToTalk = _pushToTalkSaved;
  Map<Object, bool Function()>? _unsaved;
  var _saving = false;
  String? _error;

  OpencordRepository get _repository => ref.read(repositoryProvider);

  bool get _isVoice => widget.channel.kind == ChannelKind.voice;

  int? get _everyoneRoleId =>
      ref.read(serverProvider(widget.serverKey)).data?.info.everyoneRoleId;

  /// The @everyone overwrite here, if there is one.
  PermissionOverwrite? get _everyoneOverwrite {
    final everyone = _everyoneRoleId;
    if (everyone == null) return null;
    return widget.channel.overwrites
        .where((o) => o.targets(OverwriteTargetKind.role, everyone))
        .firstOrNull;
  }

  /// "Push-to-talk required" is @everyone denied Use voice activity here
  /// (Phase 2 plan §5.3).
  bool get _pushToTalkSaved =>
      _everyoneOverwrite?.deny.has(Permissions.useVoiceActivity) ?? false;

  bool get _voiceChanged =>
      _bitrate != widget.channel.bitrate ||
      _userLimit != widget.channel.userLimit ||
      _textInVoice != widget.channel.textInVoice ||
      _pushToTalk != _pushToTalkSaved;

  bool get _changed {
    final name = _name.text.trim();
    if (name.isEmpty) return false;
    final channel = widget.channel;
    return name != channel.name ||
        (channel.kind.isTextLike &&
            _topic.text.trim() != (channel.topic ?? '')) ||
        (_isVoice && _voiceChanged);
  }

  void _changedText() => setState(() {});

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    _unsaved = SettingsScope.of(context)?..[this] = () => _changed;
  }

  @override
  void dispose() {
    _unsaved?.remove(this);
    _name.dispose();
    _topic.dispose();
    super.dispose();
  }

  Future<void> _save() async {
    setState(() {
      _saving = true;
      _error = null;
    });
    final channel = widget.channel;
    try {
      await _repository.updateChannel(
        widget.serverKey,
        channel.id,
        name: _name.text.trim(),
        topic: channel.kind.isTextLike ? _topic.text.trim() : null,
        // Only what changed: a bitrate above a since-lowered server cap
        // stays until someone moves it.
        bitrate: _isVoice && _bitrate != channel.bitrate ? _bitrate : null,
        userLimit: _isVoice && _userLimit != channel.userLimit
            ? _userLimit
            : null,
        textInVoice: _isVoice && _textInVoice != channel.textInVoice
            ? _textInVoice
            : null,
      );
      if (_isVoice && _pushToTalk != _pushToTalkSaved) {
        await _savePushToTalk(_pushToTalk);
      }
    } on RepoException catch (error) {
      if (mounted) setState(() => _error = error.message);
    }
    if (mounted) setState(() => _saving = false);
  }

  Future<void> _savePushToTalk(bool required) async {
    final everyone = _everyoneRoleId;
    if (everyone == null) return;
    final current = _everyoneOverwrite;
    final allow =
        (current?.allow ?? Permissions.none) - Permissions.useVoiceActivity;
    final deny = required
        ? (current?.deny ?? Permissions.none) | Permissions.useVoiceActivity
        : (current?.deny ?? Permissions.none) - Permissions.useVoiceActivity;
    if (allow.isEmpty && deny.isEmpty) {
      if (current != null) {
        await _repository.deleteOverwrite(
          widget.serverKey,
          widget.channel.id,
          OverwriteTargetKind.role,
          everyone,
        );
      }
      return;
    }
    await _repository.setOverwrite(
      widget.serverKey,
      widget.channel.id,
      PermissionOverwrite(
        targetKind: OverwriteTargetKind.role,
        targetId: everyone,
        allow: allow,
        deny: deny,
      ),
    );
  }

  Future<void> _moveTo(int? categoryId) async {
    try {
      await _repository.updateChannel(
        widget.serverKey,
        widget.channel.id,
        parentId: categoryId ?? 0,
      );
    } on RepoException catch (error) {
      if (mounted) showOcToast(context, error.message);
    }
  }

  Future<void> _delete() async {
    final deleted = await confirmDeleteChannel(
      context,
      ref,
      widget.serverKey,
      widget.channel,
    );
    if (deleted) widget.onDeleted();
  }

  void _showCategories(BuildContext anchor, List<Channel> categories) {
    final current = widget.channel.parentId;
    final rect = globalRectOf(anchor);
    showOcMenu(
      context: anchor,
      position: rect.bottomLeft.translate(0, 4),
      entries: [
        OcMenuItem(
          label: 'No category',
          checked: !categories.any((c) => c.id == current),
          onSelected: () => _moveTo(null),
        ),
        for (final category in categories)
          OcMenuItem(
            label: category.name,
            checked: category.id == current,
            onSelected: () => _moveTo(category.id),
          ),
      ],
    );
  }

  /// A voice channel's bitrate, user limit, voice chat and push-to-talk
  /// (Phase 2 plan §5.3).
  List<Widget> _voiceFields() {
    final colors = context.oc;
    final data = ref.watch(serverProvider(widget.serverKey)).data;
    final maxKbps =
        (data?.voiceSettings.maxVoiceBitrate ?? Channel.defaultBitrate) ~/ 1000;
    final kbps = (_bitrate ~/ 1000).clamp(Channel.minBitrate ~/ 1000, maxKbps);
    final held = data?.permissionsIn(widget.channel.id) ?? Permissions.none;
    final canRequirePushToTalk =
        held.has(Permissions.manageRoles) &&
        canGrant(held, Permissions.useVoiceActivity);
    return [
      const SizedBox(height: OcSpace.s16),
      SettingsSection(
        footer:
            'Higher bitrates sound clearer and use more bandwidth. This server '
            'allows up to $maxKbps kbps.',
        children: [
          SettingsSliderRow(
            title: 'Bitrate',
            value: kbps,
            min: Channel.minBitrate ~/ 1000,
            max: maxKbps,
            step: 8,
            shown: '$kbps kbps',
            onChanged: (value) => setState(() => _bitrate = value * 1000),
          ),
          SettingsSliderRow(
            title: 'User limit',
            value: _userLimit,
            min: 0,
            max: Channel.maxUserLimit,
            shown: _userLimit == 0
                ? 'No limit'
                : '$_userLimit ${_userLimit == 1 ? 'person' : 'people'}',
            onChanged: (value) => setState(() => _userLimit = value),
          ),
        ],
      ),
      const SizedBox(height: OcSpace.s16),
      SettingsSection(
        children: [
          SettingsSwitchRow(
            title: 'Text chat in this channel',
            subtitle: 'People in voice can write messages here',
            value: _textInVoice,
            onChanged: (value) => setState(() => _textInVoice = value),
          ),
          SettingsSwitchRow(
            title: 'Push-to-talk required',
            subtitle: canRequirePushToTalk
                ? 'Everyone holds their push-to-talk key to speak here'
                : 'Needs Manage roles and Use voice activity here',
            value: _pushToTalk,
            onChanged: canRequirePushToTalk
                ? (value) => setState(() => _pushToTalk = value)
                : null,
          ),
        ],
      ),
      const SizedBox(height: OcSpace.s8),
      Text(
        'Voice chat and push-to-talk take effect with later versions of '
        'voice.',
        style: OcText.small.copyWith(color: colors.textMuted),
      ),
    ];
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final channel = widget.channel;
    final channels =
        ref.watch(
          serverProvider(
            widget.serverKey,
          ).select((state) => state.data?.channels),
        ) ??
        const <int, Channel>{};
    final categories = channels.values.where((c) => c.isCategory).toList()
      ..sort((a, b) => a.position.compareTo(b.position));
    final parent = channels[channel.parentId];
    Widget label(String text) => Padding(
      padding: const EdgeInsets.only(bottom: OcSpace.s8),
      child: Text(text, style: OcText.label.copyWith(color: colors.textMuted)),
    );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        label(channel.isCategory ? 'CATEGORY NAME' : 'CHANNEL NAME'),
        OcTextField(
          controller: _name,
          semanticLabel: channel.isCategory ? 'Category name' : 'Channel name',
          maxLength: 100,
          inputFormatters: channel.kind.isTextLike
              ? const [KebabFormatter()]
              : null,
        ),
        if (channel.kind.isTextLike) ...[
          const SizedBox(height: OcSpace.s16),
          label('TOPIC'),
          OcTextField(
            controller: _topic,
            semanticLabel: 'Channel topic',
            hint: 'Let everyone know what this channel is for',
            maxLines: 4,
            minLines: 2,
            maxLength: 1024,
          ),
        ],
        if (_isVoice) ..._voiceFields(),
        if (_error case final error?) ...[
          const SizedBox(height: OcSpace.s12),
          InlineError(error),
        ],
        const SizedBox(height: OcSpace.s12),
        Align(
          alignment: Alignment.centerRight,
          child: OcButton.primary(
            label: 'Save changes',
            busy: _saving,
            onPressed: _changed && !_saving ? _save : null,
          ),
        ),
        const SizedBox(height: OcSpace.s24),
        SettingsSection(
          children: [
            if (!channel.isCategory)
              Builder(
                builder: (context) => SettingsRow(
                  title: 'Category',
                  subtitle: parent?.name ?? 'No category',
                  icon: OcIcons.folder,
                  trailing: Icon(
                    OcIcons.expandMore,
                    size: 18,
                    color: colors.textMuted,
                  ),
                  onTap: () => _showCategories(context, categories),
                ),
              ),
            SettingsRow(
              title: channel.isCategory ? 'Delete category' : 'Delete channel',
              icon: OcIcons.delete,
              onTap: _delete,
            ),
          ],
        ),
      ],
    );
  }
}

/// Whose overwrite is being edited.
typedef _Target = ({OverwriteTargetKind kind, int id});

PermissionOverwrite _blank(_Target target) => PermissionOverwrite(
  targetKind: target.kind,
  targetId: target.id,
  allow: Permissions.none,
  deny: Permissions.none,
);

class _Permissions extends ConsumerStatefulWidget {
  const _Permissions({required this.serverKey, required this.channel});

  final String serverKey;
  final Channel channel;

  @override
  ConsumerState<_Permissions> createState() => _PermissionsState();
}

class _PermissionsState extends ConsumerState<_Permissions> {
  _Target? _selected;

  OpencordRepository get _repository => ref.read(repositoryProvider);

  Future<void> _guard(Future<void> Function() action) async {
    try {
      await action();
    } on RepoException catch (error) {
      if (mounted) showOcToast(context, error.message);
    }
  }

  Future<void> _add(BuildContext anchor, ServerData data) async {
    final channel = widget.channel;
    bool missing(_Target target) =>
        !channel.overwrites.any((o) => o.targets(target.kind, target.id));
    final target = await showPopover<_Target>(
      context: anchor,
      anchor: globalRectOf(anchor),
      builder: (context) => _TargetPicker(
        roles: [
          for (final role in data.rolesDescending)
            if (role.id != data.info.everyoneRoleId &&
                missing((kind: OverwriteTargetKind.role, id: role.id)))
              role,
        ],
        members: [
          for (final member in data.members.values)
            if (missing((kind: OverwriteTargetKind.member, id: member.id)))
              member,
        ],
      ),
    );
    if (target == null || !mounted) return;
    await _guard(
      () => _repository.setOverwrite(
        widget.serverKey,
        channel.id,
        _blank(target),
      ),
    );
    if (mounted) setState(() => _selected = target);
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final data = ref.watch(
      serverProvider(widget.serverKey).select((state) => state.data),
    );
    if (data == null) return const SizedBox.shrink();
    final channel = data.channels[widget.channel.id] ?? widget.channel;
    final everyone = (
      kind: OverwriteTargetKind.role,
      id: data.info.everyoneRoleId,
    );
    PermissionOverwrite? overwriteOf(_Target target) => channel.overwrites
        .where((o) => o.targets(target.kind, target.id))
        .firstOrNull;
    String nameOf(_Target target) => switch (target.kind) {
      OverwriteTargetKind.role => data.roles[target.id]?.name ?? 'Deleted role',
      OverwriteTargetKind.member =>
        data.members[target.id]?.displayName ?? 'Former member',
    };
    final members =
        [
          for (final overwrite in channel.overwrites)
            if (overwrite.targetKind == OverwriteTargetKind.member)
              (kind: OverwriteTargetKind.member, id: overwrite.targetId),
        ]..sort(
          (a, b) => nameOf(a).toLowerCase().compareTo(nameOf(b).toLowerCase()),
        );
    final targets = <_Target>[
      everyone,
      for (final role in data.rolesDescending)
        if (role.id != data.info.everyoneRoleId &&
            overwriteOf((kind: OverwriteTargetKind.role, id: role.id)) != null)
          (kind: OverwriteTargetKind.role, id: role.id),
      ...members,
    ];
    final target = targets.contains(_selected) ? _selected! : everyone;
    final overwrite = overwriteOf(target);
    final held = data.permissionsIn(channel.id);
    final canManage = canGrant(held, Permissions.manageRoles);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Row(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Expanded(
              child: ChoiceChips<_Target>(
                options: [
                  for (final candidate in targets)
                    (candidate, nameOf(candidate)),
                ],
                value: target,
                onChanged: (candidate) => setState(() => _selected = candidate),
                iconOf: (candidate) =>
                    candidate.kind == OverwriteTargetKind.member
                    ? OcIcons.person
                    : OcIcons.shieldPerson,
              ),
            ),
            const SizedBox(width: OcSpace.s12),
            Builder(
              builder: (context) => OcButton(
                label: 'Add role or member',
                icon: OcIcons.add,
                dense: true,
                onPressed: canManage ? () => _add(context, data) : null,
              ),
            ),
          ],
        ),
        const SizedBox(height: OcSpace.s12),
        Text(
          [
            if (channel.isCategory)
              'These apply to the category itself; the channels in it keep '
                  'their own.',
            if (target == everyone)
              'What everyone may do here, unless a role or member says '
                  'otherwise.'
            else
              'Allow or deny for ${nameOf(target)} here. Inherit keeps '
                  'what their roles say.',
            if (!canManage) 'You need Manage roles here to change these.',
          ].join(' '),
          style: OcText.small.copyWith(color: colors.textSecondary),
        ),
        const SizedBox(height: OcSpace.s12),
        SettingsSection(
          children: [
            for (final info in [
              for (final (_, group) in permissionGroups)
                for (final info in group)
                  if (info.perChannel) info,
            ])
              _PermissionRow(
                info: info,
                value: overwriteState(overwrite, info.bit),
                allowed: {
                  for (final state in TriState.values)
                    if (canSetState(
                      held,
                      overwrite ?? _blank(target),
                      info.bit,
                      state,
                    ))
                      state,
                },
                onChanged: canManage
                    ? (state) => _guard(
                        () => _repository.setOverwrite(
                          widget.serverKey,
                          channel.id,
                          withState(
                            overwrite ?? _blank(target),
                            info.bit,
                            state,
                          ),
                        ),
                      )
                    : null,
              ),
          ],
        ),
        if (overwrite != null && canManage) ...[
          const SizedBox(height: OcSpace.s16),
          Align(
            alignment: Alignment.centerLeft,
            child: OcButton(
              label: target == everyone
                  ? 'Reset to inherit'
                  : 'Remove ${nameOf(target)}',
              icon: OcIcons.delete,
              dense: true,
              onPressed: () async {
                // Lifting a View channel denial shows the channel, and its
                // history, to everyone it covers: not on one stray click.
                if (overwrite.deny.has(Permissions.viewChannel)) {
                  final name = channel.isCategory
                      ? channel.name
                      : '#${channel.name}';
                  final everyoneTarget = target == everyone;
                  final show = await confirmAction(
                    context,
                    title: everyoneTarget
                        ? 'Make $name visible to everyone?'
                        : 'Let ${nameOf(target)} see $name?',
                    message: everyoneTarget
                        ? 'Everyone on the server will see it and can read '
                              'its history.'
                        : 'They will see it and can read its history.',
                    action: everyoneTarget ? 'Make visible' : 'Remove',
                  );
                  if (!show || !mounted) return;
                }
                await _guard(
                  () => _repository.deleteOverwrite(
                    widget.serverKey,
                    channel.id,
                    target.kind,
                    target.id,
                  ),
                );
              },
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
    required this.allowed,
    required this.onChanged,
  });

  final PermissionInfo info;
  final TriState value;

  /// The states the server would accept from this editor.
  final Set<TriState> allowed;
  final ValueChanged<TriState>? onChanged;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Padding(
      padding: const EdgeInsets.symmetric(
        horizontal: OcSpace.s16,
        vertical: OcSpace.s10,
      ),
      child: Row(
        children: [
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  info.later ? '${info.label} (later)' : info.label,
                  style: OcText.body.copyWith(color: colors.text),
                ),
                const SizedBox(height: 2),
                Text(
                  info.description,
                  style: OcText.small.copyWith(color: colors.textMuted),
                ),
              ],
            ),
          ),
          const SizedBox(width: OcSpace.s12),
          TriStateControl(
            label: info.label,
            value: value,
            enabled: {value, ...allowed},
            onChanged: onChanged == null
                ? null
                : (state) {
                    if (state != value) onChanged!(state);
                  },
          ),
        ],
      ),
    );
  }
}

/// A role or member in the search results.
class _TargetTile extends StatelessWidget {
  const _TargetTile({
    required this.label,
    required this.member,
    required this.onTap,
  });

  final String label;
  final bool member;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Hoverable(
      onTap: onTap,
      semanticLabel: label,
      builder: (context, state) => AnimatedContainer(
        duration: OcMotion.of(context).hover,
        height: 34,
        margin: const EdgeInsets.only(bottom: 2),
        padding: const EdgeInsets.symmetric(horizontal: OcSpace.s8),
        decoration: BoxDecoration(
          color: state.active ? colors.hover : null,
          borderRadius: BorderRadius.circular(OcRadius.row),
        ),
        child: Row(
          children: [
            Icon(
              member ? OcIcons.person : OcIcons.shieldPerson,
              size: 16,
              color: colors.textSecondary,
            ),
            const SizedBox(width: OcSpace.s8),
            Expanded(
              child: Text(
                label,
                maxLines: 1,
                overflow: TextOverflow.ellipsis,
                style: OcText.body.copyWith(color: colors.text),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

typedef _Candidate = ({OverwriteTargetKind kind, int id, String name});

/// Searches the roles and members that have no overwrite here yet.
class _TargetPicker extends StatefulWidget {
  const _TargetPicker({required this.roles, required this.members});

  final List<Role> roles;
  final List<Member> members;

  @override
  State<_TargetPicker> createState() => _TargetPickerState();
}

class _TargetPickerState extends State<_TargetPicker> {
  final _query = TextEditingController();

  @override
  void dispose() {
    _query.dispose();
    super.dispose();
  }

  void _pick(_Candidate candidate) =>
      Navigator.pop<_Target>(context, (kind: candidate.kind, id: candidate.id));

  /// Roles, then members by name, that match the search.
  ({List<_Candidate> roles, List<_Candidate> members}) _matches(String text) {
    final query = text.trim().toLowerCase();
    bool matches(String name) => name.toLowerCase().contains(query);
    return (
      roles: [
        for (final role in widget.roles)
          if (matches(role.name))
            (kind: OverwriteTargetKind.role, id: role.id, name: role.name),
      ],
      members: [
        for (final member in widget.members)
          if (matches(member.displayName))
            (
              kind: OverwriteTargetKind.member,
              id: member.id,
              name: member.displayName,
            ),
      ]..sort((a, b) => a.name.toLowerCase().compareTo(b.name.toLowerCase())),
    );
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final query = _query.text.trim();
    final (:roles, :members) = _matches(query);
    final results = [...roles, ...members];
    Widget header(String text) => Padding(
      padding: const EdgeInsets.fromLTRB(
        OcSpace.s8,
        OcSpace.s8,
        OcSpace.s8,
        OcSpace.s4,
      ),
      child: Text(text, style: OcText.label.copyWith(color: colors.textMuted)),
    );
    return SizedBox(
      width: 260,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          OcTextField(
            controller: _query,
            autofocus: true,
            hint: 'Search roles and members',
            prefixIcon: OcIcons.search,
            semanticLabel: 'Search roles and members',
            onChanged: (_) => setState(() {}),
            onSubmitted: (text) {
              final (:roles, :members) = _matches(text);
              if ([...roles, ...members].firstOrNull case final first?) {
                _pick(first);
              }
            },
          ),
          const SizedBox(height: OcSpace.s4),
          ConstrainedBox(
            constraints: const BoxConstraints(maxHeight: 280),
            child: ListView(
              shrinkWrap: true,
              children: [
                if (roles.isNotEmpty) header('ROLES'),
                for (final candidate in roles)
                  _TargetTile(
                    label: candidate.name,
                    member: false,
                    onTap: () => _pick(candidate),
                  ),
                if (members.isNotEmpty) header('MEMBERS'),
                for (final candidate in members)
                  _TargetTile(
                    label: candidate.name,
                    member: true,
                    onTap: () => _pick(candidate),
                  ),
                if (results.isEmpty)
                  Padding(
                    padding: const EdgeInsets.all(OcSpace.s12),
                    child: Text(
                      query.isEmpty
                          ? 'Every role and member already has its own '
                                'permissions here.'
                          : 'No role or member matches.',
                      style: OcText.small.copyWith(color: colors.textMuted),
                    ),
                  ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}
