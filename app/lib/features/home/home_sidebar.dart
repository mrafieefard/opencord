import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/private_state.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/channels/server_head.dart';
import 'package:opencord/features/home/friend_rows.dart';
import 'package:opencord/features/home/home_tiles.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/widgets/badges.dart';
import 'package:opencord/ui/widgets/confirm_dialog.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/section_label.dart';

/// The home sidebar header float (§5.16), 52 px tall.
class HomeHead extends StatelessWidget {
  const HomeHead({super.key});

  @override
  Widget build(BuildContext context) {
    return const ServerHead();
  }
}

/// The sidebar for the home view (§6.2).
class HomeSidebar extends ConsumerWidget {
  const HomeSidebar({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final contacts = ref.watch(privateContactsProvider);
    final currentPeer = ref.watch(currentPeerProvider);
    final isHome = ref.watch(homeShownProvider);

    // Direct messages: accepted, not hidden, not blocked, newest first
    final conversations =
        contacts.all
            .where(
              (c) =>
                  c.conversation == ConversationStatus.accepted &&
                  !c.hidden &&
                  !c.blocked,
            )
            .toList()
          ..sort((a, b) {
            final atA =
                a.lastActivity ?? DateTime.fromMillisecondsSinceEpoch(0);
            final atB =
                b.lastActivity ?? DateTime.fromMillisecondsSinceEpoch(0);
            return atB.compareTo(atA);
          });

    return ColoredBox(
      color: colors.sidebar,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          const HomeTiles(),
          const SectionLabel(
            'DIRECT MESSAGES',
            padding: EdgeInsets.fromLTRB(16, 10, 16, 4),
          ),
          Expanded(
            child: ListView.builder(
              padding: const EdgeInsets.only(bottom: 8),
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
        ],
      ),
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
          onSelected: () =>
              ref.read(repositoryProvider).setPrivateMuted(peer, muted: !muted),
        ),
        OcMenuItem(
          label: 'Close conversation',
          icon: OcIcons.close,
          onSelected: () => ref.read(repositoryProvider).hideConversation(peer),
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
    final presence = ref.watch(contactPresenceProvider(contact.peer));

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
          margin: const EdgeInsets.fromLTRB(8, 0, 8, 2),
          padding: const EdgeInsets.symmetric(horizontal: 12),
          decoration: BoxDecoration(
            color: bg,
            borderRadius: BorderRadius.circular(10),
          ),
          child: Row(
            children: [
              ContactAvatar(peer: contact.peer, size: 32, presence: presence),
              const SizedBox(width: 12),
              Expanded(
                child: Text(
                  name,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    fontSize: 14,
                    color: selected ? colors.text : colors.textSecondary,
                    fontWeight: contact.unread > 0
                        ? FontWeight.w600
                        : FontWeight.w500,
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
