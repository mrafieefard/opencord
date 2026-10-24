import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/private_state.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/channels/user_panel.dart';
import 'package:opencord/features/channels/voice_panel.dart';
import 'package:opencord/features/home/friend_rows.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/features/window/header_bar.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/badges.dart';
import 'package:opencord/ui/widgets/confirm_dialog.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/section_label.dart';

/// The sidebar for the home view (§6.2).
class HomeSidebar extends ConsumerWidget {
  const HomeSidebar({super.key, this.leadingControls = false});

  final bool leadingControls;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final contacts = ref.watch(privateContactsProvider);
    final currentPeer = ref.watch(currentPeerProvider);
    final isHome = ref.watch(homeShownProvider);
    final isFriends = isHome && currentPeer == null;

    final incomingNonSpam = contacts.incomingRequests.length;
    final messageRequests = contacts.messageRequests.length;

    // Direct messages: accepted, not hidden, not blocked, newest first
    final conversations = contacts.all
        .where(
          (c) =>
              c.conversation == ConversationStatus.accepted &&
              !c.hidden &&
              !c.blocked,
        )
        .toList()
      ..sort((a, b) {
        final atA = a.lastActivity ?? DateTime.fromMillisecondsSinceEpoch(0);
        final atB = b.lastActivity ?? DateTime.fromMillisecondsSinceEpoch(0);
        return atB.compareTo(atA);
      });

    return ColoredBox(
      color: colors.sidebar,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          HeaderBar(
            leadingControls: leadingControls,
            padding: const EdgeInsets.symmetric(horizontal: OcSpace.s16),
            child: Align(
              alignment: Alignment.centerLeft,
              child: Text(
                'Private messages',
                style: OcText.header.copyWith(color: colors.text),
              ),
            ),
          ),
          const SizedBox(height: OcSpace.s8),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: OcSpace.s8),
            child: _SidebarNavRow(
              icon: OcIcons.group,
              label: 'Friends',
              selected: isFriends,
              count: incomingNonSpam,
              onTap: () =>
                  ref.read(navigationProvider.notifier).openFriends(),
            ),
          ),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: OcSpace.s8),
            child: _SidebarNavRow(
              icon: OcIcons.mail,
              label: 'Message requests',
              selected: false,
              count: messageRequests,
              onTap: () =>
                  ref.read(navigationProvider.notifier).openFriends(),
            ),
          ),
          const SizedBox(height: OcSpace.s8),
          Padding(
            padding: const EdgeInsets.symmetric(horizontal: OcSpace.s16),
            child: const SectionLabel('DIRECT MESSAGES'),
          ),
          const SizedBox(height: OcSpace.s4),
          Expanded(
            child: ListView.builder(
              padding: const EdgeInsets.symmetric(horizontal: OcSpace.s8),
              itemCount: conversations.length,
              itemBuilder: (context, index) {
                final c = conversations[index];
                final selected = isHome && currentPeer == c.peer;
                return _ConversationRow(
                  contact: c,
                  selected: selected,
                  onTap: () => ref
                      .read(navigationProvider.notifier)
                      .openConversation(c.peer),
                );
              },
            ),
          ),
          const VoiceConnectedPanel(),
          const UserPanel(),
        ],
      ),
    );
  }
}

class _SidebarNavRow extends StatelessWidget {
  const _SidebarNavRow({
    required this.icon,
    required this.label,
    required this.selected,
    required this.count,
    required this.onTap,
  });

  final IconData icon;
  final String label;
  final bool selected;
  final int count;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Hoverable(
      onTap: onTap,
      selected: selected,
      semanticLabel: label,
      builder: (context, state) {
        final bg = selected
            ? colors.selected
            : state.active
            ? colors.hover
            : Colors.transparent;
        return Container(
          height: 40,
          margin: const EdgeInsets.symmetric(vertical: 1),
          padding: const EdgeInsets.symmetric(horizontal: OcSpace.s12),
          decoration: BoxDecoration(
            color: bg,
            borderRadius: BorderRadius.circular(OcRadius.row),
          ),
          child: Row(
            children: [
              Icon(
                icon,
                size: 20,
                color: selected ? colors.text : colors.textSecondary,
              ),
              const SizedBox(width: OcSpace.s12),
              Expanded(
                child: Text(
                  label,
                  style: OcText.bodyStrong.copyWith(
                    color: selected ? colors.text : colors.textSecondary,
                  ),
                ),
              ),
              if (count > 0) UnreadBadge(count: count),
            ],
          ),
        );
      },
    );
  }
}

class _ConversationRow extends ConsumerWidget {
  const _ConversationRow({
    required this.contact,
    required this.selected,
    required this.onTap,
  });

  final PrivateContact contact;
  final bool selected;
  final VoidCallback onTap;

  Future<void> _showMenu(
    BuildContext context,
    WidgetRef ref,
    Offset position,
    String name,
  ) {
    final peer = contact.peer;
    final muted = contact.muted;
    return showOcMenu(
      context: context,
      position: position,
      entries: [
        OcMenuItem(
          label: muted ? 'Unmute notifications' : 'Mute notifications',
          icon: muted ? OcIcons.notifications : OcIcons.notificationsOff,
          onSelected: () => ref
              .read(repositoryProvider)
              .setPrivateMuted(peer, muted: !muted),
        ),
        OcMenuItem(
          label: 'Close conversation',
          icon: OcIcons.close,
          onSelected: () =>
              ref.read(repositoryProvider).hideConversation(peer),
        ),
        const OcMenuDivider(),
        OcMenuItem(
          label: 'Block',
          icon: OcIcons.block,
          onSelected: () async {
            final ok = await confirmAction(
              context,
              title: 'Block $name?',
              message:
                  'They won\'t be able to message you or see your presence.',
              action: 'Block',
            );
            if (ok) {
              await ref.read(repositoryProvider).blockUser(peer);
            }
          },
        ),
      ],
    );
  }

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final name = ref.watch(contactNameProvider(contact.peer));

    return Hoverable(
      onTap: onTap,
      onSecondaryTap: (pos) => _showMenu(context, ref, pos, name),
      selected: selected,
      semanticLabel: name,
      builder: (context, state) {
        final bg = selected
            ? colors.selected
            : state.active
            ? colors.hover
            : Colors.transparent;

        return Container(
          height: 44,
          margin: const EdgeInsets.symmetric(vertical: 1),
          padding: const EdgeInsets.symmetric(horizontal: OcSpace.s8),
          decoration: BoxDecoration(
            color: bg,
            borderRadius: BorderRadius.circular(OcRadius.row),
          ),
          child: Row(
            children: [
              ContactAvatar(peer: contact.peer, size: 32),
              const SizedBox(width: OcSpace.s8),
              Expanded(
                child: Text(
                  name,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: OcText.body.copyWith(
                    color: selected ? colors.text : colors.textSecondary,
                    fontWeight: contact.unread > 0
                        ? FontWeight.w600
                        : FontWeight.w400,
                  ),
                ),
              ),
              if (state.hovered)
                OcIconButton(
                  icon: OcIcons.close,
                  size: OcIconButtonSize.compact,
                  tooltip: 'Close conversation',
                  onPressed: () => ref
                      .read(repositoryProvider)
                      .hideConversation(contact.peer),
                )
              else if (contact.unread > 0)
                if (contact.muted)
                  Container(
                    width: 8,
                    height: 8,
                    decoration: BoxDecoration(
                      shape: BoxShape.circle,
                      color: colors.textMuted,
                    ),
                  )
                else
                  UnreadBadge(count: contact.unread),
            ],
          ),
        );
      },
    );
  }
}
