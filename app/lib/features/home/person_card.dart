import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/providers/private_state.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/home/friend_rows.dart';
import 'package:opencord/features/home/private_errors.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/widgets/hoverable.dart';
import 'package:opencord/ui/widgets/oc_menu.dart';
import 'package:opencord/ui/widgets/popup_route.dart';

/// The kind of person card shown on Friends page tabs (§5.16).
enum PersonCardKind {
  friend,
  incoming,
  outgoing,
  blocked,
}

/// Action button on a person card or request card (§5.16):
/// height 32, padding 0 14, radius 10, 13/600, hover at 0.88, press 0.97.
class PersonActionButton extends StatelessWidget {
  const PersonActionButton({
    super.key,
    required this.label,
    required this.onPressed,
    this.primary = false,
  });

  final String label;
  final VoidCallback onPressed;
  final bool primary;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Hoverable(
      onTap: onPressed,
      semanticLabel: label,
      focusRadius: BorderRadius.circular(10),
      builder: (context, state) {
        final scale = state.pressed ? 0.97 : 1.0;
        final bg = primary ? colors.accent : colors.selected;
        final fg = primary ? colors.onAccent : colors.text;
        final opacity = state.hovered ? 0.88 : 1.0;

        return AnimatedScale(
          scale: scale,
          duration: const Duration(milliseconds: 80),
          curve: Curves.easeOut,
          child: Opacity(
            opacity: opacity,
            child: Container(
              height: 32,
              padding: const EdgeInsets.symmetric(horizontal: 14),
              alignment: Alignment.center,
              decoration: BoxDecoration(
                color: bg,
                borderRadius: BorderRadius.circular(10),
              ),
              child: Text(
                label,
                style: TextStyle(
                  fontSize: 13,
                  fontWeight: FontWeight.w600,
                  color: fg,
                ),
              ),
            ),
          ),
        );
      },
    );
  }
}

/// Round 36 button for friend actions (§5.16):
/// round 36, selected, icon 18, hover at 0.8, press 0.97.
class PersonRoundButton extends StatelessWidget {
  const PersonRoundButton({
    super.key,
    required this.icon,
    required this.tooltip,
    required this.onPressed,
  });

  final IconData icon;
  final String tooltip;
  final VoidCallback onPressed;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Hoverable(
      onTap: onPressed,
      semanticLabel: tooltip,
      focusRadius: BorderRadius.circular(18),
      builder: (context, state) {
        final scale = state.pressed ? 0.97 : 1.0;
        final opacity = state.hovered ? 0.8 : 1.0;

        return Tooltip(
          message: tooltip,
          child: AnimatedScale(
            scale: scale,
            duration: const Duration(milliseconds: 80),
            curve: Curves.easeOut,
            child: Opacity(
              opacity: opacity,
              child: Container(
                width: 36,
                height: 36,
                decoration: BoxDecoration(
                  color: colors.selected,
                  shape: BoxShape.circle,
                ),
                alignment: Alignment.center,
                child: Icon(icon, size: 18, color: colors.text),
              ),
            ),
          ),
        );
      },
    );
  }
}

/// A person card for the Friends page (§5.16):
/// min height 60, padding 8 10, radius 12, background sidebar, 1 px border,
/// 6 below, gap 12; hover hover; avatar 40 (initials 13.5); name 14/600;
/// line 12.5 textMuted; offline: avatar and text at 0.55.
class PersonCard extends ConsumerWidget {
  const PersonCard({
    super.key,
    required this.contact,
    required this.kind,
  });

  final PrivateContact contact;
  final PersonCardKind kind;

  Future<void> _showMore(
    BuildContext context,
    WidgetRef ref,
    String name,
  ) async {
    final rect = globalRectOf(context);
    await showOcMenu(
      context: context,
      position: Offset(rect.right, rect.bottom + 4),
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
    final isOffline = kind == PersonCardKind.friend && presence == Presence.offline;

    final presenceLabel = switch (presence) {
      Presence.online => 'Online',
      Presence.idle => 'Idle',
      Presence.doNotDisturb => 'Do not disturb',
      Presence.offline => 'Offline',
    };

    final subtitle = switch (kind) {
      PersonCardKind.friend => presenceLabel,
      PersonCardKind.incoming => 'Incoming friend request',
      PersonCardKind.outgoing => 'Outgoing friend request',
      PersonCardKind.blocked => 'Blocked',
    };

    return Hoverable(
      onTap: kind == PersonCardKind.friend
          ? () => openContactConversation(context, ref, contact)
          : null,
      semanticLabel: name,
      focusRadius: BorderRadius.circular(12),
      builder: (context, state) {
        final bg = state.active ? colors.hover : colors.sidebar;

        return Container(
          constraints: const BoxConstraints(minHeight: 60),
          margin: const EdgeInsets.only(bottom: 6),
          padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 8),
          decoration: BoxDecoration(
            color: bg,
            borderRadius: BorderRadius.circular(12),
            border: Border.all(color: colors.border, width: 1),
          ),
          child: Row(
            children: [
              Expanded(
                child: Opacity(
                  opacity: isOffline ? 0.55 : 1.0,
                  child: Row(
                    children: [
                      ContactAvatar(peer: contact.peer, size: 40, presence: presence),
                      const SizedBox(width: 12),
                      Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          mainAxisAlignment: MainAxisAlignment.center,
                          children: [
                            Text(
                              name,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: TextStyle(
                                fontSize: 14,
                                fontWeight: FontWeight.w600,
                                color: colors.text,
                              ),
                            ),
                            Text(
                              subtitle,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: TextStyle(
                                fontSize: 12.5,
                                color: colors.textMuted,
                              ),
                            ),
                          ],
                        ),
                      ),
                    ],
                  ),
                ),
              ),
              const SizedBox(width: 12),
              _buildActions(context, ref, name),
            ],
          ),
        );
      },
    );
  }

  Widget _buildActions(BuildContext context, WidgetRef ref, String name) {
    return switch (kind) {
      PersonCardKind.friend => Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            PersonRoundButton(
              icon: OcIcons.chat,
              tooltip: 'Message',
              onPressed: () => openContactConversation(context, ref, contact),
            ),
            const SizedBox(width: 6),
            Builder(
              builder: (btnContext) => PersonRoundButton(
                icon: OcIcons.moreHoriz,
                tooltip: 'More options',
                onPressed: () => _showMore(btnContext, ref, name),
              ),
            ),
          ],
        ),
      PersonCardKind.incoming => Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            PersonActionButton(
              label: 'Accept',
              primary: true,
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
            const SizedBox(width: 6),
            PersonActionButton(
              label: 'Decline',
              primary: false,
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
      PersonCardKind.outgoing => PersonActionButton(
          label: 'Cancel',
          primary: false,
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
      PersonCardKind.blocked => PersonActionButton(
          label: 'Unblock',
          primary: false,
          onPressed: () async {
            try {
              await ref.read(repositoryProvider).unblockUser(contact.peer);
            } on RepoException catch (e) {
              if (!context.mounted) return;
              await showPrivateFailure(context, e, name: name);
            }
          },
        ),
    };
  }
}
