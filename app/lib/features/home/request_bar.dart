import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/private_state.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/home/private_errors.dart';
import 'package:opencord/features/shell/navigation.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/confirm_dialog.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

/// Shared helper to accept a message request (§5.16).
Future<void> acceptMessageRequestAction(
  BuildContext context,
  WidgetRef ref,
  String peer,
  String name, {
  bool openChat = false,
}) async {
  try {
    await ref.read(repositoryProvider).acceptMessageRequest(peer);
    if (openChat && context.mounted) {
      ref.read(navigationProvider.notifier).openConversation(peer);
    }
  } on RepoException catch (e) {
    if (!context.mounted) return;
    await showPrivateFailure(context, e, name: name);
  }
}

/// Shared helper to decline (ignore) a message request (§5.16).
Future<bool> declineMessageRequestAction(
  BuildContext context,
  WidgetRef ref,
  String peer,
  String name, {
  bool navigateToFriends = false,
}) async {
  final confirmed = await confirmAction(
    context,
    title: 'Ignore request?',
    message: 'This removes the conversation.',
    action: 'Ignore',
  );
  if (!confirmed || !context.mounted) return false;
  try {
    await ref.read(repositoryProvider).ignoreMessageRequest(peer);
    if (navigateToFriends && context.mounted) {
      ref.read(navigationProvider.notifier).openFriends();
    }
    return true;
  } on RepoException catch (e) {
    if (!context.mounted) return false;
    await showPrivateFailure(context, e, name: name);
    return false;
  }
}

/// The bar shown at the bottom of a conversation in request mode (§6.3, §5.12).
class RequestBar extends ConsumerWidget {
  const RequestBar({super.key, required this.peer});

  final String peer;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final name = ref.watch(contactNameProvider(peer));

    return Container(
      constraints: const BoxConstraints(minHeight: OcFloats.bottom),
      padding: const EdgeInsets.symmetric(
        horizontal: OcSpace.s16,
        vertical: OcSpace.s12,
      ),
      alignment: Alignment.center,
      child: Row(
        children: [
          Expanded(
            child: Text(
              '$name wants to message you.',
              style: OcText.body.copyWith(
                color: colors.text,
                fontWeight: FontWeight.w500,
              ),
            ),
          ),
          OcButton.primary(
            label: 'Accept',
            onPressed: () =>
                acceptMessageRequestAction(context, ref, peer, name),
          ),
          const SizedBox(width: OcSpace.s8),
          OcButton(
            label: 'Ignore',
            onPressed: () => declineMessageRequestAction(
              context,
              ref,
              peer,
              name,
              navigateToFriends: true,
            ),
          ),
          const SizedBox(width: OcSpace.s8),
          OcButton(
            label: 'Block',
            onPressed: () async {
              final confirmed = await confirmBlock(context, name);
              if (!confirmed || !context.mounted) return;
              try {
                await ref.read(repositoryProvider).blockUser(peer);
                if (!context.mounted) return;
                ref.read(navigationProvider.notifier).openFriends();
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
