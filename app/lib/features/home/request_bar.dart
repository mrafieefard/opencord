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

/// The bar shown at the bottom of a conversation in request mode (§6.3).
class RequestBar extends ConsumerWidget {
  const RequestBar({super.key, required this.peer});

  final String peer;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final colors = context.oc;
    final name = ref.watch(contactNameProvider(peer));

    return Container(
      padding: const EdgeInsets.symmetric(
        horizontal: OcSpace.s16,
        vertical: OcSpace.s12,
      ),
      decoration: BoxDecoration(
        color: colors.surface,
        border: Border(top: BorderSide(color: colors.border)),
      ),
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
            onPressed: () async {
              try {
                await ref.read(repositoryProvider).acceptMessageRequest(peer);
              } on RepoException catch (e) {
                if (!context.mounted) return;
                await showPrivateFailure(context, e, name: name);
              }
            },
          ),
          const SizedBox(width: OcSpace.s8),
          OcButton(
            label: 'Ignore',
            onPressed: () async {
              final confirmed = await confirmAction(
                context,
                title: 'Ignore request?',
                message: 'This removes the conversation.',
                action: 'Ignore',
              );
              if (!confirmed || !context.mounted) return;
              try {
                await ref.read(repositoryProvider).ignoreMessageRequest(peer);
                if (!context.mounted) return;
                ref.read(navigationProvider.notifier).openFriends();
              } on RepoException catch (e) {
                if (!context.mounted) return;
                await showPrivateFailure(context, e, name: name);
              }
            },
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
