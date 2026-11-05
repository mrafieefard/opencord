import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/providers/private_state.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/features/home/friends_page.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/widgets/badges.dart';
import 'package:opencord/ui/widgets/hoverable.dart';

/// The two tiles at the top of the private sidebar (§5.16): Friends and
/// Message requests.
class HomeTiles extends ConsumerWidget {
  const HomeTiles({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final contacts = ref.watch(privateContactsProvider);
    final currentPeer = ref.watch(currentPeerProvider);
    final isHome = ref.watch(homeShownProvider);
    final isFriends = isHome && currentPeer == null;
    final tab = ref.watch(friendsTabProvider);
    final isRequestsOpen = isFriends && tab == FriendsTab.requests;
    final isFriendsOpen = isFriends && tab != FriendsTab.requests;

    final incomingRequests = contacts.incomingRequests.length;
    final messageRequests = contacts.messageRequests.length;

    // Count online friends from contacts.
    final onlineFriends = contacts.friends.where((c) {
      final presence = ref.watch(contactPresenceProvider(c.peer));
      return presence != Presence.offline;
    }).length;

    final requestsSubtitle = messageRequests > 0
        ? '$messageRequests waiting'
        : 'All answered';

    return Padding(
      padding: const EdgeInsets.fromLTRB(8, 8, 8, 4),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Expanded(
            child: HomeTile(
              icon: OcIcons.group,
              name: 'Friends',
              subtitle: '$onlineFriends online',
              badge: incomingRequests,
              selected: isFriendsOpen,
              onTap: () {
                if (tab == FriendsTab.requests) {
                  ref
                      .read(friendsTabProvider.notifier)
                      .select(FriendsTab.online);
                }
                ref.read(navigationProvider.notifier).openFriends();
              },
            ),
          ),
          const SizedBox(width: 8),
          Expanded(
            child: HomeTile(
              icon: OcIcons.mail,
              name: 'Message requests',
              subtitle: requestsSubtitle,
              badge: messageRequests,
              selected: isRequestsOpen,
              onTap: () {
                ref
                    .read(friendsTabProvider.notifier)
                    .select(FriendsTab.requests);
                ref.read(navigationProvider.notifier).openFriends();
              },
            ),
          ),
        ],
      ),
    );
  }
}

/// A tile in the private sidebar (§5.16).
class HomeTile extends StatelessWidget {
  const HomeTile({
    super.key,
    required this.icon,
    required this.name,
    required this.subtitle,
    this.badge = 0,
    required this.selected,
    required this.onTap,
  });

  final IconData icon;
  final String name;
  final String subtitle;
  final int badge;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Hoverable(
      onTap: onTap,
      selected: selected,
      semanticLabel: name,
      focusRadius: BorderRadius.circular(12),
      builder: (context, state) {
        final scale = state.pressed ? 0.97 : 1.0;
        final bg = selected
            ? colors.accent
            : (state.active ? colors.selected : colors.hover);
        final borderColor = selected ? colors.accent : colors.border;
        final textColor = selected ? colors.onAccent : colors.text;
        final subColor = selected
            ? colors.onAccent.withValues(alpha: 0.7)
            : colors.textMuted;
        final iconTileBg = selected ? colors.onAccent : colors.selected;
        final iconColor = selected ? colors.accent : colors.text;

        return AnimatedScale(
          scale: scale,
          duration: const Duration(milliseconds: 80),
          curve: Curves.easeOut,
          child: Container(
            constraints: const BoxConstraints(minHeight: 92),
            padding: const EdgeInsets.all(10),
            decoration: BoxDecoration(
              color: bg,
              borderRadius: BorderRadius.circular(12),
              border: Border.all(color: borderColor, width: 1),
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisSize: MainAxisSize.min,
              children: [
                Row(
                  mainAxisAlignment: MainAxisAlignment.spaceBetween,
                  children: [
                    Container(
                      width: 32,
                      height: 32,
                      decoration: BoxDecoration(
                        color: iconTileBg,
                        borderRadius: BorderRadius.circular(10),
                      ),
                      alignment: Alignment.center,
                      child: Icon(icon, size: 20, color: iconColor),
                    ),
                    if (badge > 0)
                      UnreadBadge(count: badge, inverted: selected),
                  ],
                ),
                const SizedBox(height: 6),
                Text(
                  name,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    fontSize: 13,
                    fontWeight: FontWeight.w600,
                    height: 1.2,
                    color: textColor,
                  ),
                ),
                const SizedBox(height: 2),
                Text(
                  subtitle,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(
                    fontSize: 11.5,
                    fontWeight: FontWeight.w400,
                    color: subColor,
                  ),
                ),
              ],
            ),
          ),
        );
      },
    );
  }
}
