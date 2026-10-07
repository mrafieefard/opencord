import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/members/member_actions.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/avatar.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_text_field.dart';
import 'package:opencord/ui/widgets/popover.dart';
import 'package:opencord/ui/widgets/section_label.dart';

/// Members (§8.2): a searchable table of everyone, with roles, join date
/// and the member menu (roles, kick, ban).
class ServerMembersPage extends ConsumerStatefulWidget {
  const ServerMembersPage({super.key, required this.serverKey});

  final String serverKey;

  @override
  ConsumerState<ServerMembersPage> createState() => _ServerMembersPageState();
}

class _ServerMembersPageState extends ConsumerState<ServerMembersPage> {
  String _query = '';

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final data = ref.watch(
      serverProvider(widget.serverKey).select((state) => state.data),
    );
    if (data == null) return const SizedBox.shrink();
    final needle = _query.trim().toLowerCase();
    final members =
        [
          for (final member in data.members.values)
            if (needle.isEmpty ||
                member.displayName.toLowerCase().contains(needle))
              member,
        ]..sort(
          (a, b) => a.displayName.toLowerCase().compareTo(
            b.displayName.toLowerCase(),
          ),
        );
    final actions = MemberActions(
      context: context,
      ref: ref,
      serverKey: widget.serverKey,
    );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        OcTextField(
          hint: 'Search ${countLabel(data.members.length)} members',
          prefixIcon: OcIcons.search,
          radius: OcRadius.searchPill,
          semanticLabel: 'Search members',
          onChanged: (value) => setState(() => _query = value),
        ),
        const SizedBox(height: OcSpace.s16),
        const Padding(
          padding: EdgeInsets.symmetric(horizontal: OcSpace.s12),
          child: Row(
            children: [
              Expanded(flex: 5, child: SectionLabel('Member')),
              Expanded(flex: 5, child: SectionLabel('Roles')),
              Expanded(flex: 3, child: SectionLabel('Joined')),
              SizedBox(width: OcSize.hitCompact),
            ],
          ),
        ),
        const SizedBox(height: OcSpace.s6),
        DecoratedBox(
          decoration: BoxDecoration(
            color: colors.surface,
            borderRadius: BorderRadius.circular(OcRadius.section),
            border: Border.all(color: colors.border),
          ),
          child: members.isEmpty
              ? Padding(
                  padding: const EdgeInsets.all(OcSpace.s24),
                  child: Text(
                    'No members match "${_query.trim()}".',
                    textAlign: TextAlign.center,
                    style: OcText.small.copyWith(color: colors.textMuted),
                  ),
                )
              : Column(
                  children: [
                    for (final (index, member) in members.indexed) ...[
                      if (index > 0) Divider(height: 1, color: colors.border),
                      _MemberLine(
                        member: member,
                        roles: [
                          for (final role in data.rolesDescending)
                            if (member.roleIds.contains(role.id)) role.name,
                        ],
                        owner: data.info.ownerId == member.id,
                        actions: actions,
                      ),
                    ],
                  ],
                ),
        ),
      ],
    );
  }
}

class _MemberLine extends StatelessWidget {
  const _MemberLine({
    required this.member,
    required this.roles,
    required this.owner,
    required this.actions,
  });

  final Member member;
  final List<String> roles;
  final bool owner;
  final MemberActions actions;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final shown = roles.take(3).toList();
    final more = roles.length - shown.length;
    return Padding(
      padding: const EdgeInsets.symmetric(
        horizontal: OcSpace.s12,
        vertical: OcSpace.s8,
      ),
      child: Row(
        children: [
          Expanded(
            flex: 5,
            child: Row(
              children: [
                OcAvatar(
                  id: '${member.id}',
                  name: member.displayName,
                  size: 28,
                ),
                const SizedBox(width: OcSpace.s10),
                Flexible(
                  child: Text(
                    member.displayName,
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                    style: OcText.body.copyWith(color: colors.text),
                  ),
                ),
                if (owner) ...[
                  const SizedBox(width: OcSpace.s4),
                  Icon(OcIcons.star, size: 14, color: colors.textMuted),
                ],
              ],
            ),
          ),
          Expanded(
            flex: 5,
            child: Wrap(
              spacing: OcSpace.s4,
              runSpacing: OcSpace.s4,
              children: [
                if (roles.isEmpty)
                  Text(
                    '—',
                    style: OcText.small.copyWith(color: colors.textMuted),
                  ),
                for (final role in shown)
                  Container(
                    padding: const EdgeInsets.symmetric(
                      horizontal: OcSpace.s8,
                      vertical: 2,
                    ),
                    decoration: BoxDecoration(
                      borderRadius: BorderRadius.circular(OcRadius.reaction),
                      border: Border.all(color: colors.border),
                    ),
                    child: Text(
                      role,
                      style: OcText.small.copyWith(color: colors.text),
                    ),
                  ),
                if (more > 0)
                  Text(
                    '+$more',
                    style: OcText.small.copyWith(color: colors.textMuted),
                  ),
              ],
            ),
          ),
          Expanded(
            flex: 3,
            child: Text(
              longDate(member.joinedAt),
              style: OcText.small.copyWith(color: colors.textSecondary),
            ),
          ),
          Builder(
            builder: (context) => OcIconButton(
              icon: OcIcons.moreHoriz,
              tooltip: 'Actions for ${member.displayName}',
              size: OcIconButtonSize.compact,
              onPressed: () =>
                  actions.showMenu(member, globalRectOf(context).bottomLeft),
            ),
          ),
        ],
      ),
    );
  }
}
