import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/providers/private_state.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/home/private_errors.dart';
import 'package:opencord/features/members/member_avatar.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/avatar.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_button.dart';
import 'package:opencord/ui/widgets/oc_icon_button.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/popup_route.dart';

/// Shows someone's avatar across servers, with presence.
class ContactAvatar extends ConsumerWidget {
  const ContactAvatar({
    super.key,
    required this.peer,
    this.presence,
    this.size = 36,
  });

  final String peer;
  final Presence? presence;
  final double size;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final avatar = ref.watch(contactAvatarProvider(peer));
    final name = ref.watch(contactNameProvider(peer));
    final p = presence ?? ref.watch(contactPresenceProvider(peer));
    if (avatar != null) {
      return MemberAvatar(
        serverKey: avatar.serverKey,
        userId: avatar.userId,
        name: name,
        size: size,
        presence: p,
      );
    }
    return OcAvatar(id: peer, name: name, size: size, presence: p);
  }
}

/// Helper to open a conversation from a contact.
void openContactConversation(
  BuildContext context,
  WidgetRef ref,
  PrivateContact contact,
) async {
  final nav = ref.read(navigationProvider.notifier);
  if (contact.conversation != ConversationStatus.none) {
    nav.openConversation(contact.peer);
    return;
  }
  final shared = ref.read(sharedServersProvider(contact.peer));
  if (shared.isNotEmpty) {
    try {
      await ref
          .read(repositoryProvider)
          .openConversation(shared.first.serverKey, shared.first.peerUserId);
    } on RepoException catch (e) {
      if (!context.mounted) return;
      await showPrivateFailure(
        context,
        e,
        name: ref.read(contactNameProvider(contact.peer)),
      );
      return;
    }
  }
  if (!context.mounted) return;
  nav.openConversation(contact.peer);
}

/// A friend row on the Online / All tabs (§6.2).
class FriendRow extends ConsumerWidget {
  const FriendRow({super.key, required this.contact});

  final PrivateContact contact;

  void _showMore(BuildContext context, WidgetRef ref, String name) {
    final rect = globalRectOf(context);
    showOcMenu(
      context: context,
      position: Offset(rect.right, rect.bottom + OcSpace.s4),
      entries: [
        OcMenuItem(
          label: 'Remove friend',
          icon: OcIcons.personRemove,
          onSelected: () async {
            final confirmed = await confirmRemoveFriend(context, name);
            if (!confirmed || !context.mounted) return;
            try {
              await ref.read(repositoryProvider).removeFriend(contact.peer);
            } on RepoException catch (e) {
              if (!context.mounted) return;
              await showPrivateFailure(context, e, name: name);
            }
          },
        ),
        OcMenuItem(
          label: 'Block',
          icon: OcIcons.block,
          onSelected: () async {
            final confirmed = await confirmBlock(context, name);
            if (!confirmed || !context.mounted) return;
            try {
              await ref.read(repositoryProvider).blockUser(contact.peer);
            } on RepoException catch (e) {
              if (!context.mounted) return;
              await showPrivateFailure(context, e, name: name);
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
    final presenceLabel = switch (presence) {
      Presence.online => 'Online',
      Presence.idle => 'Idle',
      Presence.doNotDisturb => 'Do not disturb',
      Presence.offline => 'Offline',
    };

    return Hoverable(
      onTap: () => openContactConversation(context, ref, contact),
      semanticLabel: name,
      builder: (context, state) => Container(
        height: 56,
        padding: const EdgeInsets.symmetric(
          horizontal: OcSpace.s12,
          vertical: OcSpace.s8,
        ),
        decoration: BoxDecoration(
          color: state.active ? colors.hover : null,
          border: Border(
            top: BorderSide(
              color: colors.border.withValues(alpha: 0.4),
              width: 1,
            ),
          ),
        ),
        child: Row(
          children: [
            ContactAvatar(peer: contact.peer, presence: presence),
            const SizedBox(width: OcSpace.s12),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  Text(
                    name,
                    style: OcText.body.copyWith(
                      color: colors.text,
                      fontWeight: FontWeight.w600,
                    ),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                  ),
                  Text(
                    presenceLabel,
                    style: OcText.small.copyWith(color: colors.textMuted),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                  ),
                ],
              ),
            ),
            OcIconButton(
              icon: OcIcons.chat,
              tooltip: 'Message',
              onPressed: () => openContactConversation(context, ref, contact),
            ),
            const SizedBox(width: OcSpace.s4),
            Builder(
              builder: (btnContext) => OcIconButton(
                icon: OcIcons.moreHoriz,
                tooltip: 'More options',
                onPressed: () => _showMore(btnContext, ref, name),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// An incoming friend request row (§6.2).
class IncomingRequestRow extends ConsumerWidget {
  const IncomingRequestRow({super.key, required this.contact});

  final PrivateContact contact;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final name = ref.watch(contactNameProvider(contact.peer));

    return Container(
      height: 56,
      padding: const EdgeInsets.symmetric(
        horizontal: OcSpace.s12,
        vertical: OcSpace.s8,
      ),
      decoration: BoxDecoration(
        border: Border(
          top: BorderSide(
            color: colors.border.withValues(alpha: 0.4),
            width: 1,
          ),
        ),
      ),
      child: Row(
        children: [
          ContactAvatar(peer: contact.peer),
          const SizedBox(width: OcSpace.s12),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                Text(
                  name,
                  style: OcText.body.copyWith(
                    color: colors.text,
                    fontWeight: FontWeight.w600,
                  ),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                ),
                Text(
                  'Incoming friend request',
                  style: OcText.small.copyWith(color: colors.textMuted),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                ),
              ],
            ),
          ),
          OcIconButton(
            icon: OcIcons.check,
            tooltip: 'Accept',
            onPressed: () async {
              try {
                await ref
                    .read(repositoryProvider)
                    .answerFriendRequest(contact.peer, accept: true);
              } on RepoException catch (e) {
                if (!context.mounted) return;
                await showPrivateFailure(
                  context,
                  e,
                  name: name,
                  server: contact.relayServerKey,
                  action: PrivateAction.answer,
                );
              }
            },
          ),
          const SizedBox(width: OcSpace.s4),
          OcIconButton(
            icon: OcIcons.close,
            tooltip: 'Decline',
            onPressed: () async {
              try {
                await ref
                    .read(repositoryProvider)
                    .answerFriendRequest(contact.peer, accept: false);
              } on RepoException catch (e) {
                if (!context.mounted) return;
                await showPrivateFailure(
                  context,
                  e,
                  name: name,
                  server: contact.relayServerKey,
                  action: PrivateAction.answer,
                );
              }
            },
          ),
        ],
      ),
    );
  }
}

/// An outgoing friend request row (§6.2).
class OutgoingRequestRow extends ConsumerWidget {
  const OutgoingRequestRow({super.key, required this.contact});

  final PrivateContact contact;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final name = ref.watch(contactNameProvider(contact.peer));

    return Container(
      height: 56,
      padding: const EdgeInsets.symmetric(
        horizontal: OcSpace.s12,
        vertical: OcSpace.s8,
      ),
      decoration: BoxDecoration(
        border: Border(
          top: BorderSide(
            color: colors.border.withValues(alpha: 0.4),
            width: 1,
          ),
        ),
      ),
      child: Row(
        children: [
          ContactAvatar(peer: contact.peer),
          const SizedBox(width: OcSpace.s12),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              mainAxisAlignment: MainAxisAlignment.center,
              children: [
                Text(
                  name,
                  style: OcText.body.copyWith(
                    color: colors.text,
                    fontWeight: FontWeight.w600,
                  ),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                ),
                Text(
                  'Outgoing friend request',
                  style: OcText.small.copyWith(color: colors.textMuted),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                ),
              ],
            ),
          ),
          OcIconButton(
            icon: OcIcons.close,
            tooltip: 'Cancel',
            onPressed: () async {
              try {
                await ref
                    .read(repositoryProvider)
                    .cancelFriendRequest(contact.peer);
              } on RepoException catch (e) {
                if (!context.mounted) return;
                await showPrivateFailure(
                  context,
                  e,
                  name: name,
                  action: PrivateAction.friendRequest,
                );
              }
            },
          ),
        ],
      ),
    );
  }
}

/// A message request row (§6.2).
class MessageRequestRow extends ConsumerWidget {
  const MessageRequestRow({super.key, required this.contact});

  final PrivateContact contact;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final name = ref.watch(contactNameProvider(contact.peer));
    final preview = contact.preview.isNotEmpty
        ? contact.preview
        : 'Wants to message you';

    return Hoverable(
      onTap: () =>
          ref.read(navigationProvider.notifier).openConversation(contact.peer),
      semanticLabel: name,
      builder: (context, state) => Container(
        height: 56,
        padding: const EdgeInsets.symmetric(
          horizontal: OcSpace.s12,
          vertical: OcSpace.s8,
        ),
        decoration: BoxDecoration(
          color: state.active ? colors.hover : null,
          border: Border(
            top: BorderSide(
              color: colors.border.withValues(alpha: 0.4),
              width: 1,
            ),
          ),
        ),
        child: Row(
          children: [
            ContactAvatar(peer: contact.peer),
            const SizedBox(width: OcSpace.s12),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                mainAxisAlignment: MainAxisAlignment.center,
                children: [
                  Text(
                    name,
                    style: OcText.body.copyWith(
                      color: colors.text,
                      fontWeight: FontWeight.w600,
                    ),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
                  ),
                  Text(
                    preview,
                    style: OcText.small.copyWith(color: colors.textMuted),
                    maxLines: 1,
                    overflow: TextOverflow.ellipsis,
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

/// A blocked person row (§6.2).
class BlockedRow extends ConsumerWidget {
  const BlockedRow({super.key, required this.contact});

  final PrivateContact contact;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final name = ref.watch(contactNameProvider(contact.peer));

    return Container(
      height: 56,
      padding: const EdgeInsets.symmetric(
        horizontal: OcSpace.s12,
        vertical: OcSpace.s8,
      ),
      decoration: BoxDecoration(
        border: Border(
          top: BorderSide(
            color: colors.border.withValues(alpha: 0.4),
            width: 1,
          ),
        ),
      ),
      child: Row(
        children: [
          ContactAvatar(peer: contact.peer),
          const SizedBox(width: OcSpace.s12),
          Expanded(
            child: Text(
              name,
              style: OcText.body.copyWith(
                color: colors.text,
                fontWeight: FontWeight.w600,
              ),
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
            ),
          ),
          OcButton(
            label: 'Unblock',
            onPressed: () async {
              try {
                await ref.read(repositoryProvider).unblockUser(contact.peer);
              } on RepoException catch (e) {
                if (!context.mounted) return;
                await showPrivateFailure(context, e, name: name);
              }
            },
          ),
        ],
      ),
    );
  }
}
