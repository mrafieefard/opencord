import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/providers/private_state.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/features/home/own_device_notice.dart';
import 'package:opencord/features/home/person_card.dart';
import 'package:opencord/features/home/requests_page.dart';
import 'package:opencord/features/shell/shell_keys.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/panel_header.dart';
import 'package:opencord/ui/widgets/section_label.dart';

const friendsTabKey = 'ui.friendsTab';

/// The tabs on the Friends page (§6.2, §5.16).
enum FriendsTab { online, all, pending, requests, blocked }

/// The Friends page's tab, remembered across launches. The home sidebar's
/// Message requests row opens [FriendsTab.requests].
class FriendsTabNotifier extends Notifier<FriendsTab> {
  @override
  FriendsTab build() {
    final saved = ref.read(keyValueStoreProvider).read(friendsTabKey);
    return FriendsTab.values.firstWhere(
      (tab) => tab.name == saved,
      orElse: () => FriendsTab.online,
    );
  }

  void select(FriendsTab tab) {
    state = tab;
    ref.read(keyValueStoreProvider).write(friendsTabKey, tab.name);
  }
}

final friendsTabProvider = NotifierProvider<FriendsTabNotifier, FriendsTab>(
  FriendsTabNotifier.new,
);

/// The Friends page (§5.16).
class FriendsPage extends ConsumerStatefulWidget {
  const FriendsPage({super.key, this.onOpenSidebar});

  final VoidCallback? onOpenSidebar;

  @override
  ConsumerState<FriendsPage> createState() => _FriendsPageState();
}

class _FriendsPageState extends ConsumerState<FriendsPage> {
  bool _spamExpanded = false;

  @override
  Widget build(BuildContext context) {
    final tab = ref.watch(friendsTabProvider);
    if (tab == FriendsTab.requests) {
      return RequestsPage(onOpenSidebar: widget.onOpenSidebar);
    }

    final colors = context.oc;
    final contacts = ref.watch(privateContactsProvider);

    final onlineCount = contacts.friends.where((c) {
      final presence = ref.watch(contactPresenceProvider(c.peer));
      return presence != Presence.offline;
    }).length;
    final allCount = contacts.friends.length;
    final pendingCount = contacts.incomingRequests.length +
        contacts.spamRequests.length +
        contacts.outgoingRequests.length;
    final blockedCount = contacts.blocked.length;

    return Column(
      children: [
        PanelHeader(
          key: ShellKeys.hd,
          padding: const EdgeInsets.symmetric(horizontal: OcSpace.s16),
          child: Row(
            children: [
              if (widget.onOpenSidebar != null) ...[
                OcIconButton(
                  icon: OcIcons.menu,
                  tooltip: 'Open channels',
                  onPressed: widget.onOpenSidebar,
                ),
                const SizedBox(width: OcSpace.s8),
              ],
              Text(
                'Friends',
                style: OcText.header.copyWith(
                  fontSize: 15,
                  fontWeight: FontWeight.w600,
                  color: colors.text,
                ),
              ),
              const SizedBox(width: 8),
              Flexible(
                child: SingleChildScrollView(
                  scrollDirection: Axis.horizontal,
                  child: Row(
                    children: [
                      _FriendsTabButton(
                        label: 'Online',
                        count: onlineCount,
                        selected: tab == FriendsTab.online,
                        onTap: () => ref
                            .read(friendsTabProvider.notifier)
                            .select(FriendsTab.online),
                      ),
                      const SizedBox(width: 4),
                      _FriendsTabButton(
                        label: 'All',
                        count: allCount,
                        selected: tab == FriendsTab.all,
                        onTap: () => ref
                            .read(friendsTabProvider.notifier)
                            .select(FriendsTab.all),
                      ),
                      const SizedBox(width: 4),
                      _FriendsTabButton(
                        label: 'Pending',
                        count: pendingCount,
                        selected: tab == FriendsTab.pending,
                        onTap: () => ref
                            .read(friendsTabProvider.notifier)
                            .select(FriendsTab.pending),
                      ),
                      const SizedBox(width: 4),
                      _FriendsTabButton(
                        label: 'Blocked',
                        count: blockedCount,
                        selected: tab == FriendsTab.blocked,
                        onTap: () => ref
                            .read(friendsTabProvider.notifier)
                            .select(FriendsTab.blocked),
                      ),
                    ],
                  ),
                ),
              ),
            ],
          ),
        ),

        // This person's other devices with other chat keys (design §7.1).
        const OwnDeviceNotice(),

        // Main content area, fading in on tab switch (100 ms)
        Expanded(
          child: AnimatedSwitcher(
            duration: const Duration(milliseconds: 100),
            switchInCurve: Curves.easeOut,
            switchOutCurve: Curves.easeIn,
            transitionBuilder: (child, animation) => FadeTransition(
              opacity: animation,
              child: child,
            ),
            child: KeyedSubtree(
              key: ValueKey(tab),
              child: Padding(
                padding: const EdgeInsets.all(12),
                child: Align(
                  alignment: Alignment.topCenter,
                  child: ConstrainedBox(
                    constraints: const BoxConstraints(maxWidth: 760),
                    child: switch (tab) {
                      FriendsTab.online => _buildOnlineTab(context, contacts),
                      FriendsTab.all => _buildAllTab(context, contacts),
                      FriendsTab.pending => _buildPendingTab(context, contacts),
                      FriendsTab.blocked => _buildBlockedTab(context, contacts),
                      FriendsTab.requests => const SizedBox.shrink(),
                    },
                  ),
                ),
              ),
            ),
          ),
        ),
      ],
    );
  }

  Widget _buildOnlineTab(BuildContext context, PrivateContactsState contacts) {
    final allFriends = contacts.friends;
    final onlineFriends = allFriends.where((c) {
      final presence = ref.watch(contactPresenceProvider(c.peer));
      return presence != Presence.offline;
    }).toList();

    if (onlineFriends.isEmpty) {
      return const PrivateEmptyState(
        icon: OcIcons.group,
        message: "No one's online right now.",
      );
    }

    return ListView(
      children: [
        SectionLabel(
          'ONLINE — ${onlineFriends.length}',
          padding: const EdgeInsets.fromLTRB(10, 12, 10, 6),
        ),
        for (final friend in onlineFriends)
          PersonCard(contact: friend, kind: PersonCardKind.friend),
      ],
    );
  }

  Widget _buildAllTab(BuildContext context, PrivateContactsState contacts) {
    final allFriends = contacts.friends;

    if (allFriends.isEmpty) {
      return const PrivateEmptyState(
        icon: OcIcons.group,
        message: "You haven't added any friends yet.",
      );
    }

    return ListView(
      children: [
        SectionLabel(
          'ALL — ${allFriends.length}',
          padding: const EdgeInsets.fromLTRB(10, 12, 10, 6),
        ),
        for (final friend in allFriends)
          PersonCard(contact: friend, kind: PersonCardKind.friend),
      ],
    );
  }

  Widget _buildPendingTab(BuildContext context, PrivateContactsState contacts) {
    final colors = context.oc;
    final incoming = contacts.incomingRequests;
    final outgoing = contacts.outgoingRequests;
    final spam = contacts.spamRequests;
    final totalNonSpam = incoming.length + outgoing.length;

    if (totalNonSpam == 0 && spam.isEmpty) {
      return const PrivateEmptyState(
        icon: OcIcons.mail,
        message: 'No pending requests.',
      );
    }

    return ListView(
      children: [
        if (incoming.isNotEmpty) ...[
          SectionLabel(
            'INCOMING — ${incoming.length}',
            padding: const EdgeInsets.fromLTRB(10, 12, 10, 6),
          ),
          for (final inc in incoming)
            PersonCard(contact: inc, kind: PersonCardKind.incoming),
        ],
        if (outgoing.isNotEmpty) ...[
          SectionLabel(
            'SENT — ${outgoing.length}',
            padding: const EdgeInsets.fromLTRB(10, 12, 10, 6),
          ),
          for (final out in outgoing)
            PersonCard(contact: out, kind: PersonCardKind.outgoing),
        ],
        if (spam.isNotEmpty) ...[
          const SizedBox(height: 12),
          Hoverable(
            onTap: () => setState(() => _spamExpanded = !_spamExpanded),
            semanticLabel: 'Spam',
            builder: (context, state) => Container(
              padding: const EdgeInsets.symmetric(
                horizontal: 12,
                vertical: 8,
              ),
              decoration: BoxDecoration(
                color: state.active ? colors.hover : null,
                borderRadius: BorderRadius.circular(10),
              ),
              child: Row(
                children: [
                  Icon(
                    _spamExpanded ? OcIcons.expandMore : OcIcons.chevronRight,
                    size: 16,
                    color: colors.textMuted,
                  ),
                  const SizedBox(width: 8),
                  Text(
                    'Spam — ${spam.length}',
                    style: OcText.label.copyWith(color: colors.textMuted),
                  ),
                ],
              ),
            ),
          ),
          if (_spamExpanded)
            for (final s in spam)
              PersonCard(contact: s, kind: PersonCardKind.incoming),
        ],
      ],
    );
  }

  Widget _buildBlockedTab(BuildContext context, PrivateContactsState contacts) {
    final blocked = contacts.blocked;

    if (blocked.isEmpty) {
      return const PrivateEmptyState(
        icon: OcIcons.block,
        message: "You haven't blocked anyone.",
      );
    }

    return ListView(
      children: [
        SectionLabel(
          'BLOCKED — ${blocked.length}',
          padding: const EdgeInsets.fromLTRB(10, 12, 10, 6),
        ),
        for (final b in blocked)
          PersonCard(contact: b, kind: PersonCardKind.blocked),
      ],
    );
  }
}

class _FriendsTabButton extends StatelessWidget {
  const _FriendsTabButton({
    required this.label,
    required this.count,
    required this.selected,
    required this.onTap,
  });

  final String label;
  final int count;
  final bool selected;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Hoverable(
      onTap: onTap,
      selected: selected,
      semanticLabel: '$label $count',
      focusRadius: BorderRadius.circular(15),
      builder: (context, state) {
        final bg = selected
            ? colors.accent
            : (state.active ? colors.hover : Colors.transparent);
        final textColor = selected ? colors.onAccent : colors.textSecondary;

        return Container(
          height: 30,
          padding: const EdgeInsets.symmetric(horizontal: 12),
          alignment: Alignment.center,
          decoration: BoxDecoration(
            color: bg,
            borderRadius: BorderRadius.circular(15),
          ),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              Text(
                label,
                style: TextStyle(
                  fontSize: 13,
                  fontWeight: FontWeight.w500,
                  color: textColor,
                ),
              ),
              const SizedBox(width: 6),
              Opacity(
                opacity: 0.65,
                child: Text(
                  '$count',
                  style: TextStyle(
                    fontSize: 11.5,
                    fontWeight: FontWeight.w500,
                    color: textColor,
                  ),
                ),
              ),
            ],
          ),
        );
      },
    );
  }
}
