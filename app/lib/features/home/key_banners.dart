import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';

import 'package:opencord/core/providers/private_state.dart';
import 'package:opencord/core/providers/providers.dart';
import 'package:opencord/core/repository/repository.dart';
import 'package:opencord/features/home/encryption_dialog.dart';
import 'package:opencord/features/home/private_errors.dart';
import 'package:opencord/ui/theme/oc_colors.dart';
import 'package:opencord/ui/theme/oc_icons.dart';
import 'package:opencord/ui/theme/oc_metrics.dart';
import 'package:opencord/ui/theme/oc_text.dart';
import 'package:opencord/ui/widgets/oc_button.dart';

/// Above a chat's composer: their keys changed (design 2026-11-01 §8.2),
/// or the words didn't match (§9.3).
class KeyBanners extends ConsumerWidget {
  const KeyBanners({super.key, required this.peer});

  final String peer;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final contact = ref.watch(
      privateContactsProvider.select((contacts) => contacts.byPeer[peer]),
    );
    if (contact == null) return const SizedBox.shrink();
    final name = ref.watch(contactNameProvider(peer));
    return Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        if (contact.verification == PrivateVerification.mismatch)
          const KeyBanner(
            icon: OcIcons.warning,
            warning: true,
            message:
                "The words didn't match. Someone may be able to read this "
                "chat. Don't send anything private.",
          ),
        if (contact.newKeys)
          KeyBanner(
            icon: OcIcons.key,
            message:
                "$name's keys changed. They may have reinstalled, or someone "
                "may be trying to read this chat. Your messages won't reach "
                'the new keys until you compare the words.',
            actions: [
              OcButton.primary(
                label: 'Compare words',
                onPressed: () => showEncryptionDialog(
                  context,
                  ref,
                  peer: peer,
                  name: name,
                  newKeys: true,
                ),
              ),
              OcButton(
                label: 'Use new keys anyway',
                onPressed: () async {
                  try {
                    await ref
                        .read(repositoryProvider)
                        .acceptNewPrivateKeys(peer);
                  } on RepoException catch (e) {
                    if (!context.mounted) return;
                    await showPrivateFailure(context, e, name: name);
                  }
                },
              ),
            ],
          ),
      ],
    );
  }
}

/// One line of notice with its buttons, on the chat's surface.
class KeyBanner extends StatelessWidget {
  const KeyBanner({
    super.key,
    required this.icon,
    required this.message,
    this.actions = const [],
    this.warning = false,
  });

  final IconData icon;
  final String message;
  final List<Widget> actions;

  /// Drawn in the recording color: something may be wrong.
  final bool warning;

  @override
  Widget build(BuildContext context) {
    final colors = context.oc;
    return Container(
      width: double.infinity,
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
          Icon(
            icon,
            size: 18,
            color: warning ? colors.recording : colors.textSecondary,
          ),
          const SizedBox(width: OcSpace.s12),
          Expanded(
            child: Text(
              message,
              style: OcText.body.copyWith(color: colors.text),
            ),
          ),
          for (final action in actions) ...[
            const SizedBox(width: OcSpace.s8),
            action,
          ],
        ],
      ),
    );
  }
}
