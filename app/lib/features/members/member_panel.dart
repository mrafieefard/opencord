import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/format.dart';
import 'package:opencord/core/model/misc.dart';
import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/model/user.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/members/member_actions.dart';
import 'package:opencord/features/members/member_sections.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/window/header_bar.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_motion.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/avatar.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_text_field.dart';
import 'package:opencord/ui/widgets/section_label.dart';

/// The member panel (§4.7, Telegram's "group info"): who can see the open
/// channel, in sections by hoisted role, online and offline.
class MemberPanel extends ConsumerStatefulWidget {
  const MemberPanel({
    super.key,
    required this.onClose,
    this.trailingControls = false,
  });

  final VoidCallback onClose;

  /// Carries the right-hand window controls (§3.1).
  final bool trailingControls;

  @override
  ConsumerState<MemberPanel> createState() => _MemberPanelState();
}

class _MemberPanelState extends ConsumerState<MemberPanel> {
  final _search = TextEditingController();
  String _query = '';
  bool _offlineExpanded = false;

  @override
  void dispose() {
    _search.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final server = ref.watch(currentServerProvider);
    final channelId = ref.watch(currentChannelProvider);
    final data = server == null
        ? null
        : ref.watch(serverProvider(server).select((state) => state.data));
    final presence = server == null
        ? PresenceState.empty
        : ref.watch(presenceProvider(server));
    final voice = server == null
        ? const <int, List<VoiceParticipant>>{}
        : ref.watch(voiceProvider(server));
    final channel = data?.channels[channelId];
    final members = data == null
        ? const <Member>[]
        : channel != null && channel.kind.isTextLike
        ? channelViewers(data, channel)
        : data.members.values.toList();
    String? voiceChannelOf(int userId) {
      for (final MapEntry(key: id, value: participants) in voice.entries) {
        if (participants.any((participant) => participant.userId == userId)) {
          return data?.channels[id]?.name;
        }
      }
      return null;
    }

    final entries = data == null
        ? const <MemberEntry>[]
        : memberSections(
            data,
            members,
            presence,
            query: _query,
            voiceChannelOf: voiceChannelOf,
            offlineExpanded: _offlineExpanded,
          );
    return ColoredBox(
      color: colors.sidebar,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          HeaderBar(
            trailingControls: widget.trailingControls,
            padding: const EdgeInsets.only(
              left: OcSpace.s16,
              right: OcSpace.s8,
            ),
            child: Row(
              children: [
                Expanded(
                  child: IgnorePointer(
                    child: Text.rich(
                      TextSpan(
                        children: [
                          const TextSpan(text: 'Members'),
                          TextSpan(
                            text: '  ${countLabel(members.length)}',
                            style: OcText.body.copyWith(
                              color: colors.textMuted,
                            ),
                          ),
                        ],
                      ),
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: OcText.header.copyWith(color: colors.text),
                    ),
                  ),
                ),
                // Window buttons beside it would make two × in a row; the
                // list still toggles from the chat header.
                if (!widget.trailingControls ||
                    !showsTrailingWindowControls(context, ref))
                  OcIconButton(
                    icon: OcIcons.close,
                    tooltip: 'Close member list',
                    size: OcIconButtonSize.compact,
                    onPressed: widget.onClose,
                  ),
              ],
            ),
          ),
          Padding(
            padding: const EdgeInsets.fromLTRB(
              OcSpace.s12,
              OcSpace.s12,
              OcSpace.s12,
              OcSpace.s4,
            ),
            child: OcTextField(
              controller: _search,
              hint: 'Search members',
              prefixIcon: OcIcons.search,
              radius: OcRadius.searchPill,
              semanticLabel: 'Search members',
              onChanged: (value) => setState(() => _query = value),
            ),
          ),
          Expanded(
            child: entries.isEmpty
                ? Padding(
                    padding: const EdgeInsets.all(OcSpace.s24),
                    child: Text(
                      _query.trim().isEmpty
                          ? 'No one is here yet.'
                          : 'No members match "${_query.trim()}".',
                      textAlign: TextAlign.center,
                      style: OcText.small.copyWith(color: colors.textMuted),
                    ),
                  )
                : ListView.builder(
                    padding: const EdgeInsets.fromLTRB(
                      OcSpace.s8,
                      OcSpace.s4,
                      OcSpace.s8,
                      OcSpace.s12,
                    ),
                    itemCount: entries.length,
                    itemBuilder: (context, index) => switch (entries[index]) {
                      final MemberHeader header => _SectionHeader(
                        key: ValueKey(header.key),
                        header: header,
                        onToggle: header.collapsible
                            ? () => setState(
                                () => _offlineExpanded = !_offlineExpanded,
                              )
                            : null,
                      ),
                      final MemberLine line => MemberRow(
                        key: ValueKey(line.key),
                        line: line,
                        actions: MemberActions(
                          context: context,
                          ref: ref,
                          serverKey: server!,
                        ),
                      ),
                    },
                  ),
          ),
        ],
      ),
    );
  }
}

class _SectionHeader extends StatelessWidget {
  const _SectionHeader({super.key, required this.header, this.onToggle});

  final MemberHeader header;
  final VoidCallback? onToggle;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final label = SectionLabel(
      '${header.label} — ${countLabel(header.count)}',
      padding: const EdgeInsets.fromLTRB(
        OcSpace.s8,
        OcSpace.s16,
        OcSpace.s8,
        OcSpace.s6,
      ),
      trailing: onToggle == null
          ? null
          : Icon(
              header.collapsed ? OcIcons.expandMore : OcIcons.expandLess,
              size: OcSize.iconInline,
              color: colors.textMuted,
            ),
    );
    if (onToggle == null) return label;
    return Hoverable(
      onTap: onToggle,
      semanticLabel:
          '${header.label}, ${header.count}, '
          '${header.collapsed ? 'collapsed' : 'expanded'}',
      builder: (context, state) => label,
    );
  }
}

/// One member (§4.7): avatar with presence, name, the owner's star and an
/// activity line. Click for the profile, right-click for the menu.
class MemberRow extends StatelessWidget {
  const MemberRow({super.key, required this.line, required this.actions});

  final MemberLine line;
  final MemberActions actions;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final member = line.member;
    final offline = line.presence == Presence.offline;
    final activity = line.activity;
    return Hoverable(
      onTap: () => actions.profile(member),
      onSecondaryTap: (position) => actions.showMenu(member, position),
      semanticLabel: [
        member.displayName,
        line.presence.label,
        if (line.owner) 'server owner',
        ?activity,
      ].join(', '),
      builder: (context, state) => AnimatedContainer(
        duration: OcMotion.of(context).hover,
        constraints: const BoxConstraints(minHeight: 44),
        padding: const EdgeInsets.symmetric(
          horizontal: OcSpace.s8,
          vertical: OcSpace.s4,
        ),
        decoration: BoxDecoration(
          color: state.active ? colors.hover : null,
          borderRadius: BorderRadius.circular(OcRadius.row),
        ),
        child: Row(
          children: [
            Opacity(
              opacity: offline ? 0.55 : 1,
              child: OcAvatar(
                id: '${member.id}',
                name: member.displayName,
                size: OcSize.memberAvatar,
                presence: line.presence,
                ringColor: state.active ? colors.hover : colors.sidebar,
              ),
            ),
            const SizedBox(width: OcSpace.s12),
            Expanded(
              child: Column(
                mainAxisAlignment: MainAxisAlignment.center,
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Row(
                    children: [
                      Flexible(
                        child: Text(
                          member.displayName,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: OcText.body.copyWith(
                            fontWeight: FontWeight.w500,
                            color: offline ? colors.textMuted : colors.text,
                          ),
                        ),
                      ),
                      if (line.owner) ...[
                        const SizedBox(width: OcSpace.s4),
                        Icon(OcIcons.star, size: 14, color: colors.textMuted),
                      ],
                    ],
                  ),
                  if (activity != null)
                    Text(
                      activity,
                      maxLines: 1,
                      overflow: TextOverflow.ellipsis,
                      style: OcText.small.copyWith(color: colors.textMuted),
                    ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}
