import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/model/presence.dart';
import 'package:opencord/core/providers/private_state.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/home/private_errors.dart';
import 'package:opencord/features/members/member_avatar.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/widgets/avatar.dart';

/// Shows someone's avatar across servers, with presence.
class ContactAvatar extends ConsumerWidget {
  const ContactAvatar({
    super.key,
    required this.peer,
    this.presence,
    this.size = 36,
    this.presenceDot = true,
  });

  final String peer;
  final Presence? presence;
  final double size;

  /// Off beside messages, as channel messages show none.
  final bool presenceDot;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    ref.watch(
      privateContactsProvider.select((c) => c.byPeer[peer]?.relayServerKey),
    );
    final avatar = ref.read(contactAvatarProvider(peer));
    final name = ref.watch(contactNameProvider(peer));
    final p = presenceDot
        ? presence ?? ref.watch(contactPresenceProvider(peer))
        : null;
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
