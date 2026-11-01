import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/providers/private_state.dart';
import 'package:opencord/core/settings/key_value_store.dart';
import 'package:opencord/features/home/friend_rows.dart';
import 'package:opencord/features/window/header_bar.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/choice_chips.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/section_label.dart';

const friendsTabKey = 'ui.friendsTab';

/// The tabs on the Friends page (§6.2).
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

class FriendsPage extends ConsumerStatefulWidget {
  const FriendsPage({
    super.key,
    this.leadingControls = false,
    this.trailingControls = false,
  });

  final bool leadingControls;
  final bool trailingControls;

  @override
  ConsumerState<FriendsPage> createState() => _FriendsPageState();
}

class _FriendsPageState extends ConsumerState<FriendsPage> {
  bool _spamExpanded = false;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    final contacts = ref.watch(privateContactsProvider);
    final tab = ref.watch(friendsTabProvider);

    return Column(
      children: [
        // 60 px Header with title and tabs (§6.2)
        HeaderBar(
          leadingControls: widget.leadingControls,
          trailingControls: widget.trailingControls,
          padding: const EdgeInsets.symmetric(horizontal: OcSpace.s16),
          child: Row(
            children: [
              IgnorePointer(
                child: Text(
                  'Friends',
                  style: OcText.title.copyWith(color: colors.text),
                ),
              ),
              const SizedBox(width: OcSpace.s20),
              // Scrolls sideways when the window is too narrow for all tabs;
              // only as wide as the tabs, so the space after them drags.
              Flexible(
                child: SingleChildScrollView(
                  scrollDirection: Axis.horizontal,
                  child: ChoiceChips<FriendsTab>(
                    value: tab,
                    onChanged: ref.read(friendsTabProvider.notifier).select,
                    options: const [
                      (FriendsTab.online, 'Online'),
                      (FriendsTab.all, 'All'),
                      (FriendsTab.pending, 'Pending'),
                      (FriendsTab.requests, 'Message requests'),
                      (FriendsTab.blocked, 'Blocked'),
                    ],
                  ),
                ),
              ),
            ],
          ),
        ),

        // Main content area
        Expanded(
          child: switch (tab) {
            FriendsTab.online => _buildOnlineTab(context, contacts),
            FriendsTab.all => _buildAllTab(context, contacts),
            FriendsTab.pending => _buildPendingTab(context, contacts),
            FriendsTab.requests => _buildRequestsTab(context, contacts),
            FriendsTab.blocked => _buildBlockedTab(context, contacts),
          },
        ),
      ],
    );
  }

  Widget _buildEmptyState(String message) {
    final colors = context.oc;
    return Padding(
      padding: const EdgeInsets.only(top: OcSpace.s32),
      child: Center(
        child: Text(
          message,
          style: OcText.body.copyWith(color: colors.textMuted),
        ),
      ),
    );
  }

  Widget _buildOnlineTab(BuildContext context, PrivateContactsState contacts) {
    final allFriends = contacts.friends;
    final onlineFriends = allFriends.where((c) {
      final presence = ref.watch(contactPresenceProvider(c.peer));
      return presence != Presence.offline;
    }).toList();

    return ListView(
      padding: const EdgeInsets.symmetric(
        horizontal: OcSpace.s20,
        vertical: OcSpace.s16,
      ),
      children: [
        SectionLabel(
          'ONLINE — ${onlineFriends.length}',
          padding: const EdgeInsets.only(bottom: OcSpace.s8),
        ),
        if (onlineFriends.isEmpty)
          _buildEmptyState("No one's online right now.")
        else
          for (final friend in onlineFriends) FriendRow(contact: friend),
      ],
    );
  }

  Widget _buildAllTab(BuildContext context, PrivateContactsState contacts) {
    final allFriends = contacts.friends;

    return ListView(
      padding: const EdgeInsets.symmetric(
        horizontal: OcSpace.s20,
        vertical: OcSpace.s16,
      ),
      children: [
        SectionLabel(
          'ALL — ${allFriends.length}',
          padding: const EdgeInsets.only(bottom: OcSpace.s8),
        ),
        if (allFriends.isEmpty)
          _buildEmptyState("You haven't added any friends yet.")
        else
          for (final friend in allFriends) FriendRow(contact: friend),
      ],
    );
  }

  Widget _buildPendingTab(BuildContext context, PrivateContactsState contacts) {
    final colors = context.oc;
    final incoming = contacts.incomingRequests;
    final outgoing = contacts.outgoingRequests;
    final spam = contacts.spamRequests;
    final totalNonSpam = incoming.length + outgoing.length;

    return ListView(
      padding: const EdgeInsets.symmetric(
        horizontal: OcSpace.s20,
        vertical: OcSpace.s16,
      ),
      children: [
        SectionLabel(
          'PENDING — $totalNonSpam',
          padding: const EdgeInsets.only(bottom: OcSpace.s8),
        ),
        if (totalNonSpam == 0 && spam.isEmpty)
          _buildEmptyState('No pending requests.')
        else ...[
          for (final inc in incoming) IncomingRequestRow(contact: inc),
          for (final out in outgoing) OutgoingRequestRow(contact: out),
          if (spam.isNotEmpty) ...[
            const SizedBox(height: OcSpace.s12),
            Hoverable(
              onTap: () => setState(() => _spamExpanded = !_spamExpanded),
              semanticLabel: 'Spam',
              builder: (context, state) => Container(
                padding: const EdgeInsets.symmetric(
                  horizontal: OcSpace.s12,
                  vertical: OcSpace.s8,
                ),
                decoration: BoxDecoration(
                  color: state.active ? colors.hover : null,
                  borderRadius: BorderRadius.circular(OcRadius.row),
                ),
                child: Row(
                  children: [
                    Icon(
                      _spamExpanded ? OcIcons.expandMore : OcIcons.chevronRight,
                      size: 16,
                      color: colors.textMuted,
                    ),
                    const SizedBox(width: OcSpace.s8),
                    Text(
                      'Spam — ${spam.length}',
                      style: OcText.label.copyWith(color: colors.textMuted),
                    ),
                  ],
                ),
              ),
            ),
            if (_spamExpanded)
              for (final s in spam) IncomingRequestRow(contact: s),
          ],
        ],
      ],
    );
  }

  Widget _buildRequestsTab(
    BuildContext context,
    PrivateContactsState contacts,
  ) {
    final requests = contacts.messageRequests;

    return ListView(
      padding: const EdgeInsets.symmetric(
        horizontal: OcSpace.s20,
        vertical: OcSpace.s16,
      ),
      children: [
        SectionLabel(
          'MESSAGE REQUESTS — ${requests.length}',
          padding: const EdgeInsets.only(bottom: OcSpace.s8),
        ),
        if (requests.isEmpty)
          _buildEmptyState('No message requests.')
        else
          for (final req in requests) MessageRequestRow(contact: req),
      ],
    );
  }

  Widget _buildBlockedTab(BuildContext context, PrivateContactsState contacts) {
    final blocked = contacts.blocked;

    return ListView(
      padding: const EdgeInsets.symmetric(
        horizontal: OcSpace.s20,
        vertical: OcSpace.s16,
      ),
      children: [
        SectionLabel(
          'BLOCKED — ${blocked.length}',
          padding: const EdgeInsets.only(bottom: OcSpace.s8),
        ),
        if (blocked.isEmpty)
          _buildEmptyState("You haven't blocked anyone.")
        else
          for (final b in blocked) BlockedRow(contact: b),
      ],
    );
  }
}
