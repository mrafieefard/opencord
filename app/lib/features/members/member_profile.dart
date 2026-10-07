import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
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
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_dialog.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/section_label.dart';
import 'package:opencord/ui/widgets/toast.dart';

/// Opens a member's profile (§4.8): from the member panel, an avatar, a
/// name or a mention.
Future<void> showMemberProfile(
  BuildContext context, {
  required String serverKey,
  required int userId,
}) => showOcDialog<void>(
  context: context,
  builder: (context) =>
      MemberProfileDialog(serverKey: serverKey, userId: userId),
);

/// "In voice · General", or what they said they are doing.
String? memberActivity(WidgetRef ref, String serverKey, int userId) {
  final data = ref.watch(serverProvider(serverKey).select((s) => s.data));
  final voice = ref.watch(voiceProvider(serverKey));
  for (final MapEntry(key: channelId, value: participants) in voice.entries) {
    if (participants.any((participant) => participant.userId == userId)) {
      final name = data?.channels[channelId]?.name;
      if (name != null) return 'In voice · $name';
    }
  }
  return ref.watch(
    presenceProvider(serverKey).select((state) => state.activities[userId]),
  );
}

/// The profile itself (380 px): avatar with presence, name, presence label,
/// then identity key, roles, member since and activity.
class MemberProfileDialog extends ConsumerWidget {
  const MemberProfileDialog({
    super.key,
    required this.serverKey,
    required this.userId,
  });

  final String serverKey;
  final int userId;

  static const double width = 380;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final data = ref.watch(serverProvider(serverKey).select((s) => s.data));
    final member = data?.members[userId];
    if (data == null || member == null) {
      return OcDialog(
        title: 'Member',
        width: width,
        showClose: true,
        child: const Text('This member is not on the server any more.'),
      );
    }
    final presence = ref.watch(
      presenceProvider(serverKey).select((state) => state.of(userId)),
    );
    final activity = memberActivity(ref, serverKey, userId);
    final roles = [
      for (final role in data.rolesDescending)
        if (member.roleIds.contains(role.id)) role,
    ];
    final actions = MemberActions(
      context: context,
      ref: ref,
      serverKey: serverKey,
    );
    final canMention = ref.watch(canMentionProvider(serverKey));
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(OcSpace.s24),
        child: SizedBox(
          width: width,
          child: Material(
            type: MaterialType.transparency,
            child: Container(
              padding: const EdgeInsets.all(OcSpace.s24),
              decoration: BoxDecoration(
                color: colors.elevated,
                borderRadius: BorderRadius.circular(OcRadius.dialog),
                border: Border.all(color: colors.border),
              ),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  Center(
                    child: OcAvatar(
                      id: '${member.id}',
                      name: member.displayName,
                      size: 64,
                      presence: presence,
                      ringColor: colors.elevated,
                    ),
                  ),
                  const SizedBox(height: OcSpace.s12),
                  Semantics(
                    header: true,
                    child: Text(
                      member.displayName,
                      textAlign: TextAlign.center,
                      style: OcText.title.copyWith(
                        fontSize: 19,
                        color: colors.text,
                      ),
                    ),
                  ),
                  const SizedBox(height: OcSpace.s2),
                  Text(
                    data.info.ownerId == member.id
                        ? '${presence.label} · Server owner'
                        : presence.label,
                    textAlign: TextAlign.center,
                    style: OcText.small.copyWith(color: colors.textSecondary),
                  ),
                  if (member.user.fingerprint.isNotEmpty)
                    _Section(
                      label: 'Identity key',
                      child: Row(
                        children: [
                          Expanded(
                            child: SelectableText(
                              member.user.fingerprint.replaceAll('-', ' '),
                              style: OcText.mono.copyWith(color: colors.text),
                            ),
                          ),
                          OcIconButton(
                            icon: OcIcons.contentCopy,
                            tooltip: 'Copy fingerprint',
                            size: OcIconButtonSize.compact,
                            onPressed: () {
                              Clipboard.setData(
                                ClipboardData(text: member.user.fingerprint),
                              );
                              showOcToast(context, 'Fingerprint copied');
                            },
                          ),
                        ],
                      ),
                    ),
                  _Section(
                    label: 'Roles',
                    child: roles.isEmpty
                        ? Text(
                            'No roles',
                            style: OcText.small.copyWith(
                              color: colors.textMuted,
                            ),
                          )
                        : Wrap(
                            spacing: OcSpace.s6,
                            runSpacing: OcSpace.s6,
                            children: [
                              for (final role in roles) _RoleChip(role: role),
                            ],
                          ),
                  ),
                  _Section(
                    label: 'Member since',
                    child: Text(
                      longDate(member.joinedAt),
                      style: OcText.body.copyWith(color: colors.text),
                    ),
                  ),
                  if (activity != null)
                    _Section(
                      label: 'Activity',
                      child: Text(
                        activity,
                        style: OcText.body.copyWith(color: colors.text),
                      ),
                    ),
                  const SizedBox(height: OcSpace.s24),
                  Row(
                    mainAxisAlignment: MainAxisAlignment.end,
                    children: [
                      if (canMention && member.id != data.self.id) ...[
                        OcButton(
                          label: 'Mention',
                          onPressed: () {
                            Navigator.pop(context);
                            actions.mention(member);
                          },
                        ),
                        const SizedBox(width: OcSpace.s8),
                      ],
                      const Tooltip(
                        message: 'Direct messages are coming in a later phase',
                        child: OcButton.primary(label: 'Message'),
                      ),
                    ],
                  ),
                ],
              ),
            ),
          ),
        ),
      ),
    );
  }
}

class _Section extends StatelessWidget {
  const _Section({required this.label, required this.child});

  final String label;
  final Widget child;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(top: OcSpace.s20),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SectionLabel(label),
        const SizedBox(height: OcSpace.s6),
        child,
      ],
    ),
  );
}

/// An outlined role chip; roles are never colored (§4.5, §4.8).
class _RoleChip extends StatelessWidget {
  const _RoleChip({required this.role});

  final Role role;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Container(
      padding: const EdgeInsets.symmetric(
        horizontal: OcSpace.s10,
        vertical: OcSpace.s4,
      ),
      decoration: BoxDecoration(
        borderRadius: BorderRadius.circular(OcRadius.reaction),
        border: Border.all(color: colors.border),
      ),
      child: Text(role.name, style: OcText.small.copyWith(color: colors.text)),
    );
  }
}
